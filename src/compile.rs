use crate::ast::parse_schema;
use crate::catalog::{build_catalog, Catalog};
use crate::emit::emit_schema;
use crate::error::HolocronError;
use crate::resolve::resolve_views;

/// The result of compiling a YAML schema: the validated catalog (the symbol
/// table queries are checked against) and the rendered PostgreSQL DDL.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub catalog: Catalog,
    pub ddl: String,
}

/// Compile a YAML schema end-to-end: parse → build catalog → resolve views →
/// emit DDL.
///
/// Each phase collects *every* error it finds before returning, so on
/// failure the caller receives the full list of problems for one rendering
/// pass — see CLAUDE.md `HOLO-DIAGNOSTICS`. A phase only runs once the
/// previous one succeeded (cascade prevention).
///
/// # Errors
/// Returns every diagnostic the failing phase produced, as a `Vec`. Parse
/// errors arrive wrapped in a single-element vec (a broken YAML tree leaves
/// nothing useful to keep walking).
pub fn compile(input: &str) -> Result<Compiled, Vec<HolocronError>> {
    let document = parse_schema(input).map_err(|error| vec![error])?;
    let catalog = build_catalog(&document)?;
    let catalog = resolve_views(catalog, &document)?;
    let ddl = emit_schema(&document);
    Ok(Compiled { catalog, ddl })
}
