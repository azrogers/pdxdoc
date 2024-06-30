use std::{collections::HashMap, rc::Rc};

use clauser::data::script_doc_parser::doc_string::DocString;
use serde::Serialize;
use serde_json::value::RawValue;

use crate::{
    dossier::{CollatedCrossReferences, DocCategory, DocEntry, Dossier},
    util::DocStringSer,
};

pub struct PageContext {}

impl PageContext {
    pub fn url_for_entry(&self, entry: &dyn DocEntry) -> String {
        todo!()
    }
}

pub enum Template {
    List,
    Scope,
}

pub struct PageInfo {
    title: String,
    template: Template,
}

pub trait Page {
    fn info(&self) -> PageInfo;
    fn body(&self) -> DocString;
    fn data(&self, context: &PageContext) -> serde_json::Value;
}

pub struct CategoryListPage {
    category: DocCategory,
    dossier: Rc<Dossier>,
}

impl CategoryListPage {
    pub fn new(category: DocCategory, dossier: Rc<Dossier>) -> CategoryListPage {
        CategoryListPage { category, dossier }
    }
}

impl Page for CategoryListPage {
    fn info(&self) -> PageInfo {
        PageInfo {
            title: self.category.display_name.clone(),
            template: Template::List,
        }
    }

    fn body(&self) -> DocString {
        DocString::new_text("todo")
    }

    fn data(&self, context: &PageContext) -> serde_json::Value {
        #[derive(Serialize)]
        struct Entry {
            name: String,
            body: Option<DocStringSer>,
            properties: Vec<(String, DocStringSer)>,
            cross_refs: CollatedCrossReferences,
        }

        #[derive(Serialize)]
        struct Data {
            entries: Vec<Entry>,
        }

        let mut entries = Vec::new();
        for entry in &self.category.entries {
            let entry = self.dossier.entries.get(&entry).unwrap();
            let mut properties = entry.properties(context, &self.dossier);
            let body = entry.body();
            entries.push(Entry {
                name: entry.name().to_owned(),
                body: body.and_then(|d| Some(DocStringSer(d, self.dossier.clone()))),
                properties: properties
                    .drain(..)
                    .map(|(name, val)| (name, DocStringSer(val, self.dossier.clone())))
                    .collect(),
                cross_refs: Dossier::collate_references(self.dossier.clone(), context, entry.id()),
            });
        }

        serde_json::to_value(Data { entries }).unwrap()
    }
}
