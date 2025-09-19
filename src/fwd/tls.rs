use std::net::{IpAddr, Ipv4Addr};

use anyhow::Result;
use either::Either;
use rcgen::{CertificateParams, CertifiedKey, KeyPair};
use tokio::{sync::OnceCell, task};

use crate::fwd::Target;

pub static CERTIFICATE: OnceCell<CertifiedKey<KeyPair>> = OnceCell::const_new();

fn get_certificate_inner(target: Target<'static>) -> Result<CertifiedKey<KeyPair>> {
    let mut names = match target {
        Either::Left(resource) => vec![resource.alias.clone()],
        Either::Right(resources) => resources
            .iter()
            .map(|resource| resource.alias.clone())
            .collect::<Vec<_>>(),
    };

    names.push("localhost".to_string());

    let key = KeyPair::generate()?;
    let mut cert = CertificateParams::new(vec![
        "localhost".to_string(), // CN
    ])?;

    cert.subject_alt_names
        .push(rcgen::SanType::DnsName("localhost".try_into()?));

    cert.subject_alt_names
        .push(rcgen::SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));

    Ok(CertifiedKey {
        cert: cert.self_signed(&key)?,
        signing_key: key,
    })
}

pub async fn get_certificate(target: Target<'static>) -> Result<&'static CertifiedKey<KeyPair>> {
    CERTIFICATE
        .get_or_try_init(|| async {
            task::spawn_blocking(move || get_certificate_inner(target)).await?
        })
        .await
}
