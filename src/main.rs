use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::Result;
use clauser::string_table::StringTable;
use config::{Config, Profile, ProfileGame};
use dossier::{DocInfo, Dossier};
use error::Error;
use games::GameDocProvider;
use generator::{SiteGenerator, SiteMapper};
use log::info;
use theme::DefaultTheme;

mod config;
mod dossier;
mod entry;
mod error;
mod games;
mod generator;
mod page;
mod theme;
mod util;

fn process_profile(profile: &Profile, mapper: Rc<RefCell<SiteMapper>>) -> Result<Dossier> {
    info!("processing profile {}", profile.name);

    let provider = games::provider_for_game(&profile.game);
    let version = provider.read_version_info(&profile)?;
    info!(
        "found {:?} version {}",
        profile.game, version.version_number
    );

    info!("parsing script docs");
    let mut script_docs = provider.read_script_docs(&profile)?;

    let scopes = match script_docs {
        Some(ref docs) => docs.scopes(),
        None => vec![],
    };

    let entries = match script_docs.as_mut() {
        Some(docs) => docs.entries.drain().map(|(_, v)| v).collect(),
        None => vec![],
    };

    let string_table = match script_docs {
        Some(docs) => docs.string_table,
        None => StringTable::new(),
    };

    let mut dossier = Dossier::new(
        profile,
        provider.get_categories(&profile)?,
        scopes,
        string_table,
        DocInfo::new(version),
        mapper,
    );

    dossier.add_entries(entries.into_iter())?;
    info!("collected {} entries", dossier.entries.len());

    Ok(dossier)
}

fn main() -> Result<()> {
    colog::init();

    let config = Config::create(&PathBuf::from("config.json"))?;

    let mut generator = SiteGenerator::new(&config);
    for profile in &config.profiles {
        let dossier = process_profile(profile, generator.mapper.clone())?;
        generator.add_profile(profile.clone(), dossier);
    }

    generator.generate(&DefaultTheme::new())?;

    Ok(())
}
