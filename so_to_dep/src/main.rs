use anyhow::{Context, Result, anyhow};
use clap::Parser;
use regex::{Regex, escape};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};
use std::{
    collections::HashSet,
    io::{self, Write},
};
use tokio::process::Command;

#[derive(Debug, Parser)]
struct Cli {
    sos: String,
}

#[derive(Debug, Deserialize)]
struct Input {
    so: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Output {
    libraries: HashSet<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let data: Input = from_str(&args.sos).unwrap();
    let mut deps: HashSet<String> = HashSet::new();
    let ldconfig = Command::new("/sbin/ldconfig")
        .arg("-p")
        .output()
        .await
        .unwrap();

    if !ldconfig.status.success() {
        return Err(anyhow!(
            "ldconfig exited with exited with a none zero exit code"
        ));
    }

    let output = String::from_utf8_lossy(&ldconfig.stdout);

    for so in data.so {
        let pattern = format!(r"^\s*{}\s+\(.*?\)\s+=>\s+(\S+)", escape(&so));
        let re = Regex::new(&pattern)?;

        for line in output.lines() {
            if let Some(caps) = re.captures(line) {
                let cap = caps[0].split("=>").last().context("e")?;
                let rpm = Command::new("rpm")
                    .args(vec!["-q", "--whatprovides"])
                    .arg(cap.trim())
                    .output()
                    .await
                    .unwrap();

                if !rpm.status.success() {
                    io::stderr()
                        .write_all(&rpm.stderr)
                        .expect("Failed to write to stderr");

                    return Err(anyhow!(
                        "rpm exited with exited with a none zero exit code {}",
                        rpm.status.code().unwrap()
                    ));
                }

                let output_s = String::from_utf8_lossy(&rpm.stdout);
                let output = output_s.trim();
                deps.insert(output.to_owned());
                break;
            }
        }
    }

    let output = Output { libraries: deps };

    println!("{}", to_string(&output).unwrap());

    Ok(())
}
