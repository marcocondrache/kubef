use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Config {
    pub groups: HashMap<String, Vec<Resource>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Resource {
    pub namespace: Namespace,
    pub selector: ResourceSelector,
    pub alias: String,
    pub ports: Ports,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Ports {
    pub remote: u16,
    pub local: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
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

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "match")]
pub enum ResourceSelector {
    Label(Vec<(String, String)>),
    Deployment(String),
    Service(String),
}
