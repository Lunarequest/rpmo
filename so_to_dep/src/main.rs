use clap::Parser;
use serde::Deserialize;
use serde_json::from_str;
use std::path::PathBuf;
use tokio::{process::Command, select, signal::ctrl_c};

#[derive(Debug, Parser)]
struct Cli {
    sos: String,
}

#[derive(Debug, Deserialize)]
struct Input {
    so: Vec<String>,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let data: Input = from_str(&args.sos).unwrap();
    let deps: Vec<String> = vec![];
    let output = Command::new("ldconfig").arg("-p").output().await.unwrap();

    for so in data.so {}
}
