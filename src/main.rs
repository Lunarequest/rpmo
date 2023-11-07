mod build_instructions;
mod cli;
mod utils;
use anyhow::Result;
use clap::Parser;
use cli::Cli;
use utils::{build::build, pack::pack};

#[tokio::main]
async fn main() -> Result<()> {
    let cliargs = Cli::parse();
    match cliargs.command {
        cli::CliCommand::Build { path } => {
            let path = build(path).await?;
            pack(path)?;
        }
    }

    Ok(())
}
