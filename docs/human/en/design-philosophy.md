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

## Translation tier system (ADR-0052c)

The `@py_compat` tag is more than documentation — it is the typed contract
the L2 behavior verifier enforces, end-to-end. ADR-0052c (2026-05-17)
makes the tier a typed enum at every layer:

- **Spec layer**: `corpus/<lib>/spec.toml` declares `py_compat = "strict"`
  (or `"semantic"`, `"numerical(rtol=1e-7)"`, `"none"`). Malformed
  strings (e.g. `"strikt"`) reject at spec-load time with a diagnostic
  naming the typo and the expected variants — the `§2.5
  compile-time-catch` rule applied to spec data.
- **Verifier layer**: `TierVerifier` dispatches per-tier verdict policy:
  - `Strict` → byte-identity check; any divergence rejects.
  - `Semantic` → structural equivalence (dict key order, whitespace,
    JSON-shaped output normalized before compare).
  - `Numerical { rtol }` → `numpy.testing.assert_allclose(rtol=...)`
    semantics for f64 comparisons.
  - `None` → gate disabled; verdict honestly recorded as `Skip` per
    ADR-0040.
- **Router layer**: per-tier routing override via `[routing.translate_<tier>]`
  blocks. `Strict` routes through consensus (n=2 by default), `Numerical`
  routes through cost (cheap single-model is fine since rtol absorbs
  emission variance), `Semantic` falls back to the global Quality
  default.
- **Prompt layer**: each tier emits a tier-specific instruction block
  into the L1 translation prompt, telling the LLM the contract its
  emission must satisfy ("output MUST be bit-identical" vs "output
  MUST satisfy assert_allclose(rtol=...)" vs "output MUST match
  structurally").

The backward-compat M7+ numpy-corpus sidecar form (`py_compat =
"numerical"` + `py_compat_rtol = 1e-7`) remains accepted; bare
`"numerical"` without a sidecar defaults to `rtol = 1e-7`.

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

## Method-form as PRELUDE-fn sugar (ADR-0052d-prereq Direction D binding)

ADR-0052d-prereq adopts a per-type method-call form (`s.split(",")`)
that rewrites to the canonical PRELUDE-fn form (`split(s, ",")`)
at type-check time. The decision honors §2.5 "training-data-overlap"
rule: Python (`str.split`) and Rust (`str::split`) both use the
method-call surface; the dot-after-receiver shape is the most
common ergonomic in the LLM's training distribution.

Key design properties:

- **Static resolution**: the method form rewrites at type-check
  time to a PRELUDE-fn call with the receiver as the first arg.
  No vtable, no dynamic dispatch, no runtime cost. Method-form
  and PRELUDE-fn form produce identical machine code (see
  [ADR-0052d-prereq](../../agent/adr/0052d-prereq-method-dispatch-infra.md)
  §"Surface — method-table contents per type").
- **PRELUDE-fn form stays canonical**: every method-form has a
  PRELUDE-fn alias the user can write directly. Method-form is
  pure sugar — there is no method-form that cannot be expressed
  as a PRELUDE-fn call.
- **§2.5 "compile-time-catch" sharpening**: typos like
  `s.splittt(",")` surface as `TypeError::UnknownMethod` with a
  "did you mean 'split'?" hint. The LLM's compile-error feedback
  loop now decodes "I called a method that doesn't exist on str;
  the suggestion lists the available methods" — a stronger signal
  than the previous behaviour (silent fresh-var inference on the
  `Attr` callee).

Method-table coverage today (25 methods): `str` (10), `list[T]`
(5), `f64` (5), `i64` (5). Dict methods ship under separate
ADR-0050d sub-sprints. Method-form for user-declared types is
post-M11 (trait-based dispatch).

## Further reading

- [Architecture](architecture.md)
- [Milestones](milestones.md)
- Project constitution `CLAUDE.md` (repo root)
