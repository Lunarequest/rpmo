mod build;
mod cli;
mod pack;
use anyhow::Result;
use build::build;
use clap::Parser;
use cli::Cli;
use pack::pack;

fn main() -> Result<()> {
    let cliargs = Cli::parse();
    match cliargs.command {
        cli::CliCommand::Build { path } => {
            let path = build(path)?;
            pack(path)?;
        }
    }

    Ok(())
}
