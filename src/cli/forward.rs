use anyhow::Result;
use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use either::Either;

use crate::{
    config,
    forward::{Forwarder, Target},
};

#[derive(Args)]
pub struct ForwardCommandArguments {
    #[arg(value_name = "TARGET", help = "The resource or group to forward",
          add = ArgValueCompleter::new(super::complete_targets))]
    pub target: String,

    #[arg(short, long, help = "The kubeconfig context to use")]
    pub context: Option<String>,
}

pub async fn init(
    ForwardCommandArguments { target, context }: ForwardCommandArguments,
) -> Result<()> {
    let config = config::extract().await?;

    let resources = get_target(config, &target)?;
    let context = context.as_deref().or(config.context.as_deref());

    let forwarder = Forwarder::default()
        .with_context(context)
        .with_loopback(config.loopback);

    match resources {
        Either::Left(resource) => forwarder.forward(resource).await?,
        Either::Right(resources) => forwarder.forward_all(resources).await?,
    }

    tokio::signal::ctrl_c().await?;
    forwarder.shutdown().await?;

    Ok(())
}

fn get_target<'config>(
    config: &'config config::schema::Config,
    target: &str,
) -> Result<Target<'config>> {
    if let Some(resource) = config
        .groups
        .values()
        .flat_map(|resources| resources.iter())
        .find(|resource| resource.alias == target)
    {
        return Ok(Either::Left(resource));
    }

    if let Some(resources) = config.groups.get(target) {
        Ok(Either::Right(resources))
    } else {
        let mut suggestions: Vec<(&str, f64)> = config
            .groups
            .values()
            .flat_map(|resources| resources.iter())
            .map(|r| r.alias.as_str())
            .chain(config.groups.keys().map(String::as_str))
            .map(|candidate| (candidate, strsim::jaro_winkler(target, candidate)))
            .filter(|&(_, score)| score > 0.7)
            .collect();

        suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if suggestions.is_empty() {
            Err(anyhow::anyhow!(
                "unknown target '{target}'\nRun `kubef list` to see configured aliases and groups."
            ))
        } else {
            let names: Vec<&str> = suggestions.iter().map(|(name, _)| *name).collect();
            Err(anyhow::anyhow!(
                "unknown target '{target}'\nDid you mean: {}?",
                names.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{Config, PortSpec, Ports, Resource, ResourceSelector};
    use std::collections::HashMap;

    fn config_with(alias: &str, group: &str) -> Config {
        let mut groups = HashMap::new();
        groups.insert(
            group.to_string(),
            vec![Resource {
                alias: alias.to_string(),
                namespace: None,
                context: None,
                policy: None,
                selector: ResourceSelector::Service("svc".into()),
                ports: Ports {
                    remote: PortSpec::Number(80),
                    local: Some(8080),
                    mapping: None,
                },
            }],
        );
        Config {
            context: None,
            groups,
            loopback: None,
            contexts: HashMap::new(),
            ports: None,
        }
    }

    #[test]
    fn finds_alias() {
        let config = config_with("frontend", "web");
        let target = get_target(&config, "frontend").unwrap();
        assert!(matches!(target, Either::Left(resource) if resource.alias == "frontend"));
    }

    #[test]
    fn finds_group() {
        let config = config_with("frontend", "web");
        let target = get_target(&config, "web").unwrap();
        assert!(matches!(target, Either::Right(resources) if resources.len() == 1));
    }

    #[test]
    fn unknown_target_mentions_list() {
        let config = config_with("frontend", "web");
        let err = get_target(&config, "pdf").unwrap_err();
        assert!(format!("{err}").contains("kubef list"));
    }

    #[test]
    fn close_target_suggests() {
        let config = config_with("frontend", "web");
        let err = get_target(&config, "frontends").unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("unknown target 'frontends'"));
        assert!(message.contains("frontend"));
    }
}
