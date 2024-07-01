use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    rc::Rc,
};

use handlebars::Handlebars;
use itertools::Itertools;
use log::info;
use serde::Serialize;
use serde_json::Value;

use crate::{
    config::{Config, Profile, UrlScheme},
    dossier::Dossier,
    error::Error,
    page::{Page, PageContext, Template},
    theme::Theme,
};

struct SiteProfile {
    profile: Profile,
    dossier: Rc<Dossier>,
    pages: Vec<Box<dyn Page>>,
}

impl SiteProfile {
    pub fn new(profile: Profile, dossier: Dossier) -> SiteProfile {
        let dossier = Rc::new(dossier);
        let pages = Dossier::create_pages(dossier.clone());

        SiteProfile {
            profile,
            dossier,
            pages,
        }
    }
}

struct SiteMapperPath {
    disk: PathBuf,
    path: String,
}

pub struct SiteMapper<'config> {
    page_paths: HashMap<u64, SiteMapperPath>,
    entry_anchors: HashMap<u64, String>,
    /// Maps each entry ID to a page ID
    entry_pages: HashMap<u64, u64>,
    config: &'config Config,
}

impl<'config> SiteMapper<'config> {
    pub fn new(config: &'config Config) -> SiteMapper {
        SiteMapper {
            page_paths: HashMap::new(),
            entry_anchors: HashMap::new(),
            entry_pages: HashMap::new(),
            config,
        }
    }

    fn record_profile(&mut self, p: &SiteProfile) {
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

            for id in page.entries() {
                self.entry_pages.insert(id, page_id);
            }

            for (id, anchor) in page.anchors() {
                self.entry_anchors.insert(id, anchor);
            }
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
                let a = PathBuf::from(&from_path.path).parent().unwrap().to_owned();
                let b = to_path.parent().unwrap().to_owned();
                let diff = pathdiff::diff_paths(b, a)
                    .unwrap()
                    .join(&to_path.file_name().unwrap());
                diff.to_str().unwrap().replace("\\", "/")
            }
            UrlScheme::Absolute { base_url } => format!("{}{}", &base_url, &to_path.path),
        };

        match self.entry_anchors.get(&to_id) {
            Some(anchor) => format!("{}#{}", url, anchor),
            None => url,
        }
    }
}

pub struct SiteGenerator<'config> {
    profiles: Vec<SiteProfile>,
    mapper: SiteMapper<'config>,
    config: &'config Config,
}

impl<'config> SiteGenerator<'config> {
    pub fn new(config: &'config Config) -> SiteGenerator<'config> {
        SiteGenerator {
            profiles: Vec::new(),
            config,
            mapper: SiteMapper::new(&config),
        }
    }

    pub fn add_profile(&mut self, profile: Profile, dossier: Dossier) {
        let profile = SiteProfile::new(profile, dossier);
        self.mapper.record_profile(&profile);
        self.profiles.push(profile)
    }

    pub fn generate<'t>(&self, theme: &'t dyn Theme<'t>) -> Result<(), Error> {
        let mut handlebars = Handlebars::new();

        handlebars.register_partial("layout", theme.str_for_template(Template::Layout)?)?;

        let templates: Vec<Template> = self
            .profiles
            .iter()
            .flat_map(|p| p.pages.iter())
            .map(|p| p.info().template)
            .unique()
            .collect();

        // make sure all templates are compiled before going through each page
        for template in templates {
            handlebars
                .register_template_string(template.into(), theme.str_for_template(template)?)?;
        }

        #[derive(Serialize)]
        struct PageData {
            title: String,
            name: String,
            data: Value,
        }

        let context = PageContext::new(&self.mapper);

        for p in &self.profiles {
            for page in &p.pages {
                let info = page.info();
                let title = format!("{} | {}", &info.title, &p.profile.title);
                let name = info.title.clone();
                let data = PageData {
                    title,
                    name,
                    data: page.data(&context),
                };

                let rendered = handlebars.render(info.template.into(), &data)?;
                /*let minified = String::from_utf8(minify_html::minify(
                    rendered.as_bytes(),
                    &Cfg::spec_compliant(),
                ))
                .unwrap();*/

                let path = self.mapper.page_paths.get(&page.id()).unwrap();
                if let Some(dir) = path.disk.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&path.disk, rendered)?;

                info!(
                    "rendered page {} to {}",
                    info.title,
                    path.disk.to_str().unwrap()
                );
            }
        }

        Ok(())
    }
}
