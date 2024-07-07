use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    path::{Component, Path, PathBuf},
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, Profile, UrlScheme},
    generator::SiteProfile,
    page::{Page, PaginationInfo},
    util,
};

#[derive(Clone, Debug)]
pub struct SiteMapperPath {
    pub disk: PathBuf,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SiteMap {
    pub title: String,
    pub key: String,
    pub page_ids: Vec<u64>,
    pub absolute_url: String,
    pub children: HashMap<String, RefCell<SiteMap>>,
    pub page: Option<PaginationInfo>,
}

impl SiteMap {
    pub fn from_pages(profile: &SiteProfile) -> SiteMap {
        let mut map: SiteMap = SiteMap {
            title: profile.profile.title.clone(),
            key: String::new(),
            absolute_url: "/index.html".into(),
            children: HashMap::new(),
            page: None,
            page_ids: Vec::new(),
        };

        for page in &profile.pages {
            let p = PathBuf::from(page.info().path);
            let components = p.components().collect_vec();
            map.fill_from_path(components.as_slice(), page.as_ref());
        }

        map
    }

    fn fill_from_path(&mut self, components: &[Component], page: &dyn Page) {
        if components.is_empty() {
            let info = page.info();
            self.title = info.short_title;
            self.page = info.pagination.clone();
            self.page_ids.push(page.id());
            let mut url = PathBuf::from(match info.pagination {
                Some(pagination) if pagination.total_pages > 1 => page.page_url(1).clone(),
                _ => info.path.clone(),
            });
            url.set_extension("html");
            self.absolute_url = url.to_str().unwrap().to_string();
            return;
        }

        self.page_ids.push(page.id());

        let first = components.first().unwrap();
        let first = first.as_os_str().to_str().unwrap().to_string();
        // this is an index - let's actually apply its properties here instead
        if first.starts_with("index.") {
            self.fill_from_path(&components[1..], page);
            return;
        }

        if let Some(prev) = self.children.get(&first) {
            prev.borrow_mut().fill_from_path(&components[1..], page);
            return;
        }

        let mut map = SiteMap {
            key: first.clone(),
            title: String::new(),
            absolute_url: String::new(),
            children: HashMap::new(),
            page: None,
            page_ids: vec![page.id()],
        };

        map.fill_from_path(&components[1..], page);
        self.children.insert(first, map.into());
    }
}

pub struct SiteMapper {
    pub page_paths: HashMap<u64, SiteMapperPath>,
    pub groups: HashMap<u64, Vec<(usize, u64)>>,
    pub page_groups: HashMap<u64, u64>,
    entry_anchors: HashMap<u64, String>,
    /// Maps each entry ID to a page ID
    entry_pages: HashMap<u64, u64>,
    config: Config,

    page_profiles: HashMap<u64, u64>,
    profiles: HashMap<u64, Profile>,
}

impl SiteMapper {
    pub fn new(config: Config) -> SiteMapper {
        SiteMapper {
            page_paths: HashMap::new(),
            entry_anchors: HashMap::new(),
            entry_pages: HashMap::new(),
            config,
            page_profiles: HashMap::new(),
            profiles: HashMap::new(),
            page_groups: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    pub fn page_path_mapping(&self) -> HashMap<u64, String> {
        self.page_paths
            .iter()
            .map(|(p, path)| (*p, path.path.clone()))
            .collect()
    }

    pub fn asset_url(&self, from_id: u64, item: &str) -> String {
        Self::url_from(
            &PathBuf::from(&self.page_paths.get(&from_id).unwrap().path),
            &PathBuf::from("/assets").join(item),
        )
    }

    pub fn asset_path(&self, item: &str) -> PathBuf {
        self.config
            .output_dir
            .clone()
            .join("assets")
            .join(item)
            .to_owned()
    }

    pub fn page_to_entry_url(&self, from_page: &u64, to_entry: &u64) -> String {
        Self::url_from(
            &PathBuf::from(&self.page_paths.get(&from_page).unwrap().path),
            &PathBuf::from(
                &self
                    .page_paths
                    .get(self.entry_pages.get(to_entry).unwrap())
                    .unwrap()
                    .path,
            ),
        )
    }

    pub fn asset_url_with_mapping(
        mapping: &HashMap<u64, String>,
        from_id: u64,
        item: &str,
    ) -> String {
        Self::url_from(
            &PathBuf::from(&mapping.get(&from_id).unwrap()),
            &PathBuf::from("/assets").join(item),
        )
    }

    pub fn url_with_mapping(mapping: &HashMap<u64, String>, from_id: u64, item: &str) -> String {
        Self::url_from(
            &PathBuf::from(&mapping.get(&from_id).unwrap()),
            &PathBuf::from(item),
        )
    }

    pub fn record_profile(&mut self, p: &SiteProfile) {
        let profile_id = util::hash(&p.profile.name);
        self.profiles.insert(profile_id, p.profile.clone());

        for page in &p.pages {
            let info = page.info();
            let page_id = page.id();
            let mut path = PathBuf::new();
            if self.config.profiles.len() > 1 || self.config.use_subfolder_for_single_profile {
                path.push(&p.profile.name);
            }
            path.push(info.path);
            path.set_extension("html");

            let url = path.to_str().unwrap();
            let disk = self.config.output_dir.clone().join(&path);
            self.page_paths.insert(
                page_id,
                SiteMapperPath {
                    disk,
                    path: url.to_owned(),
                },
            );

            if let Some(pagination) = info.pagination {
                let group_id = page.group_id();
                self.page_groups.insert(page_id, group_id);
                let group = match self.groups.entry(group_id) {
                    Entry::Occupied(entries) => entries.into_mut(),
                    Entry::Vacant(v) => v.insert(Vec::new()),
                };

                group.push((pagination.current_page, page_id));
            }

            for id in page.entries() {
                self.entry_pages.insert(id, page_id);
            }

            for (id, anchor) in page.anchors() {
                self.entry_anchors.insert(id, anchor);
            }

            self.page_profiles.insert(page_id, profile_id);
        }
    }

    pub fn url_for_entry(&self, from_id: u64, to_id: u64) -> String {
        let to_path = self
            .page_paths
            .get(self.entry_pages.get(&to_id).unwrap())
            .unwrap();

        let url = match &self.config.url_scheme {
            UrlScheme::Relative => {
                let from_path = self
                    .page_paths
                    .get(self.entry_pages.get(&from_id).unwrap())
                    .unwrap();
                // diff the two paths to generate a relative URL
                let to_path = PathBuf::from(&to_path.path);
                Self::url_from(&PathBuf::from(&from_path.path), &to_path)
            }
            UrlScheme::Absolute { base_url } => format!("{}{}", &base_url, &to_path.path),
        };

        match self.entry_anchors.get(&to_id) {
            Some(anchor) => format!("{}#{}", url, anchor),
            None => url,
        }
    }

    fn url_from(source: &Path, dest: &Path) -> String {
        let filename = dest.file_name();
        let source = source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(PathBuf::from(""));
        let dest = dest
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(PathBuf::from(""));
        let mut diff = pathdiff::diff_paths(dest, source).unwrap();
        if let Some(filename) = filename {
            diff = diff.join(filename);
        }
        diff.to_str().unwrap().replace("\\", "/")
    }
}
