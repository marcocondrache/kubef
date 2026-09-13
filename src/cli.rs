use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{
    CompleteEnv,
    engine::{ArgValueCompleter, CompletionCandidate},
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod forward;
mod list;
mod proxy;

const LOGO: &str = r"
  _          _           __ 
 | | ___   _| |__   ___ / _|
 | |/ / | | | '_ \ / _ \ |_ 
 |   <| |_| | |_) |  __/  _|
 |_|\_\\__,_|_.__/ \___|_|        
";

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_RELEASE: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = PKG_NAME, bin_name = "kubef")]
#[command(version = PKG_RELEASE, before_help = LOGO)]
#[command(disable_version_flag = false, arg_required_else_help = true)]
struct Cli {
    #[arg(value_name = "RESOURCE", hide = true, add = ArgValueCompleter::new(complete_targets))]
    alias_completion_hook: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Forward a resource")]
    Forward(forward::ForwardCommandArguments),
    #[command(about = "Proxy an internal ip address")]
    Proxy(proxy::ProxyCommandArguments),
    #[command(about = "List configured aliases and groups")]
    List,
}

const KNOWN_SUBCOMMANDS: &[&str] = &["forward", "proxy", "list", "help"];

fn inject_forward_subcommand() -> Vec<String> {
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

fn load_config_sync() -> Option<crate::config::schema::Config> {
    let path = crate::config::config_path().ok()?;
    crate::config::load_from_path(&path).ok()
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

    let Cli {
        command,
        alias_completion_hook: _,
    } = Cli::parse_from(inject_forward_subcommand());

    let output = match command {
        Some(Commands::Forward(args)) => forward::init(args).await,
        Some(Commands::Proxy(args)) => proxy::init(args).await,
        Some(Commands::List) => list::init().await,
        None => Err(anyhow::anyhow!("No target specified")),
    };

    if let Err(e) = output {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
