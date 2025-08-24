use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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

pub async fn init() -> ExitCode {
    let env = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env)
        .init();

    let args = Cli::parse();

    let output = match args.command {
        Some(Commands::Forward(args)) => forward::init(args).await,
        None => {
            if let Some(target) = args.target {
                forward::init(forward::ForwardCommandArguments { target }).await
            } else {
                Err(anyhow::anyhow!("No target specified"))
            }
        }
    };

    if let Err(_e) = output {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
