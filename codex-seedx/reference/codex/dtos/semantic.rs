use std::collections::{HashMap, HashSet};

use lax_shared::{
    dtos::db::codex::{CodexRole, DEFAULT_LIST, ReadShape},
    error::{CodexError, LaxResult},
};

use crate::relational_basket::codex::dtos::{RawCodexDocument, RawCodexTable, RawCodexView};

const BUILT_IN_TYPES: &[&str] = &[
    "text",
    "uuid",
    "boolean",
    "timestamptz",
    "jsonb",
    "bigint",
    "integer",
    "smallint",
    "text[]",
];

const COL_VERSION: &str = "version";

pub fn check(document: &RawCodexDocument) -> LaxResult<()> {
    let aggregate_roots = collect_aggregate_roots(&document.tables)?;
    let enum_names = document.enum_names();

    check_roles_on_tables(&document.tables, &aggregate_roots)?;
    check_entity_aggregates_resolve(&document.tables, &aggregate_roots)?;
    check_sinks_resolve(&document.tables, &aggregate_roots)?;
    check_column_types_resolve(&document.tables, &enum_names)?;
    check_primary_key_matches_roles(&document.tables)?;
    check_views_have_single_id(&document.views)?;
    check_identity_columns_not_selectable(&document.views)?;
    check_view_shapes_well_formed(&document.views)?;
    check_canonical_default_list_per_surface(&document.views, &aggregate_roots)?;
    check_no_duplicate_surface_names(&document.views)?;

    Ok(())
}

fn collect_aggregate_roots(tables: &[RawCodexTable]) -> LaxResult<HashMap<String, String>> {
    let mut roots = HashMap::new();
    for table in tables {
        let Some(x_aggregate) = &table.x_aggregate else {
            continue;
        };
        let id_column = table.id_column().ok_or_else(|| {
            CodexError::semantic(format!(
                "aggregate root `{}` must declare exactly one column with `x_role: Id`",
                table.name
            ))
        })?;
        roots.insert(x_aggregate.name.clone(), id_column);
    }
    Ok(roots)
}

fn check_roles_on_tables(tables: &[RawCodexTable], roots: &HashMap<String, String>) -> LaxResult<()> {
    for table in tables {
        if !table.is_cqrs() {
            continue;
        }

        let id_count = table.count_role(CodexRole::Id);
        if id_count != 1 {
            return Err(CodexError::semantic(format!(
                "table `{}` must have exactly one `x_role: Id` column (found {id_count})",
                table.name
            )));
        }

        let Some(parent_aggregate) = table.parent_aggregate() else {
            continue;
        };
        let Some(root_id) = roots.get(parent_aggregate) else {
            continue;
        };
        let id_name = table.id_column().unwrap_or_default();
        if &id_name != root_id {
            return Err(CodexError::semantic(format!(
                "table `{}`: `x_role: Id` column is `{id_name}` but aggregate `{parent_aggregate}` uses `{root_id}` — entity and sink tables must carry the root's Id column",
                table.name
            )));
        }
    }
    Ok(())
}

fn check_entity_aggregates_resolve(tables: &[RawCodexTable], roots: &HashMap<String, String>) -> LaxResult<()> {
    for table in tables {
        let Some(x_entity) = &table.x_entity else {
            continue;
        };
        if !roots.contains_key(&x_entity.aggregate) {
            return Err(CodexError::semantic(format!(
                "x_entity.aggregate `{}` does not match any x_aggregate.name",
                x_entity.aggregate
            )));
        }
    }
    Ok(())
}

fn check_sinks_resolve(tables: &[RawCodexTable], roots: &HashMap<String, String>) -> LaxResult<()> {
    for table in tables {
        let Some(x_sink) = &table.x_sink else {
            continue;
        };
        if !roots.contains_key(&x_sink.aggregate) {
            return Err(CodexError::semantic(format!(
                "x_sink.aggregate `{}` does not match any x_aggregate.name",
                x_sink.aggregate
            )));
        }
    }
    Ok(())
}

fn check_column_types_resolve(tables: &[RawCodexTable], enum_names: &HashSet<String>) -> LaxResult<()> {
    for table in tables {
        for (column_name, column_def) in &table.columns {
            let type_name = column_def.type_name.as_str();
            if !BUILT_IN_TYPES.contains(&type_name) && !enum_names.contains(type_name) {
                return Err(CodexError::semantic(format!(
                    "column `{}.{column_name}`: type `{type_name}` is not a built-in or declared enum",
                    table.name
                )));
            }
        }
    }
    Ok(())
}

/// Invariant: for every CQRS table, `primary_key.columns \ {"version"}` must
/// equal the set of columns with `x_role: Id` or `x_role: Reference`. Catches
/// schema drift where a Reference column is added but not added to the PK,
/// or an extra non-role column is smuggled into the PK.
fn check_primary_key_matches_roles(tables: &[RawCodexTable]) -> LaxResult<()> {
    for table in tables {
        if !table.is_cqrs() {
            continue;
        }
        let Some(pk) = &table.primary_key else {
            return Err(CodexError::semantic(format!(
                "CQRS table `{}` must declare a primary_key",
                table.name
            )));
        };

        let pk_without_version: HashSet<&str> = pk
            .columns
            .iter()
            .filter(|column| column.as_str() != COL_VERSION)
            .map(String::as_str)
            .collect();

        let expected: HashSet<&str> = table
            .columns
            .iter()
            .filter(|(_, definition)| matches!(definition.x_role, Some(CodexRole::Id) | Some(CodexRole::Reference)))
            .map(|(name, _)| name.as_str())
            .collect();

        if pk_without_version != expected {
            let mut expected_sorted: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            let mut actual_sorted: Vec<String> = pk_without_version.iter().map(|s| s.to_string()).collect();
            expected_sorted.sort();
            actual_sorted.sort();
            return Err(CodexError::mismatched_primary_key(
                &table.name,
                expected_sorted,
                actual_sorted,
            ));
        }
    }
    Ok(())
}

fn check_views_have_single_id(views: &[RawCodexView]) -> LaxResult<()> {
    for view in views {
        let id_count = view.count_id_roles();
        if id_count != 1 {
            return Err(CodexError::semantic(format!(
                "view `{}` must have exactly one x_role: Id column (found {id_count})",
                view.name
            )));
        }
    }
    Ok(())
}

fn check_identity_columns_not_selectable(views: &[RawCodexView]) -> LaxResult<()> {
    for view in views {
        for item in &view.select {
            let role = item.role();
            let selectable = item.x_selectable();
            if matches!(role, Some(CodexRole::Id) | Some(CodexRole::Reference)) && selectable == Some(true) {
                let alias = item.display_name().unwrap_or("<unnamed>");
                return Err(CodexError::semantic(format!(
                    "view `{}` select item `{alias}`: `x_selectable: true` is not allowed on {:?} columns",
                    view.name,
                    role.unwrap()
                )));
            }
        }
    }
    Ok(())
}

fn check_view_shapes_well_formed(views: &[RawCodexView]) -> LaxResult<()> {
    for view in views {
        if let Some(x_aggregate) = view.x_aggregate.as_ref() {
            x_aggregate.shape(&view.name)?;
        }
        if let Some(x_entity) = view.x_entity.as_ref() {
            x_entity.shape(&view.name)?;
        }
    }
    Ok(())
}

fn check_canonical_default_list_per_surface(views: &[RawCodexView], roots: &HashMap<String, String>) -> LaxResult<()> {
    let mut root_default_counts: HashMap<&str, usize> = HashMap::new();
    let mut entity_default_counts: HashMap<(&str, &str), usize> = HashMap::new();
    let mut entities_seen: HashMap<&str, HashSet<&str>> = HashMap::new();

    for view in views {
        if let Some(x_aggregate) = view.x_aggregate.as_ref()
            && let Ok(ReadShape::List(surface_name)) = x_aggregate.shape(&view.name)
            && surface_name == DEFAULT_LIST
        {
            *root_default_counts.entry(x_aggregate.name.as_str()).or_insert(0) += 1;
        }
        if let Some(x_entity) = view.x_entity.as_ref() {
            entities_seen
                .entry(x_entity.aggregate.as_str())
                .or_default()
                .insert(x_entity.name.as_str());
            if let Ok(ReadShape::List(surface_name)) = x_entity.shape(&view.name)
                && surface_name == DEFAULT_LIST
            {
                *entity_default_counts
                    .entry((x_entity.aggregate.as_str(), x_entity.name.as_str()))
                    .or_insert(0) += 1;
            }
        }
    }

    for aggregate_name in roots.keys() {
        let count = root_default_counts.get(aggregate_name.as_str()).copied().unwrap_or(0);
        if count != 1 {
            return Err(CodexError::semantic(format!(
                "aggregate `{aggregate_name}` root must declare exactly one `list: {DEFAULT_LIST}` view (found {count})"
            )));
        }
    }
    for (aggregate_name, entities) in &entities_seen {
        for entity_name in entities {
            let count = entity_default_counts
                .get(&(*aggregate_name, *entity_name))
                .copied()
                .unwrap_or(0);
            if count != 1 {
                return Err(CodexError::semantic(format!(
                    "entity `{aggregate_name}.{entity_name}` must declare exactly one `list: {DEFAULT_LIST}` view (found {count})"
                )));
            }
        }
    }
    Ok(())
}

fn check_no_duplicate_surface_names(views: &[RawCodexView]) -> LaxResult<()> {
    let mut seen: HashMap<(String, Option<String>), HashSet<ReadShape>> = HashMap::new();
    for view in views {
        if let Some(x_aggregate) = view.x_aggregate.as_ref() {
            let shape = x_aggregate.shape(&view.name)?;
            let key = (x_aggregate.name.clone(), None);
            if !seen.entry(key).or_default().insert(shape.clone()) {
                return Err(CodexError::semantic(format!(
                    "aggregate `{}`: duplicate root surface `{shape}`",
                    x_aggregate.name
                )));
            }
        }
        if let Some(x_entity) = view.x_entity.as_ref() {
            let shape = x_entity.shape(&view.name)?;
            let key = (x_entity.aggregate.clone(), Some(x_entity.name.clone()));
            if !seen.entry(key).or_default().insert(shape.clone()) {
                return Err(CodexError::semantic(format!(
                    "entity `{}.{}`: duplicate surface `{shape}`",
                    x_entity.aggregate, x_entity.name
                )));
            }
        }
    }
    Ok(())
}
