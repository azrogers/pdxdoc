use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::Result;
use handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, Output, RenderContext, RenderErrorReason,
};
use itertools::Itertools;
use log::info;
use serde::Serialize;
use serde_json::Value;

use crate::{
    config::{Config, Profile, UrlScheme},
    dossier::Dossier,
    page::{Page, PageContext, Template},
    theme::Theme,
    util,
};

struct SiteProfile {
    profile: Profile,
    dossier: Rc<Dossier>,
    pages: Vec<Box<dyn Page>>,
}

impl SiteProfile {
    pub fn new(config: &Config, profile: Profile, dossier: Dossier) -> SiteProfile {
        let dossier = Rc::new(dossier);
        let pages = Dossier::create_pages(dossier.clone(), config);

        SiteProfile {
            profile,
            dossier,
            pages,
        }
    }
}

#[derive(Clone)]
struct SiteMapperPath {
    disk: PathBuf,
    path: String,
}

pub struct SiteMapper {
    page_paths: HashMap<u64, SiteMapperPath>,
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
        }
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

    fn record_profile(&mut self, p: &SiteProfile) {
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
        let filename = dest.file_name().unwrap();
        let source = source.parent().unwrap();
        let dest = dest.parent().unwrap();
        let diff = pathdiff::diff_paths(dest, source).unwrap().join(filename);
        diff.to_str().unwrap().replace("\\", "/")
    }
}

pub struct SiteGenerator<'config> {
    profiles: Vec<SiteProfile>,
    pub mapper: Rc<RefCell<SiteMapper>>,
    config: &'config Config,
}

impl<'config> SiteGenerator<'config> {
    pub fn new(config: &'config Config) -> SiteGenerator<'config> {
        SiteGenerator {
            profiles: Vec::new(),
            mapper: Rc::new(RefCell::new(SiteMapper::new(config.clone()))),
            config,
        }
    }

    pub fn add_profile(&mut self, profile: Profile, dossier: Dossier) {
        let profile = SiteProfile::new(&self.config, profile, dossier);
        self.mapper.borrow_mut().record_profile(&profile);
        self.profiles.push(profile)
    }

    pub fn generate<'t>(&self, theme: &'t dyn Theme<'t>) -> Result<()> {
        let mut handlebars = Handlebars::new();

        #[derive(Clone)]
        struct AssetHelper {
            mapper: HashMap<u64, String>,
        }

        impl HelperDef for AssetHelper {
            fn call<'reg: 'rc, 'rc>(
                &self,
                h: &Helper,
                _hb: &Handlebars,
                context: &Context,
                _rc: &mut RenderContext,
                out: &mut dyn Output,
            ) -> HelperResult {
                let asset = h.param(0).and_then(|v| v.value().as_str()).ok_or(
                    RenderErrorReason::ParamTypeMismatchForName(
                        "asset",
                        "0".to_string(),
                        "&str".to_string(),
                    ),
                )?;

                let page_id = context
                    .data()
                    .as_object()
                    .unwrap()
                    .get("page_id")
                    .unwrap()
                    .as_number()
                    .unwrap()
                    .as_u64()
                    .unwrap();

                out.write(&SiteMapper::asset_url_with_mapping(
                    &self.mapper,
                    page_id,
                    asset,
                ))?;
                Ok(())
            }
        }

        handlebars.register_partial("layout", theme.str_for_template(Template::Layout)?)?;
        handlebars.register_helper(
            "asset_url",
            Box::new(AssetHelper {
                mapper: self
                    .mapper
                    .borrow()
                    .page_paths
                    .iter()
                    .map(|(p, path)| (*p, path.path.clone()))
                    .collect(),
            }),
        );

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
            page_id: u64,
        }

        let context = PageContext::new(self.mapper.clone());

        for p in &self.profiles {
            for page in &p.pages {
                let info = page.info();
                let title = format!("{} | {}", &info.title, &p.profile.title);
                let name = info.title.clone();
                let data = PageData {
                    title,
                    name,
                    page_id: page.id(),
                    data: page.data(&context),
                };

                let rendered = handlebars.render(info.template.into(), &data)?;

                let mapper = self.mapper.borrow();
                let path = mapper.page_paths.get(&page.id()).unwrap();
                if let Some(dir) = path.disk.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&path.disk, rendered)?;

                info!(
                    "rendered page {} to {}",
                    info.title,
                    path.disk.to_str().unwrap().replace("\\", "/")
                );
            }
        }

        let assets_dir = PathBuf::from(&self.config.output_dir).join("assets");
        if !assets_dir.is_dir() {
            fs::create_dir(&assets_dir)?;
        }

        // write theme assets
        for (path, bytes) in theme.assets() {
            let out_path = assets_dir.clone().join(path);
            fs::write(&out_path, &bytes)?;
            info!(
                "wrote asset {}",
                out_path.to_str().unwrap().replace("\\", "/")
            );
        }

        info!("generated to {}", self.config.output_dir.to_str().unwrap());

        Ok(())
    }
}
