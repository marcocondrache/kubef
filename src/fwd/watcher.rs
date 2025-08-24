use std::sync::Arc;

use anyhow::{Context, Result};
use futures::{StreamExt, future::ready};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api, Client, ResourceExt,
    core::Selector,
    runtime::{
        WatchStreamExt,
        reflector::{self, ReflectHandle, Store},
        watcher,
    },
};
use rand::seq::IndexedRandom;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct PodWatcher {
    store: Store<Pod>,
    subscriber: ReflectHandle<Pod>,
}

impl PodWatcher {
    pub async fn new(
        client: Client,
        namespace: String,
        selector: Selector,
        token: CancellationToken,
    ) -> Result<Self> {
        let api: Api<Pod> = Api::namespaced(client, namespace.as_ref());
        let config = watcher::Config::default().labels_from(&selector);

        let (store, writer) = reflector::store_shared(256);

        let subscriber = writer.subscribe().context("Failed to create subscriber")?;

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

        store.wait_until_ready().await?;

        Ok(Self { store, subscriber })
    }

    pub async fn wait_pod(&self) -> Result<Arc<Pod>> {
        let state = self.store.state();
        let mut subscriber = self.subscriber.clone();

        if !state.is_empty() {
            return Ok(state.choose(&mut rand::rng()).unwrap().clone());
        }

        subscriber.next().await.context("Cannot get next pod")
    }
}
