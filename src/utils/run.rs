use anyhow::{anyhow, Context, Result};
use std::{path::PathBuf, process::Command};

pub fn run_init(path: PathBuf, file_path: PathBuf) -> Result<()> {
    let path = path.to_str().context("path was not kosher")?;
    let init_file = file_path.to_str().context("path was not kosher")?;
    eprintln!("{init_file}");
    let status = Command::new("bwrap")
        .args([
            "--bind",
            path,
            "/initroot",
            "--bind",
            "/etc/resolv.conf",
            "/etc/resolv.conf",
            "--bind",
            init_file,
            "/init.sh",
            "--unshare-pid",
            "--uid",
            "0",
            "--gid",
            "0",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--clearenv",
            "--new-session",
            "--dir",
            "/tmp",
            "--dir",
            "/var",
            "--symlink",
            "../tmp",
            "/var/tmp",
            "--setenv",
            "SOURCE_DATE_EPOCH",
            "0",
            "--setenv",
            "PATH",
            "/sbin:/usr/sbin:/usr/local/sbin:/root/bin:/usr/local/bin:/usr/bin:/bin",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib64",
            "/lib64",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/sbin",
            "/sbin",
            "--unshare-all",
            "--share-net",
            "/bin/sh",
            "-c",
            "/bin/sh /init.sh",
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        return Err(anyhow!("init of workspace failed"));
    }
}
