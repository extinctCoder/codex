use crate::catalog::column::CatalogColumn;

/// Whether a relation is a physical table or a derived view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Table,
    View,
}

/// A named relation (table or view) and its resolved columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRelation {
    pub name: String,
    pub kind: RelationKind,
    pub columns: Vec<CatalogColumn>,
}

impl CatalogRelation {
    /// Find a column by name, or `None` if this relation has no such column.
    pub fn column(&self, name: &str) -> Option<&CatalogColumn> {
        self.columns.iter().find(|column| column.name == name)
    }
}
