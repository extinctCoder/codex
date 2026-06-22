//! Declarative schema & query compiler — one YAML as the source of truth.

mod ast;
mod catalog;
mod error;

pub use ast::{
    parse_schema, Column, EnumType, FromClause, Index, Join, JoinKind, PrimaryKey, SchemaDocument,
    SelectColumn, SelectExpression, SelectItem, Table, View,
};
pub use catalog::{
    build_catalog, Catalog, CatalogColumn, CatalogRelation, CatalogType, RelationKind,
};

pub use error::HolocronError;
