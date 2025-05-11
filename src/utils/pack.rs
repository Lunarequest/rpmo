use crate::build_instructions::Manifest;
use anyhow::Result;
use rpm::{CompressionWithLevel, FileOptions, PackageBuilder};
use std::{env::consts::ARCH, path::PathBuf};
use walkdir::WalkDir;

pub fn pack(path: PathBuf, manifest: Manifest) -> Result<PathBuf> {
    let root = path.to_string_lossy().to_string();
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

    for entry in WalkDir::new(path) {
        match entry {
            Err(e) => eprintln!("{}", e),
            Ok(dir_ent) => {
                let real_loc = dir_ent.path();
                let relative_location = real_loc.to_string_lossy().replace(&root, "/");
                if !real_loc.is_dir() {
                    rpm = rpm
                        .with_file(
                            real_loc.to_string_lossy().to_string(),
                            FileOptions::new(relative_location),
                        )
                        .unwrap();
                }
            }
        }
    }

    let pkg = rpm.build().expect("failed to build rpm");

    pkg.write_file(format!(
        "{}-{}-{}.rpm",
        manifest.package.name, manifest.package.version, manifest.package.release
    ))
    .expect("failed to write rpm");

    Ok(PathBuf::new())
}
