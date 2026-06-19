use lax_shared::dtos::db::mason::MasonValue;
use sqlx::{Postgres, QueryBuilder};

/// Postgres identifier quoting — doubles embedded double-quotes per SQL standard.
pub(super) fn quote_ident(name: &str) -> String {
    format!(r#""{}""#, name.replace('"', "\"\""))
}

/// Push a comma-separated list of quoted identifiers.
pub(super) fn push_ident_list<Item: AsRef<str>>(builder: &mut QueryBuilder<'_, Postgres>, names: &[Item]) {
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }
        builder.push(quote_ident(name.as_ref()));
    }
}

/// Bind a `MasonValue`, emitting `NULL` literally for `Null` and attaching the
/// casts Postgres needs to accept the bound parameter.
///
/// `MasonValue: Type<Postgres>` statically reports `TEXT` for every variant (it
/// has no access to the actual variant at compile time). Postgres auto-coerces
/// text → bool/int/timestamptz, but refuses text → uuid/jsonb/text[], so we
/// append an explicit cast for those variants. An optional `enum_name` cast is
/// appended last when the target column is a Postgres enum.
pub(super) fn bind_with_enum_cast(
    builder: &mut QueryBuilder<'_, Postgres>,
    value: &MasonValue,
    enum_name: Option<&String>,
) {
    if matches!(value, MasonValue::Null) {
        builder.push("NULL");
        return;
    }
    builder.push_bind(value.clone());
    match value {
        MasonValue::Uuid(_) => {
            builder.push("::uuid");
        }
        MasonValue::Json(_) => {
            builder.push("::jsonb");
        }
        MasonValue::TextArray(_) => {
            builder.push("::text[]");
        }
        MasonValue::Timestamp(_) => {
            builder.push("::timestamptz");
        }
        _ => {}
    }
    if let Some(enum_type) = enum_name {
        builder.push("::").push(quote_ident(enum_type));
    }
}
