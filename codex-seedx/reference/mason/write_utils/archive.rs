use std::sync::Arc;

use chrono::Utc;
use lax_domain::{contracts::infrastructure::drivers::sql::SqlDriver, shared::events::LaxEvent};
use lax_shared::error::{DriverError, LaxResult};
use serde_json::to_value as to_json_value;
use sqlx::{Postgres, QueryBuilder};

/// Persist events to the archive and advance the consumer checkpoint in one transaction.
pub async fn archive_events(
    sql_driver: &Arc<dyn SqlDriver<Postgres>>,
    events: &[LaxEvent],
    consumer: &str,
    checkpoint: u64,
) -> LaxResult<()> {
    lax_shared::process_span!(
        "mason::archive",
        event_count = events.len(),
        consumer = %consumer,
        checkpoint = checkpoint,
    );
    if events.is_empty() {
        return Ok(());
    }
    let builders = vec![
        build_events_insert(events)?,
        build_checkpoint_upsert(consumer, checkpoint),
    ];
    sql_driver.transaction(builders).await?;
    Ok(())
}

fn build_events_insert<'a>(events: &[LaxEvent]) -> LaxResult<QueryBuilder<'a, Postgres>> {
    let mut builder = QueryBuilder::new(
        r#"INSERT INTO "arcv_events" ("event_id", "aggregate_version", "aggregate_id", "aggregate_type", "schema_version", "event_type", "event_data", "provenance", "idempotency_key", "occurred_at") "#,
    );
    let mut separated = builder.push_values(events, |mut row, event| {
        let event_data_json = to_json_value(&event.event_data);
        let provenance_json = to_json_value(&event.provenance);
        row.push_bind(event.event_id)
            .push_bind(event.aggregate_version as i64)
            .push_bind(event.aggregate_id)
            .push_bind(event.aggregate_type.clone())
            .push_bind(event.schema_version as i64)
            .push_bind(event.event_type())
            .push_bind(event_data_json.unwrap_or(serde_json::Value::Null))
            .push_bind(provenance_json.unwrap_or(serde_json::Value::Null))
            .push_bind(event.idempotency_key.clone())
            .push_bind(event.occurred_at);
    });
    // `push_values` returns the same QueryBuilder wrapper; finalize with ON CONFLICT.
    let _ = &mut separated;
    builder.push(r#" ON CONFLICT ("event_id") DO NOTHING"#);
    // Ensure serde_json didn't silently fail for any event (defensive check).
    for event in events {
        to_json_value(&event.event_data).map_err(DriverError::from)?;
        to_json_value(&event.provenance).map_err(DriverError::from)?;
    }
    Ok(builder)
}

fn build_checkpoint_upsert<'a>(consumer: &str, checkpoint: u64) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new(
        r#"INSERT INTO "syst_consumer_checkpoints" ("consumer_name", "last_sequence", "updated_at") VALUES ("#,
    );
    builder
        .push_bind(consumer.to_string())
        .push(", ")
        .push_bind(checkpoint as i64)
        .push(", ")
        .push_bind(Utc::now())
        .push(
            r#") ON CONFLICT ("consumer_name") DO UPDATE SET "last_sequence" = EXCLUDED."last_sequence", "updated_at" = EXCLUDED."updated_at" WHERE "syst_consumer_checkpoints"."last_sequence" < "#,
        )
        .push_bind(checkpoint as i64);
    builder
}
