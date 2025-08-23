use clap::Args;
use miette::Result;

use crate::fwd;

#[derive(Args)]
pub struct ForwardCommandArguments {
    #[arg(short, long, help = "The resource or group to forward")]
    pub target: String,
}

pub async fn init(ForwardCommandArguments { target }: ForwardCommandArguments) -> Result<()> {
    fwd::init(target).await
}
