use clap::Parser;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, YamlExtended},
};
use serde::{Deserialize, Serialize};

#[derive(Parser, Serialize, Deserialize)]
pub struct Config {}

pub fn extract() -> Config {
    Figment::new()
        .merge(Serialized::defaults(Config::parse()))
        .merge(YamlExtended::file("config.yaml"))
        .merge(Env::prefixed("KUBEF_"))
        .extract()
        .unwrap()
}
