use std::env;

use anyhow::Result;
use figment::{
    Figment,
    providers::{Format, Json},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub resources: Vec<Resource>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Resource {
    pub namespace: Namespace,
    pub selector: ResourceSelector,
    pub alias: String,
    pub group: Option<String>,
    pub ports: Ports,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "match")]
pub enum ResourceSelector {
    Label(Vec<(String, String)>),
    Deployment(String),
    Service(String),
}

pub fn extract() -> Result<Config> {
    let xdg = xdg::BaseDirectories::with_prefix("kubef");

    let path = match env::var("KUBEF_CONFIG_PATH") {
        Ok(val) => std::path::PathBuf::from(val),
        Err(_) => xdg
            .place_config_file("config.json")
            .expect("Failed to create default config file"),
    };

    Figment::new()
        .merge(Json::file(&path))
        .extract()
        .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))
}
