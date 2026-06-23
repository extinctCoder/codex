//! The catalog: the resolved, queryable model lowered from the parsed AST.

mod build;
mod column;
mod data_type;
mod relation;

pub use build::build_catalog;
pub use column::CatalogColumn;
pub use data_type::CatalogType;
pub use relation::{CatalogRelation, RelationKind};

use indexmap::IndexMap;

use crate::error::HolocronError;
use crate::span::Span;

/// The symbol table: relations and enum types, each looked up by name.
/// Ordered maps keep iteration (and therefore emitted output) deterministic.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    relations: IndexMap<String, CatalogRelation>,
    enums: IndexMap<String, Vec<String>>,
    /// Original declaration spans, kept alongside the relations so a duplicate
    /// diagnostic can underline the *first* occurrence as well as the second.
    relation_spans: IndexMap<String, Span>,
}

impl Catalog {
    /// Construct an empty catalog pre-populated with the given enum types.
    /// Used by [`build_catalog`] after collecting enum errors so the rest of
    /// the build can continue against a partial enum map.
    pub(crate) fn from_enums(enums: IndexMap<String, Vec<String>>) -> Self {
        Self {
            enums,
            relations: IndexMap::new(),
            relation_spans: IndexMap::new(),
        }
    }

    /// Look up a relation (table or view) by name.
    pub fn relation(&self, name: &str) -> Option<&CatalogRelation> {
        self.relations.get(name)
    }

    /// Look up an enum type's allowed values by name.
    pub fn enum_type(&self, name: &str) -> Option<&[String]> {
        self.enums.get(name).map(Vec::as_slice)
    }

    /// Iterate all relations in declaration order.
    pub fn relations(&self) -> impl Iterator<Item = &CatalogRelation> {
        self.relations.values()
    }

    /// Add a relation, erroring if one with the same name already exists.
    /// `name_span` points the diagnostic at the offending name in the source.
    pub(crate) fn insert_relation(
        &mut self,
        relation: CatalogRelation,
        name_span: Span,
    ) -> Result<(), HolocronError> {
        let name = relation.name.clone();
        if let Some(&first_span) = self.relation_spans.get(&name) {
            return Err(HolocronError::duplicate_relation(
                name, first_span, name_span,
            ));
        }
        self.relation_spans.insert(name.clone(), name_span);
        self.relations.insert(name, relation);
        Ok(())
    }
}
