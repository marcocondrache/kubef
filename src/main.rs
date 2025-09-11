use std::process::ExitCode;

mod cli;
mod cnf;
mod dns;
mod env;
mod fwd;

#[tokio::main]
async fn main() -> ExitCode {
    cli::init().await
}
