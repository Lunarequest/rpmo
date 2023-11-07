use anyhow::{anyhow, Context, Result};
use std::{path::PathBuf, process::Command};

pub fn run_init(path: PathBuf, file_path: PathBuf) -> Result<()> {
    let path = path.to_str().context("path was not kosher")?;
    let init_file = file_path.to_str().context("path was not kosher")?;
    eprintln!("{init_file}");
    let status = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--cap-add",
            "CAP_SYS_CHROOT",
            "-v",
            format!("{path}:/newroot").as_str(),
            "-v",
            format!("{init_file}:/init.sh").as_str(),
            "registry.opensuse.org/opensuse/tumbleweed:latest",
            "/bin/bash",
            "-x",
            "/init.sh",
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        return Err(anyhow!("init of workspace failed"));
    }
}
