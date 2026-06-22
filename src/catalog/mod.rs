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

/// The symbol table: relations and enum types, each looked up by name.
/// Ordered maps keep iteration (and therefore emitted output) deterministic.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    relations: IndexMap<String, CatalogRelation>,
    enums: IndexMap<String, Vec<String>>,
}

impl Catalog {
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
}
