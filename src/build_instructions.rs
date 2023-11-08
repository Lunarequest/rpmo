use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub package: Package,
    pub environment: Environment,
    pub pipeline: Vec<Pipeline>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Environment {
    pub repositories: Vec<String>,
    pub keyring: Option<Vec<String>>,
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pipeline {
    pub name: String,
    pub runs: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub release: u32,
    pub description: String,
    pub copyright: Vec<CopyRight>,
    pub dependecies: Option<Vec<String>>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CopyRight {
    pub license: String,
    pub paths: Vec<String>,
}
