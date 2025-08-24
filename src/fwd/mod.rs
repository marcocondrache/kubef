use std::{net::SocketAddr, time::Duration};

use crate::cnf::{self, Resource};
use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client, ResourceExt};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument};

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
    let watcher = watcher::PodWatcher::new(client, &resource).await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let pod = watcher
        .get_pod()
        .await
        .ok_or(anyhow::anyhow!("Pod not found"))?;

    TcpListenerStream::new(server)
        .take_until(token.cancelled())
        .try_for_each(|connection| {
            let resource = resource.clone();
            let api = api.clone();
            let pod = pod.clone();
            let token = token.clone();

            async move {
                info!("Forwarding to {}", pod.name_any());

                tokio::spawn(forward(
                    resource.clone(),
                    api,
                    pod,
                    connection,
                    token.child_token(),
                ));

                Ok(())
            }
        })
        .await?;

    Ok(())
}

#[instrument(skip(api, connection, token), fields(resource = %resource.name, pod = %pod.name_any()))]
pub async fn forward(
    resource: Resource,
    api: Api<Pod>,
    pod: Pod,
    mut connection: TcpStream,
    token: CancellationToken,
) -> Result<()> {
    let name = pod.name_any();
    let ports = [resource.ports.remote];
    let mut forwarding = api.portforward(&name, &ports).await?;
    let mut upstream = forwarding
        .take_stream(resource.ports.remote)
        .ok_or(anyhow::anyhow!("Failed to take stream"))?;

    tokio::select! {
        biased;
        _ = token.cancelled() => {}
        _ = tokio::io::copy_bidirectional(&mut connection, &mut upstream) => {}
    };

    drop(upstream);

    forwarding.join().await?;

    Ok(())
}
