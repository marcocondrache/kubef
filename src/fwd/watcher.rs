use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api,
    api::PartialObjectMeta,
    core::Selector,
    runtime::{
        WatchStreamExt, predicates,
        reflector::{self, ReflectHandle, Store},
        watcher::{self},
    },
};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::cnf::schema::SelectorPolicy;

pub struct PodWatcher {
    store: Store<PartialObjectMeta<Pod>>,
    subscriber: ReflectHandle<PartialObjectMeta<Pod>>,
    counter: Arc<AtomicUsize>,
    policy: SelectorPolicy,
    handle: JoinHandle<()>,
}

impl PodWatcher {
    pub async fn new(api: Api<Pod>, selector: Selector, policy: SelectorPolicy) -> Result<Self> {
        let config = watcher::Config::default().labels_from(&selector);

        let (store, writer) = reflector::store_shared(256);

        let subscriber = writer.subscribe().context("Failed to create subscriber")?;

        let handle = tokio::spawn(
            watcher::metadata_watcher(api, config)
                .reflect(writer)
                .default_backoff()
                .applied_objects()
                .predicate_filter(predicates::labels)
                .for_each(|_| ()),
        );

        tokio::time::timeout(Duration::from_secs(10), store.wait_until_ready())
            .await
            .context("Timeout waiting for pods")?
            .context("Failed to wait for pods")?;

        Ok(Self {
            store,
            subscriber,
            counter: Arc::new(AtomicUsize::new(0)),
            policy,
            handle,
        })
    }

    pub fn shutdown(&self) {
        self.handle.abort();
    }

    pub fn get(&self) -> Option<Arc<PartialObjectMeta<Pod>>> {
        if self.store.is_empty() {
            return None;
        }

        let state = self.store.state();
        let counter = match self.policy {
            SelectorPolicy::Sticky => self.counter.load(Ordering::Relaxed),
            SelectorPolicy::RoundRobin => self.counter.fetch_add(1, Ordering::Relaxed),
        };

        let index = counter % state.len();

        debug!("Selecting pod {} of {}", index, state.len());

        state.get(index).cloned()
    }

    pub async fn next(&self) -> Result<Arc<PartialObjectMeta<Pod>>> {
        self.subscriber
            .clone()
            .next()
            .await
            .context("Cannot get next pod")
    }
}
