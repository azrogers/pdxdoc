use std::{
    cell::RefCell, collections::HashMap, hash::Hash, marker::PhantomData, path::PathBuf, rc::Rc,
};

use clauser::data::script_doc_parser::{
    doc_string::{DocString, DocStringSegment},
    ScriptDocContent, ScriptDocEntry,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

use crate::{
    config::Config,
    dossier::{CollatedCrossReferences, DocCategory, Dossier},
    entry::{DocEntry, EmptyDocEntry},
    generator::SiteProfile,
    mapper::SiteMapper,
    theme::Template,
    util::{self, paginate, DocStringSer},
};

#[derive(Deserialize, Serialize, Clone)]
pub enum Breadcrumb {
    Single {
        title: String,
        absolute_url: String,
    },
    Paged {
        title: String,
        root_url: String,
        page: PaginationInfo,
    },
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct Breadcrumbs {
    pub crumbs: Vec<Breadcrumb>,
}

impl Breadcrumbs {
    pub fn new(crumbs: Vec<Breadcrumb>) -> Breadcrumbs {
        Breadcrumbs { crumbs }
    }

    /// Creates a new set of breadcrumbs from a page and its parents.
    pub fn from_page(page: &dyn Page, profile: &SiteProfile) -> Breadcrumbs {
        let mut crumbs = vec![Breadcrumb::Single {
            title: profile.profile.title.clone(),
            absolute_url: "index".into(),
        }];

        crumbs.extend(Self::from_page_inner(page, profile).crumbs);
        Breadcrumbs { crumbs }
    }

    fn from_page_inner(page: &dyn Page, profile: &SiteProfile) -> Breadcrumbs {
        let mut crumbs = Vec::new();

        // get parent crumbs if we have them
        if let Some(parent_id) = page.parent_id() {
            let parent = profile.pages.iter().find(|p| p.id() == parent_id).unwrap();
            crumbs.extend(Self::from_page_inner(parent.as_ref(), profile).crumbs);
        }

        let page_info = page.info();
        let crumb = match page_info.pagination {
            Some(pagination) if pagination.total_pages > 1 => Breadcrumb::Paged {
                title: page_info.short_title.clone(),
                root_url: Self::ensure_html_on_url(&page.page_url(1)),
                page: pagination.clone(),
            },
            _ => Breadcrumb::Single {
                title: page_info.short_title.clone(),
                absolute_url: Self::ensure_html_on_url(&page_info.path),
            },
        };

        crumbs.push(crumb);
        Breadcrumbs::new(crumbs)
    }

    pub fn len(&self) -> usize {
        self.crumbs.len()
    }

    fn ensure_html_on_url(str: &str) -> String {
        let mut path = PathBuf::from(str);
        path.set_extension("html");
        path.to_str().unwrap().to_string()
    }
}

impl IntoIterator for Breadcrumbs {
    type Item = (usize, Breadcrumb);

    type IntoIter = std::iter::Enumerate<std::vec::IntoIter<Breadcrumb>>;

    fn into_iter(self) -> Self::IntoIter {
        self.crumbs.into_iter().enumerate().into_iter()
    }
}

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

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PaginationInfo {
    pub current_page: usize,
    pub total_pages: usize,
}

impl PaginationInfo {
    pub fn new(info: (usize, usize)) -> PaginationInfo {
        let (current_page, total_pages) = info;
        PaginationInfo {
            current_page,
            total_pages,
        }
    }
}

pub struct PageInfo {
    pub title: String,
    pub short_title: String,
    pub template: Template,
    pub path: String,
    pub pagination: Option<PaginationInfo>,
}

/// Trait implemented by all renderable pages.
pub trait Page {
    fn id(&self) -> u64;
    /// All pages that this is paginated with should be here.
    fn group_id(&self) -> u64;
    fn info(&self) -> PageInfo;
    fn entries(&self) -> Vec<u64>;
    fn page_url(&self, page: usize) -> String;
    fn parent_id(&self) -> Option<u64>;
    /// All of the anchors (destinations that can be reached with the URL hash) on this page, and their entry ID.
    fn anchors(&self) -> Vec<(u64, String)>;
    fn data(&self, context: &PageContext) -> serde_json::Value;
}

/// Object that produces pages.
pub trait PageBuilder {
    fn build_entries(&self, dossier: &Dossier, config: &Config) -> Vec<Box<dyn DocEntry>>;
    fn build_pages(&self, dossier: Rc<Dossier>, config: &Config) -> Vec<Box<dyn Page>>;
}

pub trait GenericListPage: Page + Sized {
    fn new(dossier: Rc<Dossier>, id: u64, entry_id: u64, name: String) -> Vec<Self>;
    fn category_id() -> u64;
    fn entry_id_for_name(name: &str) -> u64;
    fn index_page(dossier: Rc<Dossier>, entries: &[(u64, Rc<String>)]) -> Option<Box<dyn Page>>;
}

pub struct CategoryListPage {
    category: DocCategory,
    dossier: Rc<Dossier>,
    entries: Vec<u64>,
    page: PaginationInfo,
}

impl CategoryListPage {
    pub fn new(
        category: DocCategory,
        entries: &[u64],
        page_info: (usize, usize),
        dossier: Rc<Dossier>,
    ) -> CategoryListPage {
        CategoryListPage {
            category,
            dossier,
            entries: entries.to_vec(),
            page: PaginationInfo::new(page_info),
        }
    }
}

impl Page for CategoryListPage {
    fn info(&self) -> PageInfo {
        PageInfo {
            title: self.category.display_name.clone(),
            short_title: self.category.display_name.clone(),
            template: Template::CategoryList,
            path: match self.page.total_pages {
                1 => self.category.name.clone(),
                _ => Self::page_url(&self, self.page.current_page),
            },
            pagination: Some(self.page.clone()),
        }
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
            pagination: PaginationInfo,
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
            body: DocStringSer(DocString::default(), self.id(), context.mapper.clone()),
            entries,
            pagination: self.page.clone(),
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
        util::hash(&self.category) ^ util::hash(&self.page.current_page)
    }

    fn group_id(&self) -> u64 {
        util::hash(&self.category.name)
    }

    fn parent_id(&self) -> Option<u64> {
        None
    }

    fn page_url(&self, page: usize) -> String {
        format!("{}_p{}", self.category.name, page)
    }
}

pub struct GenericListPageBuilder<P: Page + GenericListPage> {
    items: Vec<usize>,
    _phantom: PhantomData<P>,
}

impl<P: GenericListPage + 'static> GenericListPageBuilder<P> {
    pub fn new(items: Vec<usize>) -> GenericListPageBuilder<P> {
        GenericListPageBuilder {
            items,
            _phantom: PhantomData::default(),
        }
    }
}

impl<P: GenericListPage + 'static> PageBuilder for GenericListPageBuilder<P> {
    fn build_entries(&self, dossier: &Dossier, _config: &Config) -> Vec<Box<dyn DocEntry>> {
        let category_id = P::category_id();
        self.items
            .iter()
            .map(move |s| dossier.string_table.get(*s).unwrap())
            .map(move |s| {
                Box::new(EmptyDocEntry::new(
                    P::entry_id_for_name(s.as_str()),
                    category_id,
                    (*s).clone(),
                )) as Box<dyn DocEntry>
            })
            .collect_vec()
    }

    fn build_pages(&self, dossier: Rc<Dossier>, _config: &Config) -> Vec<Box<dyn Page>> {
        let category_id = P::category_id();
        let d2 = dossier.clone();
        let mut entry_ids = self
            .items
            .iter()
            .map(|s| dossier.string_table.get(*s).unwrap())
            .map(|name| (P::entry_id_for_name(name.as_str()), name))
            .collect_vec();

        entry_ids.sort_by_key(|(_, name)| name.as_str().to_owned());

        let mut pages = entry_ids
            .iter()
            .flat_map(|(id, name)| {
                let page_id = util::hash(&format!("{}_{}", category_id, id));
                P::new(d2.clone(), page_id, *id, name.to_string())
            })
            .map(|p| Box::new(p) as Box<dyn Page>)
            .collect_vec();

        if let Some(index) = P::index_page(dossier, entry_ids.as_slice()) {
            pages.push(index);
        }

        pages
    }
}

pub struct IndexPage {
    dossier: Rc<Dossier>,
    id: u64,
    title: String,
    path: String,
    entries: Vec<u64>,
    parent_id: Option<u64>,
}

impl Page for IndexPage {
    fn id(&self) -> u64 {
        self.id
    }

    fn group_id(&self) -> u64 {
        self.id
    }

    fn info(&self) -> PageInfo {
        PageInfo {
            title: self.title.clone(),
            short_title: self.title.clone(),
            template: Template::ListIndex,
            path: self.path.clone(),
            pagination: None,
        }
    }

    fn entries(&self) -> Vec<u64> {
        vec![]
    }

    fn anchors(&self) -> Vec<(u64, String)> {
        vec![]
    }

    fn data(&self, context: &PageContext) -> serde_json::Value {
        #[derive(Serialize)]
        struct Data {
            items: Vec<DocStringSer>,
        }

        let mut items = Vec::new();
        for entry in &self.entries {
            let entry = self.dossier.entries.get(entry).unwrap();
            items.push(DocStringSer(
                DocString::from(DocStringSegment::Link {
                    contents: entry.name().into(),
                    url: context
                        .mapper
                        .borrow()
                        .page_to_entry_url(&self.id, &entry.id()),
                }),
                self.id,
                context.mapper.clone(),
            ))
        }

        serde_json::to_value(Data { items }).unwrap()
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id.clone()
    }

    fn page_url(&self, page: usize) -> String {
        self.path.clone()
    }
}

pub struct ScopePage {
    dossier: Rc<Dossier>,
    id: u64,
    entry_id: u64,
    name: String,
}

impl GenericListPage for ScopePage {
    fn entry_id_for_name(name: &str) -> u64 {
        util::hash(&format!("scope_{}", name))
    }

    fn new(dossier: Rc<Dossier>, id: u64, entry_id: u64, name: String) -> Vec<Self> {
        vec![ScopePage {
            dossier,
            id,
            entry_id,
            name,
        }]
    }

    fn category_id() -> u64 {
        util::hash(&"SCOPES")
    }

    fn index_page(dossier: Rc<Dossier>, entries: &[(u64, Rc<String>)]) -> Option<Box<dyn Page>> {
        Some(Box::new(IndexPage {
            dossier,
            id: util::hash(&"SCOPES_INDEX"),
            title: "Scopes".into(),
            path: "scopes/index.html".into(),
            entries: entries.iter().map(|(id, _)| *id).collect_vec(),
            parent_id: None,
        }))
    }
}

impl Page for ScopePage {
    fn id(&self) -> u64 {
        self.id
    }

    fn info(&self) -> PageInfo {
        PageInfo {
            short_title: self.name.clone(),
            title: format!("Scope: {}", self.name),
            path: format!("scopes/{}", self.name),
            template: Template::Scope,
            pagination: None,
        }
    }

    fn entries(&self) -> Vec<u64> {
        vec![self.entry_id]
    }

    fn anchors(&self) -> Vec<(u64, String)> {
        vec![]
    }

    fn data(&self, context: &PageContext) -> serde_json::Value {
        let refs =
            Dossier::collate_references(self.dossier.clone(), context, self.id(), self.entry_id);

        #[derive(Serialize)]
        struct Data {
            cross_refs: CollatedCrossReferences,
        }

        serde_json::to_value(Data { cross_refs: refs }).unwrap()
    }

    fn group_id(&self) -> u64 {
        Self::category_id()
    }

    fn parent_id(&self) -> Option<u64> {
        Some(util::hash(&"SCOPES_INDEX"))
    }

    fn page_url(&self, _page: usize) -> String {
        format!("scopes/{}", self.name)
    }
}

pub struct MaskPage {
    dossier: Rc<Dossier>,
    id: u64,
    entry_id: u64,
    name: String,
    modifiers: Vec<u64>,
    page: PaginationInfo,
}

impl GenericListPage for MaskPage {
    fn entry_id_for_name(name: &str) -> u64 {
        util::hash(&format!("mask_{}", name))
    }

    fn new(dossier: Rc<Dossier>, id: u64, entry_id: u64, name: String) -> Vec<Self> {
        let mut modifiers = Dossier::find_references_to(dossier.clone(), entry_id);
        modifiers.sort_by_key(|f| dossier.entries.get(f).unwrap().name());

        let mut page = 0;
        paginate(
            &dossier.config.pagination,
            4,
            modifiers.as_slice(),
            |num_pages, chunk| {
                page += 1;
                MaskPage {
                    dossier: dossier.clone(),
                    id,
                    entry_id,
                    name: name.clone(),
                    modifiers: chunk.to_vec(),
                    page: PaginationInfo::new((page, num_pages)),
                }
            },
        )
    }

    fn category_id() -> u64 {
        util::hash(&"MASKS")
    }

    fn index_page(dossier: Rc<Dossier>, entries: &[(u64, Rc<String>)]) -> Option<Box<dyn Page>> {
        Some(Box::new(IndexPage {
            dossier,
            id: util::hash(&"MODIFIERS_INDEX"),
            title: "Modifiers".into(),
            path: "modifiers/index.html".into(),
            entries: entries.iter().map(|(id, _)| *id).collect_vec(),
            parent_id: None,
        }))
    }
}

impl Page for MaskPage {
    fn id(&self) -> u64 {
        self.id ^ (self.page.current_page as u64)
    }

    fn info(&self) -> PageInfo {
        PageInfo {
            short_title: self.name.clone(),
            title: format!("Modifiers for Mask: {}", self.name),
            path: match self.page.total_pages {
                1 => format!("modifiers/{}", self.name),
                _ => Self::page_url(&self, self.page.current_page),
            },
            template: Template::Mask,
            pagination: Some(self.page.clone()),
        }
    }

    fn entries(&self) -> Vec<u64> {
        let mut modifiers = self.modifiers.clone();
        if self.page.current_page == 1 {
            modifiers.push(self.entry_id);
        }

        modifiers
    }

    fn anchors(&self) -> Vec<(u64, String)> {
        vec![]
    }

    fn data(&self, context: &PageContext) -> serde_json::Value {
        #[derive(Serialize)]
        struct Modifier {
            name: String,
            display_name: Option<DocStringSer>,
            description: Option<DocStringSer>,
        }

        let modifiers = self
            .modifiers
            .iter()
            .map(|m| self.dossier.entries.get(m).unwrap())
            .map(|m| m.as_any().downcast_ref::<ScriptDocEntry>().unwrap())
            .map(|m| {
                if let ScriptDocContent::Modifiers {
                    display_name,
                    description,
                    ..
                } = m.content.as_ref().unwrap()
                {
                    return Modifier {
                        name: m.name.clone(),
                        display_name: display_name
                            .as_ref()
                            .map(|s| DocStringSer(s.clone(), self.id, context.mapper.clone())),
                        description: description
                            .as_ref()
                            .map(|s| DocStringSer(s.clone(), self.id, context.mapper.clone())),
                    };
                }

                panic!("ScriptDocContent::Modifiers expected!");
            })
            .collect_vec();

        #[derive(Serialize)]
        struct Data {
            modifiers: Vec<Modifier>,
            pagination: PaginationInfo,
        }

        serde_json::to_value(Data {
            modifiers,
            pagination: self.page.clone(),
        })
        .unwrap()
    }

    fn group_id(&self) -> u64 {
        util::hash(&format!("modifiers_{}", self.name))
    }

    fn parent_id(&self) -> Option<u64> {
        Some(util::hash(&"MODIFIERS_INDEX"))
    }

    fn page_url(&self, page: usize) -> String {
        format!("modifiers/{}_p{}", self.name, page)
    }
}
