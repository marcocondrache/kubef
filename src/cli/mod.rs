use clap::{Parser, Subcommand};
use miette::Result;

mod forward;

#[derive(Parser)]
#[command(name = "kubef", bin_name = "kubef")]
struct Cli {
    #[arg(value_name = "RESOURCE", help = "Resource to process")]
    target: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Forward a resource")]
    Forward(forward::ForwardCommandArguments),
}

pub async fn init() -> Result<()> {
    let args = Cli::parse();

    match args.command {
        Some(Commands::Forward(args)) => forward::init(args).await,
        None => {
            if let Some(target) = args.target {
                forward::init(forward::ForwardCommandArguments { target }).await
            } else {
                Err(miette::Report::msg("No target specified"))
            }
        }
    }
}
