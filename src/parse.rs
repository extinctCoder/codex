use crate::ast::SchemaDocument;
use crate::error::HolocronError;

/// Parse YAML into the schema AST (Layer 1: shape validation only).
///
/// # Errors
/// Returns [`HolocronError::Parse`] when the YAML does not fit the schema shape.
pub fn parse_schema(input: &str) -> Result<SchemaDocument, HolocronError> {
    serde_yaml_ng::from_str(input).map_err(|error| HolocronError::parse(error.to_string()))
}
