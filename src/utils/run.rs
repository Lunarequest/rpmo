use anyhow::{anyhow, Context, Result};
use std::{path::PathBuf, process::Command};

pub fn run_init(path: PathBuf, file_path: PathBuf) -> Result<()> {
    let path = path.to_str().context("path was not kosher")?;
    let init_file = file_path.to_str().context("path was not kosher")?;
    eprintln!("{init_file}");
    let status = Command::new("bwrap")
        .args([
            "--bind", path, "/initroot",
            "--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf",
            "--bind",
            init_file,
            "/init.sh",
            "--unshare-pid",
            "--unshare-user",
            "--uid", "0",
            "--gid", "0",
            "--cap-add",
            "CAP_SYS_CHROOT",
            "--clearenv",
            "--new-session",
            "--dir",
            "/tmp",
            "--dev",
            "/dev",
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
