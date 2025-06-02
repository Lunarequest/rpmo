use crate::build_instructions::Manifest;
use anyhow::Result;
use goblin::Object;
use rpm::{CompressionWithLevel, Dependency, FileOptions, PackageBuilder};
use std::collections::HashSet;
use std::{
    env::consts::ARCH,
    fs::read,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub fn analyis_file(path: impl AsRef<Path>) -> HashSet<String> {
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

pub fn pack(path: impl AsRef<Path>, manifest: Manifest) -> Result<impl AsRef<Path>> {
    let root = path.as_ref().to_string_lossy().to_string();
    let mut license = String::new();
    for copyright in manifest.package.copyright {
        if license.is_empty() {
            license = license + &copyright.license;
        } else {
            license += " AND ";
            license = license + &copyright.license;
        }
    }

    let mut rpm = PackageBuilder::new(
        &manifest.package.name,
        &manifest.package.version,
        &license,
        ARCH,
        &manifest.package.description,
    )
    .compression(CompressionWithLevel::Zstd(19));

    let mut deps: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(path) {
        match entry {
            Err(e) => eprintln!("{}", e),
            Ok(dir_ent) => {
                let real_loc = dir_ent.path();
                let relative_location = real_loc.to_string_lossy().replace(&root, "/");
                if !real_loc.is_dir() {
                    let file_deps = analyis_file(real_loc);
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

    for dep in deps {
        rpm = rpm.requires(Dependency::any(dep));
    }

    let pkg = rpm.build().expect("failed to build rpm");

    pkg.write_file(format!(
        "{}-{}-{}.rpm",
        manifest.package.name, manifest.package.version, manifest.package.release
    ))
    .expect("failed to write rpm");

    Ok(PathBuf::new())
}
