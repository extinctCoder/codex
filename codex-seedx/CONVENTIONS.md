# Conventions & Preferences (carried over)

> Extracted from the `lax-flows-rust` `CLAUDE.md` + persistent memory. These are the
> **portable** engineering rules and working preferences that should carry into the new
> standalone project — the domain-specific parts (aggregates, outbox, Lua, housekeeping,
> CQRS, HTTP handlers) were deliberately left behind. Adapt this into the new repo's own
> `CLAUDE.md`. Rule IDs are kept so they're recognizable.

---

## How to work with me (highest priority)

- **Read constitutional docs first.** Before designing, read the project's rules
  (`CLAUDE.md` / this file). They prune the design space.
- **Discuss → lock → implement.** Exhaustive design chat first; **wait for an explicit lock
  word** ("go" / "do it" / "lock") before writing code. Then edit files directly.
- **LAX-USER-COMMITS** — never run `git commit` or `git push`. Staging, branching, writing
  code are fine; the commit is mine. Don't offer to commit unless asked.
- **LAX-EXHAUST-BEFORE-BUILD** — map every scenario, edge case, and decision point and weigh
  the options *before* implementation, never mid-way.
- **LAX-VERIFY-DONT-ASSUME** — read the actual file/symbol/config; never infer from names or
  memory. When the source contradicts an assumption, the source wins.

## Response style

- **LAX-TLDR-FIRST / LAX-TLDR-ONLY** — lead with the answer, not the reasoning. The
  conclusion, not the supporting context. I'll ask when I want more.
- **LAX-CITE-PATH-LINE** — cite code as `path:line`, don't paste blocks unless the block is
  the answer.
- **LAX-NO-HEADERS** — no `##` everywhere for routine answers; no tables for things that fit
  in two sentences.
- **LAX-NO-PREAMBLE** — no "honest framing" / "to be clear" / "important caveat" preambles;
  no restating the question. State the caveat directly.
- **LAX-NO-NEXT-STEPS** — no "next steps / see also / would you like me to…" unless a real
  decision needs making.
- **LAX-EXPAND-ON-REQUEST** — expand only when asked ("deep dive", "explain", "why", "walk
  me through").

---

## Errors (the layered pattern — keep it, even for a library)

- **LAX-LAYERED-ERRORS** — errors form layered enums; each layer wraps the one below, never
  skips. There is a single root error type.
- **LAX-ERROR-ENUM-FILE** — every error enum in its own file under a shared error module.
- **LAX-ERROR-FROM** — `#[from]` for direct child types; a `transitive_from!`-style macro for
  internal types 2+ hops from the root.
- **LAX-ERROR-HELPERS** — variant-construction helpers return the root error directly (build
  variant + `.into()` inside the helper). Call sites never construct variants directly and
  never chain `.into()`.
- **LAX-INTERNAL-ERR** — internal error types use `?` directly.
- **LAX-EXTERNAL-ERR** — external crate errors use `.map_err(LeafError::from)?`; `#[from]` for
  the external type lives on the leaf enum only.
- **LAX-NO-GENERIC-ERR** — no `Generic { source: Box<dyn Error> }`, no `wrap()`, no `.into()`
  at call sites, no external types on the root error.
- **LAX-ERROR-INTO-STRING** — helpers take `impl Into<String>` and do the `.into()` internally.
- **Rejected: `anyhow` / `eyre`.** Use the layered enum hierarchy. (`M-APP-ERROR` and
  `M-ERRORS-CANONICAL-STRUCTS` are explicitly rejected.)

## Imports & modules

- **LAX-IMPORTS-TOP** — all `use` at the top of the file, before any code.
- **LAX-IMPORTS-CRATE** — within-crate `use crate::…`; across crates `use crate_name::…`.
  Import the short name; no inline `crate::module::Type` paths.
- **LAX-NO-SUPER-SELF** — no `use super::`, no `use self::`.
- **LAX-NO-LOCAL-USE** — no `use` inside functions, blocks, `impl` blocks, or closures.
- Glob re-exports (`pub use foo::*`) are **allowed** for a deliberate single-import facade
  (`M-NO-GLOB-REEXPORTS` is rejected for that one case).

## Naming

- **LAX-DESCRIPTIVE-NAMES** — names describe what the value represents, not its type/position
  (`error` not `e`, `connection` not `conn`, `shard_count` not `n`).
- **LAX-NO-ABBREV** — no single-letter variables, no vowel/syllable-dropped abbreviations.
- **LAX-LOOP-NAMES** — loop/closure params name the item (`for item in &items`,
  `.map(|value| …)`).
- **M-CONCISE-NAMES** — no weasel words (`Service`, `Manager`, `Factory`); prefer domain
  metaphors.
- **LAX-GENERIC-NAMES** — generic params are `T`-prefixed PascalCase nouns (`TAggregate`,
  `TRequest`), never single letters.
- **LAX-NAME-END-TO-END** — one concept carries one name across every layer; don't rename a
  field as it crosses a boundary. Two different concepts never share a name.

## Types & functions

- **LAX-PORT-FIELD-DYN** — port-bearing struct fields are `Arc<dyn Trait>`, never
  `Arc<ConcreteAdapter>`.
- **LAX-DEBUG-NON-EXHAUSTIVE** — hand-write `Debug` for service types holding `Arc<dyn Trait>`
  (`finish_non_exhaustive()`); never `#[derive(Debug)]` on them.
- **LAX-FREE-HELPERS-BELOW** — free helpers used by an `impl` live below it in the same file,
  default visibility.
- **LAX-SEED-STRUCT** — when a helper would take 6+ params, bundle them into a `<Name>Seed<'a>`
  of `&'a` borrows + `&'a dyn Trait` ports.
- **M-REGULAR-FN** — associated functions are for construction (`new`, `from_*`, `with_*`);
  general computation is free functions or instance methods.

## Magic values

- **LAX-CONST-LITERALS** — named `const` for any literal that isn't `0`, `1`, `true`, `false`.
- **LAX-CONST-REASON** — every `const` carries a brief comment explaining *why* the value.
- **LAX-NO-MAGIC-STRINGS** — no magic strings for keys/prefixes/identifiers; name them `const`.

## Documentation

- **LAX-LIB-ONELINER** — every `lib.rs` opens with a single `//!` line stating the crate's
  purpose; no filler.
- **LAX-DEEP-DOCS-EXTERNAL** — complex modules pull deep docs from `docs/` via
  `#![doc = include_str!(...)]`.
- **LAX-FIRST-DOC-SENTENCE** — first sentence of any doc comment is under 15 words.
- **LAX-DOC-NON-OBVIOUS** — `///` only where behavior is non-obvious; never restate the
  signature.
- **LAX-DOC-ERRORS-PANICS** — `# Errors` when failure modes aren't obvious from the return
  type; `# Panics` when it can panic.
- **LAX-NO-DOC-FILLER** — no mechanical docs, no parameter tables, no long `//!` in `mod.rs`.
- **LAX-WHY-COMMENT-BUGS** — multi-line `//` blocks are welcome when they explain *why* code
  is shaped a way (cite the bug/decision); never to describe *what* the code does.

## Universal / correctness

- **LAX-PUBLIC-DEBUG** — all public structs/enums derive `Debug`.
- **LAX-ERROR-TRAITS** — error types implement `Display` + `std::error::Error`.
- **LAX-USER-DISPLAY** — user-facing types implement `Display`.
- **LAX-PANIC-INVARIANT** — panics only for detected programming bugs / impossible states;
  return `Result` for recoverable conditions.
- **LAX-EXPECT-REASON** — `.expect("reason")` naming the invariant, over `.unwrap()`.
- **LAX-NO-UNWRAP** — no `.unwrap()` in library/production code.
- **LAX-EXPECT-LINT** — `#[expect(lint, reason = "…")]` over `#[allow(lint)]`.
- **M-STATIC-VERIFICATION** — run `cargo fmt --all`, `cargo clippy --workspace`,
  `cargo check --workspace` before any commit. Treat clippy warnings as errors.
- **M-LOG-STRUCTURED** — structured logging via message templates + named properties; redact
  sensitive fields.

## Safety

- **M-UNSAFE / M-UNSAFE-IMPLIES-UB / M-UNSOUND** — `unsafe` only for novel abstractions,
  perf-critical paths, or FFI, with written justification; misuse must imply UB; all code
  sound. (Note: this *will* matter here — FFI bindings for the polyglot story are a
  legitimate `unsafe` site; document each.)

## Library guidelines (these matter a lot for an OSS library)

- **M-TYPES-SEND** — public types are `Send`; all futures are `Send`; no `!Send` (`Rc`,
  `RefCell`) in async fns.
- **M-DONT-LEAK-TYPES** — don't leak vendor types (`sqlx::Row`, etc.) across crate
  boundaries; convert at the driver edge.
- **M-AVOID-STATICS** — no `static mut`, no global mutable state; thread state through typed
  handles.
- **M-MOCKABLE-SYSCALLS** — I/O goes behind a trait so tests can mock it.
- **M-AVOID-WRAPPERS** — keep `Arc`/`Rc`/`Box`/`RefCell` out of public API surfaces; expose
  clean inherent methods.
- **M-SERVICES-CLONE** — heavyweight types implement shared-ownership `Clone` via
  `Arc<Inner>`.
- **M-STRONG-TYPES** — `PathBuf` for paths, `Uuid` for IDs, `chrono::DateTime` for time —
  never `String`.
- **M-ESSENTIAL-FN-INHERENT** — core functionality is inherent; traits forward to it so
  callers needn't import every trait.
- **M-IMPL-ASREF** — accept `impl AsRef<T>` in utility signatures.
- **M-INIT-BUILDER / M-INIT-CASCADED** — 4+ init permutations → builder; 4+ params → cascade
  through semantic helper types.
- **M-SIMPLE-ABSTRACTIONS** — don't expose nested parametrized types
  (`Caching<GenericES<Fallback<…>>>`) in primary API surfaces. *(Directly relevant: keep the
  compiler's public surface clean.)*

## Performance

- **M-HOTPATH** — identify hot paths early, benchmark, profile. (Compilation of large
  schemas / query validation throughput is the candidate hot path here — treat it as such.)
- **M-YIELD-POINTS** — long-running async loops include `tokio::task::yield_now().await`.
- **M-THROUGHPUT** — optimize items/CPU-cycle; batch; avoid empty poll cycles.

## Architectural posture (carries, reworded for a library)

- **Hexagonal core.** The compiler's heart (parse → resolve → check → lower) depends on
  nothing external. Parsers, SQL renderers, DB introspection, and FFI are *adapters* at the
  edges, swappable. (From `HEX-*` — the principle, not the flow-engine wording.)
- **Value objects / illegal-states-unrepresentable.** Invariant-bearing types use private
  fields + `fn new(...) -> Result<Self>` enforcing the invariant; an invalid value can't
  exist. (From `DDD-VALUE-OBJECT-IMMUTABLE` / `DDD-VO-SHAPE`; pairs with "parse, don't
  validate" in DESIGN §9.)
- **`M-DI-HIERARCHY`** — prefer concrete > generic > `dyn Trait`; reach for `dyn` only for
  deliberate heterogeneous collections.

## Tech-debt tracking (when something works but crosses a line)

- **Flag, merge, track — don't block.** Blocking is for correctness/safety/data-loss, not
  structural debt. File one tracking issue per accepted-debt change; cite the specific rule
  it violates ("why it's debt" without a cited rule is an opinion, resolve it in review).
