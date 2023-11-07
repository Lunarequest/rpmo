use std::{
    fs::{create_dir_all, metadata, set_permissions, File},
    io::prelude::Write,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, Result};
use serde_yaml::from_reader;
use tempfile::TempDir;

use crate::build_instructions::Manifest;

use super::{fetch_sources::fetch_sources, run::run_init};

pub fn build(path: PathBuf) -> Result<PathBuf> {
    if !path.exists() {
        return Err(anyhow!("no such file or directory {}", path.display()));
    }

    let file = File::open(path)?;
    let build_instructions: Manifest = from_reader(file)?;

    let buildroot = TempDir::with_prefix("rpmo-workspace")?;
    let initfile = TempDir::with_prefix("rpmo-init")?;
    let buildhome = TempDir::with_prefix("rpmo-guest")?;
    create_dir_all(buildroot.path())?;
    let buildhome_path = buildhome.path();
    let buildroot_path = buildroot.path();
    let initfile_path = initfile.path();

    let mut packages = match build_instructions.package.dependecies {
        Some(deps) => {
            let mut deps = deps.clone();
            let mut runtime = build_instructions.environment.packages;
            deps.append(&mut runtime);
            deps
        }
        None => build_instructions.environment.packages,
    };
    packages.dedup();

    let init_file = init_rootfs(
        initfile_path,
        packages,
        build_instructions.environment.repositories,
    )?;

    // set up env with build dependencies
    run_init(buildroot_path, init_file.as_path())?;
    fetch_sources(buildhome_path, build_instructions.package.sources);

    let duration = Duration::from_secs(60);

    sleep(duration);
    Ok(PathBuf::new())
}

fn init_rootfs(buildroot: &Path, packages: Vec<String>, repos: Vec<String>) -> Result<PathBuf> {
    let mut buildrootPath = buildroot.to_path_buf();
    let mut repo_commands = String::new();
    for repo in repos {
        let ar = format!("zypper  --root /newroot ar -G -f {}\n", repo);
        repo_commands = repo_commands + &ar;
    }

    if repo_commands.is_empty() {
        return Err(anyhow!(
            "No repos defined, zypper will not be able to install anything"
        ));
    }
    let commands = format!(
        "
        #!/bin/bash -x
        {repo_commands}
        zypper --root /newroot in  --no-recommends -y -t pattern devel_basis
        zypper --root /newroot in --no-recommends -y {}
        ",
        packages.concat().to_string()
    );

    let initfile = buildroot.to_path_buf().join("init.sh");

    let mut init = File::create(&initfile)?;
    init.write_all(commands.as_bytes())?;

    let mut perms = metadata(&initfile)?.permissions();
    perms.set_mode(447);
    set_permissions(&initfile, perms)?;

    Ok(initfile)
}
