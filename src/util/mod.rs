use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use clauser::data::script_doc_parser::doc_string::{DocString, DocStringSegment};
use log::warn;
use serde::{ser, Serialize};
use syntax_highlight::SyntaxHighlighter;

use crate::dossier::Dossier;
use crate::generator::SiteMapper;

use std::fs::{self, File};

use anyhow::{Error, Result};
use image::load_from_memory_with_format;

mod game_asset;
mod icons;
mod syntax_highlight;

pub use game_asset::*;
pub use icons::IconFinder;

pub fn hash<T: Hash>(item: &T) -> u64 {
    let mut s = DefaultHasher::default();
    item.hash(&mut s);
    s.finish()
}

pub fn humanize_camel_case(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    let mut make_upper = true;
    for c in text.chars() {
        if make_upper {
            s.push_str(&c.to_uppercase().to_string());
            make_upper = false;
        } else if c == '_' {
            s.push(' ');
            make_upper = true;
        } else {
            s.push(c);
        }
    }

    s
}

pub struct DocStringSer(pub DocString, pub u64, pub Rc<RefCell<SiteMapper>>);

impl Serialize for DocStringSer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = self
            .to_html()
            .map_err(|e| <S::Error as ser::Error>::custom(format!("{:?}", e)))?;
        serializer.serialize_str(&s)
    }
}

impl DocStringSer {
    fn segment_to_html(
        page_id: u64,
        mapper: Rc<RefCell<SiteMapper>>,
        s: &mut String,
        segment: &DocStringSegment,
    ) -> Result<(), Error> {
        match segment {
            DocStringSegment::Text { contents } => Ok(s.push_str(&contents)),
            DocStringSegment::Code { contents } => SyntaxHighlighter::to_html(s, contents),
            DocStringSegment::RawCode { contents } => {
                Ok(s.push_str(&format!("<div class=\"clcode\">{}</div>", contents)))
            }
            DocStringSegment::Symbol {
                identifier,
                namespace,
            } => {
                let image_url =
                    mapper
                        .borrow_mut()
                        .url_for_icon(identifier, Some(namespace), page_id);
                let escaped = handlebars::html_escape(&identifier);
                let new = match image_url {
                    Some(image_url) => format!(
						"<img src=\"{image_url}\" alt=\"{escaped}\" title=\"{escaped}\" class=\"symbol-inline\" />"
					),
                    None => {
                        warn!("don't know how to find icon {}", identifier);
                        format!("[icon: {escaped}]")
                    }
                };
                Ok(s.push_str(&new))
            }
            DocStringSegment::Concept { identifier } => {
                warn!(
                    "Don't know how to handle concepts yet! Ignoring [{}]",
                    identifier
                );
                Ok(s.push_str(&identifier))
            }
            DocStringSegment::Link { contents, url } => {
                Ok(s.push_str(&format!("<a href=\"{}\">{}</a>", url, contents)))
            }
        }?;

        Ok(())
    }

    pub fn to_html(&self) -> Result<String, Error> {
        let mut s = String::new();
        for segment in self.0.segments() {
            DocStringSer::segment_to_html(self.1, self.2.clone(), &mut s, segment)?;
        }
        Ok(s)
    }
}
