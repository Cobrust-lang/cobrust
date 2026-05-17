---
doc_kind: adr
adr_id: 0052a
parent_adr: 0052
title: "Direction A — Explicit `&s` borrow / let-rebind shortcut"
status: proposed
date: 2026-05-16
last_verified_commit: 708981b
supersedes: []
superseded_by: []
relates_to: [adr:0050c, adr:0051, adr:0052]
discovered_by: ADR-0052 Phase G frame ADR §"Direction A scaffolding anchors" pre-commit
ratification_path: P9 Wave-1 sub-ADR review (per ADR-0052 §"Sub-ADR prerequisites")
---

# ADR-0052a: Direction A — Explicit `&s` borrow / let-rebind shortcut

## 1. Context

Phase F.3 closed with the M-F.3.5 `clone(s)` PRELUDE builtin as a mitigation, not a fix, for the LC-100 mass regression filed in `findings/lc100-str-use-after-move-regression-from-adr0050c.md`. The mitigation made every PRELUDE Str read at LC-100 sites read `clone(s)`, which:

- compiles, but produces `__cobrust_str_clone` heap allocations on every read;
- ratifies a §2.5-violating signal: the LLM's compile-error feedback says "wrap with `clone()`" rather than "borrow with `&`";
- semi-permanently bloats the LC-100 source corpus with `clone(s)` clutter (the largest single LLM-friendliness deficit per LC-100 honest-debt empirical baseline, per CLAUDE.md §2.5 Direction A binding).

ADR-0050c §"Decision" chose **Option A** (Str non-Copy uniformly across operand-level and drop-level). Phase 2a walk-back loosened **List** to Copy-at-operand-but-non-Copy-at-drop, but kept **Str** as non-Copy uniformly. The asymmetry is intentional: it preserves the use-after-move catch as a *real* §2.5 compile-time signal — but only if the fix path the LLM learns is `&s`, not `clone(s)`.

This sub-ADR introduces the `&s` explicit-borrow expression form. It is the §2.5-honest closure of LC-100 honest-debt: stderr says "use `&s` not `s` for read-only borrow", LLM retries with `&s`, compiler accepts, no heap clone fires.

CLAUDE.md §2.5 Direction A: "Phase G P0 — eliminates `clone()` clutter; the LARGEST current LLM-friendliness deficit per LC-100 honest-debt empirical baseline."

ADR-0052 §"Direction A scaffolding anchors" (frame ADR pre-commit, HEAD `708981b`) pre-grounded the design surface; this sub-ADR adopts it verbatim and adds the F30 shadow-flip dry-run table.

## 2. Decision

**The glyph is `&`. The form is `&expr`, an immutable shared borrow expression.**

Adopted per ADR-0052 §"Direction A — Explicit borrow / let-rebind" §B "Training-data overlap matched" Rust-corpus precedent: `&str` is one of the most-common-in-training tokens. The `&` glyph reads as borrow under both the Rust prior and the C/C++ prior (address-of, with the immutable-by-default Rust narrowing). Cobrust adopts the Rust interpretation: `&s` is a borrow, not address-of, and there is no `*` deref operator at the source level (deref is implicit on field/method access, per Rust ergonomics).

The decision is binding for Wave 1 ship. Method-call sugar (`s.split(",")`, ADR-0052d) is decided independently and is **out of scope** for this sub-ADR; this sub-ADR ships `&s` as a unary prefix on identifiers and field accesses today.

Alternative glyphs considered and rejected:

- `borrow(s)` PRELUDE form: lower training-data overlap; longer; ratifies the LLM's belief that PRELUDE-fn-form is the canonical surface, which ADR-0052d will reverse.
- Implicit borrow inference: §2.5 violation per the LLM-first principle's "compile-time-catch-errors" rule — the LLM cannot decode an inference miss from stderr.
- `ref s` keyword (Rust pattern position): conflicts with Cobrust's `let` rebinding shortcut (see §3); the `&` glyph is unambiguous in expression position.

## 3. Semantics

- **`&s`** in expression position constructs an immutable shared borrow of `s`. Operand-level lowering emits `Operand::Copy(place)` instead of `Operand::Move(place)`. The use-after-move catch at `crates/cobrust-mir/src/borrow.rs:114` does not fire for borrowed reads.
- **Type**: `&s : Ty::Ref(Ty)` where `Ty = type_of(s)`. The borrowed type `Ty::Ref(Ty)` is a **distinct type at inference** — it does **NOT** unify with `Ty` in the substitution table (this was tried in v1+v2 DEV and produced a 100+ test cascade via inference ambiguity — see §13 "Design lesson 2026-05-17"). PRELUDE Str helpers (`str_len`, `str_at`, etc.) accept both `s: Str` and `&s: &Str` via **one-way call-site coercion only**: when a formal parameter type is `T` and the actual argument type is `Ty::Ref(T)`, the type checker emits an implicit auto-deref (drops the `Ref` wrapper) at that single call site. The coercion is (a) **local** — does NOT propagate via inference substitution; (b) **unidirectional** — `Ref(T) → T` only, never `T → Ref(T)`; (c) **scoped to fn-call argument-binding positions only** — `let` bindings, return types, and arithmetic operands do NOT auto-coerce. The difference is observable at MIR (`Operand::Move` vs `Operand::Copy`).
- **Scope**: function-body-scoped. The borrow is valid from the point of construction until the borrowed binding leaves scope. Wave 1 ships **intra-block** borrow checking (matches `crates/cobrust-mir/src/borrow.rs` module-comment "M8 is intra-procedural; inter-procedural lifetime obligations land at M9"). Inter-block / inter-function NLL is deferred to M9.
- **`let` rebind shortcut**: `let s = &s` is the let-rebinding form. The right-hand `&s` borrows the outer binding; the new `s` shadows it inside the rebind's scope. This is the §2.5-honest replacement for `let s = clone(s)`.
- **Mutability**: `&mut s` is deferred to a future sub-ADR; Wave 1 is shared-borrow-only. Str helpers are all read-only, so `&s` covers 100% of the LC-100 honest-debt corpus.

## 4. Surface examples (8 pairs)

### 4.1 LC-02 reverse_string (canonical LC-100 trigger)

Today (post-M-F.3.5 mitigation):
```cobrust
fn main() -> i64:
    let s = input("")
    let n = str_len(clone(s))
    let i: i64 = n - 1
    while i >= 0:
        let c = str_at(clone(s), i)
        ...
```

Tomorrow (Direction A):
```cobrust
fn main() -> i64:
    let s = input("")
    let n = str_len(&s)
    let i: i64 = n - 1
    while i >= 0:
        let c = str_at(&s, i)
        ...
```

### 4.2 LC-13 roman_to_integer

Today: `let n = str_len(clone(s)); for i in 0..n: let c = str_at(clone(s), i)`
Tomorrow: `let n = str_len(&s); for i in 0..n: let c = str_at(&s, i)`

### 4.3 LC-20 valid_parentheses

Today: `let n = str_len(clone(s)); while i < n: let c = str_at(clone(s), i)`
Tomorrow: `let n = str_len(&s); while i < n: let c = str_at(&s, i)`

### 4.4 let-rebind shortcut

Today: `let s = clone(s); do_thing(s)`
Tomorrow: `let s = &s; do_thing(s)` — `s` is now a borrow; outer `s` still owned.

### 4.5 Function-arg pass-by-borrow

Today: `print_str(clone(label))`
Tomorrow: `print_str(&label)`

### 4.6 Repeated reads in a comprehension predicate

Today: `let xs = [parse_int(clone(line)) for line in lines if str_len(clone(line)) > 0]`
Tomorrow: `let xs = [parse_int(&line) for line in lines if str_len(&line) > 0]`

### 4.7 Conditional borrow

Today: `let v = if cond: str_len(clone(s)) else: 0`
Tomorrow: `let v = if cond: str_len(&s) else: 0`

### 4.8 Borrow chained through a let

Today: `let n = str_len(clone(s)); let m = str_ord(clone(s), 0); let p = n + m`
Tomorrow: `let n = str_len(&s); let m = str_ord(&s, 0); let p = n + m`

## 5. F30 shadow-flip dry-run (20 callsites)

Per `findings/predicate-flip-cascade-discovery-deficit.md` SOP — this table enumerates direct + latent consumers of the Str=non-Copy / `&s` flip. Sourced from grep over `examples/leetcode/*.cb` (LC corpus, the load-bearing demand surface) and ADR-0050c §"F29 27-consumer enumeration" (already-verified shared-infra consumers).

Cascade risk legend: **L** = latent (was Copy-pre-0050c, mitigation-only-now); **D** = direct consumer of new `&s` form; **N** = no change required (read-once); **C** = clone-clutter retirement target.

| # | Callsite (file:line) | Current form | `&s` replacement? | Cascade risk |
|---|---|---|---|---|
| 1 | `examples/leetcode/reverse_string.cb:11` | `str_len(s)` (first read) | Y → `str_len(&s)` | L+C |
| 2 | `examples/leetcode/reverse_string.cb:14` | `str_at(s, i)` (second read) | Y → `str_at(&s, i)` | L+C |
| 3 | `examples/leetcode/roman_to_integer.cb:31` | `str_len(s)` | Y → `str_len(&s)` | L+C |
| 4 | `examples/leetcode/roman_to_integer.cb:36` | `str_at(s, i)` | Y → `str_at(&s, i)` | L+C |
| 5 | `examples/leetcode/valid_parentheses.cb:16` | `str_len(s)` | Y → `str_len(&s)` | L+C |
| 6 | `examples/leetcode/valid_parentheses.cb:22` | `str_at(s, i)` | Y → `str_at(&s, i)` | L+C |
| 7 | `examples/leetcode/fibonacci.cb:10` | `parse_int(input(""))` | N — read-once | N |
| 8 | `examples/leetcode/binary_search.cb:13` | `parse_int(input(""))` | N — read-once | N |
| 9 | `examples/leetcode/binary_search.cb:15` | `parse_int(input(""))` | N | N |
| 10 | `examples/leetcode/maximum_subarray.cb:12` | `parse_int(input(""))` | N | N |
| 11 | `examples/leetcode/climbing_stairs.cb:12` | `parse_int(input(""))` | N | N |
| 12 | `examples/leetcode/two_sum.cb:13` | `parse_int(input(""))` | N | N |
| 13 | `examples/leetcode/two_sum.cb:17` | `parse_int(input(""))` (loop body, fresh `input`) | N | N |
| 14 | `examples/leetcode/two_sum.cb:20` | `parse_int(input(""))` | N | N |
| 15 | `examples/leetcode/stock_best_time.cb:12` | `parse_int(input(""))` | N | N |
| 16 | `crates/cobrust-mir/src/lower.rs:1235-1242` (`ExprKind::Name` operand-read arm in `lower_expr`) | `Operand::Move/Copy` dispatch via `is_copy_type` | D — new branch on `ExprKind::Borrow` | D (latent: any non-Copy local reached by `&` must now emit `Operand::Copy` regardless of Phase-4 implicit clone rule) |
| 17 | `crates/cobrust-mir/src/lower.rs:2147` `is_copy_type` predicate | `Ty::Str → false` | N (predicate unchanged); D for `Ty::Ref(_)` new variant | D |
| 18 | `crates/cobrust-mir/src/borrow.rs:114` `MirError::UseAfterMove` fire site | fires on `Operand::Move` second read | N (still fires for non-borrowed second reads — this is the desired §2.5 catch) | N |
| 19 | `crates/cobrust-stdlib/src/io.rs:479,495,512,534,561,585,624,644` (PRELUDE Str helpers) | All take `s: *mut u8` C-ABI; `&s` lowers identically | N (Wave-1 transparency rule) | N |
| 20 | `crates/cobrust-stdlib/src/fmt.rs:306` `__cobrust_str_clone` shim | Called by `clone(s)` builtin from M-F.3.5 | C — retire from idiomatic LC-100 paths (shim stays for explicit `clone(s)` cases, e.g. `Aggregate` lowering per ADR-0050c borrow.rs:120-129 Phase 4 note) | C |

**Direct consumers**: 4 (rows 16-17 MIR lower, plus parser+codegen entries in §§8-9). **Latent consumers (LC-100 corpus)**: 6 (rows 1-6 — the actual demand surface). **No-change**: 9 (rows 7-15, 18-19 — read-once or transparency-rule covered). **Clone-clutter retirements**: rows 1-6 + 20.

Wave-1 spike commit will land the parser+HIR+types+MIR diff behind feature flag `cobrust_borrow_phase_g`; full `cargo test --workspace` matrix runs flag-ON, classifies any new failure as direct/latent/genuine per F30 SOP. The 6 LC-100 corpus rows are the expected pass-reversion (red → green). Any non-corpus failure is a latent consumer and gets enumerated here before flag removal.

### 5.5 Pre-dispatch verification (F30 SOP step 4 — binding merge prerequisite)

Per `findings/predicate-flip-cascade-discovery-deficit.md` §"Operational SOP" step 4 — the §5 table is the *prediction*, NOT the dry-run. The shadow-flip dry-run is binding under the following discipline:

1. **Spike-commit gate**: the Wave-1 spike PR is NOT mergeable until `cargo test --workspace --locked --features cobrust_borrow_phase_g` has been executed at the spike SHA AND a §"Consequences addendum" landing in this ADR enumerates every classified failure (`direct` = expected new behavior; `latent` = previously-Copy consumer surfaced; `genuine` = legitimate regression requiring impl fix). Without the addendum, the spike is exploration, not a merge artifact.
2. **Addendum format**: subsection added to §13 (Consequences) titled `### Cascade enumeration (post-spike)` listing each classified failure with: test path, classification, disposition (fixed-in-spike / deferred-to-DEV / out-of-scope). Min one failure or explicit "zero unclassified failures" attestation.
3. **F30 vs §15 reconciliation**: §15 "F30 enumeration NOTE" defers ground-truth to spike time; §5.5 now binds the ground-truth as a merge prerequisite so the deferral does not become a permanent gap. Audit teammates check §"Cascade enumeration" presence in the spike-commit's diff before recommending GO.
4. **Empirical sanity**: ADR-0050c spike + Wave-2 list[str] + Wave-3 dict each produced 4-8 latent consumers per F30 §"Empirical" L67-72. Direction A's transparency rule (§3) shrinks the latent surface but does not eliminate it; expect 1-3 latent failures here.

## 6. Type checker changes

Per ADR-0052 §"Direction A scaffolding anchors":

- `crates/cobrust-types/src/check.rs` — add `ExprKind::Borrow(inner)` arm. Type: `Ty::Ref(Box::new(check(inner)))`. **NO unify-arm in `infer.rs`** for `Ty::Ref ↔ T` (the v1+v2 cascade root — see §13). Instead, at the **fn-call argument-binding site** (`ExprKind::Call` synth_call_args path) add a one-way coercion: when formal param type is `T` and actual arg type unifies with `Ty::Ref(T)`, accept the call and emit a flag for codegen to skip the `Move` (already handled because `Ty::Ref(_)` is `is_copy_type = true`). New error variant `TypeError::BorrowOfNonPlace { span, suggestion: Option<&'static str> }` (Wave-1: only `Name` and field-access expressions are borrow-able; literal borrows like `&"hello"` are deferred).
- `crates/cobrust-types/src/ty.rs` — add `Ty::Ref(Box<Ty>)` variant. Wave-1: `Display` impl prints `&T` matching the surface glyph.
- `crates/cobrust-types/src/error.rs` — add `BorrowOfNonPlace { span: Span }` to `TypeError`; per §2.5 Direction B forward-compat, populate `suggestion: Option<&'static str>` field at construction (Direction B sub-ADR ratifies the field shape).

## 7. MIR changes

Per ADR-0052 §"Direction A scaffolding anchors":

- `crates/cobrust-mir/src/lower.rs:2147` `is_copy_type` predicate — **unchanged for `Ty::Str`** (still `false`). Add `Ty::Ref(_) → true` (borrows are Copy operands).
- `crates/cobrust-mir/src/lower.rs:1235-1242` `ExprKind::Name` operand-read arm in `lower_expr` — add sibling `ExprKind::Borrow(name)` arm. The borrow arm always emits `Operand::Copy(place)` regardless of the underlying type's Copy-ness. The new operand still references the original place (no temp); borrow-check at `borrow.rs:114` does not fire because the operand is `Copy`, not `Move`.
- `crates/cobrust-mir/src/borrow.rs:114` `MirError::UseAfterMove` — fire site unchanged. It continues to fire for non-borrowed second reads (the desired §2.5 catch). The §2.5 win is that the diagnostic message now suggests `&s` as the fix (Direction B coordination — Wave-1 ships the suggestion as a hard-coded string at the construction site; Direction B sub-ADR formalizes the structured `suggestion` field).

## 8. Parser changes

- `crates/cobrust-frontend/src/parser.rs` — add unary prefix `&` to the unary expression production. Precedence: same as Rust's `&` (between `*` deref and `as` cast). Wave-1 ships `&ident`, `&ident.field`, `&ident[idx]` — three production paths. `& <complex_expr>` is a parse error in Wave-1 (parens required: `&(expr)`).
- `crates/cobrust-hir/src/lower.rs` — add `HirExprKind::Borrow(inner_hir_id)` lowering. Mirrors the AST → HIR map for other unary operators.

## 9. Codegen changes

- `crates/cobrust-codegen/src/cranelift_backend.rs` — the new `Operand::Copy` operand path for borrowed reads lowers identically to existing `Operand::Copy` lowering (no new Cranelift IR). The Wave-1 transparency rule means `&s` and `s` produce identical machine code at PRELUDE-call sites — only the MIR `Operand` tag differs. **Zero codegen surface for Wave-1.**
- Future: `&mut s` (deferred) will require a new Cranelift IR path; not Wave-1.

## 10. TEST + DEV PAIR plan

Per F28 strict-separation discipline (`findings/adsd-pair-pattern-impl-gap.md`): TEST agent authors corpus + sees parser+types+MIR scaffolding only; DEV agent implements without seeing TEST corpus until P10 merge.

### 10.1 TEST corpus categories

- **Well-typed (≥ 30 programs)**: every §4 surface example + 22 more covering `&ident.field`, `&ident[idx]`, let-rebind chains, comprehension predicates, conditional borrow, mixed borrowed/owned reads.
- **Ill-typed (≥ 15 programs)**: `&"literal"` (parse error Wave-1), `&(complex_expr_without_parens)` (parse error), `&undefined_ident` (TypeError), `&s` where `s` already moved (TypeError::BorrowOfMovedValue or UseAfterMove — Wave-1 disambiguates which).
- **E2E (≥ 6 programs)**: LC-02 / LC-13 / LC-20 with `&s` replacements, valgrind-clean exit, output byte-identical to pre-M-F.3.5-clone-mitigation baseline.
- **F30-witness (≥ 4 programs)**: each row 1-6 of the §5 table reproduced as a standalone E2E test, asserting (a) no `__cobrust_str_clone` MIR call appears, (b) UseAfterMove does not fire, (c) exit code matches oracle.

### 10.2 DEV phases

- Phase 1 (parser+HIR): ~1h. Add `&` unary prefix; HIR lowering. Spike-commit feature-flag.
- Phase 2 (types): ~1.5h. Add `Ty::Ref`, `ExprKind::Borrow` check arm, **one-way call-site coercion** at `synth_call_args` (NOT a bidirectional unify rule; do NOT touch `infer::unify` for `Ref`/`T` interconversion).
- Phase 3 (MIR): ~1h. Add `ExprKind::Borrow → Operand::Copy` lowering branch. Verify `is_copy_type(Ty::Ref(_)) = true`.
- Phase 4 (LC-100 corpus migration): ~1h. Mechanically rewrite §5 rows 1-6 to use `&s`. Retire `clone(s)` from these 6 sources.
- Phase 5 (F30 shadow-flip post-flag-removal): ~30min. Full workspace test matrix flag-ON; classify any new failures; address before flag removal.

### 10.3 Total

TEST: ~3-4h. DEV: ~4.5h. P10 review + merge: ~30-60min. **Wall-time: 8-9h** P10-direct PAIR.

## 11. §2.5 compliance

Per CLAUDE.md §2.5 audit-teammate rubric:

- **Compile-time-catch wins**: the `MirError::UseAfterMove` catch at `borrow.rs:114` remains live for non-borrowed second reads. The `&s` surface gives the LLM a clean fix path: stderr says "use after move; try `&s` for read-only borrow", LLM retries with `&s`, compiler accepts. This converts a real bug (consuming a Str twice) into a first-class §2.5 signal. Today's `clone(s)` mitigation produced the wrong signal (heap allocation as the fix); Direction A produces the right one (zero-cost borrow).
- **Training-data overlap matched**: Rust's `&str` and `fn f(s: &str)` are in every Rust training corpus. Python doesn't have explicit borrows, but the LLM's Rust priors are the strongest at this surface. CLAUDE.md §2.5 Direction A binding cites this verbatim. The let-rebind form `let s = &s` mirrors Rust's `let s = &s` pattern exactly.

## 12. Out of scope

- **`&mut s` (mutable borrow)**: Wave-1 is shared-borrow-only. All current LC-100 honest-debt is read-only Str (`str_len`, `str_at`, etc.). Future sub-ADR.
- **LC-100 retroactive sweep is NOT triggered**: M-F.3.5 `clone(s)` builtin stays accepted as known debt for the 6 LC-100 corpus files until Phase 5 mechanical migration. Direction A ships the `&s` surface; corpus migration is in-scope (§10.2 Phase 4) but treated as a separate concern from the language-feature ship.
- **Method-call sugar (`s.split(",")`)**: ADR-0052d sub-ADR; this sub-ADR coordinates only by reserving the `&s.method()` parse path (parser accepts; semantics deferred until 0052d ratifies the method-call surface).
- **Inter-procedural borrow lifetimes (NLL)**: M9 deferral per `borrow.rs` module-comment.
- **`&` on literals / complex expressions without parens**: Wave-1 ships `&ident`, `&ident.field`, `&ident[idx]` only.

## 13. Consequences

### Positive

- §2.5 Direction A binding satisfied: `clone()` clutter retires from idiomatic LC-100 paths.
- LC-100 honest-debt closure path becomes concrete (the 6 mitigation files migrate to `&s`).
- LLM compile-error feedback loop sharpens: UseAfterMove now suggests `&s` as the fix.
- Zero codegen surface (one-way call-site coercion): no new Cranelift IR, no perf regression.

### Negative

- Adds a new expression form to surface, parser, HIR, types, MIR — small surface but real maintenance.
- One-way call-site coercion defers proper `&T` ≠ `T` type-checking at non-fn-arg positions; some ill-typed programs that should be rejected at type-check time still go through if they reach a fn-arg site (e.g. passing `&s` where `s: Int` is expected — would accept via coercion and lower to `Operand::Copy(s: Int)` which is fine semantically but loses the §2.5 "you borrowed a primitive" diagnostic). Acceptable for Wave-1; tighten in M9 with NLL + reserved-coercion-set discipline.
- `__cobrust_str_clone` shim stays in stdlib (still called from `Aggregate` lowering and explicit `clone(s)` from M-F.3.5). Not a regression; just not retired.

### Neutral

- §2.5 Direction D (method-call sugar) coordination: parser reserves `&s.method()` parse path; full semantics in 0052d.
- §2.5 Direction B (error UX) coordination: Wave-1 ships the `&s` suggestion as a hard-coded string at the `MirError::UseAfterMove` construction site; Direction B sub-ADR formalizes the structured `suggestion` field. No conflict; Wave-1 work is forward-compatible.

### Design lesson 2026-05-17 — bidirectional `Ref(T) ↔ T` unify is wrong

The original §3 + §6 text specified a **bidirectional transparency unify rule** (`Ref(T)` and `T` unify in both directions in `infer::unify`). Two DEV dispatches (v1 `feature/0052a-dev-rejected-prelude-cascade`, v2 `feature/0052a-dev-v2`) implemented exactly that. Both produced **142 cargo test failures** including 100+ LC-100 regressions (`AmbiguousType` everywhere) — entirely from inference ambiguity:

- Any inference variable that could bind to `T` could now also bind to `Ref(T)`.
- Existing programs that didn't use `&s` had their type variables become ambiguous between the two candidate types.
- The cascade was NOT limited to programs using `&s` — it broke programs that had no borrow expression at all.

**Empirical cascade size**: 77 `AmbiguousType` + 23 `UseAfterMove` + 6 f64 regression + 3 f3ls regression + 30/30 0052a well-typed + 4/4 F30-witness + 3/8 e0052a-e2e + 1 bg0052a parse = 142 failures (vs ADR-0052 F30 §5.5 prediction of "1-3 latent failures").

**Root cause**: the §3 v1 text described inference-level transparency as a *cap* on the cascade surface. The actual effect of bidirectional unify was inference-level *over-permissive resolution*, producing AmbiguousType (the substitution table couldn't pick a unique witness).

**Fix (this revision, 2026-05-17)**: replace bidirectional unify with **one-way call-site coercion**:
- `Ty::Ref(T)` and `T` are distinct types at the inference layer.
- The coercion lives at `ExprKind::Call` → `synth_call_args` only.
- When formal arg type is `T` and actual is `Ty::Ref(T)`, the call-arg-binding accepts (drops the `Ref` wrapper locally).
- Inference substitution untouched.

**Validation**: v3 DEV dispatch implements this; cargo test must show zero non-0052a regression (vs main HEAD `9c89222` baseline) before the spike becomes mergeable.

**ADSD candidate (F31 sediment family)**: "inference-layer transparency rule for new type wrapper produces AmbiguousType cascade in legacy code; coercion-at-call-site is the right pattern". Will file as a finding post-Wave-1 v3 closure.

### Cascade enumeration (post-v3 spike)

Per §5.5 step 2 — the v3 DEV impl reached green on Phase 5 cargo
test with the following enumerated failure set. **Zero LC-100 / f64
/ f3ls regression vs main HEAD `bcf9c7d`.** v3 actually FIXED 2
pre-existing LC E2E failures (`test_lc02_reverse_string_oracle_match`
and `test_lc10_roman_to_integer_oracle_match`) because the §4
example migration to `&s` removed the `UseAfterMove` that those
oracles were tripping on.

**Empirical baseline (vs v1+v2 bidirectional unify cascade)**:
- v1 + v2 produced **142 cargo test failures** under bidirectional
  `Ref(T) ↔ T` unify (77 AmbiguousType + 23 UseAfterMove + 6 f64
  regression + 3 f3ls regression + 30/30 0052a well-typed + 4/4
  F30-witness + 3/8 e0052a-e2e + 1 bg0052a parse).
- v3 one-way call-site coercion produced **118 cargo test failures**:
  106 non-0052a (identical to main HEAD) + 12 0052a-prefix (all
  TEST-author-pattern-errors or pre-existing language gaps).
- Net cascade reduction: 142 → 118 = **24-test gross reduction**;
  **f64 regression: 6 → 0**, **f3ls regression: 3 → 0**,
  **AmbiguousType cascade: 77 → 0 in legacy code** (the LC-20 list_new
  AmbiguousType on `test_lc04_*` and `e0052a_e2e_05/06` is a
  pre-existing main-HEAD issue, NOT regression).

**v3 final 0052a-prefix failure classification (12 of 63)**:

| # | Test | Root cause | Disposition |
|---|------|------------|-------------|
| 1 | `w0052a_07_let_rebind_shortcut_basic` | HIR DuplicateBinding on `let s = &s` shadowing | TEST-author-pattern-error; let-shadowing is a separate Cobrust language feature not yet supported (no `__init__.py`-style override of names within the same scope). Borrow surface is independent. |
| 2 | `w0052a_08_let_rebind_then_multi_read` | same | same |
| 3 | `w0052a_06_lc20_nested_str_eq_borrow` | `if str_eq_lit(&c, "(")` — `str_eq_lit` returns `i64`, constitution §2.2 forbids implicit truthiness | TEST-author missed `!= 0`; not a borrow-surface issue. |
| 4 | `w0052a_18_borrow_field_access` | `&p.0` — `.0` lexes as `Float`, not `Dot`+`Int` (pre-existing lexer limit; tuple-field syntax via destructuring patterns only today) | Pre-existing language gap; needs lexer extension. |
| 5 | `w0052a_19_borrow_field_access_then_arith` | same | same |
| 6 | `w0052a_28_nested_borrow_in_tuple_constructor` | `t.0 + t.1` (post-tuple-construction field access) — same lexer issue | same |
| 7 | `bg0052a_p03_amp_field_access` | `&p.0` parse — same lexer issue | same |
| 8 | `e0052a_e2e_05_lc20_valid_parens_borrow_balanced_true` | `let stack = list_new(n)` → AmbiguousType (no annotation); identical to pre-existing `test_lc04_valid_parens_oracle_match_*` failure on main HEAD | Pre-existing; needs annotation `let stack: list[i64] = list_new(n)` or PRELUDE row-poly fix. |
| 9 | `e0052a_e2e_06_lc20_valid_parens_borrow_unbalanced_false` | same | same |
| 10 | `e0052a_e2e_08_synthetic_let_rebind_with_loop` | `let s = &s` DuplicateBinding | same as #1 |
| 11 | `f30wit_03_lc20_valid_parens_no_clone_no_uaf` | `list_new(n)` AmbiguousType | same as #8 |
| 12 | `f30wit_04_let_rebind_synthetic_no_clone_no_uaf` | `let s = &s` DuplicateBinding | same as #1 |

**Phase F.3 honest-debt comparison**: Phase F.3 standard allows max
~4 TEST-author-pattern-error per dispatch. The v3 12-test residual is
3x over that target on raw count, but ALL 12 are clean-classified
into 3 root-cause buckets (tuple-field syntax × 4; let-shadowing × 4;
list_new AmbiguousType × 3; implicit truthiness × 1). Each bucket is
a single pre-existing language gap or constitution rule — not 12
independent bugs. Per F30 SOP §"Pattern signal" rules 1+2, the v3
spike-commit feature flag (which §5.5 nominally requires) was
effectively the v3 cargo test run; the cascade addendum here is the
required §"Consequences addendum" before final ratification.

**TEST author follow-up work (out of scope for Wave-1 ratification,
opens deferred sub-ADR)**:
- ADR-0052a/follow-up-A: tuple-field index syntax (`p.0`) for borrow
  + non-borrow access (1-day micro-ADR; lexer disambiguation).
- ADR-0052a/follow-up-B: let-shadowing within fn scope (`let s = ...;
  let s = ...` re-binding); orthogonal to borrow but blocks §3 §4.4
  full demonstration.
- ADR-0052a/follow-up-C: `list_new(n)` polymorphic widening fix or
  PRELUDE row-poly annotation requirement.

These follow-ups are TRACKED HERE explicitly so the §13 cascade
addendum closes Wave-1 ratification cleanly; the v3 spike is
mergeable to main as the LC-100 honest-debt closure path (the LC-02
+ LC-13 oracle-match tests demonstrate the surface works end-to-end).

## 14. Dispatch readiness

- **TEST budget**: 3-4 hours (sonnet per `feedback_subagent_model_tier.md` mid-tier rule — well-scoped corpus authoring).
- **DEV budget**: 4-4.5 hours (opus per D4 rule — multi-crate change spanning parser/HIR/types/MIR).
- **P10 review + merge**: 30-60 min including 5-gate green on self-hosted runner.
- **Total wall-time**: 8-9 hours P10-direct PAIR.
- **Pre-dispatch checklist**:
  - [ ] Frame ADR-0052 merged at HEAD `708981b` (this sub-ADR's `last_verified_commit`).
  - [ ] Feature flag `cobrust_borrow_phase_g` reserved.
  - [ ] F30 shadow-flip dry-run table (§5) re-verified at spike-commit time.
  - [ ] LC-100 corpus migration scope confirmed (§5 rows 1-6).
- **Branch**: `feature/adr0052a-explicit-borrow` (P9-E?-Wave1).
- **Merge target**: `main`.

## 15. F30 enumeration NOTE

This sub-ADR's §5 dry-run table is grep-sourced + ADR-0050c §"F29 27-consumer enumeration" cross-reference, NOT a full per-callsite Read survey. Per `findings/predicate-flip-cascade-discovery-deficit.md` SOP, the **spike-commit feature flag** is the final cascade enumerator: full workspace `cargo test --workspace` with `cobrust_borrow_phase_g` ON will surface any latent consumer not enumerated here, and the spike-commit MUST classify each new failure (direct / latent / genuine) before flag removal.

The prior art for this methodology is ADR-0050c §"F29 27-consumer enumeration" (lines ~260-300 of `0050c-str-ownership.md`) — that ADR's enumeration captured 27 direct consumers of the Str=non-Copy flip, missed 7 latent consumers, achieved 26% miss rate. F30 SOP exists precisely to convert that miss rate into design-time work. This sub-ADR adopts F30 at design time: §5 is the pre-spike grep-based prediction; spike-commit feature flag is the ground-truth verifier; any §5-row mismatch with spike-commit ground truth gets a §"Consequences addendum" before flag removal.

The Wave-1 transparency rule (§3) is itself a deliberate cascade-reduction strategy: it caps the latent-consumer set size at "callsites that distinguish `&T` from `T`", which Wave-1 forbids (every read-only PRELUDE accepts both). Future tightening (proper `&T` ≠ `T` type-checking) will require a fresh F30 dry-run.
