use std::env;

use anyhow::Result;
use tokio::{sync::OnceCell, task};

pub mod schema;

/// Resolves the namespace for a resource using the following chain:
/// 1. Explicit `resource.namespace`
/// 2. Namespace from the context alias, if the resource's context names one
/// 3. `"default"`
#[must_use]
pub fn resolve_namespace<'a>(
    resource: &'a schema::Resource,
    config: &'a schema::Config,
) -> &'a str {
    resource
        .namespace
        .as_deref()
        .or_else(|| {
            resource
                .context
                .as_deref()
                .and_then(|ctx| config.contexts.get(ctx))
                .and_then(|alias| alias.namespace.as_deref())
        })
        .unwrap_or("default")
}

static CNF: OnceCell<schema::Config> = OnceCell::const_new();

/// Resolves the config file path: `KUBEF_CONFIG` env var, or `config.yaml`
/// under the platform config dir (`~/.config/kubef` on unix,
/// `%APPDATA%\kubef` on windows).
pub fn config_path() -> Option<std::path::PathBuf> {
    if let Ok(val) = env::var("KUBEF_CONFIG") {
        return Some(std::path::PathBuf::from(val));
    }
    dirs::config_dir().map(|d| d.join("kubef").join("config.yaml"))
}

pub async fn extract() -> Result<&'static schema::Config> {
    let path = config_path().expect("Failed to resolve default config file path");

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
