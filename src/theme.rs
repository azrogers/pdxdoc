use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Error;
use anyhow::Result;
use grass::Options;
use itertools::Itertools;
use serde::Deserialize;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Deserialize)]
#[repr(u8)]
pub enum Template {
    #[serde(rename = "category_list")]
    CategoryList,
    #[serde(rename = "list_index")]
    ListIndex,
    #[serde(rename = "scope")]
    Scope,
    #[serde(rename = "mask")]
    Mask,
}

impl From<Template> for &str {
    fn from(value: Template) -> Self {
        match value {
            Template::CategoryList => "category_list",
            Template::Scope => "scope",
            Template::Mask => "mask",
            Template::ListIndex => "list_index",
        }
    }
}

impl From<&str> for Template {
    fn from(value: &str) -> Self {
        match value {
            "category_list" => Template::CategoryList,
            "scope" => Template::Scope,
            "mask" => Template::Mask,
            "list_index" => Template::ListIndex,
            _ => panic!(),
        }
    }
}

pub trait Theme<'t> {
    fn str_for_template(&'t self, template: Template) -> Result<&'t str>;
    fn partials(&'t self) -> Vec<(&'t str, &'t str)>;
    fn assets(&'t self) -> &'t Vec<(String, Vec<u8>)>;
}

/*
const DEFAULT_TEMPLATES: [(Template, &str); 6] = [
    (
        Template::Layout,
        include_str!("../themes/default/layout.hbs"),
    ),
    (
        Template::CategoryList,
        include_str!("../themes/default/category_list.hbs"),
    ),
    (Template::Scope, include_str!("../themes/default/scope.hbs")),
    (Template::Mask, include_str!("../themes/default/mask.hbs")),
    (
        Template::Pagination,
        include_str!("../themes/default/pagination.hbs"),
    ),
    (
        Template::ListIndex,
        include_str!("../themes/default/list_index.hbs"),
    ),
];

const STYLESHEET: &str = include_str!("../themes/default/assets/style.scss");

pub struct DefaultTheme {
    assets: Vec<(&'static str, Vec<u8>)>,
}

impl DefaultTheme {
    pub fn new() -> DefaultTheme {
        let options = Options::default();
        let stylesheet = grass::from_string(STYLESHEET, &options).unwrap();
        DefaultTheme {
            assets: vec![("style.css", stylesheet.as_bytes().to_vec())],
        }
    }
}

impl<'t> Theme<'t> for DefaultTheme {
    fn str_for_template(&self, template: Template) -> Result<&'t str> {
        for (t, str) in &DEFAULT_TEMPLATES {
            if template == *t {
                return Ok(str);
            }
        }

        Err(Error::msg(format!(
            "Failed to find template for {:?}",
            template
        )))
    }

    fn assets(&self) -> Vec<(String, Vec<u8>)> {
        self.assets
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }
}*/

#[derive(Deserialize)]
#[serde(untagged)]
enum GlobOrKeys {
    SingleGlob(String),
    MultiGlob(Vec<String>),
    Keys(HashMap<String, String>),
}

impl GlobOrKeys {
    pub fn read(&self, dir: &Path) -> HashMap<String, String> {
        if let Self::Keys(keys) = self {
            return keys
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        fs::read_to_string(dir.to_path_buf().join(v)).unwrap(),
                    )
                })
                .collect();
        }

        let globs = match self {
            Self::SingleGlob(glob) => vec![glob],
            Self::MultiGlob(globs) => globs.iter().collect_vec(),
            _ => panic!(),
        };

        let files = globs
            .iter()
            .map(|g| wax::Glob::new(&g).unwrap())
            .flat_map(|g| g.walk(dir).collect_vec())
            .collect_vec();

        let mut table = HashMap::new();
        let parent_components_num = dir.components().count();
        for f in files {
            let f = f.unwrap();
            let child_components = f
                .path()
                .components()
                .skip(parent_components_num)
                .collect_vec();
            let child_path = PathBuf::from_iter(child_components.into_iter())
                .to_str()
                .unwrap()
                .to_string();
            table.insert(
                f.path()
                    .file_stem()
                    .and_then(|f| f.to_str())
                    .unwrap()
                    .to_string(),
                fs::read_to_string(f.path()).unwrap(),
            );
        }

        table
    }
}

#[derive(Deserialize)]
struct PackagedThemeManifest {
    name: String,
    assets: Vec<String>,
    templates: GlobOrKeys,
    partials: GlobOrKeys,
}

pub struct PackagedTheme {
    dir: PathBuf,
    manifest: PackagedThemeManifest,
    assets: Vec<(String, Vec<u8>)>,
    templates: HashMap<Template, String>,
    partials: HashMap<String, String>,
}

impl PackagedTheme {
    pub fn new(dir: &Path) -> anyhow::Result<PackagedTheme> {
        let manifest_path = dir.clone().join("theme.json");
        if !manifest_path.is_file() {
            return Err(Error::msg(format!("Can't find theme.json in {:?}", dir)));
        }

        let manifest =
            serde_json::from_str::<PackagedThemeManifest>(&fs::read_to_string(&manifest_path)?)?;

        let asset_files = manifest
            .assets
            .iter()
            .flat_map(|g| wax::Glob::new(&g).unwrap().walk(&dir).collect_vec())
            .map(|r| r.unwrap())
            .collect_vec();

        let mut assets = Vec::new();

        let options = Options::default();
        let parent_components_num = dir.components().count();
        for sass in asset_files
            .iter()
            .filter(|f| f.path().extension().map(|e| e == "scss").unwrap_or(false))
        {
            let compiled = grass::from_path(sass.path(), &options)?;
            let child_components = sass
                .path()
                .components()
                .skip(parent_components_num)
                .collect_vec();
            let mut child_path = PathBuf::from_iter(child_components.into_iter());
            child_path.set_extension("css");

            assets.push((
                child_path.to_str().unwrap().to_string(),
                compiled.as_bytes().to_vec(),
            ))
        }

        let templates = manifest
            .templates
            .read(dir)
            .into_iter()
            .map(|(k, v)| (Template::from(k.as_str()), v))
            .collect();

        let partials = manifest.partials.read(dir);

        Ok(PackagedTheme {
            dir: dir.to_path_buf(),
            manifest,
            assets,
            templates,
            partials,
        })
    }
}

impl<'t> Theme<'t> for PackagedTheme {
    fn str_for_template(&'t self, template: Template) -> Result<&'t str> {
        self.templates
            .get(&template)
            .map(|s| s.as_str())
            .ok_or(Error::msg(format!("Missing template {:?}", template)))
    }

    fn partials(&'t self) -> Vec<(&'t str, &'t str)> {
        self.partials
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_str()))
            .collect_vec()
    }

    fn assets(&'t self) -> &'t Vec<(String, Vec<u8>)> {
        &self.assets
    }
}
