use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use futures::StreamExt;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Pod, Service},
};
use kube::{
    Api, Client,
    api::PartialObjectMeta,
    client::scope::Namespace,
    core::Selector,
    runtime::{
        WatchStreamExt, predicates,
        reflector::{self, ReflectHandle, Store},
        watcher::{self},
    },
};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::cnf::schema::{Resource, ResourceSelector, SelectorPolicy};

type Object = PartialObjectMeta<Pod>;

pub struct Watcher {
    store: Store<Object>,
    subscriber: ReflectHandle<Object>,
    counter: AtomicUsize,
    policy: SelectorPolicy,
    handle: JoinHandle<()>,
}

impl Watcher {
    pub async fn new(api: Api<Pod>, selector: &Selector, policy: SelectorPolicy) -> Result<Self> {
        let (store, writer) = reflector::store_shared(256);

        let config = watcher::Config::default().labels_from(selector);
        let subscriber = writer.subscribe().context("Failed to create subscriber")?;

        let handle = tokio::spawn(
            watcher::metadata_watcher(api, config)
                .reflect(writer)
                .default_backoff()
                .applied_objects()
                .predicate_filter(predicates::labels)
                .for_each(|_| async {}),
        );

        tokio::time::timeout(Duration::from_secs(10), store.wait_until_ready())
            .await
            .context("Timeout waiting for pods")?
            .context("Failed to wait for pods")?;

        Ok(Self {
            store,
            subscriber,
            counter: AtomicUsize::new(0),
            policy,
            handle,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    pub fn get(&self) -> Option<Arc<Object>> {
        if self.store.is_empty() {
            return None;
        }

        let state = self.store.state();
        let counter = match self.policy {
            SelectorPolicy::Sticky => self.counter.load(Ordering::Relaxed),
            SelectorPolicy::RoundRobin => self.counter.fetch_add(1, Ordering::Relaxed),
        };

        let index = if state.len().is_power_of_two() {
            counter & (state.len() - 1)
        } else {
            counter % state.len()
        };

        debug!("Selecting pod {} of {}", index, state.len());

        state.get(index).cloned()
    }

    pub async fn next(&mut self) -> Result<Arc<Object>> {
        self.subscriber.next().await.context("Cannot get next pod")
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn select(client: &Client, resource: &Resource) -> Result<Selector> {
    match &resource.selector {
        ResourceSelector::Label(labels) => Ok(Selector::from_iter(labels.clone())),
        ResourceSelector::Deployment(name) => {
            let deployment = client
                .get::<Deployment>(name, &Namespace::from(resource.namespace.clone()))
                .await?;

            let selector = deployment
                .spec
                .context("Deployment has no spec")?
                .selector
                .try_into()?;

            Ok(selector)
        }
        ResourceSelector::Service(name) => {
            let service = client
                .get::<Service>(name, &Namespace::from(resource.namespace.clone()))
                .await?;

            let selector = service
                .spec
                .context("Service has no spec")?
                .selector
                .context("Service has no selector")?;

            // TODO: it's a hack, kube-rs does something horrible behind the scenes
            Ok(Selector::from_iter(selector))
        }
    }
}
