use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use clauser::data::script_doc_parser::doc_string::{DocString, DocStringSegment};
use clauser::error::Error as ClError;
use log::warn;
use serde::{ser, Serialize};
use syntax_highlight::SyntaxHighlighter;

use crate::{dossier::Dossier, error::Error};

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

pub struct DocStringSer(pub DocString, pub Rc<Dossier>);

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
        _dossier: &Dossier,
        s: &mut String,
        segment: &DocStringSegment,
    ) -> Result<(), Error> {
        match segment {
            DocStringSegment::Text { contents } => Ok(s.push_str(&contents)),
            DocStringSegment::Code { contents } => SyntaxHighlighter::to_html(s, contents),
            DocStringSegment::RawCode { contents } => {
                Ok(s.push_str(&format!("<code>{}</code>", contents)))
            }
            DocStringSegment::Symbol { identifier } => {
                warn!("Don't know how to handle symbols yet!");
                Ok(s.push_str(&identifier))
            }
            DocStringSegment::Concept { identifier } => {
                warn!("Don't know how to handle concepts yet!");
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
            DocStringSer::segment_to_html(&self.1, &mut s, segment)?;
        }
        Ok(s)
    }
}
