use std::collections::{HashMap, HashSet};

use lax_shared::{
    dtos::db::codex::{CodexKind, CodexRelation, CodexRole},
    error::{CodexError, LaxResult},
};
use serde::Deserialize;

use crate::relational_basket::codex::dtos::{RawCodexColumnDefinition, RawCodexPartial};

/// YAML shape for a `tables:` entry.
#[derive(Debug, Deserialize)]
pub struct RawCodexTable {
    pub name: String,
    #[serde(default)]
    pub columns: HashMap<String, RawCodexColumnDefinition>,
    #[serde(default)]
    pub primary_key: Option<RawCodexPrimaryKey>,
    #[serde(default)]
    pub x_aggregate: Option<RawCodexTableAggregate>,
    #[serde(default)]
    pub x_entity: Option<RawCodexTableEntity>,
    #[serde(default)]
    pub x_sink: Option<RawCodexTableSink>,
    #[serde(default)]
    pub x_partials: Vec<String>,
}

/// `primary_key:` block — table's SQL primary key declaration.
#[derive(Debug, Deserialize)]
pub struct RawCodexPrimaryKey {
    #[serde(default)]
    pub name: Option<String>,
    pub columns: Vec<String>,
}

/// `x_aggregate:` block — marks a table as an aggregate root.
#[derive(Debug, Deserialize)]
pub struct RawCodexTableAggregate {
    pub name: String,
}

/// `x_entity:` block — marks a table as a child entity of an aggregate.
/// `as:` (optional) is the display name used in composition response payloads
/// (`{"<as>": [...]}`). Falls back to `name` when omitted.
#[derive(Debug, Deserialize)]
pub struct RawCodexTableEntity {
    pub aggregate: String,
    pub name: String,
    #[serde(default, rename = "as")]
    pub display_name: Option<String>,
}

/// `x_sink:` block — marks a table as a downstream sink for an aggregate or entity.
#[derive(Debug, Deserialize)]
pub struct RawCodexTableSink {
    pub aggregate: String,
    #[serde(default)]
    pub entity: Option<String>,
}

impl RawCodexTable {
    /// Resolve `x_partials` by merging each referenced column-partial's columns.
    pub fn expand_partials(&mut self, partials: &HashMap<String, RawCodexPartial>) -> LaxResult<()> {
        let references = std::mem::take(&mut self.x_partials);
        for name in references {
            let partial = partials
                .get(&name)
                .ok_or_else(|| CodexError::partials(format!("table `{}`: unknown partial `{name}`", self.name)))?;
            let RawCodexPartial::Columns(columns_partial) = partial else {
                return Err(CodexError::partials(format!(
                    "table `{}`: partial `{name}` is not a columns partial (tables can only reference x_partial_columns)",
                    self.name
                )));
            };
            for (column_name, column_def) in &columns_partial.columns {
                self.columns.insert(
                    column_name.clone(),
                    RawCodexColumnDefinition {
                        type_name: column_def.type_name.clone(),
                        x_role: column_def.x_role,
                    },
                );
            }
        }
        Ok(())
    }

    /// The single column with `x_role: Id`, if exactly one exists. `None` means 0 or >1.
    pub fn id_column(&self) -> Option<String> {
        let mut matches = self
            .columns
            .iter()
            .filter(|(_, column)| column.x_role == Some(CodexRole::Id));
        let first = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(first.0.clone())
    }

    /// Count columns declaring the given role.
    pub fn count_role(&self, role: CodexRole) -> usize {
        self.columns
            .values()
            .filter(|column| column.x_role == Some(role))
            .count()
    }

    /// Whether this table is part of the CQRS model (root / entity / sink).
    pub fn is_cqrs(&self) -> bool {
        self.x_aggregate.is_some() || self.x_entity.is_some() || self.x_sink.is_some()
    }

    /// Aggregate name this table belongs to as an entity or sink (if any).
    pub fn parent_aggregate(&self) -> Option<&str> {
        self.x_entity
            .as_ref()
            .map(|entity| entity.aggregate.as_str())
            .or_else(|| self.x_sink.as_ref().map(|sink| sink.aggregate.as_str()))
    }

    /// Produce the runtime `CodexRelation` (Table kind) from this YAML shape.
    pub fn into_relation(&self, enums: &HashSet<String>) -> CodexRelation {
        let columns = self
            .columns
            .iter()
            .map(|(name, definition)| definition.into_column(name, enums))
            .collect();
        CodexRelation {
            name: self.name.clone(),
            kind: CodexKind::Table,
            columns,
        }
    }
}
