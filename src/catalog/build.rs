use indexmap::IndexMap;

use crate::ast::SchemaDocument;
use crate::catalog::{Catalog, CatalogColumn, CatalogRelation, CatalogType, RelationKind};
use crate::error::HolocronError;

/// Lower the parsed document into a resolved catalog (tables + enums).
///
/// # Errors
/// An unknown column type, a duplicate enum name, or a duplicate relation name.
pub fn build_catalog(document: &SchemaDocument) -> Result<Catalog, HolocronError> {
    let enums = build_enums(document)?;
    let relations = build_tables(document, &enums)?;
    Ok(Catalog { relations, enums })
}

fn build_enums(document: &SchemaDocument) -> Result<IndexMap<String, Vec<String>>, HolocronError> {
    let mut enums = IndexMap::new();
    for declared in &document.types {
        if enums
            .insert(declared.name.clone(), declared.r#enum.clone())
            .is_some()
        {
            return Err(HolocronError::duplicate_enum(&declared.name));
        }
    }
    Ok(enums)
}

fn build_tables(
    document: &SchemaDocument,
    enums: &IndexMap<String, Vec<String>>,
) -> Result<IndexMap<String, CatalogRelation>, HolocronError> {
    let mut relations = IndexMap::new();
    for table in &document.tables {
        let mut columns = Vec::with_capacity(table.columns.len());
        for (name, column) in &table.columns {
            let data_type = resolve_type(&column.r#type, enums)
                .ok_or_else(|| HolocronError::unknown_type(&table.name, name, &column.r#type))?;
            columns.push(CatalogColumn {
                name: name.clone(),
                data_type,
                nullable: column.null,
            });
        }
        let relation = CatalogRelation {
            name: table.name.clone(),
            kind: RelationKind::Table,
            columns,
        };
        if relations.insert(table.name.clone(), relation).is_some() {
            return Err(HolocronError::duplicate_relation(&table.name));
        }
    }
    Ok(relations)
}

/// Resolve a YAML type name: a built-in, or a declared enum, else `None`.
fn resolve_type(name: &str, enums: &IndexMap<String, Vec<String>>) -> Option<CatalogType> {
    CatalogType::from_sql_name(name).or_else(|| {
        enums
            .contains_key(name)
            .then(|| CatalogType::Enum(name.to_string()))
    })
}
