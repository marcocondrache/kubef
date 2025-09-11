use anyhow::{Context, Ok, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::TcpSocket;
#[cfg(target_os = "macos")]
use tracing::instrument;

use ipnet::{IpAddrRange, IpNet, Ipv4Net};

pub struct SocketPool {
    loopback: IpNet,
    pool: Option<IpAddrRange>,
}

impl Default for SocketPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SocketPool {
    fn drop(&mut self) {
        if cfg!(target_os = "macos") && self.pool.is_some() {
            tokio::spawn(Self::drop_loopback(self.loopback));
        }
    }
}

impl SocketPool {
    fn bind(socket: &TcpSocket, addr: SocketAddr) -> Result<()> {
        socket.set_reuseaddr(true)?;
        socket.set_keepalive(true)?;
        socket.set_nodelay(true)?;

        socket.bind(addr)?;

        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[instrument(skip(subnet))]
    async fn ensure_loopback(subnet: IpNet) -> Result<()> {
        use tokio::process::Command;
        use tracing::debug;

        debug!("Ensuring loopback: {}", subnet.to_string());

        let exit = Command::new("/sbin/ifconfig")
            .args(["lo0", "alias", &subnet.to_string()])
            .status()
            .await?;

        exit.success()
            .then(|| Ok(()))
            .ok_or_else(|| anyhow::anyhow!("Failed to ensure loopback"))?
    }

    #[cfg(target_os = "macos")]
    #[instrument(skip(subnet))]
    async fn drop_loopback(subnet: IpNet) -> Result<()> {
        use tokio::process::Command;

        let exit = Command::new("/sbin/ifconfig")
            .args(["lo0", "-alias", &subnet.to_string()])
            .status()
            .await?;

        exit.success()
            .then(|| Ok(()))
            .ok_or_else(|| anyhow::anyhow!("Failed to drop loopback"))?
    }

    #[cfg(not(target_os = "macos"))]
    async fn ensure_loopback(subnet: IpNet) -> Result<()> {
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    async fn drop_loopback(subnet: IpNet) -> Result<()> {
        Ok(())
    }
}

impl SocketPool {
    pub fn new() -> Self {
        let net = IpNet::V4(Ipv4Net::new(Ipv4Addr::LOCALHOST, 32).unwrap());

        Self {
            loopback: net,
            pool: None,
        }
    }

    pub async fn new_with_loopback(net: IpNet) -> Result<Self> {
        Self::ensure_loopback(net).await?;

        Ok(Self {
            loopback: net,
            pool: Some(net.hosts()),
        })
    }

    pub fn get_loopback(&mut self, port: Option<u16>) -> Result<(TcpSocket, IpAddr)> {
        let loopback = match self.pool {
            Some(mut pool) => pool
                .next()
                .context("No more loopback addresses available")?,
            None => Ipv4Addr::LOCALHOST.into(),
        };

        let address = SocketAddr::from((loopback, port.unwrap_or(0)));
        let socket = match loopback {
            IpAddr::V4(_) => TcpSocket::new_v4()?,
            IpAddr::V6(_) => TcpSocket::new_v6()?,
        };

        Self::bind(&socket, address)?;

        Ok((socket, loopback))
    }
}
