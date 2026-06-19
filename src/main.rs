//! # Holocron
//!
//! A **declarative schema & query compiler** — one YAML file is the single source
//! of truth for both a database schema *and* a type-checked query catalog.
//!
//! Any query (in RSQL or YAML) is validated against the catalog **before it runs**,
//! with no database connection needed, because the YAML *is* the schema.
//!
//! This crate is currently a pre-implementation stub; see `holocron-seed/DESIGN.md`
//! for the full design and roadmap.
//!
//! ## Links
//!
//! - **Repository:** <https://github.com/extinctCoder/holocron>
//! - **Issues:** <https://github.com/extinctCoder/holocron/issues>

/// Returns the project's banner line.
///
/// Kept as a small documented function so the generated docs (`cargo doc --open`)
/// have real content to show while the compiler itself is being built out.
///
/// # Examples
///
/// ```
/// // The banner names the tool and its purpose.
/// let banner = "Hello from Holocron — a declarative schema & query compiler.";
/// assert!(banner.contains("Holocron"));
/// ```
fn banner() -> &'static str {
    "Hello from Holocron — a declarative schema & query compiler for version 2."
}

/// Program entry point. Prints the project [`banner`].
fn main() {
    println!("{}", banner());
}
