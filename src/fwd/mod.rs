use std::net::{Ipv4Addr, SocketAddr};

use crate::{
    cnf::schema::{Resource, ResourceSelector, SelectorPolicy},
    env::MAX_CONCURRENT_CONNECTIONS,
    fwd::pool::ClientPool,
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
    io,
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

mod pool;
mod watcher;

pub async fn init(resources: Vec<Resource>, context: Option<String>) -> Result<()> {
    if resources.is_empty() {
        return Err(anyhow::anyhow!("No resources found"));
    }

    let mut pool = ClientPool::new().await?;

    let token = CancellationToken::new();
    let mut set = JoinSet::new();

    for resource in resources {
        let client = match (&resource.context, &context) {
            (Some(context), _) | (_, Some(context)) => pool.get_or_insert(context).await?,
            _ => pool.default(),
        };

        set.spawn(bind(resource, client, token.child_token()));
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
    // TODO: How to handle IPv6?
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, resource.ports.local));
    let server = TcpListener::bind(addr).await?;

    info!(
        "Listening TCP on {} forwarded to {}",
        server.local_addr()?,
        resource.alias
    );

    let default_namespace = client.default_namespace().to_string();
    let namespace = resource.namespace.as_ref().unwrap_or(&default_namespace);

    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let selector = select(&resource.selector, client.clone(), namespace).await?;

    let watcher = watcher::PodWatcher::new(
        client,
        namespace,
        selector,
        resource.policy.unwrap_or(SelectorPolicy::RoundRobin),
        token.child_token(),
    )
    .await?;

    TcpListenerStream::new(server)
        .take_until(token.cancelled())
        .try_for_each_concurrent(MAX_CONCURRENT_CONNECTIONS, |connection| {
            let api = api.clone();
            let next_pod = watcher.next();
            let token = token.child_token();

            async move {
                let pod = tokio::select! {
                    biased;
                    _ = token.cancelled() => Err(anyhow::anyhow!("Selection cancelled")),
                    pod = next_pod => pod
                }
                .map_err(io::Error::other)?;

                let pod_name = pod.name_any();
                let pod_port = resource.ports.remote;

                // TODO: where should we wait for the pod to be running?
                // TODO: this could significantly slow down the connections
                // await_condition(api.clone(), &pod_name, is_pod_running())
                //     .await
                //     .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                info!(
                    "Forwarding connection from {} to {}",
                    connection.peer_addr()?,
                    pod_name
                );

                tokio::spawn(forward(api, pod_port, pod_name, connection, token));

                Ok::<_, io::Error>(())
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

    debug!("Opening upstream connection to {}", pod_name);

    let ports = [pod_port];
    let mut forwarding = api.portforward(&pod_name, &ports).await?;
    let mut upstream = forwarding
        .take_stream(pod_port)
        .context("Failed to take stream")?;

    let closer = forwarding
        .take_error(pod_port)
        .context("Failed to take error stream")?;

    debug!("Upstream connection opened");

    tokio::select! {
        biased;
        () = token.cancelled() => {}
        Some(e) = closer => {
            error!("Error forwarding: {}", e);

            forwarding.abort();
        }
        Err(e) = tokio::io::copy_bidirectional(&mut connection, &mut upstream) => {
            error!("Error forwarding: {}", e);

            forwarding.abort();
        }
    };

    drop(upstream);

    forwarding.join().await?;

    Ok(())
}

pub async fn select(
    selector: &ResourceSelector,
    client: Client,
    namespace: &str,
) -> Result<Selector> {
    match selector {
        ResourceSelector::Label(labels) => {
            let mut selector = Selector::default();

            selector.extend(
                labels
                    .iter()
                    .map(|(k, v)| Expression::In(k.to_owned(), [v.to_owned()].into())),
            );

            Ok(selector)
        }
        ResourceSelector::Deployment(name) => {
            let api: Api<Deployment> = Api::namespaced(client, namespace);
            let deployment = api.get(name).await?;
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
            let api: Api<Service> = Api::namespaced(client, namespace);
            let service = api.get(name).await?;
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
