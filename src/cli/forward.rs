use std::collections::HashMap;

use anyhow::Result;
use clap::Args;

use crate::{
    cnf::{self, schema::Resource},
    fwd,
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
    let mut config = cnf::extract()?;

    let resources = find_resources(&mut config, &target)?;
    let context = match (config.context, context.as_deref()) {
        (Some(_), Some(arg_context)) => Some(arg_context),
        (Some(context), _) | (_, Some(context)) => Some(context),
        _ => None,
    };

    fwd::init(resources, context).await
}

fn find_resources<'a>(
    config: &mut cnf::schema::Config<'a>,
    target: &str,
) -> Result<Vec<Resource<'a>>> {
    let alias_index: HashMap<String, Vec<Resource>> = config
        .groups
        .values()
        .flat_map(|resources| resources.iter())
        .fold(HashMap::new(), |mut acc, resource| {
            acc.entry(resource.alias.to_string())
                .or_default()
                .push(resource.clone());
            acc
        });

    if let Some(resources) = alias_index.get(target) {
        return Ok(resources.clone());
    }

    match config.groups.remove(target) {
        Some(resources) => Ok(resources),
        None => Err(anyhow::anyhow!(
            "No resources found for target '{}' in aliases or groups",
            target
        )),
    }
}
