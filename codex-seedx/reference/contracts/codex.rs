use lax_shared::dtos::db::codex::{CodexAggregate, CodexRelation};

pub trait Codex: Send + Sync {
    fn relation(&self, name: &str) -> Option<&CodexRelation>;
    fn aggregate(&self, name: &str) -> Option<&CodexAggregate>;
    fn aggregates(&self) -> &[CodexAggregate];
    fn enum_type(&self, name: &str) -> Option<&[String]>;
}
