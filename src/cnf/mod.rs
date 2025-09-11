use std::env;

use anyhow::Result;
use tokio::sync::OnceCell;

pub mod schema;

static CNF: OnceCell<schema::Config> = OnceCell::const_new();

pub async fn extract() -> Result<&'static schema::Config> {
    let xdg = xdg::BaseDirectories::with_prefix("kubef");

    println!("XDG: {:?}", xdg);
    println!("KUBEF_CONFIG: {:?}", env::var("KUBEF_CONFIG"));

    let path = match env::var("KUBEF_CONFIG") {
        Ok(val) => std::path::PathBuf::from(val),
        Err(_) => xdg
            .place_config_file("config.yaml")
            .expect("Failed to create default config file"),
    };

    let config = CNF
        .get_or_try_init(|| async {
            let config = tokio::fs::read(path).await?;
            let config: schema::Config = serde_yml::from_slice(&config)?;

            Ok::<_, anyhow::Error>(config)
        })
        .await?;

    Ok(config)
}
