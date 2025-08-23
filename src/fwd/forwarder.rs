use crate::cnf::Resource;
use anyhow::Result;
use tokio::net::TcpStream;

pub struct Forwarder {
    pub resource: Resource,
}

impl Forwarder {
    pub async fn forward(&self, connection: TcpStream) -> Result<()> {
        Ok(())
    }
}
