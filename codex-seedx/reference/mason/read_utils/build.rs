use lax_shared::dtos::db::{codex::CodexRelation, mason::MasonQuery};
use sqlx::{Postgres, QueryBuilder};

use crate::relational_basket::mason::{
    read_utils::{filter, select, sort},
    sql::quote_ident,
};

pub const TOTAL_COUNT_ALIAS: &str = "_total";

/// Build `SELECT ... , COUNT(*) OVER() AS _total FROM view WHERE ... ORDER BY ... LIMIT/OFFSET`.
/// One round-trip returns both the page and the total count.
pub fn build_list<'a>(view: &str, relation: &CodexRelation, query: &MasonQuery) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("SELECT ");
    select::emit(&query.select, relation, &mut builder);
    builder
        .push(", COUNT(*) OVER() AS ")
        .push(quote_ident(TOTAL_COUNT_ALIAS));
    builder.push(" FROM ").push(quote_ident(view));
    filter::emit_where(query, relation, &mut builder);
    sort::emit(&query.sort, &mut builder);
    if query.page_size > 0 {
        let offset = query.page.saturating_sub(1) * query.page_size;
        builder.push(" LIMIT ").push_bind(query.page_size as i64);
        builder.push(" OFFSET ").push_bind(offset as i64);
    }
    builder
}

/// Build `SELECT ... FROM view WHERE ... ORDER BY ... LIMIT 1`.
pub fn build_one<'a>(view: &str, relation: &CodexRelation, query: &MasonQuery) -> QueryBuilder<'a, Postgres> {
    let mut builder = QueryBuilder::new("SELECT ");
    select::emit(&query.select, relation, &mut builder);
    builder.push(" FROM ").push(quote_ident(view));
    filter::emit_where(query, relation, &mut builder);
    sort::emit(&query.sort, &mut builder);
    builder.push(" LIMIT 1");
    builder
}
