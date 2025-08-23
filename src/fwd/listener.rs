use std::net::SocketAddr;

use crate::cnf::Resource;
use anyhow::Result;
use futures::{StreamExt, TryStreamExt};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::sync::CancellationToken;

pub struct Listener {
    pub resource: Resource,
}

impl Listener {
    pub async fn bind(self, token: CancellationToken) -> Result<()> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.resource.ports.local));
        let server = TcpListener::bind(addr).await?;

        TcpListenerStream::new(server)
            .take_until(token.cancelled())
            .try_for_each(|connection| async move {
                let _ = tokio::spawn(async move {});

                Ok(())
            })
            .await?;

        Ok(())
    }
}
