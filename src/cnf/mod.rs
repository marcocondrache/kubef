use std::env;

use anyhow::Result;
use tokio::{sync::OnceCell, task};

pub mod schema;

static CNF: OnceCell<schema::Config> = OnceCell::const_new();

pub async fn extract() -> Result<&'static schema::Config> {
    let xdg = xdg::BaseDirectories::with_prefix("kubef");

    let path = match env::var("KUBEF_CONFIG") {
        Ok(val) => std::path::PathBuf::from(val),
        Err(_) => xdg
            .place_config_file("config.yaml")
            .expect("Failed to create default config file"),
    };

    let config = CNF
        .get_or_try_init(|| async {
            let parser = task::spawn_blocking(|| {
                if !path.exists() {
                    anyhow::bail!("Config file not found at {}", path.display());
                }

                let file = std::fs::File::open(path)?;
                let config: schema::Config = serde_yaml_ng::from_reader(file)?;

                Ok::<_, anyhow::Error>(config)
            });

            parser.await?
        })
        .await?;

    Ok(config)
}
