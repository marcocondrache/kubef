use std::sync::Arc;

use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api, Client,
    runtime::{WatchStreamExt, watcher},
};
use tokio::{pin, sync::RwLock, task::JoinHandle};

use crate::cnf;

pub struct PodWatcher {
    current_pod: Arc<RwLock<Option<Pod>>>,
    pub handle: JoinHandle<()>,
}

impl PodWatcher {
    pub async fn get_pod(&self) -> Option<Pod> {
        let read = self.current_pod.read().await;
        read.clone()
    }

    pub async fn new(client: Client, resource: &cnf::Resource) -> Self {
        // TODO: wrong, find more generic way to select the pod
        let selectors = [format!("app.kubernetes.io/name={}", resource.name)];

        let api: Api<Pod> = Api::namespaced(client, resource.namespace.as_ref());
        let config = watcher::Config::default().labels(&selectors[0]);

        let stream = watcher::watcher(api, config)
            .default_backoff()
            .applied_objects();

        let current_pod = Arc::new(RwLock::new(None));
        let current_pod_clone = current_pod.clone();
        let stream_handle = tokio::spawn(async move {
            pin!(stream);

            while let Some(Ok(pod)) = stream.next().await {
                if Self::is_pod_ready(&pod) {
                    let mut write = current_pod_clone.write().await;
                    *write = Some(pod);
                }
            }
        });

        Self {
            current_pod,
            handle: stream_handle,
        }
    }
}

impl PodWatcher {
    fn is_pod_ready(pod: &Pod) -> bool {
        pod.status
            .as_ref()
            .map(|status| {
                status.phase == Some("Running".to_string())
                    && status
                        .conditions
                        .as_ref()
                        .map(|conditions| {
                            conditions
                                .iter()
                                .any(|condition| condition.type_ == "Ready")
                        })
                        .unwrap_or(false)
            })
            .unwrap_or(false)
    }
}
