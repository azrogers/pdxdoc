use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use wax::Glob;

pub struct IconFinder {
    icons: HashMap<u64, PathBuf>,
    icons_by_name: HashMap<String, u64>,
    icons_by_mask_name: HashMap<(String, String), u64>,
}

impl IconFinder {
    pub fn new(game_dir: &Path) -> Result<IconFinder> {
        let mut icons = HashMap::new();
        let mut icons_by_name = HashMap::new();
        let mut icons_by_mask_name = HashMap::new();

        let icons_path = game_dir.to_path_buf().join("game/gfx/interface/icons");
        let glob = Glob::new("{*_icons,topbar}/*.dds")?;
        for e in glob.walk(&icons_path) {
            let e = e?;
            let (name, mask) = Self::extract_from_path(e.path()).unwrap();
            let hash = super::hash(&name);

            icons.insert(hash, e.path().to_owned());
            icons_by_name.insert(name.to_owned(), hash);
            icons_by_mask_name.insert((mask.to_owned(), name.to_owned()), hash);
        }

        Ok(IconFinder {
            icons,
            icons_by_mask_name,
            icons_by_name,
        })
    }

    pub fn find<'p>(&'p self, name: &str, mask: Option<&str>) -> Option<&'p Path> {
        let name = match name {
            "aut" => "authority_icon",
            "bur" => "bureaucracy_icon",
            "inc" => "income_power_icon",
            "inf" => "influence_icon",
            "engines" => "locomotives",
            v => v,
        };

        let name = name.to_owned();
        if let Some(mask) = mask.map(|m| m.to_owned()) {
            let id = self.icons_by_mask_name.get(&(name.clone(), mask));
            if let Some(id) = id {
                return self.icons.get(id).map(|i| i.as_path());
            }
        }

        let id = self.icons_by_name.get(&name)?;
        return self.icons.get(id).map(|i| i.as_path());
    }

    fn extract_from_path<'a>(path: &'a Path) -> Option<(&'a str, &'a str)> {
        let name = path.file_stem()?.to_str()?;
        let mask = path.parent()?.file_stem()?.to_str()?.split('_').next()?;

        Some((name, mask))
    }
}
