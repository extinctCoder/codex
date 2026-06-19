use std::collections::HashSet;

use serde::Deserialize;

/// Relational role of a column — drives copy-forward, identity composition, and URL scoping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum CodexRole {
    Id,
    Reference,
    Data,
}

/// Postgres column data type — determines which RSQL operators are valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexType {
    Text,
    Number,
    Timestamp,
    Uuid,
    Bool,
    Json,
    TextArray,
    Enum(String),
}

/// Column metadata: name, type, relational role, and permission flags.
#[derive(Debug, Clone)]
pub struct CodexColumn {
    pub name: String,
    pub data_type: CodexType,
    pub role: CodexRole,
    pub filterable: bool,
    pub searchable: bool,
    pub selectable: bool,
}

impl CodexType {
    pub fn supports_equality(&self) -> bool {
        !matches!(self, Self::Json)
    }
    pub fn supports_ordering(&self) -> bool {
        matches!(self, Self::Number | Self::Timestamp)
    }
    pub fn supports_text_pattern(&self) -> bool {
        matches!(self, Self::Text)
    }
    pub fn supports_set(&self) -> bool {
        matches!(self, Self::Text | Self::Number | Self::Uuid | Self::Enum(_))
    }
    pub fn supports_null_check(&self) -> bool {
        true
    }

    pub fn from_schema_name(name: &str, enums: &HashSet<String>) -> Self {
        match name {
            "text" => Self::Text,
            "uuid" => Self::Uuid,
            "boolean" => Self::Bool,
            "timestamptz" => Self::Timestamp,
            "jsonb" => Self::Json,
            "bigint" | "integer" | "smallint" => Self::Number,
            "text[]" => Self::TextArray,
            enum_name if enums.contains(enum_name) => Self::Enum(enum_name.to_string()),
            _ => Self::Text,
        }
    }
}

impl CodexRole {
    pub fn default_selectable(self) -> bool {
        matches!(self, Self::Data)
    }
}
