//! Runtime validation of entity-effect `keys` rows.
//!
//! Codex guarantees at boot (via `check_primary_key_matches_roles`) that every
//! CQRS table's PK minus `version` equals its Id + Reference columns. This
//! module enforces the matching boundary on entity effects: every key column
//! must be a declared `Reference` on the target relation, and the set rules
//! differ between destructive (`Delete`) and constructive (`Upsert`/`Revise`)
//! operations.

use std::collections::HashSet;

use lax_domain::contracts::infrastructure::persistence::codex::Codex;
use lax_shared::{
    dtos::db::{
        codex::{CodexAggregate, CodexRole},
        mason::{MasonEffect, MasonEffectTarget, MasonRow},
    },
    error::{LaxResult, MasonError},
};

/// Validate entity-effect `keys` for projection writes (snap_* tables).
///
/// Looks up the entity's snap relation via `aggregate.entity(name).relation`
/// and enforces the Reference-set rules.
pub(super) fn projection_entity_keys(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    effects: &[MasonEffect],
) -> LaxResult<()> {
    for effect in effects {
        let Some((entity_name, keys, is_delete)) = extract_entity_target(effect) else {
            continue;
        };
        let Some(entity) = aggregate.entity(entity_name) else {
            continue;
        };
        check_keys(codex, entity_name, &entity.relation, keys, is_delete)?;
    }
    Ok(())
}

/// Validate entity-effect `keys` for sink writes (sink_* tables).
///
/// Looks up the entity's sink relation via `aggregate.sinks` and enforces the
/// Reference-set rules against the sink table's columns.
pub(super) fn sink_entity_keys(
    codex: &dyn Codex,
    aggregate: &CodexAggregate,
    effects: &[MasonEffect],
) -> LaxResult<()> {
    for effect in effects {
        let Some((entity_name, keys, is_delete)) = extract_entity_target(effect) else {
            continue;
        };
        let Some(sink) = aggregate
            .sinks
            .iter()
            .find(|sink| sink.entity.as_deref() == Some(entity_name))
        else {
            continue;
        };
        check_keys(codex, entity_name, &sink.relation, keys, is_delete)?;
    }
    Ok(())
}

fn extract_entity_target(effect: &MasonEffect) -> Option<(&str, &MasonRow, bool)> {
    match effect {
        MasonEffect::Upsert {
            target: MasonEffectTarget::Entity { name, keys },
            ..
        }
        | MasonEffect::Revise {
            target: MasonEffectTarget::Entity { name, keys },
            ..
        } => Some((name.as_ref(), keys, false)),
        MasonEffect::Delete {
            target: MasonEffectTarget::Entity { name, keys },
        } => Some((name.as_ref(), keys, true)),
        _ => None,
    }
}

fn check_keys(
    codex: &dyn Codex,
    entity_name: &str,
    relation_name: &str,
    keys: &MasonRow,
    is_delete: bool,
) -> LaxResult<()> {
    let Some(relation) = codex.relation(relation_name) else {
        return Ok(());
    };

    let reference_cols: HashSet<&str> = relation
        .columns
        .iter()
        .filter(|column| column.role == CodexRole::Reference)
        .map(|column| column.name.as_str())
        .collect();

    let provided: HashSet<&str> = keys.columns().iter().map(|(name, _)| name.as_ref()).collect();

    // Rule 1: every provided key must be a declared Reference column.
    for key in &provided {
        if !reference_cols.contains(key) {
            return Err(MasonError::invalid_entity_key(entity_name, *key));
        }
    }

    if is_delete {
        // Rule 2a: delete allows subset but not empty (prevents accidental full-table match).
        if provided.is_empty() {
            return Err(MasonError::empty_entity_keys(entity_name));
        }
    } else {
        // Rule 2b: upsert/revise must match the full Reference set exactly.
        if provided != reference_cols {
            let mut missing: Vec<String> = reference_cols
                .difference(&provided)
                .map(|name| name.to_string())
                .collect();
            let mut extra: Vec<String> = provided
                .difference(&reference_cols)
                .map(|name| name.to_string())
                .collect();
            missing.sort();
            extra.sort();
            return Err(MasonError::incomplete_entity_keys(entity_name, missing, extra));
        }
    }

    Ok(())
}
