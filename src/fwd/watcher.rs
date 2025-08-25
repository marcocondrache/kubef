use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, Result};
use futures::{StreamExt, future::ready};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api, Client,
    api::PartialObjectMeta,
    core::Selector,
    runtime::{
        WatchStreamExt,
        reflector::{self, ReflectHandle, Store},
        watcher::{self},
    },
};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Clone)]
pub struct PodWatcher {
    store: Store<PartialObjectMeta<Pod>>,
    subscriber: ReflectHandle<PartialObjectMeta<Pod>>,
    counter: Arc<AtomicUsize>,
}

impl PodWatcher {
    pub async fn new(
        client: Client,
        namespace: &str,
        selector: Selector,
        token: CancellationToken,
    ) -> Result<Self> {
        let api: Api<Pod> = Api::namespaced(client, namespace);
        let config = watcher::Config::default().labels_from(&selector);

        let (store, writer) = reflector::store_shared(256);

        let subscriber = writer.subscribe().context("Failed to create subscriber")?;

        tokio::spawn(async move {
            watcher::metadata_watcher(api, config)
                .default_backoff()
                .reflect(writer)
                .applied_objects()
                .take_until(token.cancelled())
                .for_each(|_| ready(()))
                .await;
        });

        store.wait_until_ready().await?;

        Ok(Self {
            store,
            subscriber,
            counter: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub async fn next(&self) -> Result<Arc<PartialObjectMeta<Pod>>> {
        if !self.store.is_empty() {
            let state = self.store.state();
            let index = self.counter.fetch_add(1, Ordering::Relaxed) % state.len();

            return Ok(state
                .get(index)
                .context("Cannot get next load balanced pod")?
                .clone());
        }

        info!("No pods found, waiting for next one");

        self.subscriber
            .clone()
            .next()
            .await
            .context("Cannot get next pod")
    }
}
