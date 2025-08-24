use futures::{StreamExt, future::ready};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api, Client, ResourceExt,
    core::Selector,
    runtime::{
        WatchStreamExt,
        reflector::{self, Store},
        watcher,
    },
};
use tokio_util::sync::CancellationToken;

pub struct PodWatcher {
    pub store: Store<Pod>,
}

impl PodWatcher {
    pub async fn new(
        client: Client,
        namespace: String,
        selector: Selector,
        token: CancellationToken,
    ) -> Self {
        let api: Api<Pod> = Api::namespaced(client, namespace.as_ref());
        let config = watcher::Config::default().labels_from(&selector);

        let (store, writer) = reflector::store();

        tokio::spawn(async move {
            watcher::watcher(api, config)
                .default_backoff()
                .modify(|pod| {
                    pod.managed_fields_mut().clear();
                    pod.annotations_mut().clear();
                    pod.finalizers_mut().clear();
                })
                .reflect(writer)
                .applied_objects()
                .take_until(token.cancelled())
                .for_each(|_| ready(()))
                .await
        });

        let _ = store.wait_until_ready().await;

        Self { store }
    }
}
