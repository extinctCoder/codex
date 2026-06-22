//! The YAML vocabulary as typed Rust structs — the parsed source shape (the AST).
//!
//! These mirror the *input* one-to-one; the normalized catalog (the IR) is built
//! from them in a later phase. `#[serde(deny_unknown_fields)]` makes an unrecognised
//! key a shape error rather than a silent drop.

mod document;
mod enum_type;
mod join;
mod parse;
mod select;
mod table;
mod view;

pub use document::SchemaDocument;
pub use enum_type::EnumType;
pub use join::{FromClause, Join, JoinKind};
pub use parse::parse_schema;
pub use select::{SelectColumn, SelectExpression, SelectItem};
pub use table::{Column, Index, PrimaryKey, Table};
pub use view::View;
