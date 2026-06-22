use serde::Deserialize;

use crate::ast::enum_type::EnumType;
use crate::ast::table::Table;
use crate::ast::view::View;

/// A whole parsed schema file: the top-level `types`/`tables`/`views` lists.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaDocument {
    #[serde(default)]
    pub types: Vec<EnumType>,
    #[serde(default)]
    pub tables: Vec<Table>,
    #[serde(default)]
    pub views: Vec<View>,
}
