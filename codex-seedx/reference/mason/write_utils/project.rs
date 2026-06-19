use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use lax_domain::{
    contracts::infrastructure::{drivers::sql::SqlDriver, persistence::codex::Codex},
    shared::{aggregate::Provenance, value_objects::Actor},
};
use lax_shared::{
    dtos::db::{
        codex::{CodexAggregate, CodexEntity, CodexRole, CodexType},
        mason::{MasonEffect, MasonEffectTarget, MasonRow, MasonValue},
    },
    error::{LaxResult, MasonError},
};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::relational_basket::mason::{
    sql::{bind_with_enum_cast, push_ident_list, quote_ident},
    write_utils::validate::projection_entity_keys,
};

const COL_VERSION: &str = "version";
const COL_IS_DELETED: &str = "is_deleted";

type EnumMap = HashMap<String, String>;

/// Apply projection effects for one aggregate. Skips aggregates without a Codex definition.
///
/// `aggregate_version` is the per-aggregate sequence value this write applies at. It is
/// bound directly into every snap row's `version` column, eliminating the read–write race
/// that occurred when versions were computed from `MAX(version) + 1` under concurrent
/// writers. Source of truth is `LaxEvent.aggregate_version`, stamped by the Lua's CAS check.
#[expect(
    clippy::too_many_arguments,
    reason = "projection entry point binds together the codex, the aggregate identity, the aggregate_version, the effects, and the provenance — each is independent input and grouping them would only hide that"
)]
pub async fn project_effects(
    sql_driver: &Arc<dyn SqlDriver<Postgres>>,
    codex: &Arc<dyn Codex>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    effects: Vec<MasonEffect>,
    provenance: &Provenance,
    occurred_at: DateTime<Utc>,
) -> LaxResult<u64> {
    lax_shared::command_span!("mason::project", aggregate_id);
    if effects.is_empty() {
        return Ok(0);
    }
    let Some(aggregate) = codex.aggregate(aggregate_type) else {
        tracing::warn!(aggregate_type = %aggregate_type, "aggregate not in codex, skipping projection");
        return Ok(0);
    };

    projection_entity_keys(codex.as_ref(), aggregate, &effects)?;

    let version = i64::try_from(aggregate_version).expect("aggregate version fits in i64 (postgres bigint)");
    let audit = build_audit_row(&provenance.actor, provenance.correlation_id, occurred_at);
    let is_create = effects.iter().any(|effect| {
        matches!(
            effect,
            MasonEffect::Upsert {
                target: MasonEffectTarget::Root,
                ..
            }
        )
    });

    let projection_builders: Vec<QueryBuilder<'_, Postgres>> = if is_create {
        build_create(codex.as_ref(), aggregate, aggregate_id, version, &effects, &audit)
    } else {
        build_copy_forward(codex.as_ref(), aggregate, aggregate_id, version, &effects, &audit)
    };

    if projection_builders.is_empty() {
        return Ok(0);
    }

    // Per-aggregate transaction-scoped advisory lock prepended to the
    // projection transaction. Two concurrent Cartographer tasks for the same
    // aggregate (e.g. v=1 and v=2 racing on tokio::spawn) hash to the same
    // lock key; the second task blocks on the SELECT until the first task's
    // transaction commits and releases the lock. By the time the second task
    // runs its `INSERT INTO snap ... SELECT FROM snap WHERE version = $v-1`,
    // the prior version's row is committed and visible, so the SELECT
    // matches and the copy-forward succeeds. Race becomes impossible.
    //
    // The lock is released automatically at transaction end (commit or
    // rollback). Different aggregates hash to different keys and run in
    // parallel. See `docs/bugs/projection-copy-forward-race.md`.
    let mut builders: Vec<QueryBuilder<'_, Postgres>> = Vec::with_capacity(projection_builders.len() + 1);
    let mut lock_builder: QueryBuilder<'_, Postgres> = QueryBuilder::new("SELECT pg_advisory_xact_lock(hashtext(");
    lock_builder.push_bind(format!("{}:{}", aggregate_type, aggregate_id));
    lock_builder.push("))");
    builders.push(lock_builder);
    builders.extend(projection_builders);

    let rows_affected = sql_driver.transaction(builders).await?;

    // Defence-in-depth: a copy-forward path that affected zero rows means the
    // prior version's snap row was missing even after we held the advisory
    // lock. Under correct operation this should be unreachable — the lock
    // guarantees the prior commit is visible. If we ever see this, something
    // else is broken (event lost upstream, lock didn't apply, etc.).
    // Returning Err keeps the message in the consumer PEL for drain retry,
    // rather than silently ACK'ing under unexpected state.
    if !is_create && rows_affected == 0 {
        return Err(MasonError::projection_no_op(
            aggregate_type,
            aggregate_id,
            aggregate_version,
        ));
    }

    Ok(rows_affected)
}

// ── dispatch: create vs copy-forward ───────────────────────────────────────

fn build_create<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    aggregate_id: Uuid,
    version: i64,
    effects: &[MasonEffect],
    audit: &MasonRow,
) -> Vec<QueryBuilder<'a, Postgres>> {
    let mut builders = Vec::new();
    for effect in effects {
        match effect {
            MasonEffect::Upsert {
                target: MasonEffectTarget::Root,
                fields,
            } => {
                builders.push(build_root_create(
                    codex,
                    aggregate,
                    aggregate_id,
                    version,
                    fields,
                    audit,
                ));
            }
            MasonEffect::Upsert {
                target: MasonEffectTarget::Entity { name, keys },
                fields,
            } => {
                if let Some(entity) = aggregate.entity(name) {
                    builders.push(build_entity_insert_at_version(
                        codex,
                        aggregate,
                        entity,
                        aggregate_id,
                        version,
                        keys,
                        fields,
                        audit,
                    ));
                }
            }
            _ => {}
        }
    }
    builders
}

fn build_copy_forward<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    aggregate_id: Uuid,
    version: i64,
    effects: &[MasonEffect],
    audit: &MasonRow,
) -> Vec<QueryBuilder<'a, Postgres>> {
    let mut builders = Vec::new();
    let mut root_overrides = MasonRow::from_static(vec![]);
    let mut root_is_deleted = false;
    let mut entity_effects: HashMap<&str, Vec<&MasonEffect>> = HashMap::new();

    for effect in effects {
        match effect {
            MasonEffect::Revise {
                target: MasonEffectTarget::Root,
                fields,
            } => {
                root_overrides.extend(fields.clone());
            }
            MasonEffect::Delete {
                target: MasonEffectTarget::Root,
            } => {
                root_is_deleted = true;
            }
            MasonEffect::Upsert {
                target: MasonEffectTarget::Entity { name, .. },
                ..
            }
            | MasonEffect::Revise {
                target: MasonEffectTarget::Entity { name, .. },
                ..
            }
            | MasonEffect::Delete {
                target: MasonEffectTarget::Entity { name, .. },
            } => {
                entity_effects.entry(name.as_ref()).or_default().push(effect);
            }
            _ => {}
        }
    }

    // Entities copy-forward BEFORE root — both read MAX(version) on root so order matters.
    for entity in &aggregate.entities {
        let effects_for_entity = entity_effects
            .get(entity.name.as_str())
            .map(|vector| vector.as_slice())
            .unwrap_or(&[]);

        let excluded_keys: Vec<&MasonRow> = effects_for_entity
            .iter()
            .filter_map(|effect| match effect {
                MasonEffect::Revise {
                    target: MasonEffectTarget::Entity { keys, .. },
                    ..
                }
                | MasonEffect::Delete {
                    target: MasonEffectTarget::Entity { keys, .. },
                } => Some(keys),
                _ => None,
            })
            .collect();

        builders.push(build_entity_copy_forward(
            codex,
            aggregate,
            entity,
            aggregate_id,
            version,
            &excluded_keys,
        ));

        for effect in effects_for_entity {
            match effect {
                MasonEffect::Upsert {
                    target: MasonEffectTarget::Entity { keys, .. },
                    fields,
                } => {
                    builders.push(build_entity_add(
                        codex,
                        aggregate,
                        entity,
                        aggregate_id,
                        version,
                        keys,
                        fields,
                        audit,
                    ));
                }
                MasonEffect::Revise {
                    target: MasonEffectTarget::Entity { keys, .. },
                    fields,
                } => {
                    builders.push(build_entity_revise(
                        codex,
                        aggregate,
                        entity,
                        aggregate_id,
                        version,
                        keys,
                        fields,
                        audit,
                    ));
                }
                MasonEffect::Delete {
                    target: MasonEffectTarget::Entity { keys, .. },
                } => {
                    builders.push(build_entity_remove(
                        codex,
                        aggregate,
                        entity,
                        aggregate_id,
                        version,
                        keys,
                        audit,
                    ));
                }
                _ => {}
            }
        }
    }

    builders.push(build_root_copy_forward(
        codex,
        aggregate,
        aggregate_id,
        version,
        &root_overrides,
        root_is_deleted,
        audit,
    ));
    builders
}

// ── audit row from provenance ──────────────────────────────────────────────

fn build_audit_row(actor: &Actor, correlation_id: Uuid, occurred_at: DateTime<Utc>) -> MasonRow {
    let (actor_type, actor_id) = match actor {
        Actor::Identity(user_id) => ("Identity", user_id.to_string()),
        Actor::System(system_actor) => ("System", format!("{system_actor:?}")),
        Actor::Anonymous => ("Anonymous", String::new()),
    };
    MasonRow::from_static(vec![
        ("actor_type", MasonValue::Text(actor_type.to_string())),
        ("actor_id", MasonValue::Text(actor_id)),
        ("correlation_id", MasonValue::Uuid(correlation_id)),
        ("occurred_at", MasonValue::Timestamp(occurred_at)),
    ])
}

// ── root ───────────────────────────────────────────────────────────────────

fn build_root_create<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    aggregate_id: Uuid,
    version: i64,
    fields: &MasonRow,
    audit: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let table = aggregate.root.relation.as_str();
    let columns = column_names(codex, table);
    let enum_map = enum_map_for(codex, table);
    let context = CreateContext {
        aggregate_key: &aggregate.root.id_column,
        aggregate_id,
        version,
        is_deleted: Some(false),
        field_sources: vec![fields, audit],
        enum_map: &enum_map,
    };
    emit_plain_insert(table, &columns, &context, &root_primary_key(aggregate))
}

fn build_root_copy_forward<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    aggregate_id: Uuid,
    version: i64,
    overrides: &MasonRow,
    is_deleted: bool,
    audit: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let table = aggregate.root.relation.as_str();
    let columns = column_names(codex, table);
    let enum_map = enum_map_for(codex, table);
    let context = CopyForwardContext {
        aggregate_key: &aggregate.root.id_column,
        aggregate_id,
        version,
        source_table: table,
        is_deleted: Some(is_deleted),
        field_sources: vec![audit, overrides],
        enum_map: &enum_map,
        key_filters: &[],
        skip_deleted: false,
    };
    emit_insert_from_select(table, &columns, &context, &root_primary_key(aggregate))
}

// ── entity ─────────────────────────────────────────────────────────────────

#[expect(
    clippy::too_many_arguments,
    reason = "entity row build needs codex, aggregate, entity, identity, version, keys, fields, audit — each is independent"
)]
fn build_entity_insert_at_version<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    entity: &CodexEntity,
    aggregate_id: Uuid,
    version: i64,
    keys: &MasonRow,
    fields: &MasonRow,
    audit: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let table = entity.relation.as_str();
    let columns = column_names(codex, table);
    let enum_map = enum_map_for(codex, table);
    let context = CreateContext {
        aggregate_key: &aggregate.root.id_column,
        aggregate_id,
        version,
        is_deleted: Some(false),
        field_sources: vec![keys, fields, audit],
        enum_map: &enum_map,
    };
    emit_plain_insert(table, &columns, &context, &entity_primary_key(codex, aggregate, entity))
}

#[expect(
    clippy::too_many_arguments,
    reason = "thin wrapper over build_entity_insert_at_version — same arg surface"
)]
fn build_entity_add<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    entity: &CodexEntity,
    aggregate_id: Uuid,
    version: i64,
    keys: &MasonRow,
    fields: &MasonRow,
    audit: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    build_entity_insert_at_version(codex, aggregate, entity, aggregate_id, version, keys, fields, audit)
}

#[expect(
    clippy::too_many_arguments,
    reason = "entity revise needs codex, aggregate, entity, identity, version, keys, fields, audit — each is independent"
)]
fn build_entity_revise<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    entity: &CodexEntity,
    aggregate_id: Uuid,
    version: i64,
    keys: &MasonRow,
    fields: &MasonRow,
    audit: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let table = entity.relation.as_str();
    let columns = column_names(codex, table);
    let enum_map = enum_map_for(codex, table);
    let context = CopyForwardContext {
        aggregate_key: &aggregate.root.id_column,
        aggregate_id,
        version,
        source_table: table,
        is_deleted: None,
        field_sources: vec![fields, audit],
        enum_map: &enum_map,
        key_filters: &[keys],
        skip_deleted: false,
    };
    emit_insert_from_select(table, &columns, &context, &entity_primary_key(codex, aggregate, entity))
}

fn build_entity_remove<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    entity: &CodexEntity,
    aggregate_id: Uuid,
    version: i64,
    keys: &MasonRow,
    audit: &MasonRow,
) -> QueryBuilder<'a, Postgres> {
    let table = entity.relation.as_str();
    let columns = column_names(codex, table);
    let enum_map = enum_map_for(codex, table);
    let context = CopyForwardContext {
        aggregate_key: &aggregate.root.id_column,
        aggregate_id,
        version,
        source_table: table,
        is_deleted: Some(true),
        field_sources: vec![audit],
        enum_map: &enum_map,
        key_filters: &[keys],
        skip_deleted: false,
    };
    emit_insert_from_select(table, &columns, &context, &entity_primary_key(codex, aggregate, entity))
}

fn build_entity_copy_forward<'a>(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    entity: &CodexEntity,
    aggregate_id: Uuid,
    version: i64,
    excluded_keys: &[&MasonRow],
) -> QueryBuilder<'a, Postgres> {
    let table = entity.relation.as_str();
    let columns = column_names(codex, table);
    let enum_map = enum_map_for(codex, table);
    let context = CopyForwardContext {
        aggregate_key: &aggregate.root.id_column,
        aggregate_id,
        version,
        source_table: table,
        is_deleted: None,
        field_sources: vec![],
        enum_map: &enum_map,
        key_filters: excluded_keys,
        skip_deleted: true,
    };
    emit_insert_from_select_excluding(table, &columns, &context, &entity_primary_key(codex, aggregate, entity))
}

// ── emission contexts ──────────────────────────────────────────────────────

/// Context for a plain `INSERT ... VALUES` of a snap row at the event's version.
struct CreateContext<'a> {
    aggregate_key: &'a str,
    aggregate_id: Uuid,
    /// Bound directly into the snap row's `version` column. Source of truth is
    /// `LaxEvent.aggregate_version`.
    version: i64,
    is_deleted: Option<bool>,
    field_sources: Vec<&'a MasonRow>,
    enum_map: &'a EnumMap,
}

/// Context for `INSERT ... SELECT FROM source WHERE version = aggregate_version - 1`,
/// copying the previous snap row forward and stamping the new one with `version`.
struct CopyForwardContext<'a> {
    aggregate_key: &'a str,
    aggregate_id: Uuid,
    /// Bound directly into the new row's `version` column.
    version: i64,
    source_table: &'a str,
    is_deleted: Option<bool>,
    field_sources: Vec<&'a MasonRow>,
    enum_map: &'a EnumMap,
    key_filters: &'a [&'a MasonRow],
    skip_deleted: bool,
}

// ── INSERT VALUES (plain) ──────────────────────────────────────────────────

fn emit_plain_insert<'a>(
    table: &str,
    columns: &[String],
    context: &CreateContext,
    primary_key: &[String],
) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("INSERT INTO ");
    builder.push(quote_ident(table)).push(" (");

    // Determine which columns we have a value for.
    let included: Vec<&String> = columns
        .iter()
        .filter(|column| can_resolve_create(context, column))
        .collect();
    push_ident_list(&mut builder, &included);
    builder.push(") VALUES (");
    for (index, column) in included.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        emit_create_value(&mut builder, context, column);
    }
    builder.push(") ");
    push_on_conflict_do_nothing(&mut builder, primary_key);
    builder
}

fn can_resolve_create(context: &CreateContext, column: &str) -> bool {
    column == context.aggregate_key
        || column == COL_VERSION
        || (column == COL_IS_DELETED && context.is_deleted.is_some())
        || context
            .field_sources
            .iter()
            .any(|row| find_value(row, column).is_some())
}

fn emit_create_value(builder: &mut QueryBuilder<'_, Postgres>, context: &CreateContext, column: &str) {
    if column == context.aggregate_key {
        builder.push_bind(context.aggregate_id);
        return;
    }
    if column == COL_VERSION {
        builder.push_bind(context.version);
        return;
    }
    if column == COL_IS_DELETED
        && let Some(flag) = context.is_deleted
    {
        if flag {
            builder.push("TRUE");
        } else {
            builder.push("FALSE");
        }
        return;
    }
    for source in &context.field_sources {
        if let Some(value) = find_value(source, column) {
            bind_with_enum_cast(builder, value, context.enum_map.get(column));
            return;
        }
    }
    builder.push("NULL");
}

// ── INSERT ... SELECT (copy-forward) ───────────────────────────────────────

fn emit_insert_from_select<'a>(
    table: &str,
    columns: &[String],
    context: &CopyForwardContext,
    primary_key: &[String],
) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("INSERT INTO ");
    builder.push(quote_ident(table)).push(" (");
    push_ident_list(&mut builder, &columns.iter().collect::<Vec<_>>());
    builder.push(") SELECT ");
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        emit_copy_forward_value(&mut builder, context, column);
    }
    builder.push(" FROM ").push(quote_ident(context.source_table));
    builder.push(" WHERE ");
    builder
        .push(quote_ident(context.aggregate_key))
        .push(" = ")
        .push_bind(context.aggregate_id);
    builder.push(" AND ");
    emit_previous_version_condition(&mut builder, context);
    for keys in context.key_filters {
        emit_key_equality(&mut builder, keys, context.aggregate_key, " AND ");
    }
    builder.push(" ");
    push_on_conflict_do_nothing(&mut builder, primary_key);
    builder
}

fn emit_insert_from_select_excluding<'a>(
    table: &str,
    columns: &[String],
    context: &CopyForwardContext,
    primary_key: &[String],
) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("INSERT INTO ");
    builder.push(quote_ident(table)).push(" (");
    push_ident_list(&mut builder, &columns.iter().collect::<Vec<_>>());
    builder.push(") SELECT ");
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        emit_copy_forward_value(&mut builder, context, column);
    }
    builder.push(" FROM ").push(quote_ident(context.source_table));
    builder.push(" WHERE ");
    builder
        .push(quote_ident(context.aggregate_key))
        .push(" = ")
        .push_bind(context.aggregate_id);
    builder.push(" AND ");
    emit_previous_version_condition(&mut builder, context);
    if context.skip_deleted {
        builder.push(" AND ").push(quote_ident(COL_IS_DELETED)).push(" = FALSE");
    }
    for keys in context.key_filters {
        // Exclude these key combinations.
        for (name, value) in keys.columns() {
            let name_ref = name.as_ref();
            if name_ref == context.aggregate_key {
                continue;
            }
            builder.push(" AND ").push(quote_ident(name_ref)).push(" <> ");
            bind_with_enum_cast(&mut builder, value, None);
        }
    }
    builder.push(" ");
    push_on_conflict_do_nothing(&mut builder, primary_key);
    builder
}

fn emit_copy_forward_value(builder: &mut QueryBuilder<'_, Postgres>, context: &CopyForwardContext, column: &str) {
    if column == context.aggregate_key {
        builder.push_bind(context.aggregate_id);
        return;
    }
    if column == COL_VERSION {
        builder.push_bind(context.version);
        return;
    }
    if column == COL_IS_DELETED
        && let Some(flag) = context.is_deleted
    {
        if flag {
            builder.push("TRUE");
        } else {
            builder.push("FALSE");
        }
        return;
    }
    for source in &context.field_sources {
        if let Some(value) = find_value(source, column) {
            bind_with_enum_cast(builder, value, context.enum_map.get(column));
            return;
        }
    }
    // Pass-through from source row.
    builder.push(quote_ident(column));
}

/// Filter the copy-forward source rows by the immediately previous aggregate version.
/// Combined with the per-aggregate ordering enforced by the aggregate store, this is
/// race-free: there is at most one snap row at `aggregate_version - 1` for the aggregate.
fn emit_previous_version_condition(builder: &mut QueryBuilder<'_, Postgres>, context: &CopyForwardContext) {
    builder
        .push(quote_ident(COL_VERSION))
        .push(" = ")
        .push_bind(context.version - 1);
}

fn emit_key_equality(builder: &mut QueryBuilder<'_, Postgres>, keys: &MasonRow, aggregate_key: &str, prefix: &str) {
    for (name, value) in keys.columns() {
        let name_ref = name.as_ref();
        if name_ref == aggregate_key {
            continue;
        }
        builder.push(prefix).push(quote_ident(name_ref)).push(" = ");
        bind_with_enum_cast(builder, value, None);
    }
}

// ── lookup helpers ─────────────────────────────────────────────────────────

fn find_value<'a>(row: &'a MasonRow, column: &str) -> Option<&'a MasonValue> {
    row.columns()
        .iter()
        .find(|(name, _)| name.as_ref() == column)
        .map(|(_, value)| value)
}

fn column_names(codex: &dyn Codex, table: &str) -> Vec<String> {
    codex
        .relation(table)
        .map(|relation| relation.columns.iter().map(|column| column.name.clone()).collect())
        .unwrap_or_default()
}

fn enum_map_for(codex: &dyn Codex, table: &str) -> EnumMap {
    codex
        .relation(table)
        .map(|relation| {
            relation
                .columns
                .iter()
                .filter_map(|column| match &column.data_type {
                    CodexType::Enum(enum_name) => Some((column.name.clone(), enum_name.clone())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn root_primary_key(aggregate: &CodexAggregate) -> Vec<String> {
    vec![aggregate.root.id_column.clone(), COL_VERSION.to_string()]
}

fn entity_primary_key(codex: &dyn Codex, aggregate: &CodexAggregate, entity: &CodexEntity) -> Vec<String> {
    let mut primary_key = vec![aggregate.root.id_column.clone(), COL_VERSION.to_string()];
    if let Some(relation) = codex.relation(&entity.relation) {
        for column in &relation.columns {
            if column.role == CodexRole::Reference {
                primary_key.push(column.name.clone());
            }
        }
    }
    primary_key
}

// ── SQL emission primitives ────────────────────────────────────────────────

fn push_on_conflict_do_nothing(builder: &mut QueryBuilder<'_, Postgres>, primary_key: &[String]) {
    builder.push("ON CONFLICT (");
    push_ident_list(builder, primary_key);
    builder.push(") DO NOTHING");
}
