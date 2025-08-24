use std::net::SocketAddr;

use crate::cnf::{self, Resource};
use anyhow::Result;
use futures::{StreamExt, TryStreamExt, future};
use k8s_openapi::api::core::v1::Pod;
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
    let config = cnf::extract()?;

    let resources = config
        .resources
        .into_iter()
        .filter(|r| r.group.as_deref() == Some(&target))
        .collect::<Vec<_>>();

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
        _ = set.join_all() => {}
    };

    token.cancel();

    Ok(())
}

#[instrument(skip(client, token), fields(resource = %resource.name))]
pub async fn bind(resource: Resource, client: Client, token: CancellationToken) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], resource.ports.local));
    let server = TcpListener::bind(addr).await?;

    let api: Api<Pod> = Api::namespaced(client.clone(), resource.namespace.as_ref());
    let mut selector = Selector::default();

    selector.extend(Expression::In(
        "app.kubernetes.io/name".into(),
        [resource.name].into(),
    ));

    let watcher = watcher::PodWatcher::new(
        client,
        resource.namespace.as_ref().to_string(),
        selector,
        token.child_token(),
    )
    .await;

    TcpListenerStream::new(server)
        .take_until(token.cancelled())
        .try_for_each(|connection| {
            let state = watcher.store.state();
            let pod = state.first().unwrap();

            let api = api.clone();
            let pod_name = pod.name_any();
            let pod_port = resource.ports.remote;
            let forward_token = token.child_token();

            async move {
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

    let ports = [pod_port];
    let mut forwarding = api.portforward(&pod_name, &ports).await?;
    let mut upstream = forwarding
        .take_stream(pod_port)
        .ok_or(anyhow::anyhow!("Failed to take stream"))?;

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
