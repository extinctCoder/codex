use crate::dtos::db::codex::CodexColumn;

/// Whether this relation is a physical table or a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexKind {
    Table,
    View,
}

/// A named relation (table or view) with its columns.
#[derive(Debug, Clone)]
pub struct CodexRelation {
    pub name: String,
    pub kind: CodexKind,
    pub columns: Vec<CodexColumn>,
}

impl CodexRelation {
    pub fn column(&self, name: &str) -> Option<&CodexColumn> {
        self.columns.iter().find(|column| column.name == name)
    }
    pub fn filterable(&self) -> impl Iterator<Item = &CodexColumn> {
        self.columns.iter().filter(|column| column.filterable)
    }
    pub fn searchable(&self) -> impl Iterator<Item = &CodexColumn> {
        self.columns.iter().filter(|column| column.searchable)
    }
    pub fn selectable(&self) -> impl Iterator<Item = &CodexColumn> {
        self.columns.iter().filter(|column| column.selectable)
    }
}
