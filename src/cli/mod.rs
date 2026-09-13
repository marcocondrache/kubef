use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{
    CompleteEnv,
    engine::{ArgValueCompleter, CompletionCandidate},
};
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
    // Phantom positional: gives the completion engine a hook to suggest resource
    // aliases at the top level alongside subcommands. Never populated at runtime
    // because preprocess_args() always injects the "forward" subcommand first.
    #[arg(value_name = "RESOURCE", hide = true, add = ArgValueCompleter::new(complete_targets))]
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

const KNOWN_SUBCOMMANDS: &[&str] = &["forward", "proxy", "help"];

/// Injects `forward` before the target when the first arg is a resource alias rather than
/// a subcommand or flag, preserving `kubef <target>` as a shorthand for `kubef forward <target>`.
fn preprocess_args() -> Vec<String> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1) {
        Some(first) if !first.starts_with('-') && !KNOWN_SUBCOMMANDS.contains(&first.as_str()) => {
            let mut injected = Vec::with_capacity(args.len() + 1);
            injected.push(args[0].clone());
            injected.push("forward".to_string());
            injected.extend_from_slice(&args[1..]);
            injected
        }
        _ => args,
    }
}

fn load_config_sync() -> Option<crate::cnf::schema::Config> {
    let path = crate::cnf::config_path()?;
    let file = std::fs::File::open(&path).ok()?;
    serde_yaml_ng::from_reader(file).ok()
}

fn complete_targets(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let Some(config) = load_config_sync() else {
        return vec![];
    };
    config
        .groups
        .values()
        .flat_map(|resources| resources.iter())
        .map(|r| CompletionCandidate::new(r.alias.clone()))
        .chain(
            config
                .groups
                .keys()
                .map(|name| CompletionCandidate::new(name.clone())),
        )
        .collect()
}

pub async fn init() -> ExitCode {
    // Must run before anything writes to stdout. When COMPLETE=<shell> is set, outputs
    // completions or the shell registration script and exits. No-op otherwise.
    CompleteEnv::with_factory(Cli::command).complete();

    let env = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("KUBEF_LOG")
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env)
        .init();

    let args = Cli::parse_from(preprocess_args());

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
