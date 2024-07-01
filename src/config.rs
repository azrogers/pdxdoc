use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize, Clone)]
pub enum ProfileGame {
    #[serde(rename = "victoria3")]
    Victoria3,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    pub title: String,
    pub game: ProfileGame,
    pub game_data_dir: String,
    pub user_data_dir: String,
}

#[derive(Debug, Deserialize)]
pub enum UrlScheme {
    #[serde(rename = "relative")]
    Relative,
    #[serde(rename = "absolute")]
    Absolute { base_url: String },
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub profiles: Vec<Profile>,
    pub url_scheme: UrlScheme,
    pub output_dir: PathBuf,
    #[serde(default = "default_false")]
    pub use_subfolder_for_single_profile: bool,
}

impl Config {
    pub fn create(path: &Path) -> Result<Config, Error> {
        let body = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&body)?;

        if !config.output_dir.is_dir() {
            fs::create_dir_all(&config.output_dir)?;
        }

        Ok(config)
    }
}
