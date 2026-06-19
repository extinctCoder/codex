use std::sync::Arc;

use lax_domain::contracts::infrastructure::{drivers::sql::SqlDriver, persistence::codex::Codex};
use lax_shared::{
    dtos::db::{
        codex::{CodexAggregate, CodexRole, CodexSink},
        mason::{MasonEffect, MasonEffectTarget, MasonRow},
    },
    error::LaxResult,
};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::relational_basket::mason::sql::{bind_with_enum_cast, push_ident_list, quote_ident};

const COL_IS_DELETED: &str = "is_deleted";

/// Apply sink effects for one aggregate. Skips aggregates without declared sinks.
pub async fn sink_effects(
    sql_driver: &Arc<dyn SqlDriver<Postgres>>,
    codex: &Arc<dyn Codex>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    effects: Vec<MasonEffect>,
) -> LaxResult<()> {
    lax_shared::command_span!("mason::sink", aggregate_id);
    let Some(aggregate) = codex.aggregate(aggregate_type) else {
        tracing::warn!(aggregate_type = %aggregate_type, "aggregate not in codex, skipping sink");
        return Ok(());
    };
    if effects.is_empty() || aggregate.sinks.is_empty() {
        return Ok(());
    }

    super::validate::sink_entity_keys(codex.as_ref(), aggregate, &effects)?;

    let id_column = aggregate.root.id_column.as_str();
    let mut builders: Vec<QueryBuilder<'_, Postgres>> = Vec::with_capacity(effects.len());

    for effect in effects {
        match effect {
            MasonEffect::Upsert { target, fields } | MasonEffect::Revise { target, fields } => {
                push_upsert(
                    &mut builders,
                    codex,
                    aggregate,
                    id_column,
                    aggregate_id,
                    &target,
                    &fields,
                );
            }
            MasonEffect::Delete { target } => {
                push_delete(&mut builders, aggregate, id_column, aggregate_id, &target);
            }
        }
    }

    if builders.is_empty() {
        return Ok(());
    }
    sql_driver.transaction(builders).await?;
    Ok(())
}

fn push_upsert<'a>(
    builders: &mut Vec<QueryBuilder<'a, Postgres>>,
    codex: &Arc<dyn Codex>,
    aggregate: &CodexAggregate,
    id_column: &str,
    aggregate_id: Uuid,
    target: &MasonEffectTarget,
    fields: &MasonRow,
) {
    match target {
        MasonEffectTarget::Root => {
            if let Some(sink) = root_sink(aggregate) {
                builders.push(build_upsert_root(&sink.relation, id_column, aggregate_id, fields));
            }
        }
        MasonEffectTarget::Entity { name, keys } => {
            if let Some(sink) = entity_sink(aggregate, name) {
                let references = collect_references(codex.as_ref(), &sink.relation);
                builders.push(build_upsert_entity(
                    &sink.relation,
                    id_column,
                    &references,
                    aggregate_id,
                    keys,
                    fields,
                ));
            }
        }
    }
}

fn push_delete<'a>(
    builders: &mut Vec<QueryBuilder<'a, Postgres>>,
    aggregate: &CodexAggregate,
    id_column: &str,
    aggregate_id: Uuid,
    target: &MasonEffectTarget,
) {
    match target {
        MasonEffectTarget::Root => {
            if let Some(sink) = root_sink(aggregate) {
                builders.push(build_soft_delete_root(&sink.relation, id_column, aggregate_id));
            }
        }
        MasonEffectTarget::Entity { name, keys } => {
            if let Some(sink) = entity_sink(aggregate, name) {
                builders.push(build_soft_delete_entity(&sink.relation, id_column, aggregate_id, keys));
            }
        }
    }
}

// ── builders ───────────────────────────────────────────────────────────────

fn build_upsert_root<'a>(
    table: &str,
    id_column: &str,
    aggregate_id: Uuid,
    fields: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("INSERT INTO ");
    builder.push(quote_ident(table)).push(" (");

    let mut column_names: Vec<String> = vec![id_column.to_string()];
    for (name, _) in fields.columns() {
        column_names.push(name.to_string());
    }
    push_ident_list(&mut builder, &column_names);
    builder.push(") VALUES (");
    builder.push_bind(aggregate_id);
    for (_, value) in fields.columns() {
        builder.push(", ");
        bind_with_enum_cast(&mut builder, value, None);
    }
    builder.push(") ");
    push_on_conflict_update(&mut builder, &[id_column.to_string()], &column_names[1..]);
    builder
}

fn build_upsert_entity<'a>(
    table: &str,
    id_column: &str,
    reference_columns: &[String],
    aggregate_id: Uuid,
    keys: &MasonRow,
    fields: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("INSERT INTO ");
    builder.push(quote_ident(table)).push(" (");

    // Column order: id, keys (excluding id), fields
    let mut columns: Vec<(String, Option<lax_shared::dtos::db::mason::MasonValue>)> = Vec::new();
    columns.push((id_column.to_string(), None));
    for (name, value) in keys.columns() {
        if name.as_ref() == id_column {
            continue;
        }
        columns.push((name.to_string(), Some(value.clone())));
    }
    for (name, value) in fields.columns() {
        columns.push((name.to_string(), Some(value.clone())));
    }

    let names: Vec<String> = columns.iter().map(|(name, _)| name.clone()).collect();
    push_ident_list(&mut builder, &names);
    builder.push(") VALUES (");
    for (index, (_, value)) in columns.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        match value {
            Some(mason_value) => {
                bind_with_enum_cast(&mut builder, mason_value, None);
            }
            None => {
                // This is the aggregate id slot.
                builder.push_bind(aggregate_id);
            }
        }
    }
    builder.push(") ");

    let mut conflict_columns: Vec<String> = vec![id_column.to_string()];
    for reference in reference_columns {
        conflict_columns.push(reference.clone());
    }
    // Update only the field columns (not id/key).
    let update_columns: Vec<String> = fields.columns().iter().map(|(name, _)| name.to_string()).collect();
    push_on_conflict_update(&mut builder, &conflict_columns, &update_columns);
    builder
}

fn build_soft_delete_root<'a>(table: &str, id_column: &str, aggregate_id: Uuid) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("UPDATE ");
    builder
        .push(quote_ident(table))
        .push(" SET ")
        .push(quote_ident(COL_IS_DELETED))
        .push(" = TRUE WHERE ")
        .push(quote_ident(id_column))
        .push(" = ")
        .push_bind(aggregate_id);
    builder
}

fn build_soft_delete_entity<'a>(
    table: &str,
    id_column: &str,
    aggregate_id: Uuid,
    keys: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("UPDATE ");
    builder
        .push(quote_ident(table))
        .push(" SET ")
        .push(quote_ident(COL_IS_DELETED))
        .push(" = TRUE WHERE ")
        .push(quote_ident(id_column))
        .push(" = ")
        .push_bind(aggregate_id);
    for (name, value) in keys.columns() {
        if name.as_ref() == id_column {
            continue;
        }
        builder.push(" AND ").push(quote_ident(name.as_ref())).push(" = ");
        bind_with_enum_cast(&mut builder, value, None);
    }
    builder
}

// ── lookup helpers ─────────────────────────────────────────────────────────

fn root_sink(aggregate: &CodexAggregate) -> Option<&CodexSink> {
    aggregate.sinks.iter().find(|sink| sink.entity.is_none())
}

fn entity_sink<'a>(aggregate: &'a CodexAggregate, entity_name: &str) -> Option<&'a CodexSink> {
    aggregate
        .sinks
        .iter()
        .find(|sink| sink.entity.as_deref() == Some(entity_name))
}

fn collect_references(codex: &dyn Codex, relation_name: &str) -> Vec<String> {
    codex
        .relation(relation_name)
        .map(|relation| {
            relation
                .columns
                .iter()
                .filter(|column| column.role == CodexRole::Reference)
                .map(|column| column.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

// ── SQL emission primitives ────────────────────────────────────────────────

fn push_on_conflict_update(
    builder: &mut QueryBuilder<'_, Postgres>,
    conflict_columns: &[String],
    update_columns: &[String],
) {
    builder.push("ON CONFLICT (");
    push_ident_list(builder, conflict_columns);
    builder.push(") DO UPDATE SET ");
    for (index, name) in update_columns.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        builder
            .push(quote_ident(name))
            .push(" = EXCLUDED.")
            .push(quote_ident(name));
    }
}
