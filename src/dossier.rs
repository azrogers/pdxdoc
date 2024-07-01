use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    thread::Scope,
};

use clauser::{
    data::script_doc_parser::{
        doc_string::{DocString, DocStringSegment},
        ScriptDocCategory, ScriptDocContent, ScriptDocEntry,
    },
    string_table::StringTable,
};
use log::warn;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::{
    error::Error,
    games::GameVersion,
    page::{CategoryListPage, Page, PageContext, ScopePage},
    util::{self, humanize_camel_case, DocStringSer},
};

pub trait DocEntryContext {
    fn resolve_str(&self, id: &usize) -> &str;
}

pub trait AsAny: 'static {
    fn as_any(&self) -> &dyn Any;
}

impl<T: 'static> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub trait DocEntry: AsAny {
    fn id(&self) -> u64;
    fn category_id(&self) -> u64;
    fn name(&self) -> &str;
    fn record_cross_references(&self, dossier: &mut Dossier);
    fn body(&self) -> Option<DocString>;
    fn properties(&self, context: &PageContext, dossier: &Dossier) -> Vec<(String, DocString)>;
}

impl DocEntry for ScriptDocEntry {
    fn id(&self) -> u64 {
        self.id
    }

    fn category_id(&self) -> u64 {
        util::hash(&self.category)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn body(&self) -> Option<DocString> {
        let content = self.content.as_ref()?;
        match content {
            ScriptDocContent::CustomLocalization { .. } => None,
            ScriptDocContent::Effects { description, .. } => Some(description.clone()),
            ScriptDocContent::EventTargets { description, .. } => Some(description.clone()),
            ScriptDocContent::Modifiers { description, .. } => description.clone(),
            ScriptDocContent::OnActions { .. } => None,
            ScriptDocContent::Triggers { description, .. } => Some(description.clone()),
        }
    }

    fn record_cross_references(&self, dossier: &mut Dossier) {
        let content = self.content.as_ref();
        if content.is_none() {
            return;
        }

        let content = content.unwrap();

        match content {
            ScriptDocContent::CustomLocalization { scope, .. } => {
                dossier.add_scope_reference("Scope", self.id, *scope);
            }
            ScriptDocContent::Effects {
                supported_scopes,
                supported_targets,
                ..
            } => {
                for s in supported_scopes {
                    dossier.add_scope_reference("Supported Scopes", self.id, *s);
                }

                for s in supported_targets {
                    dossier.add_target_reference("Supported Targets", self.id, *s);
                }
            }
            ScriptDocContent::EventTargets {
                input_scopes,
                output_scopes,
                ..
            } => {
                for s in input_scopes {
                    dossier.add_scope_reference("Input Scopes", self.id, *s);
                }

                for s in output_scopes {
                    dossier.add_scope_reference("Output Scopes", self.id, *s);
                }
            }
            ScriptDocContent::Modifiers { mask, .. } => {
                dossier.add_scope_reference("Mask", self.id, *mask);
            }
            ScriptDocContent::OnActions { expected_scope, .. } => {
                dossier.add_scope_reference("Expected Scope", self.id, *expected_scope)
            }
            ScriptDocContent::Triggers {
                supported_scopes,
                supported_targets,
                ..
            } => {
                for s in supported_scopes {
                    dossier.add_scope_reference("Supported Scopes", self.id, *s);
                }

                for s in supported_targets {
                    dossier.add_target_reference("Supported Targets", self.id, *s);
                }
            }
        }
    }

    fn properties(&self, context: &PageContext, dossier: &Dossier) -> Vec<(String, DocString)> {
        let content = self.content.as_ref();
        if content.is_none() {
            return vec![];
        }
        let content = content.unwrap();

        match content {
            ScriptDocContent::CustomLocalization {
                scope,
                random_valid,
                entries,
            } => {
                vec![
                    (
                        "Scope".into(),
                        dossier.link_for_scope(context, self, scope).into(),
                    ),
                    ("Random Valid?".into(), (*random_valid).into()),
                    ("Entries".into(), entries.join("\n").into()),
                ]
            }
            ScriptDocContent::Effects {
                supported_scopes,
                supported_targets,
                ..
            } => vec![
                (
                    "Supported Scopes".into(),
                    DocString::new_from_iter(
                        supported_scopes
                            .iter()
                            .map(|s| dossier.link_for_scope(context, self, s)),
                        Some(", "),
                    ),
                ),
                (
                    "Supported Targets".into(),
                    DocString::new_from_iter(
                        supported_targets
                            .iter()
                            .map(|s| dossier.link_for_scope(context, self, s)),
                        Some(", "),
                    ),
                ),
            ],
            ScriptDocContent::EventTargets {
                requires_data,
                wild_card,
                global_link,
                input_scopes,
                output_scopes,
                ..
            } => vec![
                ("Requires Data".into(), (*requires_data).into()),
                ("Wild Card".into(), (*wild_card).into()),
                ("Global Link".into(), (*global_link).into()),
                (
                    "Input Scopes".into(),
                    DocString::new_from_iter(
                        input_scopes
                            .iter()
                            .map(|s| dossier.link_for_scope(context, self, s)),
                        Some(", "),
                    ),
                ),
                (
                    "Output Scopes".into(),
                    DocString::new_from_iter(
                        output_scopes
                            .iter()
                            .map(|s| dossier.link_for_scope(context, self, s)),
                        Some(", "),
                    ),
                ),
            ],
            ScriptDocContent::Modifiers {
                display_name, mask, ..
            } => {
                let mut properties = Vec::new();
                if let Some(display_name) = display_name {
                    properties.push(("Display Name".into(), display_name.clone()));
                }

                properties.push((
                    "Mask".into(),
                    dossier.link_for_scope(context, self, mask).into(),
                ));
                properties
            }
            ScriptDocContent::OnActions {
                from_code,
                expected_scope,
            } => vec![
                (
                    "Expected Scope".into(),
                    dossier.link_for_scope(context, self, expected_scope).into(),
                ),
                ("From Code".into(), (*from_code).into()),
            ],
            ScriptDocContent::Triggers {
                supported_scopes,
                supported_targets,
                ..
            } => vec![
                (
                    "Supported Scopes".into(),
                    DocString::new_from_iter(
                        supported_scopes
                            .iter()
                            .map(|s| dossier.link_for_scope(context, self, s)),
                        Some(", "),
                    ),
                ),
                (
                    "Supported Targets".into(),
                    DocString::new_from_iter(
                        supported_targets
                            .iter()
                            .map(|s| dossier.link_for_scope(context, self, s)),
                        Some(", "),
                    ),
                ),
            ],
        }
    }
}

// scopes is a special synthesized category
#[derive(Debug, Hash, Clone)]
pub struct ScopeDocEntry {
    pub id: u64,
    pub name: String,
    pub display_name: String,
}

impl ScopeDocEntry {
    pub fn new(name: String) -> ScopeDocEntry {
        ScopeDocEntry {
            id: ScopeDocEntry::id_from_name(&name),
            display_name: humanize_camel_case(&name),
            name,
        }
    }

    pub fn id_from_name(name: &str) -> u64 {
        util::hash(&format!("scope_{}", name))
    }
}

static SCOPES_CATEGORY: Lazy<u64> = Lazy::new(|| util::hash(&"scopes"));

impl DocEntry for ScopeDocEntry {
    fn id(&self) -> u64 {
        self.id
    }

    fn category_id(&self) -> u64 {
        *SCOPES_CATEGORY
    }

    fn record_cross_references(&self, _dossier: &mut Dossier) {}

    fn body(&self) -> Option<DocString> {
        None
    }

    fn properties(&self, context: &PageContext, dossier: &Dossier) -> Vec<(String, DocString)> {
        vec![]
    }

    fn name(&self) -> &str {
        &self.name
    }
}

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

    cross_references: Vec<CrossReference>,
    info: DocInfo,
}

impl Dossier {
    pub fn new(
        categories: impl IntoIterator<Item = DocCategory>,
        scopes: impl IntoIterator<Item = String>,
        string_table: StringTable,
        info: DocInfo,
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
        }
    }

    pub fn add_entries<T>(&mut self, entries: impl Iterator<Item = T>) -> Result<(), Error>
    where
        T: DocEntry + 'static,
    {
        for entry in entries {
            match self.categories.get_mut(&entry.category_id()) {
                Some(category) => Ok(category.entries.push(entry.id())),
                None => Err(Error::Other(
                    "Tried adding an entry with a category that doesn't exist?".into(),
                )),
            }?;

            entry.record_cross_references(self);

            self.entries.insert(entry.id(), Box::new(entry));
        }

        Ok(())
    }

    pub fn create_pages(dossier: Rc<Dossier>) -> Vec<Box<dyn Page>> {
        let mut pages: Vec<Box<dyn Page>> = Vec::new();

        for (_, category) in &dossier.categories {
            pages.push(Box::new(CategoryListPage::new(
                category.clone(),
                dossier.clone(),
            )));
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
                    body: DocStringSer(s, dossier.clone()),
                });
            }

            collated
                .groups
                .push(CrossReferenceGroup { name, properties });
        }

        collated
    }

    fn add_ref_link(
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

    fn link_for_scope(
        &self,
        context: &PageContext,
        from: &dyn DocEntry,
        scope: &usize,
    ) -> DocStringSegment {
        let scope = self.string_table.get(*scope).unwrap();
        let id = ScopeDocEntry::id_from_name(&scope);
        self.link_for_entry(context, from, &scope, &id)
    }

    fn link_for_target(
        &self,
        context: &PageContext,
        from: &dyn DocEntry,
        entry: &usize,
    ) -> DocStringSegment {
        let entry = self.string_table.get(*entry).unwrap();
        let id = ScriptDocEntry::id_for_name(&ScriptDocCategory::EventTargets, &entry);
        self.link_for_entry(context, from, &entry, &id)
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

    fn add_scope_reference(&mut self, prop: &str, this_id: u64, scope: usize) {
        self.add_reference(
            &prop,
            this_id,
            ScopeDocEntry::id_from_name(&self.string_table.get(scope).unwrap()),
        );
    }

    fn add_target_reference(&mut self, prop: &str, this_id: u64, scope: usize) {
        self.add_reference(
            &prop,
            this_id,
            ScriptDocEntry::id_for_name(
                &ScriptDocCategory::EventTargets,
                &self.string_table.get(scope).unwrap(),
            ),
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
