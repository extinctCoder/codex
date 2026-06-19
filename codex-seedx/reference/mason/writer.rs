use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lax_domain::{
    contracts::infrastructure::persistence::mason::MasonWriter,
    shared::{aggregate::Provenance, events::LaxEvent},
};
use lax_shared::{dtos::db::mason::MasonEffect, error::LaxResult};
use uuid::Uuid;

use crate::relational_basket::mason::{
    definition::Mason,
    write_utils::{archive, project, sink},
};

#[async_trait]
impl MasonWriter for Mason {
    async fn project(
        &self,
        aggregate: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        effects: Vec<MasonEffect>,
        provenance: &Provenance,
        occurred_at: DateTime<Utc>,
    ) -> LaxResult<u64> {
        project::project_effects(
            &self.sql_driver,
            &self.codex,
            aggregate,
            aggregate_id,
            aggregate_version,
            effects,
            provenance,
            occurred_at,
        )
        .await
    }

    async fn sink(&self, aggregate: &str, aggregate_id: Uuid, effects: Vec<MasonEffect>) -> LaxResult<()> {
        sink::sink_effects(&self.sql_driver, &self.codex, aggregate, aggregate_id, effects).await
    }

    async fn archive(&self, events: &[LaxEvent], consumer: &str, checkpoint: u64) -> LaxResult<()> {
        archive::archive_events(&self.sql_driver, events, consumer, checkpoint).await
    }
}
