pub mod build;
mod fetch_sources;
mod pack;
mod run;
use std::fs::read_to_string;

pub fn selinux_enabled() -> bool {
    match read_to_string("/sys/fs/selinux/enforce") {
        Ok(content) => content.trim() == "1",
        Err(_) => false,
    }
}
