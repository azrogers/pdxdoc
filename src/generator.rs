use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::Result;
use handlebars::Handlebars;
use itertools::Itertools;
use log::info;
use serde::Serialize;
use serde_json::Value;

use crate::{
    config::{Config, Profile, UrlScheme},
    dossier::{DocInfo, Dossier},
    helpers::{
        AssetHelper, BreadcrumbsHelper, ColumnsHelper, PageUrlHelper, PaginationHelper,
        SiteMapHelper,
    },
    mapper::{SiteMap, SiteMapper},
    page::{Breadcrumbs, Page, PageContext},
    theme::{Template, Theme},
    util,
};

pub struct SiteProfile {
    pub profile: Profile,
    pub dossier: Rc<Dossier>,
    pub pages: Vec<Box<dyn Page>>,
}

impl SiteProfile {
    pub fn new(config: &Config, profile: Profile, dossier: Rc<Dossier>) -> SiteProfile {
        let pages = Dossier::create_pages(dossier.clone(), config);

        SiteProfile {
            profile,
            dossier,
            pages,
        }
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

    pub fn add_profile(&mut self, profile: Profile, dossier: Rc<Dossier>) {
        let profile = SiteProfile::new(self.config, profile, dossier);
        self.mapper.borrow_mut().record_profile(&profile);
        self.profiles.push(profile)
    }

    pub fn generate<'t>(&self, theme: &'t dyn Theme<'t>) -> Result<()> {
        let mapping: HashMap<u64, String> = self.mapper.borrow().page_path_mapping();

        let mut handlebars = Handlebars::new();

        for (name, str) in theme.partials() {
            handlebars.register_partial(name, str)?;
        }

        handlebars.register_helper(
            "asset_url",
            Box::new(AssetHelper {
                mapper: mapping.clone(),
            }),
        );
        handlebars.register_helper(
            "page_url",
            Box::new(PageUrlHelper {
                mapping: mapping.clone(),
                page_to_groups: self.mapper.borrow().page_groups.clone(),
                groups_to_pages: self.mapper.borrow().groups.clone(),
            }),
        );
        handlebars.register_helper(
            "site_map",
            Box::new(SiteMapHelper {
                mapping: mapping.clone(),
            }),
        );
        handlebars.register_helper("breadcrumbs", Box::new(BreadcrumbsHelper { mapping }));
        handlebars.register_helper("pagination", Box::new(PaginationHelper));
        handlebars.register_helper("columns", Box::new(ColumnsHelper));

        handlebars_misc_helpers::register(&mut handlebars);

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
            breadcrumbs: Breadcrumbs,
            page_id: u64,
            site_map: SiteMap,
            doc_info: DocInfo,
        }

        let context = PageContext::new(self.mapper.clone());

        for p in &self.profiles {
            let site_map = SiteMap::from_pages(&p);

            for page in &p.pages {
                let info = page.info();
                let title = format!("{} | {}", &info.title, &p.profile.title);
                let name = info.title.clone();
                let data = PageData {
                    title,
                    name,
                    page_id: page.id(),
                    data: page.data(&context),
                    breadcrumbs: Breadcrumbs::from_page(page.as_ref(), p),
                    site_map: site_map.clone(),
                    doc_info: p.dossier.info.clone(),
                };

                let rendered = handlebars.render(info.template.into(), &data)?;
                let minified = html_minifier::minify(rendered).unwrap();

                let mapper = self.mapper.borrow();
                let path = mapper.page_paths.get(&page.id()).unwrap();
                if let Some(dir) = path.disk.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&path.disk, minified)?;

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
            let out_path = PathBuf::from(&self.config.output_dir).join(path);
            fs::write(&out_path, bytes)?;
            info!(
                "wrote asset {}",
                out_path.to_str().unwrap().replace("\\", "/")
            );
        }

        info!("generated to {}", self.config.output_dir.to_str().unwrap());

        Ok(())
    }
}
