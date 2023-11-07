use std::path::{Path, PathBuf};
use reqwest::Client;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;

use anyhow::Result;

pub fn fetch_sources(buildhome: &Path, sources: Vec<String>) -> Result<()> {
    let _: Vec<_> = sources.iter().map(
       |url| fetch_a_source(buildhome, url.as_str())).collect();
    Ok(())
}

async fn fetch_a_source(buildhome: &Path, url: &str) -> Result<()> {
    let mut resp = reqwest::get(url).await?;

    let mut path = buildhome.to_path_buf();
    let url_last = resp.url().path_segments().expect("No filename in URL?")
                            .last().expect("No filename in URL?");
    path.push(url_last);

    let file = File::open(path)?;
    let mut fwriter = BufWriter::new(file);

    while let Some(chunk) = resp.chunk().await? {
        fwriter.write(&chunk)?;
    }
    Ok(())
}
