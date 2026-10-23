use std::{
    collections::BTreeMap,
    fmt,
    net::SocketAddr,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::{Container, Pod, PodSpec};
use kube::{
    Api,
    api::{DeleteParams, ObjectMeta, PostParams, WatchEvent, WatchParams},
};
use nanoid::nanoid;
use tracing::{debug, instrument};

static ALPHABET: [char; 16] = [
    '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f',
];

/// A remote endpoint, either an IP socket address or a `host:port` pair.
/// Hostnames are resolved inside the cluster, so cluster DNS names work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyTarget {
    Socket(SocketAddr),
    Host(String, u16),
}

impl ProxyTarget {
    fn is_valid_hostname(host: &str) -> bool {
        let host = host.strip_suffix('.').unwrap_or(host);

        !host.is_empty()
            && host.len() <= 253
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            })
    }
}

impl FromStr for ProxyTarget {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        if let Ok(socket) = value.parse::<SocketAddr>() {
            return Ok(Self::Socket(socket));
        }

        let (host, port) = value
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("expected <host>:<port>, got '{value}'"))?;

        let port = port
            .parse::<u16>()
            .map_err(|_| anyhow::anyhow!("invalid port '{port}'"))?;

        if !Self::is_valid_hostname(host) {
            return Err(anyhow::anyhow!("invalid host '{host}'"));
        }

        Ok(Self::Host(host.to_string(), port))
    }
}

impl fmt::Display for ProxyTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Socket(socket) => write!(f, "{socket}"),
            Self::Host(host, port) => write!(f, "{host}:{port}"),
        }
    }
}

pub enum ProxyDestination {
    Tcp(ProxyTarget),
    Udp(ProxyTarget),
}

impl ProxyDestination {
    pub fn to_socat_target(&self) -> String {
        match self {
            ProxyDestination::Tcp(target) => format!("TCP:{target}"),
            ProxyDestination::Udp(target) => format!("UDP:{target}"),
        }
    }
}

pub struct Proxy {
    id: String,
    api: Api<Pod>,
    permit: AtomicBool,
}

impl Proxy {
    const NAME_PREFIX: &str = "kubef-";

    const IMAGE: &str = "alpine/socat:latest";
    const IMAGE_BIN: &str = "socat";

    pub const PORT: u16 = 8080;

    pub fn new(api: Api<Pod>) -> Self {
        Self {
            id: nanoid!(6, &ALPHABET),
            api,
            permit: AtomicBool::new(false),
        }
    }

    #[inline]
    pub fn is_spawned(&self) -> bool {
        self.permit.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn get_name(&self) -> String {
        format!("{}{}", Self::NAME_PREFIX, self.id)
    }

    pub async fn abort(&self) -> Result<()> {
        if !self.permit.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("Proxy not spawned"));
        }

        self.api
            .delete(&self.get_name(), &DeleteParams::default())
            .await?;

        self.permit.store(false, Ordering::Relaxed);

        Ok(())
    }

    pub async fn wait_until_exit(&self) -> Result<()> {
        if !self.is_spawned() {
            return Err(anyhow::anyhow!("Proxy not spawned"));
        }

        let params = WatchParams::default().labels(&format!("kubef.io/id={}", self.id));
        let stream = self.api.watch_metadata(&params, "0").await?;

        tokio::pin!(stream);

        while let Ok(Some(event)) = stream.try_next().await {
            match event {
                WatchEvent::Error(_) | WatchEvent::Deleted(_) => return Ok(()),
                _ => {
                    debug!("Proxy {} received event: {:?}", self.id, event);
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self, destination))]
    pub async fn spawn(&self, destination: &ProxyDestination) -> Result<()> {
        if self.is_spawned() {
            return Err(anyhow::anyhow!("Proxy already spawned"));
        }

        let source = format!("TCP-LISTEN:{},reuseaddr,fork", Self::PORT);
        let destination = destination.to_socat_target();

        let pod = Pod {
            metadata: ObjectMeta {
                name: Some(self.get_name()),
                labels: Some(BTreeMap::from([
                    ("kubef.io/id".to_string(), self.id.clone()),
                    ("kubef.io/proxy".to_string(), "true".to_string()),
                ])),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "socat".to_string(),
                    image: Some(Self::IMAGE.to_string()),
                    command: Some(vec![Self::IMAGE_BIN.to_string(), source, destination]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: None,
        };

        self.api.create(&PostParams::default(), &pod).await?;
        self.permit.store(true, Ordering::Relaxed);

        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if !self.is_spawned() {
            return;
        }

        let api = self.api.clone();
        let name = self.get_name();

        tokio::spawn(async move { api.delete(&name, &DeleteParams::default()).await });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4() {
        let target: ProxyTarget = "10.0.0.1:5432".parse().unwrap();
        assert_eq!(
            target,
            ProxyTarget::Socket("10.0.0.1:5432".parse().unwrap())
        );
    }

    #[test]
    fn parses_ipv6() {
        let target: ProxyTarget = "[::1]:80".parse().unwrap();
        assert_eq!(target.to_string(), "[::1]:80");
    }

    #[test]
    fn parses_domain() {
        let target: ProxyTarget = "db.default.svc.cluster.local:5432".parse().unwrap();
        assert_eq!(
            target,
            ProxyTarget::Host("db.default.svc.cluster.local".into(), 5432)
        );
    }

    #[test]
    fn parses_single_label_host() {
        let target: ProxyTarget = "redis:6379".parse().unwrap();
        assert_eq!(target, ProxyTarget::Host("redis".into(), 6379));
    }

    #[test]
    fn rejects_missing_port() {
        assert!("example.com".parse::<ProxyTarget>().is_err());
        assert!("example.com:".parse::<ProxyTarget>().is_err());
        assert!("example.com:99999".parse::<ProxyTarget>().is_err());
    }

    #[test]
    fn rejects_invalid_host() {
        assert!(":80".parse::<ProxyTarget>().is_err());
        assert!("-bad.com:80".parse::<ProxyTarget>().is_err());
        assert!("a..b:80".parse::<ProxyTarget>().is_err());
        assert!("host,fork:80".parse::<ProxyTarget>().is_err());
        assert!("::1:80".parse::<ProxyTarget>().is_err());
    }

    #[test]
    fn socat_target_uses_host() {
        let destination = ProxyDestination::Tcp("api.example.com:443".parse().unwrap());
        assert_eq!(destination.to_socat_target(), "TCP:api.example.com:443");
    }
}
