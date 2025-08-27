use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config<'a> {
    pub context: Option<&'a str>,
    pub groups: HashMap<&'a str, Vec<Resource<'a>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resource<'a> {
    pub namespace: Option<&'a str>,
    pub context: Option<&'a str>,
    pub policy: Option<SelectorPolicy>,
    pub selector: ResourceSelector<'a>,
    pub alias: &'a str,
    pub ports: Ports,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    pub remote: u16,
    pub local: u16,
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
pub enum ResourceSelector<'a> {
    Label(Vec<(&'a str, &'a str)>),
    Deployment(&'a str),
    Service(&'a str),
}
