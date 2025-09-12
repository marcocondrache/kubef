use anyhow::{Context, Ok, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::TcpSocket;
#[cfg(target_os = "macos")]
use tracing::instrument;

use ipnet::{IpAddrRange, IpNet};

pub struct LoopbackToken {
    inner: IpAddr,
}

impl LoopbackToken {
    pub async fn new(address: IpAddr) -> Result<Self> {
        if !address.is_loopback() {
            anyhow::bail!("Address is not a loopback address");
        }

        SocketPool::ensure_loopback(address).await?;

        Ok(Self { inner: address })
    }
}

impl Drop for LoopbackToken {
    fn drop(&mut self) {
        if cfg!(target_os = "macos") {
            tokio::spawn(SocketPool::drop_loopback(self.inner));
        }
    }
}

#[derive(Default)]
pub struct SocketPool {
    pool: Option<IpAddrRange>,
}

impl SocketPool {
    pub fn with_loopback(net: IpNet) -> Self {
        Self {
            pool: Some(net.hosts()),
        }
    }

    pub async fn get_loopback(
        &self,
        port: Option<u16>,
    ) -> Result<(TcpSocket, Option<LoopbackToken>)> {
        let (loopback, token) = match self.pool {
            Some(mut pool) => {
                let loopback = pool
                    .next()
                    .context("No more loopback addresses available")?;

                let token = LoopbackToken::new(loopback).await?;

                (loopback, Some(token))
            }
            None => (Ipv4Addr::LOCALHOST.into(), None),
        };

        let address = SocketAddr::from((loopback, port.unwrap_or(0)));
        let socket = match loopback {
            IpAddr::V4(_) => TcpSocket::new_v4()?,
            IpAddr::V6(_) => TcpSocket::new_v6()?,
        };

        Self::bind(&socket, address)?;

        Ok((socket, token))
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
    #[instrument(skip(address))]
    async fn ensure_loopback(address: IpAddr) -> Result<()> {
        use tokio::process::Command;
        use tracing::debug;

        debug!("Ensuring loopback: {}", address.to_string());

        let exit = Command::new("/sbin/ifconfig")
            .args(["lo0", "alias", &address.to_string()])
            .status()
            .await?;

        exit.success()
            .then(|| Ok(()))
            .ok_or_else(|| anyhow::anyhow!("Failed to ensure loopback"))?
    }

    #[cfg(target_os = "macos")]
    #[instrument(skip(address))]
    async fn drop_loopback(address: IpAddr) -> Result<()> {
        use tokio::process::Command;

        let exit = Command::new("/sbin/ifconfig")
            .args(["lo0", "-alias", &address.to_string()])
            .status()
            .await?;

        exit.success()
            .then(|| Ok(()))
            .ok_or_else(|| anyhow::anyhow!("Failed to drop loopback"))?
    }

    #[cfg(not(target_os = "macos"))]
    #[instrument(skip(address))]
    async fn ensure_loopback(address: IpAddr) -> Result<()> {
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    #[instrument(skip(address))]
    async fn drop_loopback(address: IpAddr) -> Result<()> {
        Ok(())
    }
}
