use std::collections::HashMap;

use ipnet::IpNet;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub context: Option<String>,
    pub groups: HashMap<String, Vec<Resource>>,
    #[schemars(with = "String")]
    pub loopback: Option<IpNet>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub namespace: Option<String>,
    pub context: Option<String>,
    pub policy: Option<SelectorPolicy>,
    pub selector: ResourceSelector,
    pub alias: String,
    pub ports: Ports,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    pub remote: u16,
    pub local: Option<u16>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum SelectorPolicy {
    Sticky,
    RoundRobin,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "match")]
#[serde(deny_unknown_fields)]
pub enum ResourceSelector {
    Label(Vec<(String, String)>),
    Deployment(String),
    Hostname(String),
    Service(String),
}
