# Holocron

[![CI](https://github.com/holocron-lang/holocron/actions/workflows/ci.yml/badge.svg)](https://github.com/holocron-lang/holocron/actions/workflows/ci.yml)
[![Security Audit](https://github.com/holocron-lang/holocron/actions/workflows/audit.yml/badge.svg)](https://github.com/holocron-lang/holocron/actions/workflows/audit.yml)
[![dependency status](https://deps.rs/repo/github/holocron-lang/holocron/status.svg)](https://deps.rs/repo/github/holocron-lang/holocron)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/holocron-lang/holocron/badge)](https://securityscorecards.dev/viewer/?uri=github.com/holocron-lang/holocron)
[![crates.io](https://img.shields.io/crates/v/holocron.svg)](https://crates.io/crates/holocron)
[![downloads](https://img.shields.io/crates/d/holocron.svg)](https://crates.io/crates/holocron)
[![docs.rs](https://img.shields.io/docsrs/holocron)](https://docs.rs/holocron)
[![license](https://img.shields.io/crates/l/holocron.svg)](#license)

> A declarative schema & query compiler — one YAML file as the single source of truth
> for your SQL schema **and** a type-checked query catalog.

## What is this?

Holocron is a **compiler whose target language is SQL**. You write one declarative
**YAML** file that defines both:

1. your **database schema** (tables, views, indexes), and
2. a **semantic rulebook** — which columns are filterable/searchable/sortable, what
   the aggregates and entities are, who owns a row, which view is the default list.

From that single file it produces the physical schema **and** an in-memory **catalog**.
Any query — written in **RSQL** (compact, URL-friendly) or **YAML** (full specs) — is
**type-checked against the catalog before it runs**: unknown field, not-filterable,
wrong-operator-for-type are caught at build time, **with no database connection needed**,
because the YAML *is* the schema.

## Why

ORMs are code-first and language-bound, and migration tools know nothing about how the
app is *allowed* to use the data. Holocron's novel piece is the **bridge**: one
declarative source that is simultaneously the physical schema *and* the application's
query/authz/read contract, consumable from any language.

> **The guarantee:** *any query that compiles is well-formed against the declared schema
> — every field exists, is allowed, and is used with a valid operator for its type — and
> produces runnable SQL.*

## Status

🚧 **Active development.** The compiler pipeline parses, builds a catalog, resolves
views, type-checks queries, and emits PostgreSQL DDL end-to-end — with rich,
rustc-style diagnostics that underline the exact YAML token at fault. The LSP server
gives live in-editor squiggles. Still in flight: error-collection (currently bail-on-first),
DML emit, and the RSQL parser. Full roadmap in
[`holocron-seed/DESIGN.md`](holocron-seed/DESIGN.md).

## Install

```sh
cargo install holocron
```

Installs two binaries:

- **`holocron`** — the CLI compiler
- **`holocron-lsp`** — the editor LSP server

Or download pre-built archives for macOS / Linux / Windows from the
[latest GitHub Release](https://github.com/holocron-lang/holocron/releases/latest)
(each archive contains both binaries).

To use the compiler as a Rust library:

```sh
cargo add holocron
```

## Usage

Schema files conventionally use the `.holocron.yaml` extension — the editor LSP picks
that up automatically. See [`samples/`](samples/) for a dozen-plus working examples
plus error samples demonstrating every diagnostic.

```sh
holocron samples/blog.holocron.yaml     # YAML → PostgreSQL DDL on stdout
cat schema.holocron.yaml | holocron     # or pipe from stdin
holocron --help
```

## Editor integration

`holocron-lsp` speaks standard [LSP](https://microsoft.github.io/language-server-protocol/).
Open a `.holocron.yaml` file in your editor and unknown columns, duplicate aliases,
type mismatches, etc. underline in real time with the same diagnostics you'd see at
the CLI.

### Zed (project `.zed/settings.json`)

```json
{
  "lsp": {
    "holocron": {
      "binary": {
        "path": "/path/to/holocron-lsp",
        "arguments": []
      }
    }
  },
  "languages": {
    "YAML": { "language_servers": ["holocron"] }
  }
}
```

Make sure the project is **trusted** in Zed; otherwise project-level settings (including
this one) are ignored.

### JetBrains (RustRover, IntelliJ, …) via LSP4IJ

1. Install the **LSP4IJ** plugin from the Marketplace.
2. `Settings → Languages & Frameworks → Language Servers → +` (New Language Server).
3. **Command:** the path to `holocron-lsp`.
4. **Mappings tab → File name patterns:** `*.holocron.yaml`.
5. **Language Id:** leave empty.

### Other editors

`holocron-lsp` is a stdio LSP server. Any LSP client works — point it at the binary
and associate `*.holocron.yaml`.

## Development

This project uses [Conventional Commits](https://www.conventionalcommits.org). Releases
are fully automated: merging to `main` bumps the version, updates the changelog, tags,
publishes to crates.io, and creates a GitHub Release with binary archives for every
supported platform.

```sh
cargo build      # build
cargo test       # run tests
cargo doc --open # build & view the docs
```

[pre-commit](https://pre-commit.com) hooks mirror CI (formatting, clippy, tests, commit
linting):

```sh
pre-commit install --install-hooks --hook-type commit-msg
```

## License

Licensed under the [Mozilla Public License 2.0](LICENSE).
