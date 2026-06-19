use std::collections::HashMap;

use lax_shared::{
    dtos::db::codex::{CodexKind, CodexRelation, CodexRole, CodexType, ReadShape},
    error::{CodexError, LaxResult},
};
use serde::Deserialize;

use crate::relational_basket::codex::dtos::{RawCodexPartial, RawCodexPartialReference, RawCodexSelectItem};

/// YAML shape for a `views:` entry.
#[derive(Debug, Deserialize)]
pub struct RawCodexView {
    pub name: String,
    #[serde(default)]
    pub x_aggregate: Option<RawCodexViewAggregate>,
    #[serde(default)]
    pub x_entity: Option<RawCodexViewEntity>,
    #[serde(default)]
    pub x_partials: Vec<RawCodexPartialReference>,
    #[serde(default)]
    pub select: Vec<RawCodexSelectItem>,
    /// Top-level `from:` binding — an alias pointing at a table or subquery.
    /// Optional only because the YAML type system allows it; in practice every
    /// view in this codebase declares a `from:`.
    #[serde(default)]
    pub from: Option<RawCodexViewSource>,
    /// Subsequent `join:` bindings — same alias-to-source shape.
    #[serde(default)]
    pub join: Vec<RawCodexViewSource>,
}

#[derive(Debug, Deserialize)]
pub struct RawCodexViewAggregate {
    pub name: String,
    #[serde(default)]
    pub list: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

impl RawCodexViewAggregate {
    pub fn shape(&self, view_name: &str) -> LaxResult<ReadShape> {
        resolve_shape(view_name, &self.list, &self.detail)
    }
}

#[derive(Debug, Deserialize)]
pub struct RawCodexViewEntity {
    pub aggregate: String,
    pub name: String,
    #[serde(default)]
    pub list: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

impl RawCodexViewEntity {
    pub fn shape(&self, view_name: &str) -> LaxResult<ReadShape> {
        resolve_shape(view_name, &self.list, &self.detail)
    }
}

fn resolve_shape(view_name: &str, list: &Option<String>, detail: &Option<String>) -> LaxResult<ReadShape> {
    match (list, detail) {
        (Some(surface_name), None) => Ok(ReadShape::List(surface_name.clone())),
        (None, Some(surface_name)) => Ok(ReadShape::Detail(surface_name.clone())),
        (Some(_), Some(_)) => Err(CodexError::semantic(format!(
            "view `{view_name}`: cannot declare both `list:` and `detail:`"
        ))),
        (None, None) => Err(CodexError::semantic(format!(
            "view `{view_name}`: must declare either `list: <name>` or `detail: <name>`"
        ))),
    }
}

/// YAML shape for a view's `from:` or each `join:` entry. Only the alias and
/// the underlying source (`table:` or `subquery.from:`) are captured — the
/// SQL-generation fields (`on`, `type`, `where`, `order_by`, ...) are ignored.
#[derive(Debug, Deserialize)]
pub struct RawCodexViewSource {
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub subquery: Option<RawCodexViewSubquery>,
    #[serde(rename = "as")]
    pub alias: String,
}

/// Subquery source — we only need its `from:` to know which underlying table
/// it reads from. Everything else (select list, filters, ordering) is SQL
/// rendering concern handled by the Python schema tool.
#[derive(Debug, Deserialize)]
pub struct RawCodexViewSubquery {
    pub from: String,
}

impl RawCodexViewSource {
    /// Resolve the backing relation name — either a direct table or the
    /// subquery's underlying `from:`.
    fn relation_name(&self) -> Option<&str> {
        self.table
            .as_deref()
            .or_else(|| self.subquery.as_ref().map(|subquery| subquery.from.as_str()))
    }
}

impl RawCodexView {
    /// Resolve `x_partials` by pushing each referenced select-partial's expanded
    /// columns onto `self.select`.
    pub fn expand_partials(&mut self, partials: &HashMap<String, RawCodexPartial>) -> LaxResult<()> {
        let references = std::mem::take(&mut self.x_partials);
        for reference in references {
            let partial_name = reference.partial_name();
            let partial = partials.get(partial_name).ok_or_else(|| {
                CodexError::partials(format!("view `{}`: unknown partial `{partial_name}`", self.name))
            })?;
            let RawCodexPartial::Select(select_partial) = partial else {
                return Err(CodexError::partials(format!(
                    "view `{}`: partial `{partial_name}` is not a select partial (views can only reference x_partial_select)",
                    self.name
                )));
            };
            for column in select_partial.expand(reference.from_override()) {
                self.select.push(RawCodexSelectItem::Column(column));
            }
        }
        Ok(())
    }

    /// Count select items declaring `x_role: Id`. Used by semantic checks.
    pub fn count_id_roles(&self) -> usize {
        self.select
            .iter()
            .filter(|item| item.role() == Some(CodexRole::Id))
            .count()
    }

    /// Map every declared alias in `from:` and `join:` to the relation name
    /// it ultimately reads from. Subqueries are unwrapped to their underlying
    /// `from: <table>`.
    fn alias_to_relation(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        if let Some(source) = &self.from
            && let Some(relation) = source.relation_name()
        {
            map.insert(source.alias.clone(), relation.to_string());
        }
        for source in &self.join {
            if let Some(relation) = source.relation_name() {
                map.insert(source.alias.clone(), relation.to_string());
            }
        }
        map
    }

    /// Produce the runtime `CodexRelation` (View kind). Column types are
    /// inherited from the underlying relation's column via the alias map,
    /// so `SELECT snap_flows.version AS version` correctly records `Number`,
    /// not the view-default `Text`. Expressions without aliases are dropped.
    pub fn into_relation(&self, relations: &HashMap<String, CodexRelation>) -> CodexRelation {
        let alias_to_relation = self.alias_to_relation();
        let resolve_type = |from_alias: &str, column_name: &str| -> Option<CodexType> {
            let relation_name = alias_to_relation.get(from_alias)?;
            let relation = relations.get(relation_name)?;
            relation.column(column_name).map(|column| column.data_type.clone())
        };
        let columns = self
            .select
            .iter()
            .filter_map(|item| item.into_column(&resolve_type))
            .collect();
        CodexRelation {
            name: self.name.clone(),
            kind: CodexKind::View,
            columns,
        }
    }
}
