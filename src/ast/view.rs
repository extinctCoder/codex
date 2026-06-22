use serde::Deserialize;

use crate::ast::join::{FromClause, Join};
use crate::ast::select::SelectItem;

/// A `CREATE VIEW … AS`: a saved query exposed like a table. Clause fields after
/// `select`/`from`/`join` are raw SQL, passed through verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub name: String,
    #[serde(default)]
    pub or_replace: bool,
    pub from: FromClause,
    #[serde(default)]
    pub join: Vec<Join>,
    pub select: Vec<SelectItem>,
    #[serde(default)]
    pub r#where: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub having: Option<String>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub distinct_on: Option<Vec<String>>,
}
