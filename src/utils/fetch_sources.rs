use std::path::{Path, PathBuf};
use reqwest::Client;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;

use anyhow::Result;

pub async fn fetch_sources(buildhome: &Path, sources: Vec<String>) -> Result<()> {
    for sauce in sources.iter() {
       fetch_a_source(buildhome, sauce.as_str()).await;
    }
    Ok(())
}

async fn fetch_a_source(buildhome: &Path, url: &str) -> Result<()> {
    let mut resp = reqwest::get(url).await.unwrap();

    let mut path = buildhome.to_path_buf();
    let url_last = resp.url().path_segments().expect("No filename in URL?")
                            .last().expect("No filename in URL?");
    path.push(url_last);
    println!("Download {} to {}", url, path.to_str().unwrap());

    path.try_exists().expect("File already exists!");

    let file = File::create(path).unwrap();
    let mut fwriter = BufWriter::new(file);

    while let Some(chunk) = resp.chunk().await? {
        fwriter.write(&chunk);
    }
    Ok(())
}
