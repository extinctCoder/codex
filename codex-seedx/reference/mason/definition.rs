use std::sync::Arc;

use lax_domain::contracts::infrastructure::{drivers::sql::SqlDriver, persistence::codex::Codex};
use sqlx::Postgres;

pub struct Mason {
    pub(super) codex: Arc<dyn Codex>,
    pub(super) sql_driver: Arc<dyn SqlDriver<Postgres>>,
}

impl Mason {
    pub fn new(codex: Arc<dyn Codex>, sql_driver: Arc<dyn SqlDriver<Postgres>>) -> Self {
        tracing::info!("  {:<12}: ready", "mason");
        Self { codex, sql_driver }
    }
}
