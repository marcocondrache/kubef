use std::sync::Arc;

use anyhow::Result;
use kube::Client;
use tokio::task::JoinSet;

mod forwarder;
mod forwarding_request;
mod listener;
mod watcher;

use crate::{cnf, fwd::forwarding_request::ForwardingRequest};

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

    let client = Arc::new(Client::try_default().await?);

    let mut set = JoinSet::new();
    let requests = resources
        .into_iter()
        .map(|r| ForwardingRequest::new(client.clone(), r))
        .collect::<Vec<_>>();

    for mut request in requests {
        request.init(&mut set).await;
    }

    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {},
        _ = set.join_all() => {
            return Err(anyhow::anyhow!("Failed to join all requests"));
        }
    }

    Ok(())
}
