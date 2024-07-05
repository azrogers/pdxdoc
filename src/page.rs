use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use clauser::data::script_doc_parser::doc_string::DocString;
use serde::Serialize;
use serde_json::{value::RawValue, Value};

use crate::{
    config::Config,
    dossier::{CollatedCrossReferences, DocCategory, Dossier},
    entry::{DocEntry, ScopeDocEntry},
    generator::SiteMapper,
    util::{self, DocStringSer},
};

pub struct PageContext {
    mapper: Rc<RefCell<SiteMapper>>,
}

impl PageContext {
    pub fn new(mapper: Rc<RefCell<SiteMapper>>) -> PageContext {
        PageContext {
            mapper: mapper.clone(),
        }
    }

    pub fn url_for_entry(&self, from: &dyn DocEntry, entry: &dyn DocEntry) -> String {
        self.mapper.borrow().url_for_entry(from.id(), entry.id())
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
#[repr(u8)]
pub enum Template {
    Layout,
    CategoryList,
    Scope,
}

impl From<Template> for &str {
    fn from(value: Template) -> Self {
        match value {
            Template::Layout => "layout",
            Template::CategoryList => "category_list",
            Template::Scope => "scope",
        }
    }
}

pub struct PageInfo {
    pub title: String,
    pub template: Template,
    pub path: String,
}

pub trait Page {
    fn id(&self) -> u64;
    fn info(&self) -> PageInfo;
    fn entries(&self) -> Vec<u64>;
    fn body(&self) -> DocString;
    /// All of the anchors (destinations that can be reached with the URL hash) on this page, and their entry ID.
    fn anchors(&self) -> Vec<(u64, String)>;
    fn data(&self, context: &PageContext) -> serde_json::Value;
}

pub struct CategoryListPage {
    category: DocCategory,
    dossier: Rc<Dossier>,
    entries: Vec<u64>,
}

impl CategoryListPage {
    pub fn new(category: DocCategory, dossier: Rc<Dossier>) -> CategoryListPage {
        let mut entries = category.entries.clone();
        entries.sort_by_key(|f| dossier.entries.get(f).unwrap().name());
        CategoryListPage {
            category,
            dossier,
            entries,
        }
    }
}

impl Page for CategoryListPage {
    fn info(&self) -> PageInfo {
        PageInfo {
            title: self.category.display_name.clone(),
            template: Template::CategoryList,
            path: self.category.name.clone(),
        }
    }

    fn body(&self) -> DocString {
        DocString::new_text("todo", None)
    }

    fn entries(&self) -> Vec<u64> {
        self.entries.clone()
    }

    fn data(&self, context: &PageContext) -> serde_json::Value {
        #[derive(Serialize)]
        struct Property {
            name: String,
            value: DocStringSer,
        }

        #[derive(Serialize)]
        struct Entry {
            anchor: String,
            name: String,
            body: Option<DocStringSer>,
            properties: Vec<Property>,
            cross_refs: CollatedCrossReferences,
        }

        #[derive(Serialize)]
        struct Data {
            body: DocStringSer,
            entries: Vec<Entry>,
        }

        let mut entries = Vec::new();
        for entry in &self.entries {
            let entry = self.dossier.entries.get(&entry).unwrap();
            let mut properties = entry.properties(context, self.dossier.clone());
            let body = entry.body();
            entries.push(Entry {
                anchor: entry.name().to_owned(),
                name: entry.name().to_owned(),
                body: body.and_then(|d| Some(DocStringSer(d, self.id(), context.mapper.clone()))),
                properties: properties
                    .drain(..)
                    .map(|(name, val)| Property {
                        name,
                        value: DocStringSer(val, self.id(), context.mapper.clone()),
                    })
                    .collect(),
                cross_refs: Dossier::collate_references(
                    self.dossier.clone(),
                    context,
                    self.id(),
                    entry.id(),
                ),
            });
        }

        serde_json::to_value(Data {
            body: DocStringSer(DocString::new(), self.id(), context.mapper.clone()),
            entries,
        })
        .unwrap()
    }

    fn anchors(&self) -> Vec<(u64, String)> {
        self.category
            .entries
            .iter()
            .map(|id| {
                (
                    *id,
                    self.dossier.entries.get(&id).unwrap().name().to_owned(),
                )
            })
            .collect()
    }

    fn id(&self) -> u64 {
        util::hash(&self.category)
    }
}

pub struct ScopePage {
    dossier: Rc<Dossier>,
    entry: ScopeDocEntry,
}

impl ScopePage {
    pub fn new(entry: ScopeDocEntry, dossier: Rc<Dossier>) -> ScopePage {
        ScopePage { entry, dossier }
    }
}

impl Page for ScopePage {
    fn id(&self) -> u64 {
        util::hash(&self.entry)
    }

    fn info(&self) -> PageInfo {
        PageInfo {
            title: format!("{} Scope", self.entry.display_name),
            path: format!("scopes/{}", self.entry.name),
            template: Template::Scope,
        }
    }

    fn entries(&self) -> Vec<u64> {
        vec![self.entry.id]
    }

    fn body(&self) -> DocString {
        DocString::new_text("todo", None)
    }

    fn anchors(&self) -> Vec<(u64, String)> {
        vec![]
    }

    fn data(&self, context: &PageContext) -> serde_json::Value {
        let refs =
            Dossier::collate_references(self.dossier.clone(), context, self.id(), self.entry.id);

        #[derive(Serialize)]
        struct Data {
            cross_refs: CollatedCrossReferences,
        }

        serde_json::to_value(Data { cross_refs: refs }).unwrap()
    }
}
