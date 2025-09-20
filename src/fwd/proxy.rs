use std::{collections::BTreeMap, net::SocketAddr};

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

pub struct Proxy {
    id: String,
    api: Api<Pod>,
}

impl Proxy {
    const NAME_PREFIX: &str = "kubef-";

    const IMAGE: &str = "alpine/socat:latest";
    const IMAGE_BIN: &str = "socat";

    pub fn new(api: Api<Pod>) -> Self {
        Self {
            id: nanoid!(6, &ALPHABET),
            api,
        }
    }

    pub async fn abort(&self) -> Result<()> {
        self.api
            .delete(&self.get_name(), &DeleteParams::default())
            .await?;

        Ok(())
    }

    pub fn get_name(&self) -> String {
        format!("{}{}", Self::NAME_PREFIX, self.id)
    }

    pub async fn wait_until_exit(&self) -> Result<()> {
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

    #[instrument(skip(self), fields(port = %port, target = %target, protocol = %protocol))]
    pub async fn spawn(&self, port: u16, target: &SocketAddr, protocol: &str) -> Result<()> {
        let source = format!("{protocol}-LISTEN:{port},fork");
        let destination = format!("{protocol}:{target}");

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

        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let api = self.api.clone();
        let name = self.get_name();

        tokio::spawn(async move { api.delete(&name, &DeleteParams::default()).await });
    }
}
