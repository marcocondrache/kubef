use std::net::SocketAddr;

use anyhow::Result;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api,
    api::{DeleteParams, PostParams, WatchEvent, WatchParams},
};
use leon::Template;
use nanoid::nanoid;

static ALPHABET: [char; 16] = [
    '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f',
];

// TODO: use compile-parsing template
static POD_TEMPLATE: &str = r#"
  apiVersion: v1
  kind: Pod
  metadata:
    name: kubef-{id}
    labels:
      kubef.io/id: {id}
      kubef.io/proxy: "true"
  spec:
    containers:
      - name: socat
        image: alpine/socat:latest
        command: 
          - socat
          - {protocol}-LISTEN:{port},fork
          - {protocol}:{remote_ip}:{remote_port}
"#;

pub struct Proxy {
    pub id: String,

    api: Api<Pod>,
}

impl Proxy {
    pub fn new(api: Api<Pod>) -> Self {
        Self {
            id: nanoid!(6, &ALPHABET),
            api,
        }
    }

    pub async fn delete(&self) -> Result<()> {
        self.api
            .delete(&format!("kubef-{}", self.id), &DeleteParams::default())
            .await?;

        Ok(())
    }

    pub async fn wait_until_exit(&self) -> Result<()> {
        let params = WatchParams::default().labels(&format!("kubef.io/id={}", self.id));
        let stream = self.api.watch_metadata(&params, "0").await?;

        tokio::pin!(stream);

        while let Ok(Some(event)) = stream.try_next().await {
            match event {
                WatchEvent::Error(_) | WatchEvent::Deleted(_) => return Ok(()),
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn apply(&self, port: u16, target: &SocketAddr, protocol: &str) -> Result<()> {
        // TODO: fix refs
        let parameters = [
            ("id", self.id.as_str()),
            ("port", &port.to_string()),
            ("remote_port", &target.port().to_string()),
            ("remote_ip", &target.ip().to_string()),
            ("protocol", protocol),
        ];

        let template = Template::parse(POD_TEMPLATE)?;
        let manifest = template.render(&parameters)?;

        let pod = serde_yaml_ng::from_str::<Pod>(&manifest)?;

        self.api.create(&PostParams::default(), &pod).await?;

        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let api = self.api.clone();
        let id = self.id.clone();

        tokio::spawn(async move {
            api.delete(&format!("kubef-{id}"), &DeleteParams::default())
                .await
        });
    }
}
