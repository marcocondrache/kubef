use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
    sync::Arc,
};

use anyhow::{Ok, Result};
use hickory_proto::rr::{Name, RData, Record};
use hickory_server::{
    authority::{Catalog, ZoneType},
    server::ServerFuture as HickoryServer,
    store::in_memory::InMemoryAuthority,
};
use tokio::net::UdpSocket;

pub struct DnsResolver {
    // TODO: remove origin from struct
    origin: Name,
    zone: InMemoryAuthority,
}

impl DnsResolver {
    pub fn new() -> Result<Self> {
        let origin = Name::from_str("svc.")?;

        Ok(Self {
            zone: InMemoryAuthority::empty(origin.clone(), ZoneType::Primary, false),
            origin,
        })
    }

    pub async fn add_record(&mut self, fqdn: String, addr: IpAddr) -> Result<()> {
        let name = Name::from_str(&fqdn)?;
        let rdata = match addr {
            IpAddr::V4(addr) => RData::A(addr.into()),
            IpAddr::V6(addr) => RData::AAAA(addr.into()),
        };

        self.zone
            .upsert(Record::from_rdata(name, 30, rdata), 30)
            .await;

        Ok(())
    }

    pub async fn serve(self) -> Result<()> {
        let mut catalog = Catalog::new();

        catalog.upsert(self.origin.into(), vec![Arc::new(self.zone)]);

        let mut server = HickoryServer::new(catalog);
        let udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 53)).await?;

        server.register_socket(udp);
        server.block_until_done().await?;

        Ok(())
    }
}
