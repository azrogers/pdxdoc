use std::{any::Any, hash::Hash, rc::Rc};

use clauser::data::script_doc_parser::{
    doc_string::DocString, ScriptDocCategory, ScriptDocContent, ScriptDocEntry,
};
use once_cell::sync::Lazy;

use crate::{
    dossier::Dossier,
    page::PageContext,
    util::{self, humanize_camel_case},
};

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
    fn category_id(&self) -> Option<u64>;
    fn name(&self) -> &str;
    fn record_cross_references(&self, dossier: &mut Dossier);
    fn body(&self) -> Option<DocString>;
    fn properties(&self, context: &PageContext, dossier: Rc<Dossier>) -> Vec<(String, DocString)>;
}

pub struct EmptyDocEntry {
    id: u64,
    category_id: u64,
    name: String,
}

impl EmptyDocEntry {
    pub fn new(id: u64, category_id: u64, name: String) -> EmptyDocEntry {
        EmptyDocEntry {
            id,
            category_id,
            name,
        }
    }
}

impl DocEntry for EmptyDocEntry {
    fn id(&self) -> u64 {
        self.id
    }

    fn category_id(&self) -> Option<u64> {
        None
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn record_cross_references(&self, dossier: &mut Dossier) {}

    fn body(&self) -> Option<DocString> {
        None
    }

    fn properties(
        &self,
        _context: &PageContext,
        _dossier: Rc<Dossier>,
    ) -> Vec<(String, DocString)> {
        vec![]
    }
}

impl DocEntry for ScriptDocEntry {
    fn id(&self) -> u64 {
        self.id
    }

    fn category_id(&self) -> Option<u64> {
        match self.category {
            // modifiers go into the special masks page instead
            ScriptDocCategory::Modifiers => None,
            _ => Some(util::hash(&self.category)),
        }
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
                    dossier.add_scope_reference("Supported Targets", self.id, *s);
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
                dossier.add_mask_reference("Mask", self.id, *mask);
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
                    dossier.add_scope_reference("Supported Targets", self.id, *s);
                }
            }
        }
    }

    fn properties(&self, context: &PageContext, dossier: Rc<Dossier>) -> Vec<(String, DocString)> {
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
                    dossier.link_for_mask(context, self, mask).into(),
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
