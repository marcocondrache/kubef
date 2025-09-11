use anyhow::Result;
use clap::Args;
use either::Either;

use crate::{
    cnf::{self},
    fwd::{Forwarder, Target},
};

#[derive(Args)]
pub struct ForwardCommandArguments {
    #[arg(short, long, help = "The resource or group to forward")]
    pub target: String,

    #[arg(short, long, help = "The kubeconfig context to use")]
    pub context: Option<String>,
}

pub async fn init(
    ForwardCommandArguments { target, context }: ForwardCommandArguments,
) -> Result<()> {
    let config = cnf::extract().await?;

    let resources = get_target(config, &target)?;
    let context = match (config.context.as_deref(), context.as_deref()) {
        (Some(_), Some(arg_context)) => Some(arg_context),
        (Some(context), _) | (_, Some(context)) => Some(context),
        _ => None,
    };

    let mut forwarder = Forwarder::new(context, config.loopback).await?;

    forwarder.forward(resources).await?;

    tokio::signal::ctrl_c().await?;

    forwarder.shutdown().await?;

    Ok(())
}

fn get_target<'a>(config: &'a cnf::schema::Config, target: &str) -> Result<Target<'a>> {
    if let Some(resource) = config
        .groups
        .values()
        .flat_map(|resources| resources.iter())
        .find(|resource| resource.alias == target)
    {
        return Ok(Either::Left(resource));
    }

    match config.groups.get(target) {
        Some(resources) => Ok(Either::Right(resources)),
        None => Err(anyhow::anyhow!(
            "No resources found for target '{}' in aliases or groups",
            target
        )),
    }
}
