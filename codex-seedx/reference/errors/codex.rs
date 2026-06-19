use thiserror::Error;

use crate::error::LaxError;

/// Schema compilation errors — raised during Codex build at boot.
#[derive(Debug, Error)]
pub enum CodexError {
    #[error("aggregate schema invalid: {0}")]
    SchemaInvalid(String),

    #[error("yaml violates schema: {0}")]
    SchemaViolation(String),

    #[error("yaml merge: {0}")]
    Merge(String),

    #[error("x_partials: {0}")]
    Partials(String),

    #[error("semantic: {0}")]
    Semantic(String),

    #[error(
        "table `{table}`: primary_key (excluding `version`) {actual:?} does not match Id + Reference columns {expected:?}"
    )]
    MismatchedPrimaryKey {
        table: String,
        expected: Vec<String>,
        actual: Vec<String>,
    },

    #[error("catalog: {0}")]
    Catalog(String),
}

impl CodexError {
    pub fn schema_invalid(message: impl Into<String>) -> LaxError {
        Self::SchemaInvalid(message.into()).into()
    }

    pub fn schema_violation(message: impl Into<String>) -> LaxError {
        Self::SchemaViolation(message.into()).into()
    }

    pub fn merge(message: impl Into<String>) -> LaxError {
        Self::Merge(message.into()).into()
    }

    pub fn partials(message: impl Into<String>) -> LaxError {
        Self::Partials(message.into()).into()
    }

    pub fn semantic(message: impl Into<String>) -> LaxError {
        Self::Semantic(message.into()).into()
    }

    pub fn mismatched_primary_key(table: impl Into<String>, expected: Vec<String>, actual: Vec<String>) -> LaxError {
        Self::MismatchedPrimaryKey {
            table: table.into(),
            expected,
            actual,
        }
        .into()
    }

    pub fn catalog(message: impl Into<String>) -> LaxError {
        Self::Catalog(message.into()).into()
    }
}
