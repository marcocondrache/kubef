use std::{borrow::Cow, collections::HashMap};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

type ConfigStr<'a> = Cow<'a, str>;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config<'a> {
    pub context: Option<ConfigStr<'a>>,
    pub groups: HashMap<ConfigStr<'a>, Vec<Resource<'a>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resource<'a> {
    pub namespace: Option<ConfigStr<'a>>,
    pub context: Option<ConfigStr<'a>>,
    pub policy: Option<SelectorPolicy>,
    pub selector: ResourceSelector<'a>,
    pub alias: ConfigStr<'a>,
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
    Label(Vec<(ConfigStr<'a>, ConfigStr<'a>)>),
    Deployment(ConfigStr<'a>),
    Service(ConfigStr<'a>),
}
