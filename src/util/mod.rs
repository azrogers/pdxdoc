use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use clauser::data::script_doc_parser::doc_string::{DocString, DocStringSegment};
use handlebars::html_escape;
use itertools::Itertools;
use log::warn;
use serde::{ser, Serialize};
use syntax_highlight::SyntaxHighlighter;

use crate::config::PaginationMode;
use crate::generator::SiteMapper;
use crate::page::Page;

use anyhow::{Error, Result};

mod syntax_highlight;

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

pub fn paginate<T, P, F>(mode: &PaginationMode, iter: &[T], mut to_page: F) -> Vec<P>
where
    F: FnMut(&[T]) -> P,
    P: Page,
{
    match mode {
        PaginationMode::None => std::iter::once(to_page(iter)).collect_vec(),
        PaginationMode::Absolute { limit } => iter.chunks(*limit).map(to_page).collect_vec(),
    }
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
        in_para: &mut bool,
        segment: &DocStringSegment,
    ) -> Result<(), Error> {
        match segment {
            DocStringSegment::Code { .. } | DocStringSegment::RawCode { .. } => {
                if *in_para {
                    *in_para = false;
                    s.push_str("</p>");
                }
            }
            _ => {
                if !*in_para {
                    *in_para = true;
                    s.push_str("<p>");
                }
            }
        }

        match segment {
            DocStringSegment::Text { contents } => Ok(s.push_str(contents)),
            DocStringSegment::Code { contents } => SyntaxHighlighter::to_html(s, contents),
            DocStringSegment::RawCode { contents } => {
                if *in_para {
                    *in_para = false;
                    s.push_str("</p>");
                }
                Ok(s.push_str(&format!("<div class=\"pd-raw-code\">{}</div>", contents)))
            }
            DocStringSegment::Symbol { identifier, .. } => {
                warn!("Symbols aren't yet properly handled: {}", identifier);
                Ok(s.push_str(&format!(
                    "<span class=\"pd-symbol-missing\">[symbol: {}]</span>",
                    html_escape(identifier)
                )))
            }
            DocStringSegment::Concept { identifier } => {
                warn!("Concepts aren't yet properly handled: {}", identifier);
                Ok(s.push_str(&format!(
                    "<span class=\"pd-concept-missing\">[{}]</span>",
                    html_escape(identifier)
                )))
            }
            DocStringSegment::Link { contents, url } => {
                Ok(s.push_str(&format!("<a href=\"{}\">{}</a>", url, contents)))
            }
        }?;

        Ok(())
    }

    pub fn to_html(&self) -> Result<String, Error> {
        let mut s = String::new();
        let mut in_para = false;
        for segment in self.0.segments() {
            DocStringSer::segment_to_html(self.1, self.2.clone(), &mut s, &mut in_para, segment)?;
        }
        Ok(s)
    }
}
