use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
    sync::Arc,
};

use anyhow::{Ok, Result};
use hickory_server::{
    authority::{Catalog, ZoneType},
    proto::rr::{Name, RData, Record},
    server::ServerFuture as HickoryServer,
    store::in_memory::InMemoryAuthority,
};
use tokio::{
    fs::{self},
    net::UdpSocket,
};
use tracing::debug;

pub struct DnsResolver {
    // TODO: remove origin from struct
    origin: Name,
    authority: InMemoryAuthority,
}

impl DnsResolver {
    pub const ORIGIN: &str = "svc.";

    pub fn new() -> Result<Self> {
        // TODO: should be configurable?
        let origin = Name::from_str(Self::ORIGIN)?;
        let authority = InMemoryAuthority::empty(origin.clone(), ZoneType::Primary, false);

        Ok(Self { origin, authority })
    }

    pub async fn os_resolver(&self) -> Result<()> {
        fs::create_dir_all("/etc/resolver/svc").await?;
        fs::write("/etc/resolver/svc/resolver.conf", b"nameserver 127.0.0.1").await?;

        Ok(())
    }

    pub async fn add_record(&mut self, fqdn: String, addr: IpAddr) -> Result<()> {
        let name = Name::from_str(&fqdn)?;
        let rdata = match addr {
            IpAddr::V4(addr) => RData::A(addr.into()),
            IpAddr::V6(addr) => RData::AAAA(addr.into()),
        };

        debug!("Adding record: {} -> {}", fqdn, addr);

        self.authority
            .upsert(Record::from_rdata(name, 30, rdata), 0)
            .await;

        Ok(())
    }

    pub async fn serve(self) -> Result<()> {
        self.os_resolver().await?;

        let mut catalog = Catalog::new();

        catalog.upsert(self.origin.into(), vec![Arc::new(self.authority)]);

        debug!("Serving DNS");

        let mut server = HickoryServer::new(catalog);
        let udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 53)).await?;

        server.register_socket(udp);
        server.block_until_done().await?;

        Ok(())
    }
}
