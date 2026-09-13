use std::collections::HashMap;

use ipnet::IpNet;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextAlias {
    pub kubeconfig: String,
    pub namespace: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub context: Option<String>,
    pub groups: HashMap<String, Vec<Resource>>,
    #[schemars(with = "Option<String>")]
    pub loopback: Option<IpNet>,
    #[serde(default)]
    pub contexts: HashMap<String, ContextAlias>,
    #[serde(default)]
    pub ports: Option<GlobalPorts>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlobalPorts {
    pub mapping: PortMapping,
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PortMapping {
    #[default]
    Container,
    Service,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(untagged)]
pub enum PortSpec {
    Named(String),
    Number(u16),
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub alias: String,
    #[serde(default)]
    pub namespace: Option<String>,
    pub context: Option<String>,
    pub policy: Option<SelectorPolicy>,
    pub selector: ResourceSelector,
    pub ports: Ports,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    pub remote: PortSpec,
    pub local: Option<u16>,
    pub mapping: Option<PortMapping>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum SelectorPolicy {
    Sticky,
    #[default]
    RoundRobin,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "match")]
#[serde(deny_unknown_fields)]
pub enum ResourceSelector {
    Label(Vec<(String, String)>),
    Deployment(String),
    Service(String),
}
