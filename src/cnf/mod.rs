use std::env;

use anyhow::Result;
use figment::{
    Figment,
    providers::{Format, YamlExtended},
};

pub mod schema;

pub fn extract() -> Result<schema::Config> {
    let xdg = xdg::BaseDirectories::with_prefix("kubef");

    let path = match env::var("KUBEF_CONFIG") {
        Ok(val) => std::path::PathBuf::from(val),
        Err(_) => xdg
            .place_config_file("config.json")
            .expect("Failed to create default config file"),
    };

    Figment::new()
        .merge(YamlExtended::file(&path))
        .extract()
        .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))
}
