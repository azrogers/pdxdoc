use std::{fs, path::Path};

use serde::Deserialize;

use crate::error::Error;

#[derive(Deserialize, Clone)]
pub enum ProfileGame {
    #[serde(rename = "victoria3")]
    Victoria3,
}

#[derive(Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    pub title: String,
    pub game: ProfileGame,
    pub game_data_dir: String,
    pub user_data_dir: String,
}

#[derive(Deserialize)]
pub struct Config {
    pub profiles: Vec<Profile>,
}

impl Config {
    pub fn create(path: &Path) -> Result<Config, Error> {
        let body = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&body)?;

        Ok(config)
    }
}
