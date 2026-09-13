#![warn(clippy::all, clippy::pedantic)]

use std::process::ExitCode;

mod cli;
mod clients;
mod config;
mod forward;
mod proxy;

#[tokio::main]
async fn main() -> ExitCode {
    cli::init().await
}
