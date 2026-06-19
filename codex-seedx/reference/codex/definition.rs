use std::{collections::HashMap, sync::Arc};

use lax_domain::contracts::infrastructure::{drivers::YamlDriver, persistence::codex::Codex as CodexTrait};
use lax_shared::{
    dtos::db::codex::{CodexAggregate, CodexRelation},
    error::LaxResult,
};

use crate::relational_basket::codex::dtos::{RawCodexCatalog, compile};

pub struct Codex {
    relations: HashMap<String, CodexRelation>,
    aggregates: Vec<CodexAggregate>,
    enum_types: HashMap<String, Vec<String>>,
}

impl Codex {
    /// Load the Codex from YAML schema files — the one-shot entry point.
    pub fn load(yaml_driver: Arc<dyn YamlDriver>, contents: &HashMap<String, String>) -> LaxResult<Self> {
        let catalog = compile(yaml_driver, contents)?;
        Ok(Self::from_catalog(catalog))
    }

    pub fn from_catalog(catalog: RawCodexCatalog) -> Self {
        tracing::info!(
            "  {:<12}: {} relations, {} aggregates, {} enum types",
            "codex",
            catalog.relations.len(),
            catalog.aggregates.len(),
            catalog.enum_types.len(),
        );
        Self {
            relations: catalog.relations,
            aggregates: catalog.aggregates,
            enum_types: catalog.enum_types,
        }
    }
}

impl CodexTrait for Codex {
    fn relation(&self, name: &str) -> Option<&CodexRelation> {
        self.relations.get(name)
    }
    fn aggregate(&self, name: &str) -> Option<&CodexAggregate> {
        self.aggregates.iter().find(|candidate| candidate.name == name)
    }
    fn aggregates(&self) -> &[CodexAggregate] {
        &self.aggregates
    }
    fn enum_type(&self, name: &str) -> Option<&[String]> {
        self.enum_types.get(name).map(|values| values.as_slice())
    }
}
