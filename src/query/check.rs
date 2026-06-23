use crate::catalog::{Catalog, CatalogColumn, CatalogRelation};
use crate::error::HolocronError;
use crate::query::filter::{Comparison, Filter};
use crate::query::Query;

/// A query that has passed type-checking against the catalog. The only way to
/// build one is through [`check_query`]; an unchecked query is unrepresentable
/// at this type (HOLO-PARSE-DONT-VALIDATE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedQuery {
    query: Query,
}

impl CheckedQuery {
    /// The validated underlying query.
    pub fn query(&self) -> &Query {
        &self.query
    }
}

/// Validate a query against the catalog: every referenced column must exist on
/// the target relation, be filterable, and use an operator the column's type
/// supports.
///
/// Collects every error across the filter tree so callers can render them
/// in one pass.
///
/// # Errors
/// Unknown relation (short-circuits — no relation, no filter to check),
/// or any number of unknown-column / non-filterable / operator-mismatch
/// errors gathered across the filter tree.
pub fn check_query(catalog: &Catalog, query: Query) -> Result<CheckedQuery, Vec<HolocronError>> {
    let Some(relation) = catalog.relation(&query.relation) else {
        return Err(vec![HolocronError::unknown_relation(&query.relation)]);
    };
    let mut errors = Vec::new();
    if let Some(filter) = &query.filter {
        check_filter(filter, &query.relation, relation, &mut errors);
    }
    if errors.is_empty() {
        Ok(CheckedQuery { query })
    } else {
        Err(errors)
    }
}

fn check_filter(
    filter: &Filter,
    relation_name: &str,
    relation: &CatalogRelation,
    errors: &mut Vec<HolocronError>,
) {
    match filter {
        Filter::And(children) | Filter::Or(children) => {
            for child in children {
                check_filter(child, relation_name, relation, errors);
            }
        }
        Filter::Leaf(comparison) => check_comparison(comparison, relation_name, relation, errors),
    }
}

fn check_comparison(
    comparison: &Comparison,
    relation_name: &str,
    relation: &CatalogRelation,
    errors: &mut Vec<HolocronError>,
) {
    match comparison {
        Comparison::Compare { column, op, .. } => {
            let Some(resolved) = resolve_filterable(relation_name, relation, column, errors) else {
                return;
            };
            if !op.supported_by(&resolved.data_type) {
                errors.push(HolocronError::operator_not_supported(
                    relation_name,
                    column,
                    resolved.data_type.name(),
                    op.name(),
                ));
            }
        }
        Comparison::Set { column, op, .. } => {
            let Some(resolved) = resolve_filterable(relation_name, relation, column, errors) else {
                return;
            };
            if !op.supported_by(&resolved.data_type) {
                errors.push(HolocronError::operator_not_supported(
                    relation_name,
                    column,
                    resolved.data_type.name(),
                    op.name(),
                ));
            }
        }
        Comparison::NullCheck { column, .. } => {
            // `=null=` is supported on every type, so resolution + filterable check is enough.
            resolve_filterable(relation_name, relation, column, errors);
        }
    }
}

/// Resolve a referenced column and confirm it is filterable. Appends to the
/// shared error list when either check fails; returns `None` in that case so
/// the caller can skip downstream checks on this leaf.
fn resolve_filterable<'r>(
    relation_name: &str,
    relation: &'r CatalogRelation,
    column: &str,
    errors: &mut Vec<HolocronError>,
) -> Option<&'r CatalogColumn> {
    let Some(resolved) = relation.column(column) else {
        let candidates = relation
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        // Queries are built programmatically in this layer; no AST span to attach.
        errors.push(HolocronError::unknown_column(
            relation_name,
            column,
            candidates,
            crate::span::Span::default(),
        ));
        return None;
    };
    if !resolved.filterable {
        errors.push(HolocronError::not_filterable(relation_name, column));
        return None;
    }
    Some(resolved)
}
