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
}

pub async fn init(ForwardCommandArguments { target }: ForwardCommandArguments) -> Result<()> {
    let mut config = cnf::extract()?;

    let resources = find_resources(&mut config, &target)?;

    fwd::init(resources).await
}

fn find_resources(config: &mut cnf::schema::Config, target: &str) -> Result<Vec<Resource>> {
    let alias_index: HashMap<String, Vec<Resource>> = config
        .groups
        .values()
        .flat_map(|resources| resources.iter())
        .fold(HashMap::new(), |mut acc, resource| {
            acc.entry(resource.alias.clone())
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
