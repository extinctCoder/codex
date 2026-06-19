use std::{collections::HashMap, sync::Arc};

use jsonschema::draft202012;
use lax_domain::contracts::infrastructure::drivers::YamlDriver;
use lax_shared::error::{CodexError, DriverError, LaxResult};
use serde_json::{Map, Value, from_str, from_value};

use crate::relational_basket::codex::dtos::{RawCodexCatalog, RawCodexDocument, semantic};

const AGGREGATE_SCHEMA: &str = include_str!("../../../../schema/aggregate-schema.json");

const ARRAY_KEYS: &[&str] = &["tables", "views", "types", "extensions", "sequences", "indexes"];

// File stems that pin the merge order, mirroring the Python generator's
// `discover_definition_files`. `shared` defines types/extensions every
// aggregate references; `root` creates the aggregate's base table + view that
// entity views join against; `history` composes cross-entity views and must see
// every sibling table first. Changing these strings desyncs Rust type inference
// from the SQL the Python tool emits.
const SHARED_DEFINITION: &str = "shared";
const ROOT_DEFINITION: &str = "root";
const HISTORY_DEFINITION: &str = "history";

pub fn compile(yaml_driver: Arc<dyn YamlDriver>, contents: &HashMap<String, String>) -> LaxResult<RawCodexCatalog> {
    let parsed = yaml_driver.parse_many(contents)?;
    let merged = merge(parsed)?;
    validate_schema(&merged)?;

    let mut document: RawCodexDocument =
        from_value(merged).map_err(|error| CodexError::catalog(format!("deserialize merged document: {error}")))?;

    document.expand_partials()?;
    semantic::check(&document)?;

    Ok(document.into_catalog())
}

fn validate_schema(document: &Value) -> LaxResult<()> {
    let schema: Value = from_str(AGGREGATE_SCHEMA).map_err(DriverError::from)?;
    let validator = draft202012::new(&schema)
        .map_err(|error| CodexError::schema_invalid(format!("aggregate-schema.json: {error}")))?;
    if let Err(error) = validator.validate(document) {
        return Err(CodexError::schema_violation(error.to_string()));
    }
    Ok(())
}

fn merge(documents: HashMap<String, Value>) -> LaxResult<Value> {
    let mut result = Map::new();
    for key in ARRAY_KEYS {
        result.insert((*key).to_string(), Value::Array(Vec::new()));
    }
    result.insert("x_partials".to_string(), Value::Object(Map::new()));

    // Merge files in deterministic dependency order, not `HashMap` iteration
    // order (randomized per process). Views inherit column types from the
    // relations merged before them (see `RawCodexDocument::into_catalog`), so a
    // cross-file dependent like `node_execution_io` must land after the `nodes`
    // view it reads types from — otherwise inference silently falls back to
    // `Text` on some boots and not others.
    let mut ordered: Vec<(String, Value)> = documents.into_iter().collect();
    ordered.sort_by(|(left, _), (right, _)| {
        definition_rank(left)
            .cmp(&definition_rank(right))
            .then_with(|| left.cmp(right))
    });

    for (name, document) in ordered {
        let Some(document_object) = document.as_object() else {
            return Err(CodexError::merge(format!("yaml `{name}` is not a top-level map")));
        };

        for key in ARRAY_KEYS {
            if let Some(Value::Array(array)) = document_object.get(*key)
                && let Some(Value::Array(existing)) = result.get_mut(*key)
            {
                existing.extend(array.clone());
            }
        }

        if let Some(Value::Object(document_partials)) = document_object.get("x_partials")
            && let Some(Value::Object(existing)) = result.get_mut("x_partials")
        {
            for (key, value) in document_partials {
                if existing.contains_key(key) {
                    return Err(CodexError::merge(format!(
                        "duplicate x_partials entry `{key}` across yaml files (found in `{name}`)"
                    )));
                }
                existing.insert(key.clone(), value.clone());
            }
        }
    }

    for key in ARRAY_KEYS {
        if let Some(Value::Array(array)) = result.get(*key)
            && array.is_empty()
        {
            result.remove(*key);
        }
    }
    if let Some(Value::Object(object)) = result.get("x_partials")
        && object.is_empty()
    {
        result.remove("x_partials");
    }

    Ok(Value::Object(result))
}

/// Merge-order bucket for a definition file. Variant declaration order is the
/// sort order: shared types, then aggregate roots, then everything else, then
/// cross-entity history views last.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum DefinitionRank {
    Shared,
    Root,
    Other,
    History,
}

fn definition_rank(name: &str) -> DefinitionRank {
    match name.rsplit('/').next().unwrap_or(name) {
        SHARED_DEFINITION => DefinitionRank::Shared,
        ROOT_DEFINITION => DefinitionRank::Root,
        HISTORY_DEFINITION => DefinitionRank::History,
        _ => DefinitionRank::Other,
    }
}
