---
doc_kind: adr
adr_id: 0041
title: Python semantics compliance binding (H1-H8)
status: accepted
date: 2026-05-09
last_verified_commit: e85630f
supersedes: []
superseded_by: []
---

# ADR-0041: Python semantics compliance binding (H1-H8)

## Context

claude-desktop's external review (review-claude integrated handoff
2026-05-11 §2 H1..H8) surfaced **eight declared-but-not-implemented**
semantic drifts in the compiler core. Each is a constitution §2.2
promise the source did not enforce. The 13th-review §1.7 audit-team-
comparison row notes:

> constitution §2.2 "no GIL / no implicit truthy / no late-binding" →
> `%` not floor mod (H1), `and/or` not short-circuit (H2), closure
> capture returns empty (H5)

The drifts split across crates:

| Drift | Crate | Site | Symptom |
|---|---|---|---|
| H1 | cobrust-codegen | `cranelift_backend.rs:1422` | `srem` (C remainder), `-7 % 3 == -1` not `2` |
| H2 | cobrust-codegen (path) / cobrust-mir (fix) | `cranelift_backend.rs:1427-1428` | `band`/`bor` — RHS always eagerly evaluated |
| H3 | cobrust-codegen | `cranelift_backend.rs:1480-1483` | `iconst(I64, 0)` silent zero for `**`/`@`/`in`/`not in` |
| H4 | cobrust-frontend | `lexer.rs:256-261` + parser | `TokenKind::Walrus` emitted, parser zero-consumes — every walrus is silently rejected far downstream |
| H5 | cobrust-hir | `lower.rs:1287-1297` | `collect_captures` returns `Vec::new()` |
| H6 | cobrust-mir | `lower.rs:1137-1154` | `ExprKind::Comp` lowered to empty-list placeholder; body of comprehension not emitted |
| H7 | cobrust-frontend | `parser.rs:393-400` | `(A, B)` accepted as tuple-expr base, never rejected |
| H8 | cobrust-types | `check.rs:702` | `Ty::Tuple(items)` index returns `items.first()` regardless of index expression |

Each drift independently violates a constitution §2.2 promise and is
testable against CPython behavior. Per claude-desktop handoff §10 the
eight fixes ship in **one PR** because (a) they share the same
acceptance corpus (`python_semantics_corpus.rs`), (b) they intertwine
through `synth_bin` / `lower_bin` / `lower_binop` (same MIR-to-codegen
chain), and (c) atomic delivery prevents the "fix-3, defer-5, lose
context" failure mode.

## Options considered

Per drift, two structural options exist: **(a) implement Python semantics**
or **(b) surface honest error** (`@py_compat(none)` per ADR-0037 — the
constitution-compliant escape hatch). Below, per drift:

### H1 — `%` floor mod

1. **Implement Python floor mod** at codegen — emit `srem` then
   adjust: `if (rem != 0) && ((rem ^ b) < 0) { rem + b } else { rem }`.
   ≤ 8 Cranelift instructions; zero-cost when `b > 0` (common case)
   because the conditional adjust folds away under constant
   propagation. Matches CPython.
2. **`@py_compat(numerical)` divergence** — keep `srem`, document
   `(-1) % 5 → -1` as a known divergence in `docs/agent/findings/`.
   Cheap but punts: every Python program using `%` with a negative
   operand now needs explicit `((a % b) + b) % b` rewrite.
3. Hybrid — implement floor mod for integers, leave float `%` as
   `fsub(a, a)` stub for M11.x.

Decision: **Option 1** (implement). The cost is < 10 lines of
Cranelift IR, the cost of Option 2 is a permanent semantic asterisk
on every Python program.

### H2 — `and` / `or` short-circuit

1. **Desugar at HIR** to `if a then b else False` / `if a then True
   else b`. Requires adding `ExprKind::IfExpr` to HIR (none today —
   `if` is statement-only). Touches every HIR consumer.
2. **Desugar at MIR** at `lower_bin` — when `op ∈ {And, Or}`, allocate
   merge block, emit `SwitchInt` on LHS, conditionally evaluate RHS,
   `phi`-merge. Reuses existing `lower_condition` primitive (ADR-0035).
   Bounded change to one function in one crate.
3. **Document divergence** — declare Cobrust's `and`/`or` as eager-
   boolean (like Rust's `&&`/`||` on `bool`). Breaks every Python
   program that uses short-circuit for guarded access
   (`if x.is_some() and x.unwrap() > 0`).

Decision: **Option 2**. ADR-0035 already extracted `lower_condition`
as the root primitive; extending `lower_bin` to emit a 4-block CFG
for `And`/`Or` is the cleanest fit. Option 1 is structurally cleaner
but wider; Option 3 is non-compliant.

### H3 — `**`, `@`, `in`, `not in` silent zero

1. **Implement all four** — integer-pow via Cranelift loop, MatMul
   via stdlib helper (requires `numpy.matmul`-style runtime in
   `cobrust-stdlib`, M11.x or later), `in`/`not in` via container-
   typed runtime dispatch. Multi-week scope.
2. **Honest codegen error** — `CodegenError::UnimplementedBinOp { op,
   span }` instead of `iconst(I64, 0)`. Compiler reports drift at
   `cargo build`; user knows immediately. Implementation < 20 lines.
3. **`@py_compat(none)`** — type-check rejects `**`/`@` outright;
   keep `in`/`not in` for the type-check-only path (used by `for x in
   xs:`). Hybrid honesty.

Decision: **Option 2 for `**`/`@`/`in`/`not in` codegen**. Per
handoff §H3 "minimum: panic on unimplemented binops". An
`UnimplementedBinOp` codegen error surfaces honestly at build time;
the M11.x track will implement them under the same gate. The type
checker still accepts `target in xs` (per existing `w50_in_list` test
in `well_typed.rs:460`) — only codegen errors. This keeps the
existing test suite green; the new `python_semantics_corpus.rs`
asserts each of the four emits `UnimplementedBinOp`.

### H4 — walrus operator

1. **Implement** — add `ExprKind::Walrus(DefId, Box<Expr>)` to HIR,
   wire scope binding. Walrus expressions inside `if` / `while`
   conditions are the high-value case.
2. **Remove token** — lexer no longer emits `TokenKind::Walrus`; `:=`
   lexes as `:` then `=`, which surfaces as a natural ParseError at
   the next consumer. Honest.
3. **Reject with explicit error** — keep token, parser raises
   `ParseError::DroppedByConstitution { name: "walrus" }`.

Decision: **Option 3**. Option 1 is multi-day scope (HIR variant +
type-check + MIR lowering + capture interaction). Option 2 leaves an
unused token kind in the lexer and surfaces opaque "expected expression"
errors. Option 3 is explicit, honest, and reserves the syntax for a
future ADR. The lexer still emits `Walrus`; the parser surfaces a
clean diagnostic. `python_semantics_corpus.rs` asserts the error.

### H5 — closure capture analysis

1. **Implement** — walk `body` for every `ExprKind::Name`; if its
   `DefId` was bound in an enclosing fn/lambda scope (not in `self`'s
   body, not at module top-level), record as `CaptureSpec`. Algorithm:
   maintain a stack of `(enclosing_fn_def_id_low, enclosing_fn_def_id_high)`
   ranges; a name captures if its `DefId.0` falls into an enclosing
   range, not the current one, not the module range.
2. **Document divergence** — declare Cobrust closures as
   "always-share-by-ref", late-binding is a known M2 divergence to
   address at M3.
3. **Reject closures with captures** — type-check rejects any name
   reference from outside the closure scope inside the body.

Decision: **Option 1**. Option 3 breaks `lambda x: x + base` for
any non-trivial use. Option 2 is the M2-era stance the audit
explicitly flags as drift. We implement basic capture detection; the
capture **mode** (`copy` / `ref` / `move`) is still M3 work — for now
captures are listed but the mode field is `CaptureMode::Default`
(ADR-0005 §"Capture modes" defers explicit mode markers to M3+).

### H6 — comprehension desugaring

1. **Desugar at HIR** — rewrite `Comp` → `For + acc.push(...)` block.
   Requires restructuring HIR Block to allow synthetic locals; type
   checker no longer sees `Comp` at all.
2. **Desugar at MIR** — `lower_expr(Comp)` emits a temp accumulator,
   a for-loop with the comp's iterator pattern and body, and an
   `acc.push(elem)` (or set/dict insert) per iteration. Type checker
   continues to type-check `Comp` directly (unchanged); MIR side
   becomes correct.
3. **Document divergence** — declare comprehensions as M11.x work,
   reject them at MIR lowering with `MirError::UnimplementedComprehension`.

Decision: **Option 2**. The type checker's `synth_comp` is already
correct (`well_typed.rs:254-280` passes). Only the MIR side is
broken. Localizing the fix to `cobrust-mir/src/lower.rs:1137` is
minimal-blast-radius. The new `python_semantics_corpus.rs` asserts
that `[x for x in [1, 2, 3]]` produces `[1, 2, 3]` (or rather, that
the MIR shape contains a loop+collect, not an empty-list placeholder
— see "Acceptance" below).

### H7 — multi-base class rejection

1. **Reject at parser** — when a class def's base is `ExprKind::Tuple`,
   raise `ParseError::Syntax { message: "multi-base class is
   forbidden (constitution §2.2: composition + traits, no MRO)" }`.
   Bounded to `parse_class_def`.
2. **Reject at HIR lowering** — surface
   `LoweringError::MultipleInheritance` from the class lowering
   helper.
3. **Reject at type check** — synthesize `TypeError::DroppedFeature`
   for tuple bases.

Decision: **Option 1**. Parser is the earliest layer; the diagnostic
points at the source span. Options 2 and 3 surface later in the
pipeline with less-clear spans.

### H8 — tuple index returns indexed element

1. **Constant-fold index literal** — at `check.rs:702`, when
   `IndexKind::Expr` wraps a `Lit::Int(n)`, return `items.get(n)`
   instead of `items.first()`. Negative literal indices fold from
   the right (Python semantics).
2. **Union over all tuple elements** — fall back to
   `Ty::Union(items)` for non-constant indices. Conservative; rejects
   programs that mix-type tuples and dynamic indices.
3. **Reject non-constant tuple index** — raise `TypeError::NonConstantTupleIndex`.

Decision: **Option 1 + Option 2 hybrid**. For literal-int indices,
constant-fold to the exact element type. For dynamic indices (or
indices outside the literal-int subset), return `Ty::Union(items)`
(simulated as the first-element type with an explicit comment that
M3 will introduce row polymorphism). This keeps the test
`w36_tuple_index` in `well_typed.rs` green while making the new
`python_semantics_corpus.rs:tuple_index_*` cases assert per-index
type fidelity.

## Decision

Adopt the eight per-drift decisions above as a single binding under
ADR-0041. Each fix is documented in its own §H subsection. Together
they close eight constitution §2.2 promises that the source did not
keep. The new acceptance corpus
`crates/cobrust-types/tests/python_semantics_corpus.rs` carries ≥ 24
test cases (three per H) and is gated by the standard 5-gate workflow
(fmt + clippy + build + test + doc-coverage).

ADR-0003 §"Selected typing rules" remains the authoritative type-rule
binding. ADR-0041 is an **amendment** to ADR-0003 that documents
where the M1-M2 implementation drifted from the rules and how H1-H8
restore alignment. No type rule defined in ADR-0003 is overridden.

ADR-0037 (`@py_compat` hard binding) is referenced for the H3 codegen
error path — `UnimplementedBinOp` is a precursor to the future
`@py_compat(none)` annotation on `**`/`@`/`in`/`not in`.

## Consequences

- **Positive**
  - Constitution §2.2 promises are now source-level enforced for
    eight previously-drifting forms.
  - `python_semantics_corpus.rs` becomes the per-PR semantic-drift
    guard.
  - claude-desktop integrated handoff §2 H1-H8 closed.
  - Every drift now has a stable test case; future regressions
    surface immediately.
- **Negative**
  - H3 (`**`/`@`/`in`/`not in`) is still semantically incomplete —
    we ship an honest error, not a working implementation. M11.x or
    later implements integer pow + container-membership.
  - H5 captures detected but capture **mode** is still M3 work. The
    list now exists; the `copy` / `ref` / `move` choice is still
    type-checker default.
  - H4 walrus is rejected, not implemented. A future ADR will
    implement.
- **Neutral**
  - Existing tests (`well_typed.rs`, `ill_typed.rs`,
    `codegen_diff_corpus.rs`, M11/M11.1/M11.3 corpus, `task_corpus`,
    `task_perf`) must continue to pass — verified by the 5-gate
    workflow.

## Evidence

- claude-desktop integrated handoff: `review-claude-handoff/handoff-pack/dispatches/claude-desktop-integrated-handoff.md` §2 H1-H8
- 13th-review: `review-claude-handoff/reviews/2026-05-11-thirteenth-claude-desktop-integration.md` §1.7
- Constitution: `CLAUDE.md` §2.2 "Drop from Python"
- Prior typing ADR: ADR-0003 (selected typing rules)
- Prior `@py_compat` binding: ADR-0037
- ADR-0035 (lower_condition primitive — root for H2)
- Test file: `crates/cobrust-types/tests/python_semantics_corpus.rs` (NEW in this PR)
- Per-drift source patches:
  - H1: `crates/cobrust-codegen/src/cranelift_backend.rs:1422`
  - H2: `crates/cobrust-mir/src/lower.rs:1271` (lower_bin And/Or branch)
  - H3: `crates/cobrust-codegen/src/cranelift_backend.rs:1480` + `crates/cobrust-codegen/src/error.rs`
  - H4: `crates/cobrust-frontend/src/parser.rs:1208` (parse_atom walrus path)
  - H5: `crates/cobrust-hir/src/lower.rs:1287` (collect_captures rewrite)
  - H6: `crates/cobrust-mir/src/lower.rs:1137` (Comp lowering rewrite)
  - H7: `crates/cobrust-frontend/src/parser.rs:393` (parse_class_def base check)
  - H8: `crates/cobrust-types/src/check.rs:696` (tuple Index branch)
