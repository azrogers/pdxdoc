use std::collections::HashMap;

use anyhow::Error;
use anyhow::Result;
use grass::Options;

use crate::{config::Config, page::Template};

pub trait Theme<'t> {
    fn str_for_template(&self, template: Template) -> Result<&'t str>;
    fn assets(&self) -> Vec<(String, Vec<u8>)>;
}

const DEFAULT_TEMPLATES: [(Template, &'static str); 3] = [
    (
        Template::Layout,
        include_str!("../themes/default/layout.hbs"),
    ),
    (
        Template::CategoryList,
        include_str!("../themes/default/category_list.hbs"),
    ),
    (Template::Scope, include_str!("../themes/default/scope.hbs")),
];

const STYLESHEET: &'static str = include_str!("../themes/default/assets/style.scss");

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

        return Err(Error::msg(format!(
            "Failed to find template for {:?}",
            template
        )));
    }

    fn assets(&self) -> Vec<(String, Vec<u8>)> {
        self.assets
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }
}
