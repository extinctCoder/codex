use lax_shared::dtos::db::codex::{CodexColumn, CodexRole, CodexType};
use serde::Deserialize;
use serde_json::Value;

/// YAML shape for a view's select item — one of four shapes.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawCodexSelectItem {
    JsonAgg(RawCodexSelectJsonAggregate),
    JsonObject(RawCodexSelectJsonObject),
    Column(RawCodexSelectColumn),
    Expression(RawCodexSelectExpression),
}

/// Shared per-item overrides for role and permission flags.
#[derive(Debug, Default, Clone, Copy, Deserialize)]
pub struct RawCodexSelectOverrides {
    #[serde(default)]
    pub x_role: Option<CodexRole>,
    #[serde(default)]
    pub x_filterable: Option<bool>,
    #[serde(default)]
    pub x_searchable: Option<bool>,
    #[serde(default)]
    pub x_selectable: Option<bool>,
}

impl RawCodexSelectOverrides {
    pub fn apply(&self, name: String, data_type: CodexType) -> CodexColumn {
        let role = self.x_role.unwrap_or(CodexRole::Data);
        CodexColumn {
            name,
            data_type,
            role,
            filterable: self.x_filterable.unwrap_or(false),
            searchable: self.x_searchable.unwrap_or(false),
            selectable: self.x_selectable.unwrap_or_else(|| role.default_selectable()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RawCodexSelectColumn {
    pub column: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default, rename = "as")]
    pub alias: Option<String>,
    #[serde(flatten)]
    pub overrides: RawCodexSelectOverrides,
}

#[derive(Debug, Deserialize)]
pub struct RawCodexSelectExpression {
    pub sql: String,
    #[serde(default, rename = "as")]
    pub alias: Option<String>,
    #[serde(flatten)]
    pub overrides: RawCodexSelectOverrides,
}

#[derive(Debug, Deserialize)]
pub struct RawCodexSelectJsonObject {
    #[serde(rename = "as")]
    pub alias: String,
    pub json_object: Value,
    #[serde(flatten)]
    pub overrides: RawCodexSelectOverrides,
}

#[derive(Debug, Deserialize)]
pub struct RawCodexSelectJsonAggregate {
    #[serde(rename = "as")]
    pub alias: String,
    pub from: String,
    pub json_agg: Value,
    #[serde(flatten)]
    pub overrides: RawCodexSelectOverrides,
}

impl RawCodexSelectItem {
    /// Produce the runtime `CodexColumn`. Column types are inherited from the
    /// underlying relation (table or already-compiled view) when `from:` is
    /// declared — `resolve_type(from, column)` returns the source column's
    /// type. Falls back to `Text` when the alias or column can't be resolved.
    /// `Expression` without an alias is skipped.
    pub fn into_column(&self, resolve_type: &dyn Fn(&str, &str) -> Option<CodexType>) -> Option<CodexColumn> {
        match self {
            Self::Column(item) => {
                let name = item.alias.clone().unwrap_or_else(|| item.column.clone());
                let data_type = item
                    .from
                    .as_deref()
                    .and_then(|from| resolve_type(from, &item.column))
                    .unwrap_or(CodexType::Text);
                Some(item.overrides.apply(name, data_type))
            }
            Self::Expression(item) => item
                .alias
                .as_ref()
                .map(|alias| item.overrides.apply(alias.clone(), CodexType::Text)),
            Self::JsonObject(item) => Some(item.overrides.apply(item.alias.clone(), CodexType::Json)),
            Self::JsonAgg(item) => Some(item.overrides.apply(item.alias.clone(), CodexType::Json)),
        }
    }

    /// The declared `x_role` (if any). Used by semantic checks.
    pub fn role(&self) -> Option<CodexRole> {
        self.overrides_ref().x_role
    }

    /// The declared `x_selectable` (if any). Used by semantic checks.
    pub fn x_selectable(&self) -> Option<bool> {
        self.overrides_ref().x_selectable
    }

    /// Best-effort human name for error messages.
    pub fn display_name(&self) -> Option<&str> {
        match self {
            Self::Column(item) => item.alias.as_deref().or(Some(item.column.as_str())),
            Self::Expression(item) => item.alias.as_deref(),
            Self::JsonObject(item) => Some(&item.alias),
            Self::JsonAgg(item) => Some(&item.alias),
        }
    }

    fn overrides_ref(&self) -> &RawCodexSelectOverrides {
        match self {
            Self::Column(item) => &item.overrides,
            Self::Expression(item) => &item.overrides,
            Self::JsonObject(item) => &item.overrides,
            Self::JsonAgg(item) => &item.overrides,
        }
    }
}
