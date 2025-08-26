use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
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
use tracing::{debug, info};

use crate::cnf::schema::SelectorPolicy;

#[derive(Clone)]
pub struct PodWatcher {
    store: Store<PartialObjectMeta<Pod>>,
    subscriber: ReflectHandle<PartialObjectMeta<Pod>>,
    counter: Arc<AtomicUsize>,
    policy: SelectorPolicy,
}

impl PodWatcher {
    pub async fn new(
        client: Client,
        namespace: &str,
        selector: Selector,
        policy: SelectorPolicy,
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

        tokio::time::timeout(Duration::from_secs(10), store.wait_until_ready())
            .await
            .context("Timeout waiting for pods")?
            .context("Failed to wait for pods")?;

        Ok(Self {
            store,
            subscriber,
            counter: Arc::new(AtomicUsize::new(0)),
            policy,
        })
    }

    pub async fn next(&self) -> Result<Arc<PartialObjectMeta<Pod>>> {
        if !self.store.is_empty() {
            let state = self.store.state();
            let counter = match self.policy {
                SelectorPolicy::Sticky => self.counter.load(Ordering::Relaxed),
                SelectorPolicy::RoundRobin => self.counter.fetch_add(1, Ordering::Relaxed),
            };

            let index = counter % state.len();

            debug!("Selecting pod {} of {}", index, state.len());

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
