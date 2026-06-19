use lax_shared::dtos::db::codex::CodexRelation;
use sqlx::{Postgres, QueryBuilder};

use crate::relational_basket::mason::sql::quote_ident;

/// Emit the SELECT list. Always-present columns (`x_selectable: false` in YAML
/// — typically Id/Reference plus any fields the schema locks in) are included
/// unconditionally. Optional columns (`x_selectable: true`) come from the user's
/// explicit `select` list, or default to every selectable column when the list
/// is empty. Falls back to `*` only when the relation declares nothing at all.
pub(super) fn emit(selected: &[String], relation: &CodexRelation, builder: &mut QueryBuilder<'_, Postgres>) {
    let always: Vec<&str> = relation
        .columns
        .iter()
        .filter(|column| !column.selectable)
        .map(|column| column.name.as_str())
        .collect();

    let optional: Vec<&str> = if selected.is_empty() {
        relation.selectable().map(|column| column.name.as_str()).collect()
    } else {
        selected.iter().map(String::as_str).collect()
    };

    let mut all = always;
    for name in optional {
        if !all.contains(&name) {
            all.push(name);
        }
    }

    if all.is_empty() {
        builder.push("*");
        return;
    }
    for (index, name) in all.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        builder.push(quote_ident(name));
    }
}
