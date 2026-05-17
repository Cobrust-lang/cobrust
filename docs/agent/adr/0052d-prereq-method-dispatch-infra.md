---
doc_kind: adr
adr_id: 0052d-prereq
parent_adr: 0052
title: "Phase G Direction D prereq — method-dispatch infrastructure (per-type method tables)"
relates_to: [adr:0050e, adr:0052, adr:0051, adr:0052d-pending]
last_verified_commit: 0a90594
date: 2026-05-17
status: accepted
supersedes: []
superseded_by: []
ratification_path: P9 Wave-2 sub-ADR review; ratified on Wave-2 impl merge at `0a90594`
ratified_at: 0a90594
ratified_on: 2026-05-17
---

# ADR-0052d-prereq — Method-dispatch infrastructure (per-type method tables)

## Context

Phase G Direction D (sub-ADR 0052d) ships method-call sugar
`s.split(",")` over the existing PRELUDE-fn surface `split(s, ",")` per
ADR-0050e §"Option C". 0052d is Wave-2 per ADR-0052 §"Sub-ADR
prerequisites"; this prereq ADR MUST land first because 0052d needs a
single uniform method-table mechanism, not four ad-hoc rewrites.

Today, method dispatch is **dict-only** via `try_synth_dict_method` at
`crates/cobrust-types/src/check.rs:920` (ADR-0050d sub-sprint b). It
recognises `Call { callee: Attr { base, name } }` where `base: Dict[K, V]`
and resolves a fixed roster (`.keys()` / `.values()` / `.items()` /
`.get(k)` / `.copy()`) at type-check time. No parser change was needed:
the existing `AccessKind::Attribute` path at
`crates/cobrust-frontend/src/parser.rs:1239-1249` + the AST-to-HIR
lowering at `crates/cobrust-hir/src/lower.rs:1078-1083` already produces
the `ExprKind::Attr` shape that the type-checker matches.

Phase G Direction D extends this pattern to `Str`, `List[T]`, `Float`,
`Int`. Per ADR-0050e §"Phase G migration" (L229-232), the PRELUDE-fn
form stays canonical; the method-form is sugar that resolves to a
rewritten PRELUDE-fn call at type-check time. No runtime dispatch, no
vtable, no boxing: every method call is statically rewritten to its
PRELUDE-fn equivalent before HIR-to-MIR lowering.

## Decision

Generalise the `try_synth_dict_method` pattern to **four additional
per-type method tables** in `crates/cobrust-types/src/check.rs`:

- `fn try_synth_str_method(callee, args, span) -> Result<Option<Ty>, TypeError>`
- `fn try_synth_list_method(callee, args, span) -> Result<Option<Ty>, TypeError>`
- `fn try_synth_float_method(callee, args, span) -> Result<Option<Ty>, TypeError>`
- `fn try_synth_int_method(callee, args, span) -> Result<Option<Ty>, TypeError>`

Each mirrors `try_synth_dict_method` exactly: matches `Call { callee:
Attr { base, name } }`, resolves `base`'s type, checks if `name` belongs
to the type's method-table, type-checks the args against the equivalent
PRELUDE-fn signature, returns the rewritten call's return type. The
existing dict table stays in place; the call-site dispatcher (today a
single-arm match) becomes a chain: try dict → try str → try list → try
float → try int → otherwise `UnknownMethod`.

**Key invariant**: the method-form is sugar over the PRELUDE-fn form,
not an independent dispatch mechanism. Every method name in every table
maps 1-to-1 to an existing PRELUDE stub. No method is reachable that
the PRELUDE-fn form cannot already express. This eliminates the need
for a separate codegen path, a separate intrinsic-rewrite arm, and a
separate C-ABI signature per method.

## Method-table shape

Each `try_synth_*_method` fn follows `try_synth_dict_method` at
`check.rs:920-925`: match `Call { callee: Attr { base, name } }`,
resolve `base`'s type via `synth_expr` + `subst.apply`, guard on the
target type (e.g. `let Ty::Str = base_resolved else { return Ok(None) }`),
then dispatch on `name.as_str()` to a per-method arm. Each arm
type-checks arg count + arg types, emits a rewritten
`ExprKind::Call { callee: Name("<prelude_fn>"), args: [base, ...args] }`
node, returns the PRELUDE-fn's return type. HIR-to-MIR lowering
downstream is unchanged.

## Surface — method-table contents per type

| Type | Method form → PRELUDE-fn target | Arity / Return |
|---|---|---|
| **Str** (10) | `s.len()` → `str_len(s)` | 0 / `Int` |
| | `s.split(sep)` → `split(s, sep)` | 1 / `List[Str]` |
| | `s.replace(a, b)` → `replace(s, a, b)` | 2 / `Str` |
| | `s.trim()` → `trim(s)` | 0 / `Str` |
| | `s.find(sub)` → `find(s, sub)` | 1 / `Int` |
| | `s.contains(sub)` → `contains(s, sub)` | 1 / `Bool` |
| | `s.starts_with(p)` → `starts_with(s, p)` | 1 / `Bool` |
| | `s.ends_with(p)` → `ends_with(s, p)` | 1 / `Bool` |
| | `s.lower()` → `lower(s)` | 0 / `Str` |
| | `s.upper()` → `upper(s)` | 0 / `Str` |
| **List** (5) | `xs.len()` → `len(xs)` (polymorphic `check.rs:1710`) | 0 / `Int` |
| | `xs.push(v)` → `list_push(xs, v)` | 1 / `()` |
| | `xs.get(i)` → `list_get(xs, i)` (polymorphic `check.rs:1696`) | 1 / `T` |
| | `xs.set(i, v)` → `list_set(xs, i, v)` (polymorphic `check.rs:1697`) | 2 / `()` |
| | `xs.is_empty()` → `list_is_empty(xs)` (polymorphic `check.rs:1699`) | 0 / `Bool` |
| **Float** (5) | `f.floor()` → `floor(f)` | 0 / `Float` |
| | `f.ceil()` → `ceil(f)` | 0 / `Float` |
| | `f.is_nan()` → `is_nan(f)` | 0 / `Bool` |
| | `f.is_finite()` → `is_finite(f)` | 0 / `Bool` |
| | `f.abs()` → `abs_f(f)` | 0 / `Float` |
| **Int** (5) | `n.abs()` → `abs(n)` | 0 / `Int` |
| | `n.pow(k)` → `pow(n, k)` | 1 / `Int` |
| | `n.min(m)` → `min(n, m)` | 1 / `Int` |
| | `n.max(m)` → `max(n, m)` | 1 / `Int` |
| | `n.bit_count()` → `bit_count(n)` | 0 / `Int` |

**Total: 25 methods across 4 new tables** (plus existing 5 dict
methods = 30 method-form entry points). Names not in any table fall
through to `Ok(None)` → final chain raises `TypeError::UnknownMethod`.

## Precedence with 0052a `&s`

Per ADR-0052 F-G.3 amendment (line 275): `&s.method()` parses as
`&(s.method())` — method-call binds tighter than the unary borrow.
Matches Rust corpus (`&v.len()` parses `&(v.len())`) per §2.5 §B.
No parser change needed: the existing `parser.rs:1239-1249` Attribute
production + `parser.rs:1105-1110` borrow-operand validator already
produce `Unary(Borrow, Call(Attr(s, "method"), args))` for
`&s.method(args)`.

## Parser / HIR / Types changes

**Parser**: none. The existing
`crates/cobrust-frontend/src/parser.rs:1239-1249` path
(`TokenKind::Dot` + `AccessKind::Attribute`) already produces the
correct `ExprKind::Call { callee: Attribute { base, name }, args }`
shape for `s.method(args)`, identical to today's `d.keys()` path.

**HIR**: none. The AST→HIR lowering at
`crates/cobrust-hir/src/lower.rs:1078-1083` (Attribute → `ExprKind::Attr`)
stays unchanged; method-resolution runs at type-check time and emits
a rewritten `ExprKind::Call { callee: Name("prelude_fn"), args }` —
the same tree a hand-authored PRELUDE-fn call would produce.

**Types**: 4 new fns next to `try_synth_dict_method`. The call-site
dispatcher (single-arm today) becomes a 5-arm chain:

```rust
if let Some(ret) = self.try_synth_dict_method(callee, args, span)? { return Ok(ret); }
if let Some(ret) = self.try_synth_str_method(callee, args, span)?  { return Ok(ret); }
if let Some(ret) = self.try_synth_list_method(callee, args, span)? { return Ok(ret); }
if let Some(ret) = self.try_synth_float_method(callee, args, span)?{ return Ok(ret); }
if let Some(ret) = self.try_synth_int_method(callee, args, span)?  { return Ok(ret); }
return Err(TypeError::UnknownMethod { /* ... */ });
```

Chain order is irrelevant for correctness (each table guards on
base-type) but fixed (dict-first for diffability with M12.x).

## New error variant — `TypeError::UnknownMethod`

Today `TypeError::UnknownName { name, span }` (at
`crates/cobrust-types/src/error.rs:19`) handles free-floating identifier
resolution failures. Method-call resolution failures need a richer
variant that includes the receiver's type name (so the error message
can list available methods).

```rust
TypeError::UnknownMethod {
    type_name: String,           // "str" / "list" / "float" / "int" / "dict"
    method_name: String,         // e.g. "splite" (typo)
    span: Span,
    suggestion: Option<&'static str>,  // "did you mean 'split'?" or list of available methods
}
```

The `suggestion` field aligns with ADR-0052 Direction B (sub-ADR 0052b)
"error UX rewrite — print the FIX, not just the diagnosis" per CLAUDE.md
§2.5 line 78. 0052b's structured-suggestion field stays compatible (the
`Option<&'static str>` here is a Wave-2 stub; 0052b promotes it to the
full structured-suggestion record via post-merge refactor).

## F30 shadow-flip dry-run

This Direction is **additive sugar**: no existing program changes
meaning. The method-form is a new entry point; the PRELUDE-fn form
remains canonical.

**Existing callsite grep at `8dc2723`**:

```bash
$ grep -rn "s\.split\|s\.trim\|xs\.push\|f\.floor\|n\.abs\|s\.len()" \
    examples/ test_programs/ crates/cobrust-stdlib/tests/
(no matches)
```

Zero existing non-dict method-form callsites in the repo. Existing
dict-method callsites (`d.keys()` etc., emitted by M12.x tests)
continue type-checking via `try_synth_dict_method` — they hit the
chain's first arm before any new table.

**Latent-consumer enumeration** (5-10 expected post-merge patterns):

1. `s.split(",")` in CSV parsers (translation L1 emits per ADR-0050e L229).
2. `s.trim()` in input-normalisation passes.
3. `xs.push(v)` in accumulator-loops.
4. `f.floor()` in numerical-rounding paths.
5. `n.abs()` in `if n.abs() < epsilon` predicates.
6. `s.contains(sub)` in conditionals.
7. `s.starts_with(prefix)` in tokenizers.
8. `xs.is_empty()` in guard predicates.
9. `s.replace(a, b)` in template substitutions.
10. `n.pow(k)` in numeric utilities.

Each maps to its PRELUDE-fn alias. Zero behaviour change in any
existing program. Risk profile: **minimal**.

## TEST + DEV PAIR (per F28)

Per ADR-0052 F28 (P10-direct PAIR for impl sprints):

- **TEST sprint** (Opus, ~2-3h wall): authors
  `crates/cobrust-types/tests/method_dispatch.rs` — happy-path per
  method (25 cases), per-type `UnknownMethod` typo negative (5 cases),
  cross-type misroute negative (e.g. dict method on Str), parser
  precedence test for `&s.method()` per ADR-0052 F-G.3.
- **DEV sprint** (Opus, ~3-4h wall): implements the four
  `try_synth_*_method` fns templated from `try_synth_dict_method`,
  the chain dispatcher, the `UnknownMethod` variant + `Display` impl.
- **Total wall**: ~5-7h, P10-direct parallel dispatch via worktree
  topology per `cto_operations_runbook.md`.

## §2.5 compliance

- **Compile-time-catch rule** (CLAUDE.md line 75): `TypeError::UnknownMethod`
  + per-type method-table guard mean `s.splite(",")` (typo) errors at
  type-check time with a concrete "method `splite` not found on `str`;
  available: split, trim, len, ..." message. **Catch enumeration**: 5+
  per type (typo / wrong-arity / wrong-base-type / wrong-arg-type /
  cross-type misroute) — 25+ catches total.
- **Training-data-overlap rule** (CLAUDE.md line 76): `s.split(",")` is
  the canonical surface in Python (`str.split`) AND Rust (`str::split`).
  `f.floor()`, `n.abs()`, `xs.push(v)` all match Python and Rust idioms
  one-to-one. The PRELUDE-fn form `split(s, ",")` was Cobrust-original
  per ADR-0050e Decision; the method-form closes the §B gap.

## Out of scope

- **Vtable / dynamic dispatch**: method-form resolves statically at
  type-check time. Runtime polymorphism is post-M11+ (constitution §"Milestones").
- **Trait methods**: user-declared method protocols (Python `__iter__`,
  Rust `impl Trait for T`) are future-ADR scope.
- **Method-form without PRELUDE-fn already existing**: every entry in
  every table requires its PRELUDE-fn alias to ship. Methods without
  PRELUDE backing (e.g. `s.format(...)`) stay off the table; Phase H+
  adds both at once.
- **Chaining-specific machinery**: `s.split(",").len()` and
  `xs.len().abs()` work naturally (each step is independent
  table-lookup); no extra infra needed.
- **Implicit `self` capture**: none — method-form is pure syntactic
  sugar over an explicit-receiver PRELUDE-fn call.

## Consequences

### Positive

- Generalises the 5-method dict-only precedent to a uniform 4-table
  mechanism shipping 25 additional method-form entry points. Single
  architectural surface for Phase G + future expansions.
- Zero parser / lexer / HIR-lowering change. All complexity is in the
  type-checker, the most diff-able layer.
- Surface matches Python + Rust corpus distribution per §2.5 §B,
  closing the largest Cobrust-original-surface gap after Direction A.
- Method-form is additive sugar — zero F30 cascade risk.
- Sets precedent for Phase H+ trait-based dispatch.

### Negative

- Adds 4 new `~30-line` fns to `check.rs` (already ~1900 lines); may
  warrant a `method_dispatch.rs` refactor in Phase H+.
- Chain dispatcher is `O(5)` per method-call type-check; trivial
  constant today, but a hash-table dispatcher may be warranted
  post-Phase-G if method-call density grows.
- `UnknownMethod.suggestion: Option<&'static str>` is a stub for
  0052b's structured-suggestion record; coordinated audit at Wave-2 close.

### Neutral / unknown

- Whether 0052b's structured-suggestion refactor lands before or after
  0052d. If before, `UnknownMethod.suggestion` ships in structured form
  day one; if after, one refactor pass post-Wave-2.
- Whether Phase H+ promotes per-type tables to a registration macro
  (`register_method!(Str, "split", split, 1, List[Str])`). Phase G
  hand-authors all 25 entries.

### Cascade enumeration (post-spike) — 2026-05-17 / SHA `0a90594`

Per ADR-0052a §13 §"Cascade enumeration (post-v3 spike)" methodology
+ findings/predicate-flip-cascade-discovery-deficit.md SOP. The
Wave-2 DEV impl ran `cargo test --workspace --no-fail-fast` at
HEAD `0a90594` and compared against main HEAD `74f17de` baseline:

**Empirical**:
- Main HEAD `74f17de`: 118 cargo test failures.
- Branch `0a90594`: 118 cargo test failures.
- **Set-diff**: zero new failures on branch, zero fixed failures on
  branch. The two failure sets are byte-identical.
- **Zero LC-100 / f64 / f3ls / 0052a regression.**

**0052dpre-prefix test results at `0a90594`** (45 of 46 green; 1
deferred per finding):

| Family | Count | Status |
|---|---|---|
| `w0052dpre_01..25` (well_typed) | 25 | ok |
| `i0052dpre_01..12` (ill_typed) | 12 | ok |
| `i0052dpre_cross_01` (cross-ADR with 0052b) | 1 | ok |
| `e0052dpre_e2e_01..05` (CLI build+run) | 5 | ok |
| `f30wit_method_01..02` (MIR witness) | 2 | ok |
| `f30wit_method_03` (`&<Call>` precedence) | 1 | deferred (parser blocker) |

**Deferred test — `f30wit_method_03`**: documented in
`findings/0052d-prereq-impl-blocker.md`. ADR §"Precedence with
0052a `&s`" line 117-121 claimed `&s.method()` works in the existing
parser, but `crates/cobrust-frontend/src/parser.rs:1134-1139`
`validate_borrow_operand` rejects `ExprKind::Call { .. }` per
ADR-0052a Wave-1 §8 cap. Resolution path: ADR-0052d follow-up
parser-cap relaxation sub-ADR. The §"Precedence" prose is forward-
looking design; the empirical reality at the spike SHA is `&Call(...)`
parse error. This is the ADR's "no parser change needed" forecast-
miss equivalent to ADR-0052a §13's "bidirectional unify cascade"
sediment — it is documented here so future ADRs reference the
actual scope cap.

**ADSD candidate (F32 sediment family)**: "method-form precedence
witness pre-supposes parser §8 cap relaxation; cap status MUST be
verified at design-time, not assumed". Will file as a finding post-
Wave-2 close if the pattern repeats elsewhere in Wave-2.

**Cascade reduction vs Wave-1 v1/v2 baseline**:
- Wave-1 v1/v2 (bidirectional `Ref(T) ↔ T` unify): 142 cargo test
  failures (cf. ADR-0052a §13 empirical baseline).
- Wave-2 prereq DEV (per-type method tables, no inference change):
  0 new failures vs main HEAD. Method-form is correctly identified
  as static-dispatch sugar (§13 design lesson 2026-05-17 honored).

**Attestation**: zero non-0052dpre regression vs main HEAD
`74f17de`. The 45/46 green + 1 documented deferral represents the
Wave-2 prereq ratification readiness; sub-ADR is mergeable.

## Dispatch readiness

- **TEST sprint**: ~2-3h wall (Opus). 25 happy assertions, 25 typo
  negatives, 10 cross-type-misroute negatives, 1 precedence test
  for `&s.method()`.
- **DEV sprint**: ~3-4h wall (Opus). 4 `try_synth_*_method` fns +
  chain dispatcher + `TypeError::UnknownMethod` variant + `Display`.
- **Total**: ~5-7h wall, parallel TEST+DEV via P10-direct worktree
  dispatch per F28.
- **Host**: Mac local design + spike; DG for `cargo build/test
  --workspace` per heavy-build offload binding policy.
- **Wave 2 ordering**: 0052d-prereq MUST merge before 0052d impl;
  0052d impl is then a small follow-on (~1-2 days) wiring examples +
  docs + final regression on real `.cb` programs.
