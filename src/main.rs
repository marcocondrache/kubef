#![warn(clippy::all, clippy::pedantic)]

use std::process::ExitCode;

use tokio_rustls::rustls::crypto::aws_lc_rs;

mod cli;
mod cnf;
mod env;
mod fwd;

#[tokio::main]
async fn main() -> ExitCode {
    cli::init().await
}
