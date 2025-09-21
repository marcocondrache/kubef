use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::{Container, Pod, PodSpec};
use kube::{
    Api,
    api::{DeleteParams, ObjectMeta, PostParams, WatchEvent, WatchParams},
};
use nanoid::nanoid;
use tracing::{debug, instrument};

static ALPHABET: [char; 16] = [
    '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f',
];

#[allow(dead_code)]
pub enum ProxyDestination {
    Tcp(SocketAddr),
    Udp(SocketAddr),
}

impl ProxyDestination {
    pub fn to_socat_target(&self) -> String {
        match self {
            ProxyDestination::Tcp(target) => format!("TCP:{target}"),
            ProxyDestination::Udp(target) => format!("UDP:{target}"),
        }
    }
}

pub struct Proxy {
    id: String,
    api: Api<Pod>,
    permit: AtomicBool,
}

impl Proxy {
    const NAME_PREFIX: &str = "kubef-";

    const IMAGE: &str = "alpine/socat:latest";
    const IMAGE_BIN: &str = "socat";

    pub const PORT: u16 = 8080;

    pub fn new(api: Api<Pod>) -> Self {
        Self {
            id: nanoid!(6, &ALPHABET),
            api,
            permit: AtomicBool::new(false),
        }
    }

    #[inline]
    pub fn is_spawned(&self) -> bool {
        self.permit.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn get_name(&self) -> String {
        format!("{}{}", Self::NAME_PREFIX, self.id)
    }

    pub async fn abort(&self) -> Result<()> {
        if !self.permit.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Proxy not spawned"));
        }

        self.api
            .delete(&self.get_name(), &DeleteParams::default())
            .await?;

        self.permit.store(false, Ordering::Relaxed);

        Ok(())
    }

    pub async fn wait_until_exit(&self) -> Result<()> {
        if !self.is_spawned() {
            return Err(anyhow::anyhow!("Proxy not spawned"));
        }

        let params = WatchParams::default().labels(&format!("kubef.io/id={}", self.id));
        let stream = self.api.watch_metadata(&params, "0").await?;

        tokio::pin!(stream);

        while let Ok(Some(event)) = stream.try_next().await {
            match event {
                WatchEvent::Error(_) | WatchEvent::Deleted(_) => return Ok(()),
                _ => {
                    debug!("Proxy {} received event: {:?}", self.id, event);
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self, destination))]
    pub async fn spawn(&self, destination: &ProxyDestination) -> Result<()> {
        if self.is_spawned() {
            return Err(anyhow::anyhow!("Proxy already spawned"));
        }

        let source = format!("TCP-LISTEN:{},reuseaddr,fork", Self::PORT);
        let destination = destination.to_socat_target();

        // TODO: Can we improve this?
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some(self.get_name()),
                labels: Some(BTreeMap::from([
                    ("kubef.io/id".to_string(), self.id.clone()),
                    ("kubef.io/proxy".to_string(), "true".to_string()),
                ])),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "socat".to_string(),
                    image: Some(Self::IMAGE.to_string()),
                    command: Some(vec![Self::IMAGE_BIN.to_string(), source, destination]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: None,
        };

        self.api.create(&PostParams::default(), &pod).await?;
        self.permit.store(true, Ordering::Relaxed);

        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if !self.is_spawned() {
            return;
        }

        let api = self.api.clone();
        let name = self.get_name();

        tokio::spawn(async move { api.delete(&name, &DeleteParams::default()).await });
    }
}
