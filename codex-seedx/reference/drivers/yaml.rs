use std::collections::HashMap;

use lax_domain::contracts::infrastructure::drivers::YamlDriver as YamlDriverTrait;
use lax_shared::error::{DriverError, LaxResult};
use serde_json::Value;
use serde_yaml_ng::from_str;

/// Parses YAML text into JSON-shaped `serde_json::Value` trees.
#[derive(Default)]
pub struct YamlDriver;
impl YamlDriver {
    pub fn new() -> Self {
        tracing::info!("  {:<12}: ready", "yaml");
        Self
    }
}
impl YamlDriverTrait for YamlDriver {
    fn parse(&self, content: &str) -> LaxResult<Value> {
        Ok(from_str::<Value>(content).map_err(DriverError::from)?)
    }

    fn parse_many(&self, contents: &HashMap<String, String>) -> LaxResult<HashMap<String, Value>> {
        contents
            .iter()
            .map(|(name, content)| self.parse(content).map(|value| (name.clone(), value)))
            .collect()
    }
}
