use serde::Deserialize;
use serde_json::Value;

/// YAML shape for a `types:` entry.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawCodexTypeDefinition {
    Enum(RawCodexEnumType),
    Unknown(Value),
}

#[derive(Debug, Deserialize)]
pub struct RawCodexEnumType {
    pub name: String,
    #[serde(rename = "enum")]
    pub values: Vec<String>,
}

impl RawCodexTypeDefinition {
    pub fn as_enum(&self) -> Option<&RawCodexEnumType> {
        match self {
            Self::Enum(definition) => Some(definition),
            Self::Unknown(_) => None,
        }
    }
}
