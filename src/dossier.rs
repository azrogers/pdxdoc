use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use anyhow::{Error, Result};
use clauser::{
    data::script_doc_parser::doc_string::{DocString, DocStringSegment},
    string_table::StringTable,
};
use log::warn;
use serde::Serialize;

use crate::{
    config::{Config, Profile},
    entry::{DocEntry, ScopeDocEntry},
    games::GameVersion,
    generator::SiteMapper,
    page::{CategoryListPage, Page, PageContext, ScopePage},
    util::{self, paginate, DocStringSer},
};

#[derive(Clone, Hash)]
pub struct DocCategory {
    id: u64,
    pub name: String,
    pub display_name: String,
    pub entries: Vec<u64>,
}

impl DocCategory {
    pub fn new<T: Hash>(id: &T, name: &str, display_name: &str) -> DocCategory {
        DocCategory {
            id: util::hash(id),
            name: name.to_string(),
            display_name: display_name.to_string(),
            entries: Vec::new(),
        }
    }
}

pub struct DocVersion {
    game: GameVersion,
    pdxdoc: String,
}

pub struct DocInfo {
    version: DocVersion,
}

impl DocInfo {
    pub fn new(game_version: GameVersion) -> DocInfo {
        DocInfo {
            version: DocVersion {
                game: game_version,
                pdxdoc: format!("pdxdoc {}", env!("CARGO_PKG_VERSION")),
            },
        }
    }
}

pub struct CrossReference {
    from_id: u64,
    from_property: String,
    to_id: u64,
}

#[derive(Serialize)]
pub struct CrossReferenceSection {
    name: String,
    body: DocStringSer,
}

#[derive(Serialize)]
pub struct CrossReferenceGroup {
    name: String,
    properties: Vec<CrossReferenceSection>,
}

/// A set of cross references for a single item
#[derive(Serialize)]
pub struct CollatedCrossReferences {
    groups: Vec<CrossReferenceGroup>,
}

/// The sum of information we've collected that we're trying to render into a set of documents.
pub struct Dossier {
    categories: HashMap<u64, DocCategory>,
    pub entries: HashMap<u64, Box<dyn DocEntry>>,
    scopes: Vec<u64>,
    string_table: StringTable,
    mapper: Rc<RefCell<SiteMapper>>,

    cross_references: Vec<CrossReference>,
    info: DocInfo,
}

impl Dossier {
    pub fn new(
        profile: &Profile,
        categories: impl IntoIterator<Item = DocCategory>,
        scopes: impl IntoIterator<Item = String>,
        string_table: StringTable,
        info: DocInfo,
        mapper: Rc<RefCell<SiteMapper>>,
    ) -> Dossier {
        let mut entries: HashMap<u64, Box<dyn DocEntry>> = HashMap::new();
        let mut scope_ids = Vec::new();
        for scope in scopes {
            let entry = ScopeDocEntry::new(scope);
            scope_ids.push(entry.id);
            entries.insert(entry.id, Box::new(entry));
        }

        Dossier {
            categories: categories.into_iter().map(|c| (c.id, c)).collect(),
            entries,
            info,
            cross_references: Vec::new(),
            scopes: scope_ids,
            string_table,
            mapper,
        }
    }

    pub fn add_entries<T>(&mut self, entries: impl Iterator<Item = T>) -> Result<()>
    where
        T: DocEntry + 'static,
    {
        for entry in entries {
            match self.categories.get_mut(&entry.category_id()) {
                Some(category) => Ok(category.entries.push(entry.id())),
                None => Err(Error::msg(
                    "Tried adding an entry with a category that doesn't exist?",
                )),
            }?;

            entry.record_cross_references(self);

            self.entries.insert(entry.id(), Box::new(entry));
        }

        Ok(())
    }

    pub fn create_pages(dossier: Rc<Dossier>, config: &Config) -> Vec<Box<dyn Page>> {
        let mut pages: Vec<Box<dyn Page>> = Vec::new();

        for category in dossier.categories.values() {
            let mut entries = category.entries.clone();
            entries.sort_by_key(|f| dossier.entries.get(f).unwrap().name());
            let mut page = 0;
            pages.extend(
                paginate(&config.pagination, entries.as_slice(), |entries| {
                    page += 1;
                    CategoryListPage::new(category.clone(), entries, page, dossier.clone())
                })
                .into_iter()
                .map(|p| Box::new(p) as Box<dyn Page>),
            );
        }

        for id in &dossier.scopes {
            let entry = dossier.entry_as::<ScopeDocEntry>(*id);
            pages.push(Box::new(ScopePage::new(entry.clone(), dossier.clone())));
        }

        pages
    }

    pub fn collate_references(
        dossier: Rc<Dossier>,
        context: &PageContext,
        page_id: u64,
        item: u64,
    ) -> CollatedCrossReferences {
        let entry = dossier.entries.get(&item).unwrap();

        let mut groups = HashMap::new();
        for CrossReference {
            ref from_id,
            ref from_property,
            ref to_id,
        } in dossier.cross_references.iter().filter(|c| c.to_id == item)
        {
            Dossier::add_ref_link(
                dossier.clone(),
                context,
                &mut groups,
                entry.as_ref(),
                *from_id,
                from_property,
            );
        }

        let mut collated = CollatedCrossReferences { groups: Vec::new() };

        let mut group_names: Vec<String> = groups.keys().map(|k| (*k).clone()).collect();
        group_names.sort();

        for name in group_names.drain(..) {
            let mut group = groups.remove(&name).unwrap();
            let mut property_names: Vec<String> = group.keys().map(|k| (*k).clone()).collect();
            property_names.sort();

            let mut properties = Vec::new();
            for prop in property_names.drain(..) {
                let mut items = group.remove(&prop).unwrap();
                items.sort();
                let s = DocString::new_from_iter(items.drain(..), Some(", "));
                properties.push(CrossReferenceSection {
                    name: prop,
                    body: DocStringSer(s, page_id, dossier.mapper.clone()),
                });
            }

            collated
                .groups
                .push(CrossReferenceGroup { name, properties });
        }

        collated
    }

    pub fn add_ref_link(
        dossier: Rc<Dossier>,
        context: &PageContext,
        groups: &mut HashMap<String, HashMap<String, Vec<DocStringSegment>>>,
        entry: &dyn DocEntry,
        other_id: u64,
        prop: &str,
    ) {
        let other = dossier.entries.get(&other_id).unwrap();
        let group_name = &dossier
            .categories
            .get(&other.category_id())
            .unwrap()
            .display_name;

        let group = groups
            .entry(group_name.clone())
            .or_insert_with(|| HashMap::new());

        let prop_name = util::humanize_camel_case(&prop);
        let property = group.entry(prop_name).or_insert_with(|| Vec::new());
        property.push(dossier.link_for_entry(context, entry, other.name(), &other.id()));
    }

    pub fn link_for_scope(
        &self,
        context: &PageContext,
        from: &dyn DocEntry,
        scope: &usize,
    ) -> DocStringSegment {
        let scope = self.string_table.get(*scope).unwrap();
        let id = ScopeDocEntry::id_from_name(&scope);
        self.link_for_entry(context, from, &scope, &id)
    }

    fn link_for_entry(
        &self,
        context: &PageContext,
        from: &dyn DocEntry,
        name: &str,
        id: &u64,
    ) -> DocStringSegment {
        if let Some(entry) = self.entries.get(&id) {
            let url = context.url_for_entry(from, entry.as_ref());
            return DocStringSegment::Link {
                contents: name.to_owned(),
                url: url,
            };
        }

        warn!("id without entry: {}, id {}", name, id);
        DocStringSegment::Text {
            contents: name.to_owned(),
        }
    }

    pub fn add_scope_reference(&mut self, prop: &str, this_id: u64, scope: usize) {
        self.add_reference(
            &prop,
            this_id,
            ScopeDocEntry::id_from_name(&self.string_table.get(scope).unwrap()),
        );
    }

    fn add_reference(&mut self, prop: &str, this_id: u64, that_id: u64) {
        self.cross_references.push(CrossReference {
            from_id: this_id,
            from_property: prop.to_owned(),
            to_id: that_id,
        });
    }

    fn entry_as<T: 'static>(&self, id: u64) -> &T {
        self.entries
            .get(&id)
            .unwrap()
            .as_ref()
            .as_any()
            .downcast_ref::<T>()
            .unwrap()
    }
}
