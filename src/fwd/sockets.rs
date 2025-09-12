use anyhow::{Context, Ok, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::TcpSocket;
#[cfg(target_os = "macos")]
use tracing::instrument;

use ipnet::{IpAddrRange, IpNet};

pub struct LoopbackToken {
    inner: IpAddr,
}

impl LoopbackToken {
    pub async fn new(address: IpAddr) -> Result<Self> {
        if address != IpAddr::V4(Ipv4Addr::LOCALHOST) && address != IpAddr::V6(Ipv6Addr::LOCALHOST)
        {
            SocketPool::ensure_loopback(address).await?;
        }

        Ok(Self { inner: address })
    }

    pub fn get_loopback(&self) -> IpAddr {
        self.inner
    }
}

impl Drop for LoopbackToken {
    fn drop(&mut self) {
        if cfg!(target_os = "macos") {
            tokio::spawn(SocketPool::drop_loopback(self.inner));
        }
    }
}

pub struct SocketPool {
    pool: Option<IpAddrRange>,
}

impl Default for SocketPool {
    fn default() -> Self {
        Self::new()
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
    async fn ensure_loopback(address: IpAddr) -> Result<()> {
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    async fn drop_loopback(address: IpAddr) -> Result<()> {
        Ok(())
    }
}

impl SocketPool {
    pub fn new() -> Self {
        Self { pool: None }
    }

    pub fn new_with_loopback(net: IpNet) -> Self {
        Self {
            pool: Some(net.hosts()),
        }
    }

    pub async fn get_loopback(&mut self, port: Option<u16>) -> Result<(TcpSocket, LoopbackToken)> {
        let loopback = match self.pool {
            Some(mut pool) => pool
                .next()
                .context("No more loopback addresses available")?,
            None => Ipv4Addr::LOCALHOST.into(),
        };

        let token = LoopbackToken::new(loopback).await?;

        let address = SocketAddr::from((loopback, port.unwrap_or(0)));
        let socket = match loopback {
            IpAddr::V4(_) => TcpSocket::new_v4()?,
            IpAddr::V6(_) => TcpSocket::new_v6()?,
        };

        Self::bind(&socket, address)?;

        Ok((socket, token))
    }
}
