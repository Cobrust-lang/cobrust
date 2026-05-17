# Design philosophy

> Core tension: **keep Python's ergonomics, drop Python's historical baggage, adopt Rust's safety and performance, and let the AI translation subsystem (as a first-class compiler component) close the migration cost gap.**

## Keep from Python

| Feature | Why keep |
|------|----------|
| Indentation-based blocks | Visual clarity, low ceremony |
| REPL-first feel | Tight feedback loop |
| Iteration protocols, generators | Composability |
| Decorators | Composition primitive |
| Context managers (`with`) | Resource discipline |
| Comprehensions | Expressiveness when bounded |
| Structural pattern matching | Already correct in Python 3.10+ |
| f-strings | Best string formatting in any language |

## Drop from Python (non-negotiable)

- **GIL** → ownership-based concurrency, no global lock
- **Dynamic typing as default** → static structural typing; `dyn` is opt-in, never default
- **Mutable default arguments** → compile error
- **Late closure binding** → explicit `copy` / `ref` / `move` capture
- **`__init__.py` / sys.path / packaging chaos** → single canonical package format, content-addressed, one tool
- **Monkey-patching across module boundaries** → forbidden
- **Silent coercion** (`"1" + 1`, `0 == False`, truthiness of arbitrary types) → type error
- **`is` vs `==` confusion** → `is` removed entirely; use `same_object(a, b)` if identity matters
- **Exceptions as default error path** → `Result<T, E>` is default; exceptions reserved for truly unrecoverable
- **Async / sync function coloring** → one structured-concurrency runtime, no two-color problem
- **Multiple inheritance + MRO** → composition + traits
- **Metaclasses as escape hatch** → compile-time macros + reflection
- **Implicit truthy/falsy** → `if x` requires `x: bool`; otherwise `if x.is_some()`, `if !v.is_empty()`, etc.

## Adopt from Rust

Ownership, borrowing, traits, `Result<T, E>` / `Option<T>`, exhaustive pattern matching, Cargo-style single-tool workflow.

## Cobrust originals

- **`@py_compat` tags** on stdlib items, declaring Python-compatibility tier:
  - `strict` — byte-identical behavior
  - `numerical(rtol=1e-7)` — within numerical tolerance
  - `semantic` — semantically equivalent, possibly different surface
  - `none` — explicitly incompatible (with rationale)
- **Translation provenance**: every translated module carries a manifest (source library, version, oracle artifacts, verification seeds, known divergences). **No silent translations, ever.**
- **Deterministic build IDs**: hash of source + toolchain + LLM router decisions, reproducible bit-for-bit given the same inputs.

## The "why" behind decisions

Each decision pays a real cost. Examples:

- We remove `is` because it creates a forest of beginner traps (`a is b` accidentally true within the small-int cache range), and 99% of legitimate uses can be replaced by `==` or an explicit `same_object(a, b)`
- We remove async/sync coloring because it splits the ecosystem in half and forces every library to ship two implementations — structured concurrency is a better abstraction; a single runtime ends the coloring tax

## Why `&s` not `clone(s)` (ADR-0052a Direction A binding)

Constitution §2.5 binds the design to "the language LLM agents write
correctly on the first try". The LC-100 multi-read pattern — read
the same Str twice (e.g. `let n = str_len(s); let c = str_at(s, 0)`)
— is the largest current LLM-friendliness deficit:

- The compiler today (post-ADR-0050c) rejects the second read with
  `UseAfterMove`, a real §2.5 compile-time signal.
- Phase F.3 M-F.3.5 shipped a `clone(s)` PRELUDE builtin as the
  mitigation. That fix path ratifies the wrong signal: the LLM
  learns "wrap second read with `clone(s)`" which heap-allocates,
  bloats the source corpus, and is not the canonical Rust-style
  answer.
- The right signal is **`&s`**: a zero-cost shared borrow that
  matches the LLM's Rust-priors (`&str` is one of the most-trained
  tokens in the corpus). Per CLAUDE.md §2.5 Direction A binding,
  `&s` is the §2.5-honest closure of the LC-100 honest-debt.

ADR-0052a Wave-1 ships `&s` as a unary prefix expression. The type
checker accepts `str_len(&s)` and `str_at(&s, i)` via a **one-way
call-site coercion** — local, unidirectional (`&Str → Str` only),
scoped to call-arg binding only. `clone(s)` stays available for the
explicit-copy use case but is no longer the canonical fix path
stderr suggests; the new diagnostic says "use `&s` to borrow without
consuming".

Alternative glyphs considered and rejected (ADR-0052a §2):
- `borrow(s)` PRELUDE form: lower LLM training-data overlap; longer.
- Implicit borrow inference: violates §2.5 "compile-time-catch-errors"
  rule — the LLM cannot decode an inference miss from stderr.
- `ref s` keyword (Rust pattern position): conflicts with Cobrust's
  let-rebinding shortcut.

## Further reading

- [Architecture](architecture.md)
- [Milestones](milestones.md)
- Project constitution `CLAUDE.md` (repo root)
