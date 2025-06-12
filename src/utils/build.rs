use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_yaml::from_reader;
use std::{
    collections::HashMap, env::consts::ARCH, fmt::format, fs::{create_dir_all, metadata, set_permissions, File}, io::{self, prelude::Write}, os::unix::prelude::PermissionsExt, path::{Path, PathBuf}
};

#[cfg(debug_assertions)]
use std::{thread::sleep, time::Duration};

use tempfile::{Builder, TempDir};
use tera::{Context, Tera};

use super::{fetch_sources::fetch_sources, run::run_init};
use crate::{
    mainfest::{Manifest, Pipeline},
    utils::{pack::pack, selinux_enabled},
};
use tokio::{process::Command, select, signal::ctrl_c};

#[derive(Debug, Serialize)]
pub struct Target {
    destdir: String,
    arch: &'static str,
}

fn new_tmp_dir<T: AsRef<std::ffi::OsStr>>(prefix: T) -> io::Result<TempDir> {
    let tempdir = Builder::new().prefix(&prefix).tempdir_in("/var/tmp")?;
    Ok(tempdir)
}

pub async fn build(path: PathBuf) -> Result<PathBuf> {
    if !path.exists() {
        return Err(anyhow!("no such file or directory {}", path.display()));
    }

    let file = File::open(path)?;
    let build_instructions: Manifest = from_reader(file)?;

    let buildroot = new_tmp_dir("rpmo-workspace")?;
    let initfile = new_tmp_dir("rpmo-init")?;
    let buildhome = new_tmp_dir("rpmo-guest")?;
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
    let mut outdirs = format!("{},",build_instructions.package.name);
    let mut subpkgs: Vec<String> = vec![];

    if let Some(subpackages) = &build_instructions.package.subpackages {
        for subpkg in subpackages {
            subpkgs.push(format!("{}-{}",build_instructions.package.name, &subpkg.name));

        }
        outdirs += &subpkgs.join(",");
    }

    let init_file = init_rootfs_commands(
        initfile_path,
        packages,
        build_instructions.environment.repositories.clone(),
        outdirs
    )?;

    // set up env with build dependencies
    run_init(buildroot_path, &init_file).await?;
    fetch_sources(buildhome_path, &build_instructions.package.sources).await?;
    let piplines = build_instructions.pipeline.clone();
    for pipline in piplines {
        spawn_pipeline_run(
            buildroot_path.to_path_buf(),
            buildhome_path.to_path_buf(),
            pipline,
            build_instructions.clone(),
        )
        .await?;
    }

    pack(buildroot_path, buildhome_path, &build_instructions, &build_instructions.package.name, &build_instructions.package.version, &build_instructions.package.release).await?;
    if let Some(subpkg) = &build_instructions.package.subpackages {
    for pkg in subpkg  {
        pack(buildroot_path, buildhome_path, &build_instructions, &pkg.name, &pkg.version, &pkg.release).await?;
    }
    }

    #[cfg(debug_assertions)]
    {
        // FOR DEBUGGING
        println!("Eepy time😴");
        let duration = Duration::from_secs(60);
        sleep(duration);
    }

    Ok(PathBuf::new())
}

async fn spawn_pipeline_run(
    root: PathBuf,
    home: PathBuf,
    pipline: Pipeline,
    manifest: Manifest,
) -> Result<()> {
    let buildroot = root.to_string_lossy().to_string();
    let buildhome = home.to_string_lossy().to_string();

    let name = &pipline.name.replace(' ', "");
    let mut target: HashMap<&str, Target> = HashMap::new();

    target.insert(&manifest.package.name, 
    Target {
        destdir: format!("/home/build/out/{}", &manifest.package.name),
        arch: ARCH,
    });


    if let Some(subpackages) = &manifest.package.subpackages {
        for subpkg in subpackages {
            target.insert(&subpkg.name, Target {
                destdir: format!("/home/build/out/{}", subpkg.name), 
                arch: ARCH
            });
        }
    }


    let mut tera = Tera::default();
    tera.add_raw_template(name, &pipline.runs.join("\n"))?;
    let mut context = Context::new();
    context.insert("manifest", &manifest);
    context.insert("targets", &target);
    let run = tera.render(name, &context)?;

    let pipeline_file = home.join(format!("{}.sh", name));
    let mut file = File::create(pipeline_file)?;
    file.write_all(run.as_bytes())?;

    #[rustfmt::skip]
    let mut bwrap = Command::new("bwrap").args(&[
        "--bind", &buildroot, "/",
        "--bind", &buildhome, "/home/build",
        "--unshare-pid",
        "--dev", "/dev",
        "--proc", "/proc",
        "--chdir", "/home/build",
        "--clearenv",
        "--new-session",
        "--setenv", "SOURCE_DATE_EPOCH", "0",
        "--setenv", "HOME", "/home/build",
        "--setenv", "PATH", "/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin",
        "/bin/bash", "-x", &format!("{}.sh", name)
    ]).spawn()?;

    select! {
        status = bwrap.wait() => {
            let status = status?;
            if status.success() {
                Ok(())
            } else {
                Err(anyhow!("build failure"))
            }
        }

        _ = ctrl_c() => {
             println!("Ctrl+C received, killing build process");
             let _ = bwrap.kill().await;
             Err(anyhow!("Interrupted by Ctrl+C"))
        }
    }
}

fn init_rootfs_commands(
    buildroot: &Path,
    packages: Vec<String>,
    repos: Vec<String>,
    output_dirs: String,
) -> Result<PathBuf> {
    let mut repo_commands = String::new();
    for repo in repos {
        let ar = format!("zypper  --root /newroot ar -f {}\n", repo);
        repo_commands = repo_commands + &ar;
    }

    if repo_commands.is_empty() {
        return Err(anyhow!(
            "No repos defined, zypper will not be able to install anything"
        ));
    }

    repo_commands += "zypper --root /newroot --gpg-auto-import-keys refresh\n";

    if selinux_enabled() {
        repo_commands += "export LIBSEMANAGE_ASSUME_NOSELINUX=1\n";
    }

    // we really should not need gettext-tools full but gettext-tools-mini pulls in this-is-only-for-build-envs
    // which it shouldn't since the package "this-is-only-for-build-envs" is a obs specific attribute
    // used to show a package is only need for build time... not sure if its the repos or some haunting bs
    let commands = format!(
        "
        #!/bin/bash -x
        mkdir -p /newroot/proc
        mount --bind /proc /newroot/proc
        {repo_commands}
        zypper --root /newroot in --no-recommends -y filesystem udev gettext-tools
        zypper --root /newroot in --no-recommends -y -t pattern devel_basis
        zypper --root /newroot in --no-recommends -y {}
        mkdir -p /home/build/out/{{{output_dirs}}}
        ",
        packages.join(" ")
    );

    let initfile = buildroot.to_path_buf().join("init.sh");

    let mut init = File::create(&initfile)?;
    init.write_all(commands.as_bytes())?;

    let mut perms = metadata(&initfile)?.permissions();
    perms.set_mode(0o447);
    set_permissions(&initfile, perms)?;

    Ok(initfile)
}
