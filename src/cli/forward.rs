use anyhow::Result;
use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use either::Either;

use crate::{
    cnf::{self},
    fwd::{Forwarder, Target},
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
    let config = cnf::extract().await?;

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

fn get_target<'cnf>(config: &'cnf cnf::schema::Config, target: &str) -> Result<Target<'cnf>> {
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
                "No resources found for target '{target}' in aliases or groups"
            ))
        } else {
            let names: Vec<&str> = suggestions.iter().map(|(name, _)| *name).collect();
            Err(anyhow::anyhow!(
                "No resources found for target '{target}' in aliases or groups\nDid you mean: {}?",
                names.join(", ")
            ))
        }
    }
}
