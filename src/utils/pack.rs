use crate::mainfest::Manifest;
use anyhow::{anyhow, Context, Result};
use goblin::Object;
use rpm::{CompressionWithLevel, Dependency, FileOptions, PackageBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};
use std::collections::HashSet;
use std::fmt::Display;
use std::{
    env::consts::ARCH,
    env::current_exe,
    fs::read,
    path::{Path, PathBuf},
};
use tokio::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
struct Input {
    so: HashSet<String>,
}

#[derive(Debug, Deserialize)]
struct Output {
    libraries: HashSet<String>,
}

fn analysis_file(path: impl AsRef<Path>) -> HashSet<String> {
    let mut deps = HashSet::new();
    if let Ok(buffer) = read(path) {
        if let Ok(Object::Elf(elf)) = Object::parse(&buffer) {
            for lib in elf.libraries {
                deps.insert(lib.into());
            }
        }
    }

    deps
}

fn rpm_to_version(name: impl AsRef<str>) -> Option<(String, String)> {
    let trimmed = name.as_ref().trim_end_matches(".rpm");

    // Split into name-version-release.arch
    let parts: Vec<&str> = trimmed.rsplitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }

    // Now split by dashes from the right side to extract version and release
    let dash_parts: Vec<&str> = parts[1].rsplitn(3, '-').collect();
    if dash_parts.len() != 3 {
        return None;
    }

    let version = dash_parts[1];
    let name = dash_parts[2..]
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<&str>>()
        .join("-");

    Some((name, version.to_string()))
}

pub async fn pack(
    buildroot: impl AsRef<Path>,
    buildhome: impl AsRef<Path>,
    manifest: &Manifest,
    pkgname: impl AsRef<str> + Display,
    version: impl AsRef<str> + Display,
    release: &u32
) -> Result<impl AsRef<Path>> {
    let path = buildhome.as_ref().to_path_buf().join("out").join(pkgname.as_ref());
    let current_path = current_exe()?;
    let so_to_dep = current_path
        .parent()
        .context("failed to get directory where rpmo is")?
        .join("so_to_dep");

    let mut license = String::new();
    for copyright in &manifest.package.copyright {
        if license.is_empty() {
            license = license + &copyright.license;
        } else {
            license += " AND ";
            license = license + &copyright.license;
        }
    }

    let mut rpm = PackageBuilder::new(
        pkgname.as_ref(),
        version.as_ref(),
        &license,
        ARCH,
        &manifest.package.description,
    )
    .release(manifest.package.release.to_string())
    .compression(CompressionWithLevel::Zstd(19));

    let mut deps: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(&path) {
        match entry {
            Err(e) => eprintln!("{}", e),
            Ok(dir_ent) => {
                let real_loc = dir_ent.path();
                let p = real_loc.strip_prefix(&path).unwrap().to_string_lossy();
                let relative_location = format!("/{p}");
                if !real_loc.is_dir() {
                    let file_deps = analysis_file(real_loc);
                    rpm = rpm
                        .with_file(
                            real_loc.to_string_lossy().to_string(),
                            FileOptions::new(relative_location),
                        )
                        .unwrap();
                    deps.extend(file_deps);
                }
            }
        }
    }

    if !deps.is_empty() {
        let input = Input { so: deps };

        #[rustfmt::skip]
        let bwrap = Command::new("bwrap").args([
            "--bind", buildroot.as_ref().to_str().unwrap(), "/",
            "--bind", so_to_dep.to_str().unwrap(), "/usr/bin/so_to_dep",
            "--unshare-pid",
            "--dev", "/dev",
            "--proc", "/proc",
            "--clearenv",
            "--new-session",
            "--setenv", "SOURCE_DATE_EPOCH", "0",
            "--setenv", "HOME", "/home/build",
            "--setenv", "PATH", "/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin",
            "/usr/bin/so_to_dep", &to_string(&input).unwrap()
        ]).output().await?;

        if !bwrap.status.success() {
            let output = String::from_utf8_lossy(&bwrap.stdout);
            let errout = String::from_utf8_lossy(&bwrap.stderr);
            println!("{}", output.trim());
            eprintln!("{errout}");
            return Err(anyhow!("Failed to convert so deps to pkgnames"));
        }

        let output = String::from_utf8_lossy(&bwrap.stdout);

        let out: Output =
            from_str(output.trim()).context("failed to deserialise so_to_dep output")?;

        for lib in out.libraries {
            if let Some(dependancy) = rpm_to_version(&lib) {
                rpm = rpm.requires(Dependency::eq(dependancy.0, dependancy.1));
            }
        }
    }
    let pkg = rpm.build().expect("failed to build rpm");

    pkg.write_file(format!(
        "{}-{}-{}-{ARCH}.rpm",
        pkgname.as_ref(), version.as_ref(), release
    ))
    .expect("failed to write rpm");

    Ok(PathBuf::new())
}
