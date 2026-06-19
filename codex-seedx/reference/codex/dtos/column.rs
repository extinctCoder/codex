use std::collections::HashSet;

use lax_shared::dtos::db::codex::{CodexColumn, CodexRole, CodexType};
use serde::Deserialize;

/// YAML shape for a column inside `tables.*.columns.{name}:`.
#[derive(Debug, Deserialize)]
pub struct RawCodexColumnDefinition {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub x_role: Option<CodexRole>,
}

impl RawCodexColumnDefinition {
    pub fn into_column(&self, name: &str, enums: &HashSet<String>) -> CodexColumn {
        let role = self.x_role.unwrap_or(CodexRole::Data);
        CodexColumn {
            name: name.to_string(),
            data_type: CodexType::from_schema_name(&self.type_name, enums),
            role,
            filterable: false,
            searchable: false,
            selectable: role.default_selectable(),
        }
    }
}
