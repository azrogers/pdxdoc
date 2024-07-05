use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Mutex,
};

use anyhow::Result;
use handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, JsonRender, Output, RenderContext,
    RenderError, RenderErrorReason,
};
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
    util::{self, AssetSizeMode, GameAssets, IconFinder, RequestedAsset},
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

    // A list of game assets that we need to render the page,
    requested_assets: Vec<RequestedAsset>,
    page_profiles: HashMap<u64, u64>,
    profiles: HashMap<u64, Profile>,
    icons: HashMap<u64, IconFinder>,
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
            requested_assets: Vec::new(),
            icons: HashMap::new(),
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

    pub fn url_for_icon(
        &mut self,
        icon: &str,
        namespace: Option<&str>,
        from_page: u64,
    ) -> Option<String> {
        let from_path = self.page_paths.get(&from_page)?;
        let profile_id = self.page_profiles.get(&from_page)?;
        let icons = self.icons.get(&profile_id)?;
        let icon_path = icons.find(icon, namespace)?;

        // We already have this asset listed, let's not do it again
        if let Some(prev) = self
            .requested_assets
            .iter()
            .filter(|i| i.source == icon_path)
            .next()
        {
            return Some(Self::url_from(
                &PathBuf::from(&from_path.path),
                &PathBuf::from(&prev.target_url),
            ));
        }

        let dest_path = PathBuf::from("icons/")
            .join(&GameAssets::new_filename_for_asset(&icon_path)?.file_name()?);

        let root_url = self.asset_path(dest_path.to_str()?);
        let target_url = Self::url_from(&PathBuf::from(&from_path.path), &root_url);

        self.requested_assets.push(RequestedAsset {
            target_url: target_url.clone(),
            source: icon_path.to_path_buf(),
            size_mode: AssetSizeMode::MaxDimension(64),
        });

        Some(target_url)
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

        let finder = IconFinder::new(&PathBuf::from(&p.profile.game_data_dir)).unwrap();
        self.icons.insert(profile_id, finder);
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
        let diff = pathdiff::diff_paths(dest, source).unwrap().join(&filename);
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
        let profile = SiteProfile::new(profile, dossier);
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
                hb: &Handlebars,
                context: &Context,
                rc: &mut RenderContext,
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

        for asset in &self.mapper.borrow().requested_assets {
            GameAssets::convert_image(&asset, &assets_dir)?;
            info!("wrote asset {}", asset.target_url);
        }

        info!("generated to {}", self.config.output_dir.to_str().unwrap());

        Ok(())
    }
}
