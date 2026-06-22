use thiserror::Error;

/// The crate's single root error type. Each layer wraps the one below it.
#[derive(Debug, Error)]
pub enum HolocronError {
    /// The YAML did not fit the declared schema shape (Layer 1).
    #[error("parse error: {0}")]
    Parse(String),

    /// A column's type is neither a built-in nor a declared enum.
    #[error("unknown type `{type_name}` on column `{relation}.{column}`")]
    UnknownType {
        relation: String,
        column: String,
        type_name: String,
    },

    /// Two relations share a name.
    #[error("duplicate relation `{0}`")]
    DuplicateRelation(String),

    /// Two enum types share a name.
    #[error("duplicate enum type `{0}`")]
    DuplicateEnum(String),
}

impl HolocronError {
    pub(crate) fn parse(message: impl Into<String>) -> Self {
        Self::Parse(message.into())
    }

    pub(crate) fn unknown_type(
        relation: impl Into<String>,
        column: impl Into<String>,
        type_name: impl Into<String>,
    ) -> Self {
        Self::UnknownType {
            relation: relation.into(),
            column: column.into(),
            type_name: type_name.into(),
        }
    }

    pub(crate) fn duplicate_relation(name: impl Into<String>) -> Self {
        Self::DuplicateRelation(name.into())
    }

    pub(crate) fn duplicate_enum(name: impl Into<String>) -> Self {
        Self::DuplicateEnum(name.into())
    }
}
