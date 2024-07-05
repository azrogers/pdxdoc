use std::path::{Path, PathBuf};

use anyhow::Result;
use clauser::data::script_doc_parser::{
    v3_parser::V3ScriptDocParser, ScriptDocCategory, ScriptDocParser, ScriptDocParserResult,
};
use log::warn;

use crate::{
    config::{Profile, ProfileGame},
    dossier::DocCategory,
    error::Error,
};

use super::{BranchRevParser, GameVersion};

use super::GameDocProvider;

pub struct Victoria3GameDocProvider;

impl GameDocProvider for Victoria3GameDocProvider {
    fn read_script_docs(&self, profile: &Profile) -> Result<Option<ScriptDocParserResult>> {
        let path = PathBuf::from(&profile.user_data_dir).join("docs");
        if !path.is_dir() {
            warn!(
                "tried to read Victoria 3 script docs at {:?} but no such directory found",
                path
            );
            return Ok(None);
        }

        let parser = V3ScriptDocParser::parse(&path)?;
        Ok(Some(parser))
    }

    fn read_version_info(&self, profile: &Profile) -> Result<GameVersion> {
        BranchRevParser::parse(&PathBuf::from(&profile.game_data_dir), "caligula")
    }

    fn get_categories(&self, _profile: &Profile) -> Result<Vec<DocCategory>> {
        Ok(vec![
            DocCategory::new(
                &ScriptDocCategory::CustomLocalization,
                "custom_loc",
                "Custom Localization",
            ),
            DocCategory::new(&ScriptDocCategory::Effects, "effects", "Effects"),
            DocCategory::new(
                &ScriptDocCategory::EventTargets,
                "event_targets",
                "Event Targets",
            ),
            DocCategory::new(&ScriptDocCategory::Modifiers, "modifiers", "Modifiers"),
            DocCategory::new(&ScriptDocCategory::OnActions, "on_actions", "On Actions"),
            DocCategory::new(&ScriptDocCategory::Triggers, "triggers", "Triggers"),
        ])
    }
}
