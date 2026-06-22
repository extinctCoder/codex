use crate::catalog::data_type::CatalogType;

/// A resolved column in the catalog: its name, type, and nullability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogColumn {
    pub name: String,
    pub data_type: CatalogType,
    /// `true` if the column permits NULL.
    pub nullable: bool,
}
