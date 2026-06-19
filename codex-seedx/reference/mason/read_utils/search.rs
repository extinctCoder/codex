use lax_shared::dtos::db::codex::CodexRelation;
use sqlx::{Postgres, QueryBuilder};

use crate::relational_basket::mason::sql::quote_ident;

/// Emit `(col1 ILIKE $1 OR col2 ILIKE $1 OR ...)` across all searchable columns.
/// No-op if search is empty or no searchable columns exist on this relation.
pub(super) fn emit(builder: &mut QueryBuilder<'_, Postgres>, relation: &CodexRelation, search: Option<&str>) -> bool {
    let term = match search {
        Some(raw) if !raw.trim().is_empty() => raw.trim(),
        _ => return false,
    };
    let searchable: Vec<&str> = relation.searchable().map(|column| column.name.as_str()).collect();
    if searchable.is_empty() {
        return false;
    }
    let pattern = format!("%{term}%");
    builder.push("(");
    for (index, column) in searchable.iter().enumerate() {
        if index > 0 {
            builder.push(" OR ");
        }
        builder
            .push(quote_ident(column))
            .push(" ILIKE ")
            .push_bind(pattern.clone());
    }
    builder.push(")");
    true
}
