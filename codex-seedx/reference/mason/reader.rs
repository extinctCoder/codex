use async_trait::async_trait;
use lax_domain::contracts::infrastructure::persistence::mason::MasonReader;
use lax_shared::{
    dtos::db::{
        codex::CodexRelation,
        mason::{MasonQuery, MasonRow},
    },
    error::LaxResult,
};
use serde_json::{Map, Value};

use crate::relational_basket::mason::{definition::Mason, read_utils::build};

#[async_trait]
impl MasonReader for Mason {
    async fn list(&self, relation: &CodexRelation, query: &MasonQuery) -> LaxResult<(Vec<Value>, u64)> {
        let builder = build::build_list(&relation.name, relation, query);
        let rows = self.sql_driver.fetch_all(builder).await?;

        let mut total = 0u64;
        let mut data = Vec::with_capacity(rows.len());
        for row in &rows {
            if total == 0 {
                total = row
                    .get::<i64>(build::TOTAL_COUNT_ALIAS)
                    .map(|count| count as u64)
                    .unwrap_or(0);
            }
            data.push(row_to_json(row));
        }
        Ok((data, total))
    }

    async fn one(&self, relation: &CodexRelation, query: &MasonQuery) -> LaxResult<Option<Value>> {
        let builder = build::build_one(&relation.name, relation, query);
        let row = self.sql_driver.fetch_optional(builder).await?;
        Ok(row.as_ref().map(row_to_json))
    }

    async fn entity(&self, relation: &CodexRelation, query: &MasonQuery) -> LaxResult<Vec<Value>> {
        let mut unpaginated = query.clone();
        unpaginated.page = 1;
        unpaginated.page_size = 0;
        let builder = build::build_list(&relation.name, relation, &unpaginated);
        let rows = self.sql_driver.fetch_all(builder).await?;
        Ok(rows.iter().map(row_to_json).collect())
    }
}

/// Row → JSON object. Strips the `_total` pagination column that `build_list`
/// injects via `COUNT(*) OVER()`.
fn row_to_json(row: &MasonRow) -> Value {
    let mut map = Map::new();
    for (column_name, column_value) in row.columns() {
        if column_name.as_ref() == build::TOTAL_COUNT_ALIAS {
            continue;
        }
        map.insert(column_name.to_string(), column_value.into());
    }
    Value::Object(map)
}
