#![feature(adt_const_params)]
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::Result;
use clauser::string_table::StringTable;
use config::{Config, Profile, ProfileGame};
use dossier::{DocInfo, Dossier};
use error::Error;
use games::GameDocProvider;
use generator::SiteGenerator;
use itertools::Itertools;
use log::info;
use mapper::SiteMapper;
use page::{GenericListPageBuilder, MaskPage, ScopePage};
use theme::PackagedTheme;

mod config;
mod dossier;
mod entry;
mod error;
mod games;
mod generator;
mod helpers;
mod mapper;
mod page;
mod theme;
mod util;

fn process_profile(
    profile: &Profile,
    config: &Config,
    mapper: Rc<RefCell<SiteMapper>>,
) -> Result<Rc<Dossier>> {
    info!("processing profile {}", profile.name);

    let provider = games::provider_for_game(&profile.game);
    let version = provider.read_version_info(profile)?;
    info!(
        "found {:?} version {}",
        profile.game, version.version_number
    );

    info!("parsing script docs");
    let mut script_docs = provider.read_script_docs(&profile)?;

    let entries = match script_docs.as_mut() {
        Some(docs) => docs.entries.drain().map(|(_, v)| v).collect(),
        None => vec![],
    };

    let scopes = script_docs
        .as_ref()
        .map(|docs| docs.scopes())
        .unwrap_or(vec![]);
    let masks = script_docs
        .as_ref()
        .map(|docs| docs.masks())
        .unwrap_or(vec![]);

    let string_table = match script_docs {
        Some(docs) => docs.string_table,
        None => StringTable::new(),
    };

    let mut dossier = Dossier::new(
        config.clone(),
        provider.get_categories(profile)?,
        string_table,
        DocInfo::new(profile, version),
        mapper,
    );

    dossier.add_entries(entries.into_iter())?;
    info!("collected {} entries", dossier.entries.len());

    dossier.add_builder(GenericListPageBuilder::<ScopePage>::new(scopes));
    dossier.add_builder(GenericListPageBuilder::<MaskPage>::new(masks));

    Ok(Rc::new(dossier))
}

fn main() -> Result<()> {
    colog::init();

    let config = Config::create(&PathBuf::from("config.json"))?;
    let theme = PackagedTheme::new(&PathBuf::from(format!(
        "{}/themes/default",
        env!("CARGO_MANIFEST_DIR")
    )))?;

    let mut generator = SiteGenerator::new(&config);
    for profile in &config.profiles {
        let dossier = process_profile(profile, &config, generator.mapper.clone())?;
        generator.add_profile(profile.clone(), dossier);
    }

    generator.generate(&theme)?;

    Ok(())
}
