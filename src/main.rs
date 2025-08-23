use miette::Result;

mod cli;
mod cnf;
mod fwd;

#[tokio::main]
async fn main() -> Result<()> {
    cli::init().await
}
