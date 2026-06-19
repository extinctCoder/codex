use std::collections::HashMap;

use serde::Deserialize;

use crate::relational_basket::codex::dtos::{RawCodexColumnDefinition, RawCodexSelectColumn, RawCodexSelectOverrides};

/// YAML shape for an `x_partials:` entry. Either a reusable column set (for tables)
/// or a reusable select fragment (for views).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawCodexPartial {
    Columns(RawCodexColumnsPartial),
    Select(RawCodexSelectPartial),
}

#[derive(Debug, Deserialize)]
pub struct RawCodexColumnsPartial {
    pub columns: HashMap<String, RawCodexColumnDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct RawCodexSelectPartial {
    #[serde(default)]
    pub from: Option<String>,
    pub select: HashMap<String, RawCodexSelectPartialItem>,
}

/// A select-partial entry — leaner than `RawCodexSelectColumn` because the alias is
/// the map key and `from:` is inherited from the partial (optionally overridden
/// at the reference site).
#[derive(Debug, Deserialize)]
pub struct RawCodexSelectPartialItem {
    #[serde(default)]
    pub column: Option<String>,
    #[serde(flatten)]
    pub overrides: RawCodexSelectOverrides,
}

/// How a view refers to a partial in its `x_partials:` list.
/// Tables use bare strings (`Vec<String>`); views allow qualified references.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawCodexPartialReference {
    ByName(String),
    Qualified {
        partial: String,
        #[serde(default)]
        from: Option<String>,
    },
}

impl RawCodexPartialReference {
    pub fn partial_name(&self) -> &str {
        match self {
            Self::ByName(name) => name,
            Self::Qualified { partial, .. } => partial,
        }
    }

    pub fn from_override(&self) -> Option<&str> {
        match self {
            Self::ByName(_) => None,
            Self::Qualified { from, .. } => from.as_deref(),
        }
    }
}

impl RawCodexSelectPartial {
    /// Expand this partial into ready-to-use `RawCodexSelectColumn` entries.
    /// `from_override` comes from the qualified reference (if any).
    pub fn expand(&self, from_override: Option<&str>) -> Vec<RawCodexSelectColumn> {
        let from_alias = from_override.map(str::to_string).or_else(|| self.from.clone());

        self.select
            .iter()
            .map(|(alias, item)| {
                let source_column = item.column.clone().unwrap_or_else(|| alias.clone());
                let alias_output = if source_column == *alias {
                    None
                } else {
                    Some(alias.clone())
                };
                RawCodexSelectColumn {
                    column: source_column,
                    from: from_alias.clone(),
                    alias: alias_output,
                    overrides: item.overrides,
                }
            })
            .collect()
    }
}
