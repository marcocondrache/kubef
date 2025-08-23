use figment::{
    Figment,
    providers::{Format, Json},
};
use miette::{IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub resources: Vec<Resource>,
}

#[derive(Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    pub namespace: Namespace,
    pub kind: ResourceKind,
    pub label: String,
    pub group: Option<String>,
    pub ports: Ports,
    pub loopback: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Ports {
    pub remote: u16,
    pub local: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Namespace(pub Option<String>);

impl Default for Namespace {
    fn default() -> Self {
        Self(Some("default".to_string()))
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        self.0.as_ref().map_or("default", |s| s.as_str())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Service,
    Pod,
}

pub fn extract() -> Result<Config> {
    Figment::new()
        .merge(Json::file(
            "/Users/marcocondrache/Personal/kubef/config.json",
        ))
        .extract()
        .into_diagnostic()
}
