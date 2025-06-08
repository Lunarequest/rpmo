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
    println!("{:#?}", deps);
    deps
}

pub fn pack(buildroot: impl AsRef<Path>, manifest: Manifest) -> Result<impl AsRef<Path>> {
    let path = buildroot.as_ref().to_path_buf().join("out");
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
    .release(manifest.package.release.to_string())
    .compression(CompressionWithLevel::Zstd(3));

    let mut deps: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(&path) {
        match entry {
            Err(e) => eprintln!("{}", e),
            Ok(dir_ent) => {
                let real_loc = dir_ent.path();
                let p = real_loc.strip_prefix(&path).unwrap().to_string_lossy();
                let relative_location = format!("/{p}");
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

    println!("{:#?}", pkg.metadata);

    pkg.write_file(format!(
        "{}-{}-{}.rpm",
        manifest.package.name, manifest.package.version, manifest.package.release
    ))
    .expect("failed to write rpm");

    Ok(PathBuf::new())
}
