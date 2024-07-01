use std::collections::HashMap;

use crate::{error::Error, page::Template};

pub trait Theme<'t> {
    fn str_for_template(&self, template: Template) -> Result<&'t str, Error>;
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

pub struct DefaultTheme {}

impl DefaultTheme {
    pub fn new() -> DefaultTheme {
        DefaultTheme {}
    }
}

impl<'t> Theme<'t> for DefaultTheme {
    fn str_for_template(&self, template: Template) -> Result<&'t str, Error> {
        for (t, str) in &DEFAULT_TEMPLATES {
            if template == *t {
                return Ok(str);
            }
        }

        return Err(Error::Generation(format!(
            "Failed to find template for {:?}",
            template
        )));
    }
}
