use std::{collections::HashMap, net::SocketAddr};

use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, ResourceExt};
use miette::{Context, IntoDiagnostic, Result};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

mod watcher;

use crate::{cnf, fwd::watcher::PodWatcher};

pub async fn init(target: String) -> Result<()> {
    let config = cnf::extract()?;

    let mut groups: HashMap<String, Vec<&cnf::Resource>> = HashMap::new();
    for resource in &config.resources {
        if let Some(g) = &resource.group {
            groups
                .entry(g.clone())
                .or_insert_with(Vec::new)
                .push(resource);
        }
    }

    let group = groups.get(&target).unwrap().first().unwrap();

    let client = kube::Client::try_default().await.into_diagnostic()?;
    let api: Api<Pod> = Api::namespaced(client.clone(), group.namespace.as_ref());

    let watcher = PodWatcher::new(client, group).await;

    let addr = SocketAddr::from(([127, 0, 0, 1], group.ports.local));
    let server = TcpListener::bind(addr).await.into_diagnostic()?;
    let mut stream = TcpListenerStream::new(server);

    while let Some(Ok(mut connection)) = stream.next().await {
        let pod = watcher.get_pod().await;

        if let Some(pod) = pod {
            let ports = [group.ports.remote];
            let name = pod.name_any();
            let mut forwarder = api.portforward(name.as_str(), &ports).await.unwrap();
            let mut upstream = forwarder
                .take_stream(ports[0])
                .context("failed to take stream")?;

            tokio::io::copy_bidirectional(&mut connection, &mut upstream)
                .await
                .into_diagnostic()?;

            drop(upstream);

            forwarder.join().await.into_diagnostic()?;
        }
    }

    Ok(())
}
