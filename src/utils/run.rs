use anyhow::{anyhow, Context, Result};
use std::{path::Path, process::Command};

pub fn run_init(path: &Path, file_path: &Path) -> Result<()> {
    let path = path.to_str().context("path was not kosher")?;
    let init_file = file_path.to_str().context("path was not kosher")?;
    eprintln!("{init_file}");
    // TODO forward SIGTERM to podman
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
        Err(anyhow!("init of workspace failed"))
    }
}
