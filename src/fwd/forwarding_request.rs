use std::sync::Arc;

use anyhow::Result;
use kube::Client;
use tokio::task::{AbortHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{cnf::Resource, fwd::listener::Listener};

pub struct ForwardingRequest {
    client: Arc<Client>,
    resource: Resource,
    token: CancellationToken,
    handle: Option<AbortHandle>,
}

impl ForwardingRequest {
    pub fn new(client: Arc<Client>, resource: Resource) -> Self {
        Self {
            client,
            resource,
            token: CancellationToken::new(),
            handle: None,
        }
    }

    pub async fn init(&mut self, set: &mut JoinSet<Result<()>>) {
        let listener = Listener {
            resource: self.resource.clone(),
        };

        let child = self.token.child_token();
        let handle = set.spawn(listener.bind(child));

        self.handle = Some(handle);
    }
}

impl Drop for ForwardingRequest {
    fn drop(&mut self) {
        self.token.cancel();
        self.handle.take().map(|handle| handle.abort());
    }
}
