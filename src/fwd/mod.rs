use std::net::{Ipv4Addr, SocketAddr};

use crate::{
    cnf::schema::{Resource, ResourceSelector, SelectorPolicy},
    fwd::pool::ClientPool,
};
use anyhow::{Context, Result};
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
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

mod pool;
mod watcher;

pub async fn init(resources: Vec<Resource<'static>>, context: Option<&str>) -> Result<()> {
    if resources.is_empty() {
        return Err(anyhow::anyhow!("No resources found"));
    }

    let mut pool = ClientPool::new().await?;

    let token = CancellationToken::new();
    let mut set = JoinSet::new();

    for resource in resources {
        let client = match (resource.context, context) {
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
pub async fn bind(resource: Resource<'_>, client: Client, token: CancellationToken) -> Result<()> {
    // TODO: How to handle IPv6?
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, resource.ports.local));
    let server = TcpListener::bind(addr).await?;

    info!(
        "Listening TCP on {} forwarded to {}",
        server.local_addr()?,
        resource.alias
    );

    let default_namespace = client.default_namespace();
    let namespace = resource.namespace.unwrap_or(default_namespace);

    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let selector = select(client.clone(), resource.selector, namespace).await?;

    let watcher = watcher::PodWatcher::new(
        client.clone(),
        namespace,
        selector,
        resource.policy.unwrap_or(SelectorPolicy::RoundRobin),
        token.child_token(),
    )
    .await?;

    let mut set = JoinSet::new();

    loop {
        tokio::select! {
            biased;
            () = token.cancelled() => break,
            Ok((connection, addr)) = server.accept() => {
                let api = api.clone();
                let token = token.child_token();

                // TODO: this doesn't get cancelled
                let pod = watcher.next().await?;
                let pod_name = pod.name_any();
                let pod_port = resource.ports.remote;

                info!(
                    "Forwarding connection from {} to {}",
                    addr,
                    pod_name
                );

                set.spawn(async move {
                    if let Err(e) = forward(api, pod_port, pod_name, connection, token).await {
                        error!("Error forwarding: {}", e);
                    }
                });
            }
            else => break,
        }
    }

    set.join_all().await;

    Ok(())
}

#[instrument(skip(api, connection, token))]
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
        () = token.cancelled() => {},
        Some(e) = closer => {
            forwarding.abort();

            anyhow::bail!(e);
        }
        Err(e) = tokio::io::copy_bidirectional(&mut connection, &mut upstream) => {
            forwarding.abort();

            anyhow::bail!(e);
        }
    };

    // Gracefully close the upstream connection
    drop(upstream);

    forwarding
        .join()
        .await
        .context("Failed to conclude forward")
}

pub async fn select(
    client: Client,
    selector: ResourceSelector<'_>,
    namespace: &str,
) -> Result<Selector> {
    match selector {
        ResourceSelector::Label(labels) => {
            let result = labels
                .into_iter()
                .map(|(k, v)| Expression::In(k.to_string(), [v.to_string()].into()))
                .collect::<Selector>();

            Ok(result)
        }
        ResourceSelector::Deployment(name) => {
            let api: Api<Deployment> = Api::namespaced(client, namespace);
            let deployment = api.get(name).await?;
            let selector = deployment.spec.context("Deployment has no spec")?.selector;
            // TODO: Handle match expressions
            let result = selector
                .match_labels
                .context("Deployment has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Selector>();

            Ok(result)
        }
        ResourceSelector::Service(name) => {
            let api: Api<Service> = Api::namespaced(client, namespace);
            let service = api.get(name).await?;
            let selector = service.spec.context("Service has no spec")?.selector;

            let result = selector
                .context("Service has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Selector>();

            Ok(result)
        }
    }
}
