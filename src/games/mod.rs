use std::{fs, path::Path};

use clauser::data::script_doc_parser::ScriptDocParserResult;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use victoria3::Victoria3GameDocProvider;

use crate::{
    config::{Profile, ProfileGame},
    dossier::DocCategory,
    error::Error,
};

mod victoria3;

/// Version information about a game.
#[derive(Serialize)]
pub struct GameVersion {
    /// The version number string for this release, like "1.7.1"
    pub version_number: String,
    /// A detailed version string
    pub detailed: String,
}

pub trait GameDocProvider {
    fn read_script_docs(&self, profile: &Profile) -> Result<Option<ScriptDocParserResult>, Error>;
    fn read_version_info(&self, profile: &Profile) -> Result<GameVersion, Error>;
    fn get_categories(&self, profile: &Profile) -> Result<Vec<DocCategory>, Error>;
}

pub fn provider_for_game(game: &ProfileGame) -> Box<impl GameDocProvider> {
    match game {
        ProfileGame::Victoria3 => Box::new(Victoria3GameDocProvider {}),
    }
}

pub struct BranchRevParser;

static VERSION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d+\.\d+\.\d+)\s*$").unwrap());

impl BranchRevParser {
    /// Parses prefix_branch.txt and prefix_rev.txt files in `root`, as well as
    /// clausewitz_branch.txt and clausewitz_rev.txt, to build a GameVersion.
    pub fn parse(root: &Path, prefix: &str) -> Result<GameVersion, Error> {
        let (game_branch, game_rev, cl_branch, cl_rev) = (
            fs::read_to_string(root.to_path_buf().join(prefix.to_string() + "_branch.txt"))?,
            fs::read_to_string(root.to_path_buf().join(prefix.to_string() + "_rev.txt"))?,
            fs::read_to_string(root.to_path_buf().join("clausewitz_branch.txt"))?,
            fs::read_to_string(root.to_path_buf().join("clausewitz_rev.txt"))?,
        );

        if game_rev.len() < 32 || cl_rev.len() < 32 {
            return Err(Error::Provider(match cl_rev.len() < 32 {
                true => format!("invalid revision in clausewitz_rev.txt"),
                false => format!("invalid revision in {}_rev.txt", prefix),
            }));
        }

        let version_number = match VERSION_REGEX.find(&game_branch) {
            Some(v) => Ok(v.as_str()),
            None => Err(Error::Provider(format!(
                "can't get version number from {}_branch.txt",
                prefix
            ))),
        }?
        .to_string();

        Ok(GameVersion {
            version_number,
            detailed: format!(
                "game version {} commit {}\nclausewitz version {} commit {}",
                game_branch,
                &game_rev[0..9],
                cl_branch,
                &cl_rev[0..9]
            ),
        })
    }
}
