use std::collections::{HashMap, HashSet};

use lax_shared::{
    dtos::db::codex::{CodexAggregate, CodexEntity, CodexRelation, CodexRoot, CodexSink, CodexSurface},
    error::LaxResult,
};
use serde::Deserialize;

use crate::relational_basket::codex::dtos::{RawCodexPartial, RawCodexTable, RawCodexTypeDefinition, RawCodexView};

/// Top-level YAML shape for a merged Codex document.
#[derive(Debug, Default, Deserialize)]
pub struct RawCodexDocument {
    #[serde(default)]
    pub x_partials: HashMap<String, RawCodexPartial>,
    #[serde(default)]
    pub types: Vec<RawCodexTypeDefinition>,
    #[serde(default)]
    pub tables: Vec<RawCodexTable>,
    #[serde(default)]
    pub views: Vec<RawCodexView>,
}

/// Normalized schema knowledge — what the runtime Codex stores.
pub struct RawCodexCatalog {
    pub relations: HashMap<String, CodexRelation>,
    pub aggregates: Vec<CodexAggregate>,
    pub enum_types: HashMap<String, Vec<String>>,
}

impl RawCodexDocument {
    /// Resolve every `x_partials:` reference on tables and views.
    /// Consumes `self.x_partials` and leaves each table/view fully expanded.
    pub fn expand_partials(&mut self) -> LaxResult<()> {
        let partials = std::mem::take(&mut self.x_partials);
        for table in &mut self.tables {
            table.expand_partials(&partials)?;
        }
        for view in &mut self.views {
            view.expand_partials(&partials)?;
        }
        Ok(())
    }

    /// Extract declared enum types as a `name -> values` map.
    pub fn enum_types(&self) -> HashMap<String, Vec<String>> {
        self.types
            .iter()
            .filter_map(RawCodexTypeDefinition::as_enum)
            .map(|definition| (definition.name.clone(), definition.values.clone()))
            .collect()
    }

    /// Set of declared enum names — used by columns to resolve `type: <enum>`.
    pub fn enum_names(&self) -> HashSet<String> {
        self.types
            .iter()
            .filter_map(|definition| definition.as_enum().map(|enum_type| enum_type.name.clone()))
            .collect()
    }

    /// Produce the normalized catalog — relations + aggregates + enum types.
    /// Assumes `expand_partials()` has already run.
    pub fn into_catalog(self) -> RawCodexCatalog {
        let enum_types = self.enum_types();
        let enum_names = enum_types.keys().cloned().collect::<HashSet<_>>();

        let mut relations: HashMap<String, CodexRelation> = HashMap::new();
        for table in &self.tables {
            let relation = table.into_relation(&enum_names);
            relations.insert(relation.name.clone(), relation);
        }
        // Views are compiled in YAML declaration order — dependent views
        // (e.g. `nodes` joining `flows`) must come after their dependencies.
        // Each view inherits column types from the relations already in the
        // map via its `from:` / `join:` alias chain.
        for view in &self.views {
            let relation = view.into_relation(&relations);
            relations.insert(relation.name.clone(), relation);
        }

        let aggregates = self.build_aggregates();

        RawCodexCatalog {
            relations,
            aggregates,
            enum_types,
        }
    }

    fn build_aggregates(&self) -> Vec<CodexAggregate> {
        let mut aggregates: HashMap<String, CodexAggregate> = HashMap::new();

        for table in &self.tables {
            let Some(x_aggregate) = &table.x_aggregate else {
                continue;
            };
            let id_column = table.id_column().unwrap_or_default();
            aggregates.insert(
                x_aggregate.name.clone(),
                CodexAggregate {
                    name: x_aggregate.name.clone(),
                    root: CodexRoot {
                        relation: table.name.clone(),
                        id_column,
                    },
                    entities: Vec::new(),
                    sinks: Vec::new(),
                    surfaces: Vec::new(),
                },
            );
        }

        for table in &self.tables {
            let Some(x_entity) = &table.x_entity else {
                continue;
            };
            let Some(aggregate) = aggregates.get_mut(&x_entity.aggregate) else {
                continue;
            };
            let reference_column = table.id_column().unwrap_or_default();
            let display_name = x_entity.display_name.clone().unwrap_or_else(|| x_entity.name.clone());
            aggregate.entities.push(CodexEntity {
                name: x_entity.name.clone(),
                display_name,
                relation: table.name.clone(),
                reference_column,
            });
        }

        for table in &self.tables {
            let Some(x_sink) = &table.x_sink else {
                continue;
            };
            let Some(aggregate) = aggregates.get_mut(&x_sink.aggregate) else {
                continue;
            };
            aggregate.sinks.push(CodexSink {
                relation: table.name.clone(),
                entity: x_sink.entity.clone(),
            });
        }

        for view in &self.views {
            if let Some(x_aggregate) = &view.x_aggregate
                && let Some(aggregate) = aggregates.get_mut(&x_aggregate.name)
                && let Ok(shape) = x_aggregate.shape(&view.name)
            {
                aggregate.surfaces.push(CodexSurface {
                    view: view.name.clone(),
                    entity: None,
                    shape,
                });
            }
            if let Some(x_entity) = &view.x_entity
                && let Some(aggregate) = aggregates.get_mut(&x_entity.aggregate)
                && let Ok(shape) = x_entity.shape(&view.name)
            {
                aggregate.surfaces.push(CodexSurface {
                    view: view.name.clone(),
                    entity: Some(x_entity.name.clone()),
                    shape,
                });
            }
        }

        aggregates.into_values().collect()
    }
}
