use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use anyhow::{Error, Result};
use clauser::{
    data::script_doc_parser::doc_string::{DocString, DocStringSegment},
    string_table::StringTable,
};
use itertools::Itertools;
use log::warn;
use serde::Serialize;

use crate::{
    config::{Config, Profile},
    entry::DocEntry,
    games::GameVersion,
    generator::SiteMapper,
    page::{
        CategoryListPage, GenericListPage, MaskPage, Page, PageBuilder, PageContext, ScopePage,
    },
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
    items: Vec<DocStringSer>,
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
/// Responsible for turning information into pages to be rendered.
pub struct Dossier {
    categories: HashMap<u64, DocCategory>,
    pub entries: HashMap<u64, Box<dyn DocEntry>>,
    pub string_table: StringTable,
    mapper: Rc<RefCell<SiteMapper>>,
    pub config: Config,
    builders: Vec<Box<dyn PageBuilder>>,

    cross_references: Vec<CrossReference>,
    info: DocInfo,
}

impl Dossier {
    pub fn new(
        config: Config,
        categories: impl IntoIterator<Item = DocCategory>,
        string_table: StringTable,
        info: DocInfo,
        mapper: Rc<RefCell<SiteMapper>>,
    ) -> Dossier {
        Dossier {
            categories: categories.into_iter().map(|c| (c.id, c)).collect(),
            entries: HashMap::new(),
            info,
            config,
            cross_references: Vec::new(),
            string_table,
            mapper,
            builders: Vec::new(),
        }
    }

    pub fn add_entries<T>(&mut self, entries: impl Iterator<Item = T>) -> Result<()>
    where
        T: DocEntry + 'static,
    {
        for entry in entries {
            if let Some(category_id) = entry.category_id() {
                match self.categories.get_mut(&category_id) {
                    Some(category) => Ok(category.entries.push(entry.id())),
                    None => Err(Error::msg(
                        "Tried adding an entry with a category that doesn't exist?",
                    )),
                }?;
            }

            entry.record_cross_references(self);

            self.entries.insert(entry.id(), Box::new(entry));
        }

        Ok(())
    }

    pub fn add_builder<B: PageBuilder + 'static>(&mut self, builder: B) {
        let entries = builder.build_entries(self, &self.config);

        for entry in entries {
            let id = entry.id();
            self.entries.insert(id, entry);
        }

        self.builders.push(Box::new(builder))
    }

    pub fn create_pages(dossier: Rc<Dossier>, config: &Config) -> Vec<Box<dyn Page>> {
        let mut pages: Vec<Box<dyn Page>> = Vec::new();

        for category in dossier.categories.values() {
            let mut entries = category.entries.clone();
            entries.sort_by_key(|f| dossier.entries.get(f).unwrap().name());
            let mut page = 0;
            pages.extend(
                paginate(
                    &config.pagination,
                    1,
                    entries.as_slice(),
                    |num_pages, entries| {
                        page += 1;
                        CategoryListPage::new(
                            category.clone(),
                            entries,
                            (page, num_pages),
                            dossier.clone(),
                        )
                    },
                )
                .into_iter()
                .map(|p| Box::new(p) as Box<dyn Page>),
            );
        }

        for builder in &dossier.builders {
            pages.extend(builder.build_pages(dossier.clone(), config).into_iter());
        }

        pages
    }

    /// Returns the IDs of items that reference this one
    pub fn find_references_to(dossier: Rc<Dossier>, id: u64) -> Vec<u64> {
        dossier
            .cross_references
            .iter()
            .filter(|c| c.to_id == id)
            .map(|c| c.from_id)
            .collect_vec()
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
                properties.push(CrossReferenceSection {
                    name: prop,
                    items: items
                        .iter()
                        .map(|s| {
                            DocStringSer(
                                DocString::new_from_segment(s.clone()),
                                page_id,
                                dossier.mapper.clone(),
                            )
                        })
                        .collect_vec(),
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
            .get(&other.category_id().unwrap())
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
        let id = ScopePage::entry_id_for_name(&scope);
        self.link_for_entry(context, from, &scope, &id)
    }

    pub fn link_for_mask(
        &self,
        context: &PageContext,
        from: &dyn DocEntry,
        mask: &usize,
    ) -> DocStringSegment {
        let mask = self.string_table.get(*mask).unwrap();
        let id = MaskPage::entry_id_for_name(&mask);
        self.link_for_entry(context, from, &mask, &id)
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
            ScopePage::entry_id_for_name(&self.string_table.get(scope).unwrap()),
        );
    }

    pub fn add_mask_reference(&mut self, prop: &str, this_id: u64, scope: usize) {
        self.add_reference(
            &prop,
            this_id,
            MaskPage::entry_id_for_name(&self.string_table.get(scope).unwrap()),
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
