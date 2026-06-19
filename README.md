# Holocron

[![CI](https://github.com/extinctCoder/holocron/actions/workflows/ci.yml/badge.svg)](https://github.com/extinctCoder/holocron/actions/workflows/ci.yml)
[![Security Audit](https://github.com/extinctCoder/holocron/actions/workflows/audit.yml/badge.svg)](https://github.com/extinctCoder/holocron/actions/workflows/audit.yml)
[![dependency status](https://deps.rs/repo/github/extinctCoder/holocron/status.svg)](https://deps.rs/repo/github/extinctCoder/holocron)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/extinctCoder/holocron/badge)](https://securityscorecards.dev/viewer/?uri=github.com/extinctCoder/holocron)
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

⚠️ **Early / design phase.** The *what* is settled; the implementation is just getting
started. The full design, rationale, and roadmap live in
[`holocron-seed/DESIGN.md`](holocron-seed/DESIGN.md).

## Installation

```sh
cargo install holocron
```

Or add it as a dependency:

```sh
cargo add holocron
```

## Development

This project uses [Conventional Commits](https://www.conventionalcommits.org). Releases
are fully automated: merging to `main` bumps the version, updates the changelog, tags,
publishes to crates.io, and creates a GitHub Release.

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
