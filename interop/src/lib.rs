use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize)]
pub struct Input {
    pub so: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output {
    pub libraries: HashSet<String>,
}
