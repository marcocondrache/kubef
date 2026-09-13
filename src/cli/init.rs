use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;

use crate::config;

const STARTER: &str = "\
# yaml-language-server: $schema=https://raw.githubusercontent.com/marcocondrache/kubef/main/schemas/schema.json
groups:
  example:
    - alias: web
      selector:
        type: service
        match: web
      ports:
        remote: 80
        local: 8080
";

#[derive(Args)]
pub struct InitCommandArguments {
    #[arg(
        value_name = "PATH",
        help = "Write here instead of the default config path"
    )]
    pub path: Option<PathBuf>,

    #[arg(long, help = "Overwrite an existing file")]
    pub force: bool,
}

pub fn init(InitCommandArguments { path, force }: InitCommandArguments) -> Result<()> {
    let path = match path {
        Some(path) => path,
        None => config::config_path()?,
    };

    write_starter(&path, force)?;
    println!("Wrote {}", path.display());
    println!("Edit this file, then run `kubef list`.");
    Ok(())
}

pub fn write_starter(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "config already exists at {}\nRe-run with --force to overwrite, or set KUBEF_CONFIG.",
            path.display()
        );
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, STARTER)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kubef-init-{}-{}-{tag}.yaml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn writes_parseable_starter() {
        let path = temp_path("write");
        write_starter(&path, false).unwrap();
        let config = config::load_from_path(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(config.groups["example"][0].alias, "web");
    }

    #[test]
    fn refuses_to_overwrite() {
        let path = temp_path("exists");
        write_starter(&path, false).unwrap();
        let err = write_starter(&path, false).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(format!("{err}").contains("--force"));
    }

    #[test]
    fn force_overwrites() {
        let path = temp_path("force");
        std::fs::write(&path, "stale").unwrap();
        write_starter(&path, true).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(contents.contains("alias: web"));
    }
}
