use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
pub fn config_path() -> Result<PathBuf> {
    if let Ok(val) = env::var("KUBEF_CONFIG") {
        return Ok(PathBuf::from(val));
    }
    dirs::config_dir()
        .map(|d| d.join("kubef").join("config.yaml"))
        .context(
            "cannot resolve config path; set KUBEF_CONFIG or ensure a user config directory exists",
        )
}

pub fn load_from_path(path: &Path) -> Result<schema::Config> {
    if !path.exists() {
        anyhow::bail!(
            "config file not found at {}\nCreate this file, or set KUBEF_CONFIG to an existing file.",
            path.display()
        );
    }

    let file = std::fs::File::open(path)
        .with_context(|| format!("cannot open config file {}", path.display()))?;
    serde_yaml_ng::from_reader(file)
        .with_context(|| format!("invalid config file {}", path.display()))
}

pub async fn extract() -> Result<&'static schema::Config> {
    let path = config_path()?;

    let config = CNF
        .get_or_try_init(|| async {
            let path = path.clone();
            task::spawn_blocking(move || load_from_path(&path)).await?
        })
        .await?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_names_the_path() {
        let err = load_from_path(Path::new("/tmp/kubef-no-such-config.yaml")).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("/tmp/kubef-no-such-config.yaml"));
        assert!(message.contains("KUBEF_CONFIG"));
    }

    #[test]
    fn load_from_path_reads_groups() {
        let path =
            std::env::temp_dir().join(format!("kubef-load-from-path-{}.yaml", std::process::id()));
        std::fs::write(
            &path,
            "groups:\n  web:\n    - alias: frontend\n      selector:\n        type: service\n        match: frontend\n      ports:\n        remote: 80\n",
        )
        .unwrap();
        let config = load_from_path(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(config.groups["web"][0].alias, "frontend");
    }
}
