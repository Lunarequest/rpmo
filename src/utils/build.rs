use std::{
    env::consts::ARCH,
    fs::{create_dir_all, metadata, set_permissions, File},
    io::prelude::Write,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_yaml::from_reader;
use tempfile::TempDir;
use tera::{Context, Tera};

use super::{fetch_sources::fetch_sources, run::run_init};
use crate::{
    build_instructions::{Manifest, Pipeline},
    utils::pack::pack,
};

#[derive(Debug, Serialize)]
pub struct Target {
    destdir: String,
    arch: String,
}

pub async fn build(path: PathBuf) -> Result<PathBuf> {
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

    let mut packages = match build_instructions.package.dependecies.clone() {
        Some(deps) => {
            let mut deps = deps.clone();
            let mut runtime = build_instructions.environment.packages.clone();
            deps.append(&mut runtime);
            deps
        }
        None => build_instructions.environment.packages.clone(),
    };
    packages.dedup();

    let init_file = init_rootfs_commands(
        initfile_path,
        packages,
        build_instructions.environment.repositories.clone(),
    )?;

    // set up env with build dependencies
    run_init(buildroot_path, &init_file)?;
    fetch_sources(buildhome_path, &build_instructions.package.sources).await?;

    let piplines = build_instructions.pipeline.clone();
    for pipline in piplines {
        spawn_pipeline_run(
            buildroot_path.to_path_buf(),
            buildhome_path.to_path_buf(),
            pipline,
            build_instructions.clone(),
        )?;
    }

    pack(buildhome_path.join("out").to_path_buf(), build_instructions)?;

    // FOR DEBUGGING
    println!("Eepy time😴");
    let duration = Duration::from_secs(60);
    sleep(duration);

    Ok(PathBuf::new())
}

fn spawn_pipeline_run(
    root: PathBuf,
    home: PathBuf,
    pipline: Pipeline,
    manifest: Manifest,
) -> Result<()> {
    let buildroot = root.to_string_lossy().to_string();
    let buildhome = home.to_string_lossy().to_string();
    println!("{buildhome}");
    let name = &pipline.name.replace(" ", "");
    let target = Target {
        destdir: "/home/build/out".to_string(),
        arch: ARCH.to_string(),
    };

    let mut tera = Tera::default();
    tera.add_raw_template(name, &pipline.runs)?;
    let mut context = Context::new();
    context.insert("manifest", &manifest);
    context.insert("targets", &target);
    let run = tera.render(name, &context)?;

    let pipeline_file = home.join(format!("{}.sh", name));
    let mut file = File::create(pipeline_file)?;
    file.write_all(run.as_bytes())?;

    #[rustfmt::skip]
    let status = Command::new("bwrap").args(vec![
        "--bind", &buildroot, "/",
        "--bind", &buildhome, "/home/build",
        "--unshare-pid",
        "--dev", "/dev",
        "--proc", "/proc",
        "--chdir", "/home/build",
        "--clearenv", "--new-session",
        "--setenv", "SOURCE_DATE_EPOCH", "0",
        "--setenv", "HOME", "/home/build",
        "--setenv", "PATH", "/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin",
        "/bin/bash", "-x", &format!("{}.sh", name)
    ]).status()?;

    if status.success() {
        Ok(())
    } else {
        return Err(anyhow!("build failure"));
    }
}

fn init_rootfs_commands(
    buildroot: &Path,
    packages: Vec<String>,
    repos: Vec<String>,
) -> Result<PathBuf> {
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
        mkdir -p /home/build/out
        ",
        packages.join(" ").to_string()
    );

    let initfile = buildroot.to_path_buf().join("init.sh");

    let mut init = File::create(&initfile)?;
    init.write_all(commands.as_bytes())?;

    let mut perms = metadata(&initfile)?.permissions();
    perms.set_mode(447);
    set_permissions(&initfile, perms)?;

    Ok(initfile)
}
