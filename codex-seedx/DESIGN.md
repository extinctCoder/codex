# Declarative Schema & Query Compiler — Design & Rationale

> This document captures the full design discussion behind spinning the `Codex` schema
> system out of the `lax-flows-rust` repo into a standalone, open-source product. It is
> written for someone who was **not** in the original conversation. It records the
> vision, every decision, the options rejected, the architecture, and the principles to
> build on.

---

## 1. One-paragraph pitch

A **declarative, compiler-checked data layer**. You write **one YAML file** that is the
single source of truth for both (a) your database schema (tables, views, indexes) and
(b) a *semantic rulebook* — which columns are filterable/searchable/sortable, what the
aggregates/entities are, which view is the default list, which field is the auth owner.
From that one file the system produces the physical schema **and** an in-memory
**catalog**. Any query — written in **RSQL** (compact, URL-friendly) or **YAML**
(verbose, file-friendly) — is **type-checked against the catalog before it runs**:
unknown field, not-filterable, wrong-operator-for-type are caught at author/build time,
**with no database connection needed**, because the YAML *is* the schema. YAML is the
selling point: a form both humans and programs can read and reason about; SQL is an
opaque string an app can't introspect.

---

## 2. Why this exists (the problem)

- ORMs (SQLAlchemy, SQLx, Diesel, SeaORM, Prisma) are **code-first and language-bound**:
  the schema lives inside one language, and they have **no idea how the app is allowed to
  use the data** (which fields are filterable, who owns a row, what the read surfaces are).
- Migration tools (Atlas, Flyway, Liquibase) evolve the schema safely but know **nothing
  about application semantics**.
- The genuinely novel piece — the moat — is the **bridge**: one declarative source that is
  simultaneously the physical schema *and* the application's query/authz/read contract,
  consumable from any language. Strip that semantic layer out and you're just "Atlas in
  YAML," with no reason to exist.

The closest existing things (Hasura, PostgREST, Prisma) are heavyweight servers or
single-language. None is a small, fast, embeddable library callable from any language.
**That gap is the opportunity.**

---

## 3. The two core features (the product surface)

Both are **declarative** — that consistency is the pitch. The user never hand-writes SQL
or migrations.

1. **YAML** = how you *define*. Schema + the semantic rulebook.
2. **RSQL** (and a YAML query form) = how you *query*. Declarative filters validated
   against the rulebook, never raw SQL.

Framing: **RSQL is the compact filter dialect; YAML is the full-query form; both compile
into one shared query model.** They are not competitors — RSQL covers URL-friendly
filtering, YAML covers full specs (projection + filter + sort + pagination + aggregation)
where RSQL syntax would get ugly. **Build the query model first, syntaxes second.** The
day the two syntaxes drift in capability, it becomes a mess.

---

## 4. Locked decisions

| Decision | Choice | Why |
|---|---|---|
| Build a database / execution engine? | **No.** "Level A" only: declare → validate → generate SQL; the real DB (Postgres, etc.) executes. | The product sits *in front of* existing databases. Postgres is already a world-class executor. |
| Reinvent migrations / multi-DB DDL? | **No** — but see the v1/v2 split below. | Commodity. |
| Use **Atlas**? | **No.** | Its declarative features are login/Pro-gated; not acceptably open-source for an OSS heavyweight. The user hit this wall directly. |
| Language for the engine? | **Rust only.** | Speed + a single fast core. |
| Language-agnostic delivery? | **Yes**, via thin shells over the Rust core: CLI + WASM + FFI (PyO3 / napi). | The ruff / uv / tree-sitter pattern — Rust core, used from any language. Catalog output is JSON, so a consumer can use it with zero bindings. |
| Touch the prod `lax-flows-rust` repo? | **No.** Build standalone; switch the prod system over only once the library is stable. | De-risk. |
| `sea-query` / `sea-schema`? | **Acceptable** optional shoulders (pure Rust, MIT, no rug-pull) for SQL rendering / DB introspection — or write your own small renderer. | Minor, swappable decision. |
| If execution is ever needed? | **Apache DataFusion** (Rust, Apache-2.0) — never build an engine from scratch. | The fully-OSS Rust "SQL VM." |

**Open questions (not yet locked):**
- User-defined custom validation rules — built-in rules only, or also a *constrained*
  user-rule form (a small predicate language, never arbitrary code)? Lean: built-ins
  first.
- Lean (`sea-query` + own small IR) vs batteries (DataFusion's logical plan + unparser,
  heavier) for the IR/output layer.

---

## 5. The big idea: it's a compiler

The right mental model is a **compiler that is parallel to SQL** — not a re-implementation
of SQL, but an alternative, readable, *checkable* surface language that **targets** SQL as
its output. The existing module is literally named `codex/dtos/compile/` — a compiler
front-end was built without being called one.

### 5.1 Why JSON Schema is not enough

JSON Schema is **context-free**: it checks each piece of the document *in isolation*
against fixed rules. It can verify "`from` must be a string." It **cannot** verify "the
`from` value `latest_entity` must be an alias actually declared in this view's joins" —
that needs the *whole document* plus logic. JSON Schema stops at the **grammar** check.
Everything valuable lives in the phases it can't reach.

**Vocabulary correction:** the goal is not "more dynamic." In compiler terms *static* =
checked before running (exactly what we want), *dynamic* = checked at runtime (what we're
avoiding). JSON Schema isn't *too static* — it's **too shallow**. We want **deep,
context-sensitive static analysis**. Demote JSON Schema to "editor autocomplete hints"
(`# yaml-language-server: $schema=…`); the engine's typed parse + semantic pass is the
real, more-powerful validator.

### 5.2 The compiler phases, mapped to this system

| Compiler phase | This system | JSON Schema covers it? |
|---|---|---|
| Parse text → tree | YAML/RSQL → typed structs | — |
| Grammar check | "is this well-formed" | ✅ (all it does) |
| **Symbol table** | **the catalog (rulebook)** | ❌ |
| **Semantic analysis** | field exists? filterable? operator valid for type? `from` resolves? | ❌ |
| Code generation | emit SQL (per dialect) | ❌ |

The three rows JSON Schema can't reach are the entire IP.

### 5.3 Keywords are typed references, not strings — the `from` example

To JSON Schema, `from: latest_entity` is "a string." To a compiler, `from` is a **scoped
reference** carrying a contract:

1. It's an **alias**, legal only if declared by *this view's* `from:`/`join: … as:`
   clauses. `from` is **scoped** to its view (lexical scope).
2. It must **resolve** to a real relation (table, subquery's underlying table, or another
   view).
3. The paired `column` must **exist** in that resolved relation.
4. Its **type flows upward** from the resolved column (this is what later enables query
   type-checking).
5. If it resolves to another view, that view must be **compiled first** → the compiler
   needs a dependency graph, not lucky ordering.

**The governing rule** (write it on the wall):

> **Every keyword is a typed reference with resolution rules, resolved within a scope. An
> unresolved reference is a compile error — never a silent fallback.**

This generalizes to every keyword: `column` (must exist in its `from`), `join.on`
(columns resolve in their aliases), `x_entity.aggregate` (names a real aggregate), RSQL
field names (must be filterable columns in the catalog). The next concrete design artifact
to produce is the **keyword-contract table** — every keyword, every rule it carries, what
is a compile error. *That table is the spec for the semantic pass.*

---

## 6. The guarantee — stated precisely

What the compiler **does** guarantee:

> *Any query that compiles is well-formed against the declared schema — every field
> exists, is allowed, and is used with a valid operator for its type — and produces
> runnable SQL.*

What it does **not** guarantee (do not over-claim "100%"):
- that the query *means what the user intended* (a valid query can return the wrong rows),
- that it's *fast* (valid ≠ not a full-table-scan),
- that it won't hit *runtime* errors outside the schema (bad-data casts, divide-by-zero,
  permissions, connection drops),
- and it holds **only if the live DB was built from the same YAML** — true by
  construction here, void the moment someone hand-alters the database.

**The honest pitch:** *"You can never write a query that explodes because a column doesn't
exist, isn't filterable, or got the wrong operator — checked statically, no DB connection."*
That kills the single most common, most maddening class of runtime query failure.

### 6.1 Escape hatches: contain, don't ban

A declarative subset always loses to SQL on expressiveness; the moment a user needs
something you didn't anticipate, they drop to raw SQL and the tool becomes a toy. The
resolution: a **typed escape hatch**. When a user must drop to raw SQL, they *declare its
shape* ("this fragment returns a number named `release`"). The compiler can't see *inside*
the fragment but checks everything *around* it — exactly like an `unsafe` block in Rust.
You contain the unchecked part behind a type; you never let it leak.

Bonus: the typed escape hatch is **the fix for a real bug in the current code** — computed
columns (`release`, `node_count`) silently fall back to `Text` because nothing can type
them. Let the author *declare* the type on an expression and that whole bug class
disappears. The lever you control is *how expressive you make the language* — more
expressiveness = fewer escape hatches = more compiler to build.

---

## 7. Architecture: build vs borrow

There is **no single Rust "compiler framework"** that does the whole pipeline. The
on-the-nose one for SQL compilers is **Apache Calcite** — but it's Java, which breaks
Rust-only. LLVM/MLIR/Cranelift are compiler frameworks too, but they target *machine
code*, which is the wrong layer.

In Rust, compilers are a **composable stack of focused libraries** — that stack *is* the
framework. The canonical reference is what **rust-analyzer** is built from:

| Phase | Library | Notes |
|---|---|---|
| Lexer | `logos` | fast, derive-based |
| Parser | **`chumsky`** (or `pest`, `lalrpop`, `winnow`) | chumsky = best DX + error recovery |
| Syntax tree | `rowan` | lossless trees; only if you want full editor/LSP tooling |
| Incremental analysis | **`salsa`** | recompute only what changed — *the* framework for the live "validate-as-you-type" endgame |
| Pretty errors | **`ariadne`** (or `miette`) | "your column doesn't exist, here's a ^^^ underline"; pairs with chumsky |
| SQL output | **`sea-query`** (or DataFusion's unparser) | multi-dialect rendering |

**No framework writes the semantic pass — that's your domain logic, by definition. If a
framework could write it, it wouldn't be a moat.** Frameworks make everything *around* the
logic pleasant (Axum's lesson); the moat stays a pure, framework-free core. Which is itself
good hexagonal hygiene: the compiler's heart depends on nothing; the frameworks are
swappable adapters at the edges.

### What you build vs borrow (under Rust-only + fully-OSS)

| Piece | Build or borrow |
|---|---|
| YAML → catalog (the rulebook) | **Build** — exists today (`reference/codex/`) |
| RSQL → validated SQL | **Build** — parser exists (`reference/rsql/`); borrow `sea-query` for rendering, or own it |
| Live-DB introspection | **Borrow** `sea-schema` (pure Rust, MIT) or build per-DB |
| **Schema diff → safe migration plan** | **Build** — the hard, novel, no-OSS-Rust-alternative core (v2) |
| Execute migrations | **Borrow** `sqlx` (pure Rust, MIT) |
| Polyglot delivery | CLI + WASM + PyO3/napi |

---

## 8. Roadmap (sequence value before the hard part)

- **v1 — the moat, low risk.** YAML → catalog + DDL generation + RSQL/YAML query
  validation. Fully OSS, Rust, polyglot. *No migration engine* — generate the full schema,
  the user applies it. This already does something no ORM does. Needs only
  `chumsky + ariadne + sea-query` + the hand-written semantic pass.
- **v2 — the heavyweight claim.** Add the introspect → diff → safe-migration-plan engine.
  This is the hard part (no pure-Rust-OSS multi-DB alternative exists = the real gap). Add
  `salsa` (+ maybe `rowan`) here for the incremental live-editor experience.

This repo (`lax-flows-rust`) is the **reference implementation + test oracle**: the new
library is "correct" when it reproduces this repo's catalog + DDL.

---

## 9. Compiler-construction principles to follow (the 50-year-old wheel)

There's no framework, but there's a deep body of established practice. Follow it; don't
rediscover it.

**Architecture**
1. **Strict phase separation** — parse → resolve names → semantic-check → lower to IR →
   emit SQL. Each phase a clean boundary; no phase reaches back.
2. **AST ≠ IR.** AST mirrors the *source* (YAML/RSQL shape); IR is the *normalized
   semantic model* (catalog + query plan). Lower one into the other explicitly.
3. **Many small passes, not one mega-pass.** One check per concern (`semantic.rs` already
   does this).

**Names & scopes**
4. **Name resolution is its own phase** — resolve every reference to its declaration,
   within a scope, producing a "resolved" tree.
5. **Scopes are explicit** — a `from` alias is visible only inside its view (lexical scope).

**Errors**
6. **Never fall back, never fail silently** — an unresolved name is a compile error, not a
   default. (The `Text`-fallback bug violates this.)
7. **Carry source spans + collect all errors** — point at the exact YAML line; don't die on
   the first error.

**Determinism & typing**
8. **Deterministic output** — same input → same output, byte-for-byte. No hash-order
   leaking into results. (Bug #1 below was exactly this.)
9. **"Parse, don't validate" / make illegal states unrepresentable** — construct a
   `ResolvedColumn` that *can only exist* if resolution succeeded. The single
   highest-leverage discipline for a Rust compiler.

**Testing & incrementality**
10. **Golden/snapshot tests** — compile samples, snapshot the SQL/catalog, diff on change
    (the committed `schema.sql` regenerate-and-diff *is* a golden test). The `insta` crate
    formalizes it.
11. **Demand-driven + memoized** for the live editor (`salsa`).

**Canon to read (take it from the source, not from notes):**
- *Crafting Interpreters* (Nystrom) — the modern, readable bible. Start here.
- rust-analyzer architecture notes (matklad's blog) — IDE-grade incremental compiler in Rust.
- *Parse, don't validate* (Alexis King) — the typing discipline in #9.
- Apache Calcite design docs — SQL-compiler-specific patterns (catalog, validator,
  relational algebra), even though you won't use the Java code.
- The Dragon Book / *Engineering a Compiler* — deep theory when needed.

---

## 10. What's in `reference/` and how today's system works

The copied code is the working, in-prod reference implementation. Two independent
consumers parse the same YAML today — proof the YAML-as-contract model works:

1. **Rust `Codex` compiler** (`reference/codex/`) → builds the in-memory **catalog**
   (relations, columns, types, aggregates, enum types) used to validate queries. Does
   **not** emit SQL.
2. **Python `schema_tool.py`** (`reference/python-tool/`) + the **jinja template**
   (`reference/schema/templates/schema.sql.jinja`) → renders the physical Postgres DDL
   into `reference/schema/generated/schema.sql`.

Reference map:
- `reference/codex/` — the YAML → catalog compiler. Start at `dtos/compile/mod.rs`
  (`compile()` orchestrates: parse_many → merge → validate_schema → expand_partials →
  semantic::check → into_catalog). `dtos/document.rs` builds the catalog;
  `dtos/semantic.rs` is the semantic-analysis pass (the "beyond JSON Schema" rules);
  `dtos/{table,view,column,select,partial,enum_type}.rs` are the per-keyword YAML shapes.
- `reference/schema/definitions/` — the actual YAML schema files (the DSL in real use).
- `reference/schema/aggregate-schema.json` — the JSON Schema (grammar-only validation).
- `reference/schema/templates/schema.sql.jinja` — YAML → SQL DDL rendering.
- `reference/schema/generated/schema.sql` — the generated output (golden file).
- `reference/shared-codex-dtos/` — `CodexType` / `CodexColumn` / `CodexRelation` /
  `CodexAggregate` / `ReadShape`: the catalog's data types. `CodexType::supports_ordering`
  etc. encode which RSQL operators are valid per type.
- `reference/rsql/rsql.rs` — the RSQL parser (`==`, `=in=()`, `=like=`, `;` AND, `,` OR).
- `reference/mason/` — the read/write path: turns a parsed filter + the catalog into
  project-scoped SQL (`reader.rs`, `sql.rs`, `read_utils/{filter,search,select,sort}.rs`).
  This is the "RSQL → SQL" lowering, catalog-aware.
- `reference/contracts/codex.rs` — the `Codex` trait (the catalog's public interface).
- `reference/errors/codex.rs` — `CodexError` (the layered error type).
- `reference/drivers/yaml.rs` — the YAML parsing driver.

---

## 11. Known issues in the reference code (carry the lessons, not the bugs)

1. **[FIXED in the source repo] Nondeterministic merge order.** `compile()` merged YAML
   files in `HashMap` iteration order (randomized per process), while view column types
   resolve in merged-array order. This intermittently mistyped cross-file view columns
   (e.g. `node_execution_io.batch_size` → `Text` instead of `Number`, breaking `=gt=`
   filters ~half the time on restart). Fixed by sorting files deterministically
   (`shared → roots → others alphabetical → histories`). **Lesson → principle #8.**
2. **Silent `Text` fallback** on unresolved `from`/computed columns — violates principle
   #6. The new design's typed escape hatch + "no silent fallback" rule fixes this class.
3. Partial-vs-table column precedence is *inverted* between the Rust compiler (partial
   wins) and the jinja template (table wins) — latent divergence; in the new single-engine
   design there is only one precedence.
4. Duplicate `x_partials` across files: Rust hard-errors, Python silently takes the last —
   divergent. One engine removes the divergence.

These are exactly the bugs the compiler principles above would have prevented — concrete
proof the principles are worth adopting deliberately.

---

## 12. Status

We are in the **design / discuss** phase. The *what* is settled (sections 1–7). The *how*
(query model shape, catalog surface, crate/repo split, the keyword-contract table) is the
next design work — **before** any implementation.
