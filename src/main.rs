mod algo;
mod cli;
mod daemon;
mod hypr;
mod model;
mod notify;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();
    cli::run(args).await
}
