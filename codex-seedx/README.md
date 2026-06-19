# Codex — standalone declarative schema & query compiler (spin-out seed)

**Project name: Codex.** This folder is a **self-contained starting kit** for spinning the
`Codex` schema system out of `lax-flows-rust` into its own open-source project. It is meant to be **moved out of
this repo** as-is, then grown into a new repository. Nothing here is wired into the
`lax-flows-rust` build — it's reference + docs only.

## What's inside

| File / dir | What it is |
|---|---|
| **`DESIGN.md`** | The full design discussion — vision, the what/why/how, every decision and the options rejected, the compiler architecture, the precise guarantee, build-vs-borrow, the roadmap, the compiler principles to follow, and known bugs to carry the lessons (not the bugs) from. **Read this first.** |
| **`CONVENTIONS.md`** | The portable engineering rules + working preferences carried over from the `lax-flows-rust` `CLAUDE.md` and memory. Adapt into the new repo's own `CLAUDE.md`. |
| **`reference/`** | A copy of the working, in-prod source that implements today's version of this system. Reference material — the new project's correctness oracle, not its code. |

## The idea in one line

One declarative **YAML** is the single source of truth for the database schema **and** a
semantic *rulebook* (catalog); any query in **RSQL or YAML** is **type-checked against the
catalog before it runs**, with no DB connection — because the YAML *is* the schema. It's a
**compiler** whose target language is SQL. See `DESIGN.md` §1–§5.

## `reference/` map

- `codex/` — the YAML → catalog **compiler** (start at `dtos/compile/mod.rs`;
  `dtos/semantic.rs` is the semantic-analysis pass; `dtos/document.rs` builds the catalog).
- `schema/` — the YAML DSL in real use (`definitions/`), the grammar-only JSON Schema
  (`aggregate-schema.json`), the YAML→SQL jinja template (`templates/`), and the generated
  output (`generated/schema.sql`, a golden file).
- `shared-codex-dtos/` — the catalog's data types (`CodexType`, `CodexColumn`,
  `CodexRelation`, `CodexAggregate`, `ReadShape`). `CodexType` encodes which RSQL operators
  are valid per type.
- `rsql/rsql.rs` — the RSQL parser.
- `mason/` — the catalog-aware **RSQL → SQL** lowering (read path) + projection writers.
- `contracts/codex.rs` — the `Codex` trait (catalog public interface).
- `errors/codex.rs` — `CodexError` (layered error type).
- `python-tool/schema_tool.py` — the current Python YAML→SQL generator + migration driver
  (to be replaced by the Rust engine; kept as a behavioral reference).
- `drivers/yaml.rs` — the YAML parsing driver.

## Suggested first moves in the new repo

1. Read `DESIGN.md` end to end, then `CONVENTIONS.md`.
2. Write the **keyword-contract table** (DESIGN §5.3) — every keyword, the rules it
   carries, what is a compile error. That table *is* the spec for the semantic pass.
3. Decide the **shared query model** that both RSQL and YAML compile into (DESIGN §3).
4. Scaffold the v1 crate: `chumsky + ariadne + sea-query` + the hand-written semantic pass
   (DESIGN §7–§8).

## Status

Design / discuss phase. The *what* is settled; the *how* (query model, catalog surface,
crate split) is the next design work — before any implementation.

## Naming

**Codex** — a codex is a bound catalog/manual, which fits the rulebook perfectly, and it's
the lineage of the existing module name.

⚠️ **Before publishing, verify the handle is usable:** `codex` as a bare crate name is
almost certainly taken on crates.io, and the name collides with OpenAI Codex / GitHub —
un-Googleable and trademark-risky for OSS. Likely workaround: a namespaced crate
(`codex-schema`, `codexql`) and/or a distinct GitHub org. Check crates.io + GitHub from a
networked machine to confirm.
