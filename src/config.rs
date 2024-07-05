use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize, Clone)]
pub enum ProfileGame {
    #[serde(rename = "victoria3")]
    Victoria3,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    pub name: String,
    pub title: String,
    pub game: ProfileGame,
    pub game_data_dir: String,
    pub user_data_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
pub enum UrlScheme {
    #[serde(rename = "relative")]
    Relative,
    #[serde(untagged)]
    Absolute { base_url: String },
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum PaginationMode {
    None,
    Absolute {
        #[serde(default = "default_limit")]
        limit: usize,
    },
    Alphabetic {
        #[serde(default = "default_limit")]
        sub_limit: usize,
    },
}

fn default_false() -> bool {
    false
}

fn default_pagination() -> PaginationMode {
    PaginationMode::Absolute {
        limit: default_limit(),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub profiles: Vec<Profile>,
    pub url_scheme: UrlScheme,
    pub output_dir: PathBuf,
    #[serde(default = "default_false")]
    pub use_subfolder_for_single_profile: bool,
    #[serde(default = "default_pagination")]
    pub pagination: PaginationMode,
}

impl Config {
    pub fn create(path: &Path) -> Result<Config> {
        let body = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&body)?;

        if !config.output_dir.is_dir() {
            fs::create_dir_all(&config.output_dir)?;
        }

        Ok(config)
    }
}
