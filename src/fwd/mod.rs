use std::{collections::HashMap, net::SocketAddr};

use crate::{
    cnf::{self, Resource, ResourceSelector},
    env::MAX_CONCURRENT_CONNECTIONS,
};
use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Pod, Service},
};
use kube::{
    Api, Client, ResourceExt,
    core::{Expression, Selector},
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

mod watcher;

pub async fn init(target: String) -> Result<()> {
    let mut config = cnf::extract()?;

    let resources = find_resources(&mut config, &target)?;

    if resources.is_empty() {
        return Err(anyhow::anyhow!("No resources found"));
    }

    let client = Client::try_default().await?;

    let token = CancellationToken::new();
    let mut set = JoinSet::new();

    for resource in resources {
        set.spawn(bind(resource, client.clone(), token.child_token()));
    }

    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {},
        results = set.join_all() => {
            for result in results {
                if let Err(e) = result {
                    error!("Error: {}", e);
                }
            }
        }
    };

    token.cancel();

    Ok(())
}

#[instrument(skip(client, token), fields(resource = %resource.alias))]
pub async fn bind(resource: Resource, client: Client, token: CancellationToken) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], resource.ports.local));
    let server = TcpListener::bind(addr).await?;

    info!("Listening TCP on {} forwarded to {}", addr, resource.alias);

    let api: Api<Pod> = Api::namespaced(client.clone(), resource.namespace.as_ref());
    let selector = select(
        resource.selector,
        client.clone(),
        resource.namespace.as_ref().to_string(),
    )
    .await?;

    let watcher = watcher::PodWatcher::new(
        client,
        resource.namespace.as_ref().to_string(),
        selector,
        token.child_token(),
    )
    .await?;

    TcpListenerStream::new(server)
        .take_until(token.cancelled())
        .try_for_each_concurrent(MAX_CONCURRENT_CONNECTIONS, |connection| {
            let api = api.clone();
            let next_pod = watcher.next();
            let forward_token = token.child_token();

            async move {
                let pod = next_pod.await.unwrap();
                let pod_name = pod.name_any();
                let pod_port = resource.ports.remote;

                info!(
                    "Forwarding connection from {} to {}",
                    connection.peer_addr()?,
                    pod_name
                );

                tokio::spawn(forward(api, pod_port, pod_name, connection, forward_token));

                Ok(())
            }
        })
        .await?;

    Ok(())
}

#[instrument(skip(api, connection, token), fields(pod = %pod_name))]
pub async fn forward(
    api: Api<Pod>,
    pod_port: u16,
    pod_name: String,
    mut connection: TcpStream,
    token: CancellationToken,
) -> Result<()> {
    // Optimization
    connection.set_nodelay(true)?;
    connection.set_linger(None)?;
    connection.set_ttl(128)?;

    let ports = [pod_port];
    let mut forwarding = api.portforward(&pod_name, &ports).await?;
    let mut upstream = forwarding
        .take_stream(pod_port)
        .context("Failed to take stream")?;

    tokio::select! {
        biased;
        _ = token.cancelled() => {}
        Err(e) = tokio::io::copy_bidirectional(&mut connection, &mut upstream) => {
            error!("Error forwarding: {}", e);
        }
    };

    drop(upstream);

    forwarding.join().await?;

    Ok(())
}

pub async fn select(
    selector: ResourceSelector,
    client: Client,
    namespace: String,
) -> Result<Selector> {
    match selector {
        ResourceSelector::Label(labels) => {
            let mut selector = Selector::default();

            selector.extend(
                labels
                    .into_iter()
                    .map(|(k, v)| Expression::In(k, [v].into())),
            );

            Ok(selector)
        }
        ResourceSelector::Deployment(name) => {
            let api: Api<Deployment> = Api::namespaced(client, &namespace);
            let deployment = api.get(&name).await?;
            let selector = deployment.spec.context("Deployment has no spec")?.selector;
            // TODO: Handle match expressions
            let expressions = selector
                .match_labels
                .context("Deployment has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Vec<_>>();

            Ok(Selector::from_iter(expressions))
        }
        ResourceSelector::Service(name) => {
            let api: Api<Service> = Api::namespaced(client, &namespace);
            let service = api.get(&name).await?;
            let selector = service.spec.context("Service has no spec")?.selector;
            let expressions = selector
                .context("Service has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Vec<_>>();

            Ok(Selector::from_iter(expressions))
        }
    }
}

fn find_resources(config: &mut cnf::Config, target: &str) -> Result<Vec<Resource>> {
    let alias_index: HashMap<String, Vec<Resource>> = config
        .groups
        .values()
        .flat_map(|resources| resources.iter())
        .fold(HashMap::new(), |mut acc, resource| {
            acc.entry(resource.alias.clone())
                .or_default()
                .push(resource.clone());
            acc
        });

    if let Some(resources) = alias_index.get(target) {
        return Ok(resources.clone());
    }

    match config.groups.remove(target) {
        Some(resources) => Ok(resources),
        None => Err(anyhow::anyhow!(
            "No resources found for target '{}' in aliases or groups",
            target
        )),
    }
}
