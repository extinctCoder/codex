use lax_shared::dtos::db::mason::{MasonDirection, MasonSort};
use sqlx::{Postgres, QueryBuilder};

use crate::relational_basket::mason::sql::quote_ident;

/// Emit `ORDER BY col1 ASC, col2 DESC, ...`. No-op when `clauses` is empty.
pub(super) fn emit(clauses: &[MasonSort], builder: &mut QueryBuilder<'_, Postgres>) {
    if clauses.is_empty() {
        return;
    }
    builder.push(" ORDER BY ");
    for (index, clause) in clauses.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        builder.push(quote_ident(&clause.column));
        match clause.direction {
            MasonDirection::Asc => builder.push(" ASC"),
            MasonDirection::Desc => builder.push(" DESC"),
        };
    }
}
