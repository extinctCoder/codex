use crate::ast::{SchemaDocument, View};
use crate::catalog::{Catalog, CatalogRelation, RelationKind};
use crate::error::HolocronError;
use crate::resolve::scope::Scope;
use crate::resolve::select::resolve_columns;

/// Resolve every view in the document into a catalog relation, extending the
/// table-and-enum catalog produced by the previous phase.
///
/// Collects errors across views — one broken view does not stop the others
/// from being resolved. A view with errors is *not* inserted into the
/// catalog (so subsequent references to it cleanly fail with "unknown
/// relation" rather than silently succeed against a half-resolved view).
///
/// # Errors
/// Any unresolved source, alias, or column across all views, returned
/// together in a single `Vec`.
pub fn resolve_views(
    mut catalog: Catalog,
    document: &SchemaDocument,
) -> Result<Catalog, Vec<HolocronError>> {
    let mut errors = Vec::new();
    for view in &document.views {
        match resolve_view(view, &catalog) {
            Ok(relation) => {
                if let Err(error) = catalog.insert_relation(relation, view.name.span) {
                    errors.push(error);
                }
            }
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(catalog)
    } else {
        Err(errors)
    }
}

/// Resolve a single view. Bails on the first error within the view — a
/// missing source means there's no scope to resolve the rest of the view
/// against, so collecting more errors here would be cascading noise.
fn resolve_view(view: &View, catalog: &Catalog) -> Result<CatalogRelation, HolocronError> {
    let scope = Scope::build(view, catalog)?;
    let columns = resolve_columns(view, &scope)?;
    Ok(CatalogRelation {
        name: view.name.value.clone(),
        kind: RelationKind::View,
        columns,
    })
}
