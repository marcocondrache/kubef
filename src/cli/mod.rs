use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing::{error, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::env::{LOGO, PKG_NAME, PKG_RELEASE};

mod forward;
mod proxy;

#[derive(Parser)]
#[command(name = PKG_NAME, bin_name = "kubef")]
#[command(version = PKG_RELEASE, before_help = LOGO)]
#[command(disable_version_flag = false, arg_required_else_help = true)]
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
    #[command(about = "Proxy an internal ip address")]
    Proxy(proxy::ProxyCommandArguments),
}

pub async fn init() -> ExitCode {
    let env = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("KUBEF_LOG")
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env)
        .init();

    let args = Cli::parse();

    let output = match args.command {
        Some(Commands::Forward(args)) => forward::init(args).await,
        Some(Commands::Proxy(args)) => proxy::init(args).await,
        None => {
            if let Some(target) = args.target {
                forward::init(forward::ForwardCommandArguments {
                    target,
                    context: None,
                })
                .await
            } else {
                Err(anyhow::anyhow!("No target specified"))
            }
        }
    };

    if let Err(e) = output {
        error!("{}", e);

        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
