/// A resolved column type. Built from the YAML type string; an unresolved type
/// can never exist here (illegal states unrepresentable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogType {
    Text,
    Number,
    Timestamp,
    Uuid,
    Bool,
    Json,
    TextArray,
    Enum(String),
}

impl CatalogType {
    /// Map a built-in SQL type name to its variant. `None` for unknown names —
    /// the caller turns that into a compile error (never a silent fallback).
    /// Enum type names are resolved separately, where the declared enums are known.
    pub fn from_sql_name(name: &str) -> Option<Self> {
        match name {
            "text" => Some(Self::Text),
            "uuid" => Some(Self::Uuid),
            "boolean" => Some(Self::Bool),
            "timestamptz" => Some(Self::Timestamp),
            "jsonb" => Some(Self::Json),
            "bigint" | "integer" | "smallint" => Some(Self::Number),
            "text[]" => Some(Self::TextArray),
            _ => None,
        }
    }

    /// `==` / `!=` valid for this type.
    pub fn supports_equality(&self) -> bool {
        !matches!(self, Self::Json)
    }

    /// `=gt=` / `=lt=` (ordering comparisons) valid for this type.
    pub fn supports_ordering(&self) -> bool {
        matches!(self, Self::Number | Self::Timestamp)
    }

    /// `=like=` (text pattern matching) valid for this type.
    pub fn supports_text_pattern(&self) -> bool {
        matches!(self, Self::Text)
    }

    /// `=in=` / `=out=` (set membership) valid for this type.
    pub fn supports_set(&self) -> bool {
        matches!(self, Self::Text | Self::Number | Self::Uuid | Self::Enum(_))
    }

    /// `=null=` valid for every type.
    pub fn supports_null_check(&self) -> bool {
        true
    }
}
