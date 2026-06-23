use indexmap::IndexMap;

use crate::ast::SchemaDocument;
use crate::catalog::{Catalog, CatalogColumn, CatalogRelation, CatalogType, RelationKind};
use crate::error::HolocronError;
use crate::span::Span;

/// Lower the parsed document into a resolved catalog (tables + enums).
///
/// Collects *every* L2 error before returning so the caller can render them
/// in a single pass — see CLAUDE.md `HOLO-DIAGNOSTICS`.
///
/// # Errors
/// Any unknown column types, duplicate enum names, or duplicate relation
/// names are returned together in a single `Vec`.
pub fn build_catalog(document: &SchemaDocument) -> Result<Catalog, Vec<HolocronError>> {
    let mut errors = Vec::new();
    let enums = collect_enums(document, &mut errors);
    let mut catalog = Catalog::from_enums(enums);
    add_tables(&mut catalog, document, &mut errors);
    if errors.is_empty() {
        Ok(catalog)
    } else {
        Err(errors)
    }
}

/// Build the enum-name → values map. Duplicates are recorded as errors and
/// the *second* declaration is skipped (first one wins) — same convergence
/// behaviour as a SQL `CREATE TYPE` against an existing name.
fn collect_enums(
    document: &SchemaDocument,
    errors: &mut Vec<HolocronError>,
) -> IndexMap<String, Vec<String>> {
    let mut enums = IndexMap::new();
    let mut first_spans: IndexMap<String, Span> = IndexMap::new();
    for declared in &document.types {
        let name = declared.name.value.clone();
        if let Some(&first_span) = first_spans.get(&name) {
            errors.push(HolocronError::duplicate_enum(
                name,
                first_span,
                declared.name.span,
            ));
            continue;
        }
        let values: Vec<String> = declared
            .r#enum
            .iter()
            .map(|value| value.value.clone())
            .collect();
        first_spans.insert(name.clone(), declared.name.span);
        enums.insert(name, values);
    }
    enums
}

/// Resolve each table's columns and insert the table into the catalog.
/// Columns with unresolvable types are *skipped* (their error is recorded),
/// not aborted on — the table still goes in with the columns we could
/// resolve, so downstream phases can find it.
fn add_tables(catalog: &mut Catalog, document: &SchemaDocument, errors: &mut Vec<HolocronError>) {
    for table in &document.tables {
        let mut columns = Vec::with_capacity(table.columns.len());
        for (name, column) in &table.columns {
            match resolve_type(&column.r#type.value, &catalog.enums) {
                Some(data_type) => columns.push(CatalogColumn {
                    name: name.clone(),
                    data_type,
                    nullable: column.null,
                    // Table columns are filterable by default; searchable is opt-in
                    // (view select items can override both).
                    filterable: true,
                    searchable: false,
                }),
                None => errors.push(HolocronError::unknown_type(
                    table.name.value.clone(),
                    name,
                    column.r#type.value.clone(),
                    column.r#type.span,
                )),
            }
        }
        let relation = CatalogRelation {
            name: table.name.value.clone(),
            kind: RelationKind::Table,
            columns,
        };
        if let Err(error) = catalog.insert_relation(relation, table.name.span) {
            errors.push(error);
        }
    }
}

/// Resolve a YAML type name: a built-in, or a declared enum, else `None`.
fn resolve_type(name: &str, enums: &IndexMap<String, Vec<String>>) -> Option<CatalogType> {
    CatalogType::from_sql_name(name).or_else(|| {
        enums
            .contains_key(name)
            .then(|| CatalogType::Enum(name.to_string()))
    })
}
