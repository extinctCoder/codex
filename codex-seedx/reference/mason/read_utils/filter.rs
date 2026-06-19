use lax_shared::dtos::db::{
    codex::{CodexRelation, CodexType},
    mason::{MasonFilter, MasonQuery, MasonValue},
};
use sqlx::{Postgres, QueryBuilder};

use crate::relational_basket::mason::sql::{bind_with_enum_cast, quote_ident};

/// Column name used to scope read queries to the caller's project. Every view
/// that can be hit via an authenticated endpoint must expose this column —
/// either directly (root views of aggregates that own a project binding) or
/// via a JOIN inheritance (entity views select their parent's column).
const PROJECT_ID_COLUMN: &str = "identity_project_id";

/// Emit the full WHERE clause — combines `project_ids` scoping, the parsed
/// filter tree, and free-text search across `x_searchable` columns.
/// No-op when all three are empty.
pub(super) fn emit_where(query: &MasonQuery, relation: &CodexRelation, builder: &mut QueryBuilder<'_, Postgres>) {
    let has_project_filter = !query.project_ids.is_empty();
    let has_user_filter = query.filter.is_some();
    let has_search = search_would_emit(query, relation);
    if !has_project_filter && !has_user_filter && !has_search {
        return;
    }
    builder.push(" WHERE ");
    let mut needs_and = false;
    if has_project_filter {
        builder.push(quote_ident(PROJECT_ID_COLUMN)).push(" IN (");
        for (index, project_id) in query.project_ids.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind(project_id.clone());
        }
        builder.push(")");
        needs_and = true;
    }
    if let Some(filter) = &query.filter {
        if needs_and {
            builder.push(" AND ");
        }
        emit(filter, relation, builder);
        needs_and = true;
    }
    if has_search {
        if needs_and {
            builder.push(" AND ");
        }
        super::search::emit(builder, relation, query.search.as_deref());
    }
}

/// Preview whether `search::emit` would produce a clause, without actually
/// emitting it. Used to decide if we need the `WHERE` keyword at all.
fn search_would_emit(query: &MasonQuery, relation: &CodexRelation) -> bool {
    let Some(term) = query.search.as_deref() else {
        return false;
    };
    if term.trim().is_empty() {
        return false;
    }
    relation.searchable().next().is_some()
}

fn emit(filter: &MasonFilter, relation: &CodexRelation, builder: &mut QueryBuilder<'_, Postgres>) {
    match filter {
        MasonFilter::And(children) => emit_junction(" AND ", children, relation, builder),
        MasonFilter::Or(children) => emit_junction(" OR ", children, relation, builder),

        MasonFilter::Eq(column, value) => emit_leaf(column, " = ", value, relation, builder),
        MasonFilter::NotEq(column, value) => emit_leaf(column, " <> ", value, relation, builder),
        MasonFilter::Gt(column, value) => emit_leaf(column, " > ", value, relation, builder),
        MasonFilter::Gte(column, value) => emit_leaf(column, " >= ", value, relation, builder),
        MasonFilter::Lt(column, value) => emit_leaf(column, " < ", value, relation, builder),
        MasonFilter::Lte(column, value) => emit_leaf(column, " <= ", value, relation, builder),

        MasonFilter::Like(column, pattern) => {
            builder
                .push(quote_ident(column))
                .push(" ILIKE ")
                .push_bind(format!("%{pattern}%"));
        }
        MasonFilter::StartsWith(column, prefix) => {
            builder
                .push(quote_ident(column))
                .push(" ILIKE ")
                .push_bind(format!("{prefix}%"));
        }
        MasonFilter::EndsWith(column, suffix) => {
            builder
                .push(quote_ident(column))
                .push(" ILIKE ")
                .push_bind(format!("%{suffix}"));
        }

        MasonFilter::In(column, values) => emit_set(column, " IN (", values, "FALSE", relation, builder),
        MasonFilter::NotIn(column, values) => emit_set(column, " NOT IN (", values, "TRUE", relation, builder),

        MasonFilter::IsNull(column, true) => {
            builder.push(quote_ident(column)).push(" IS NULL");
        }
        MasonFilter::IsNull(column, false) => {
            builder.push(quote_ident(column)).push(" IS NOT NULL");
        }
    }
}

fn emit_leaf(
    column: &str,
    operator: &str,
    value: &MasonValue,
    relation: &CodexRelation,
    builder: &mut QueryBuilder<'_, Postgres>,
) {
    builder.push(quote_ident(column)).push(operator);
    let (coerced, enum_name) = resolve_bind(value, column, relation);
    bind_with_enum_cast(builder, &coerced, enum_name);
}

fn emit_set(
    column: &str,
    opening: &str,
    values: &[MasonValue],
    empty_literal: &str,
    relation: &CodexRelation,
    builder: &mut QueryBuilder<'_, Postgres>,
) {
    if values.is_empty() {
        builder.push(empty_literal);
        return;
    }
    builder.push(quote_ident(column)).push(opening);
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        let (coerced, enum_name) = resolve_bind(value, column, relation);
        bind_with_enum_cast(builder, &coerced, enum_name);
    }
    builder.push(")");
}

/// Coerce a filter value into the variant matching the column's Codex type
/// and return the optional enum cast name for enum-typed columns. RSQL hands
/// every value in as `Text`; this is where we make it match the SQL column.
/// View column types are inherited from their source table by the Codex
/// compiler, so no heuristics are needed — the declared `data_type` is the
/// single source of truth.
fn resolve_bind<'a>(value: &MasonValue, column: &str, relation: &'a CodexRelation) -> (MasonValue, Option<&'a String>) {
    match relation.column(column) {
        Some(column_def) => {
            let coerced = value.coerce_to(&column_def.data_type);
            let enum_name = match &column_def.data_type {
                CodexType::Enum(name) => Some(name),
                _ => None,
            };
            (coerced, enum_name)
        }
        None => (value.clone(), None),
    }
}

fn emit_junction(
    separator: &str,
    children: &[MasonFilter],
    relation: &CodexRelation,
    builder: &mut QueryBuilder<'_, Postgres>,
) {
    if children.is_empty() {
        builder.push("TRUE");
        return;
    }
    builder.push("(");
    for (index, child) in children.iter().enumerate() {
        if index > 0 {
            builder.push(separator);
        }
        emit(child, relation, builder);
    }
    builder.push(")");
}
