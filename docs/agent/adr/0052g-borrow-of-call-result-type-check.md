---
doc_kind: adr
adr_id: 0052g
parent_adr: 0052
title: "Type-check support for `&CallResult` (method-call return-value borrow)"
status: proposed
date: 2026-05-18
last_verified_commit: 25ee43f
supersedes: []
superseded_by: []
relates_to: [adr:0052a, adr:0052d-prereq, adr:0052f, adr:0052]
discovered_by: ADR-0052f §11 "Cascade enumeration (post-spike)" L208-216 + re-ignored f30wit_method_03
ratification_path: P9 Wave-2 round-2 sub-ADR review; ratified on impl merge
---

# ADR-0052g — Type-check support for `&CallResult` (method-call return-value borrow)

## 1. Context

ADR-0052f relaxed the parser §8 cap at `crates/cobrust-frontend/src/parser.rs:1134-1139`, admitting `&Call(Attr(...))` as a valid borrow operand (8/8 `bg0052f_*` parse tests green per ADR-0052f §11 attestation at SHA `94e5544`).

**ADR-0052f §11 documented the empirical follow-on miss**: integration test `f30wit_method_03_borrow_precedence_binds_tighter_than_method_call` (`crates/cobrust-mir/tests/method_dispatch_f30_witness.rs:241-267`) still fails at the type-check + MIR-lowering combined stage on the source `let r: i64 = read_i64(&s.len())`. ADR-0052f §11 disposition: the test was **re-ignored** with the gap message; the type-check piece is **deliberately out of 0052f scope** and lives in this ADR.

The current `synth_expr` `ExprKind::Borrow` arm at `crates/cobrust-types/src/check.rs:888-891` reads:

```rust
ExprKind::Borrow(inner) => {
    let inner_ty = self.synth_expr(inner)?;
    Ok(Ty::Ref(Box::new(inner_ty)))
}
```

Two issues post-0052f admission:

1. **No place-check enforcement** that ADR-0052a §6 introduced (`BorrowOfNonPlace` exists at `error.rs:219` but is never emitted from `synth_expr` — parser §8 cap was the pre-0052f de-facto enforcement; post-0052f the parser admits `&Call(Attr(...))` and the type-check arm has no §6 logic).
2. **The downstream MIR `lower_borrow_inner` arm** at `mir/src/lower.rs:1941-1956` falls through `_ => self.lower_expr(inner)`; `Call(Attr(s, "len"))` re-enters `lower_call` (which handles 0052d-prereq method-form rewrite), but empirically produces `got callees=[]` — the inner call is silently dropped along the borrow path.

This ADR ratifies the **type-check arm narrowing** that makes `f30wit_method_03` mechanically reachable. ADR-0052a is **accepted at SHA `8f29189`**; ADR-0052f at `94e5544` (F27 immutability); the new sub-ADR carries the cap-narrowing decision.

## 2. Decision

**Narrow the `synth_expr` `ExprKind::Borrow(inner)` arm to admit only**: (a) genuine places — `Name` / `Attr` / `Index`, OR (b) method-form `Call(Attr(base, m))` where `base` is borrowable AND the method's PRELUDE-fn return type is a Copy primitive (`Int` / `Float` / `Bool`). All other shapes — `&"hello"`, `&(a + b)`, `&foo(x)`, `&s.trim()` (non-Copy return) — emit `TypeError::BorrowOfNonPlace` with §2.5 FIX-text.

The borrow targets the method's return value materialized as a Copy operand at MIR. The rewritten PRELUDE-fn call produces a temporary whose type is Copy primitive, so the borrow is structurally equivalent to `&primitive_temp` — a cosmetic wrapper that needs no backing place. Result type is `Ty::Ref(return_ty)`; the existing one-way call-site coercion (`unify_call_arg` at `check.rs:1649-1661`) drops the `Ref` wrapper when the borrow flows into a fn-arg-binding position.

## 3. Why standalone ADR (not amend 0052a §6)

- **F27 verified-at-HEAD immutability**: 0052a accepted at `8f29189`; 0052f at `94e5544`. Narrowing requires a discrete decision artifact.
- **Scope discipline**: 0052f explicitly deferred this to a follow-on sub-ADR; folding into 0052d-final would couple type-check semantics with consumer migration.
- **Coercion model unchanged**: This is **not** a relaxation of ADR-0052a §3 one-way coercion. Only the place-check at the §6 emission point narrows.

## 4. Semantics

### 4.1 Admitted: `&recv.method()` where method returns Copy primitive

`&s.len()` where `s: Str` and `str_len` returns `Int`:

- Type checker: enters `Borrow` arm; sees inner is `Call(Attr(s, "len"))`; resolves `s.len()` via the existing method-table chain (`try_synth_str_method`) → return type `Int`; `Int` is Copy primitive → admit borrow; result type `Ty::Ref(Int)`.
- Call site `read_i64(&s.len())`: `unify_call_arg(Int, Ref(Int), span)` drops the `Ref` wrapper (existing one-way coercion at `check.rs:1649-1661`); the inner method-form call lowers normally; the outer borrow is a no-op at MIR (Copy primitive is already-Copy).

Same rule for `&xs.is_empty()` (List → Bool), `&f.floor()` (Float → Float — Float IS a Copy primitive per `is_copy_type` at `mir/src/lower.rs:2147`), `&n.abs()` (Int → Int), `&d.get(k)` (Dict → polymorphic; admit when concrete return resolves Copy).

### 4.2 Rejected: `&recv.method()` where method returns non-Copy

`&s.trim()` where `trim` returns `Str`:

- Type checker: enters `Borrow` arm; resolves method → return type `Str`; `Str` is **not** Copy per ADR-0050c (non-Copy uniformly) → emit `TypeError::BorrowOfNonPlace { span, suggestion: Some("borrow of a method returning non-Copy type; call the method first and bind to a local, then borrow: `let t = s.trim(); &t`") }`.

Preserves the §2.5 compile-time-catch: returning a non-Copy temporary from a method does NOT have a stable backing place; borrowing it is the kind of bug the LLM should learn to fix. The FIX-text tells the LLM the exact rewrite pattern.

Same rule for `&s.replace(a, b)` (returns Str), `&xs.get(i)` when `i64` would resolve `i64` (admit per 4.1), `&s.split(sep)` (returns `List[Str]`, non-Copy).

### 4.3 Rejected: `&Call(Name(...), ...)` (free-fn call)

`&free_fn(x)`: parser §8 cap relaxation at ADR-0052f §2 explicitly preserved this rejection ("free-fn return is a temporary with no place to anchor"). This ADR maintains the same diagnostic at type-check time as a defense-in-depth: even if a future parser change admits more `&Call(Name, ...)` shapes, the type-check arm rejects (with `BorrowOfNonPlace` + FIX-text pointing at let-bind-then-borrow rewrite).

### 4.4 Rejected: literal / arithmetic / complex inner

`&"hello"`, `&(a + b)`, `&{x: 1}`: emit `BorrowOfNonPlace` with FIX-text per inner kind. These shapes are out-of-scope per ADR-0052a §12 and remain rejected; the §2.5 compile-time-catch is honored.

### 4.5 Method-table integration constraint

Per ADR-0052d-prereq method tables — admitted method-table entries return Copy primitive:

- **Str**: `len → Int`, `find → Int`, `contains → Bool`, `starts_with → Bool`, `ends_with → Bool`.
- **List**: `len → Int`, `is_empty → Bool`, `get → T` (if `T` Copy).
- **Float**: `floor → Float`, `ceil → Float`, `is_nan → Bool`, `is_finite → Bool`, `abs → Float`.
- **Int**: `abs → Int`, `pow → Int`, `min → Int`, `max → Int`, `bit_count → Int`.
- **Dict**: `get → V` (if `V` Copy).

Non-Copy-returning methods (`s.trim/split/replace/lower/upper`, `d.keys/values/items/copy`, `xs.push` (unit)) emit `BorrowOfNonPlace`.

## 5. Type-check changes

**Single file**: `crates/cobrust-types/src/check.rs` at L888-891 (the `ExprKind::Borrow(inner)` synth arm).

Replace:

```rust
ExprKind::Borrow(inner) => {
    let inner_ty = self.synth_expr(inner)?;
    Ok(Ty::Ref(Box::new(inner_ty)))
}
```

With:

```rust
ExprKind::Borrow(inner) => {
    // ADR-0052g §5 — narrow the Wave-1 §6 rule so genuine non-places
    // (literals, arithmetic, free-fn calls) emit `BorrowOfNonPlace`
    // while method-form `&recv.method()` with Copy-primitive return
    // type is admitted. The borrow targets the rewritten PRELUDE-fn
    // call's return value materialized as a Copy operand at MIR.
    match &inner.kind {
        // Place expressions — admit unconditionally (Wave-1 §8 cap).
        ExprKind::Name(_) | ExprKind::Attr { .. } | ExprKind::Index { .. } => {
            let inner_ty = self.synth_expr(inner)?;
            Ok(Ty::Ref(Box::new(inner_ty)))
        }
        // Method-form call — admit iff method's return type is Copy.
        ExprKind::Call { callee, .. }
            if matches!(callee.kind, ExprKind::Attr { .. }) =>
        {
            let inner_ty = self.synth_expr(inner)?;
            let resolved = self.subst.apply(&inner_ty);
            if is_copy_primitive(&resolved) {
                Ok(Ty::Ref(Box::new(inner_ty)))
            } else {
                Err(TypeError::BorrowOfNonPlace {
                    span,
                    suggestion: Some(
                        "borrow of a method returning non-Copy type; \
                         bind the return value to a local first: \
                         `let t = recv.method(); &t`",
                    ),
                })
            }
        }
        // Free-fn call, literal, arithmetic, complex expression —
        // reject per ADR-0052a §6.
        _ => Err(TypeError::BorrowOfNonPlace {
            span,
            suggestion: Some(
                "borrow operand must be a place (`Name`, `Name.field`, \
                 `Name[idx]`, or `Name.method()` returning a primitive)",
            ),
        }),
    }
}
```

Where `is_copy_primitive(ty: &Ty) -> bool` matches `Ty::Int | Ty::Float | Ty::Bool` (mirrors `is_copy_type` at `crates/cobrust-mir/src/lower.rs:2147` for the primitive subset; deliberately narrower — Wave-2 admits primitives only; `Ty::Ref(_)` exclusion enforces the existing nested-borrow ban from ADR-0052a §8).

No other file changes. The MIR side already handles the borrow-of-method-form lowering via `lower_borrow_inner` → `lower_expr` → `lower_call` → `method_form_rewrite_name` chain (the rewrite map at `mir/src/lower.rs:2526-2533` is unchanged). The borrow operand at MIR resolves to `Operand::Copy` of the temp the rewritten PRELUDE-fn assigns to.

## 6. F30 shadow-flip dry-run

Surface is small. Grep at HEAD `25ee43f`:

```bash
$ grep -rn "&[a-z_][a-z_]*\.[a-z_]*(" examples/ crates/cobrust-frontend/tests/ \
    crates/cobrust-types/tests/ crates/cobrust-mir/tests/
crates/cobrust-mir/tests/method_dispatch_f30_witness.rs:252 (deferred f30wit_method_03 src)
```

**One predicted active consumer**: `f30wit_method_03` un-ignore. Single impl validation.

**Latent-consumer enumeration** (8 expected post-merge):

| # | Pattern (return type) | Admitted? |
|---|---|---|
| 1 | `&s.len()` (Int) — canonical witness | YES |
| 2 | `&xs.len()` (Int) | YES |
| 3 | `&xs.is_empty()` (Bool) | YES |
| 4 | `&f.floor()` (Float — Copy) | YES |
| 5 | `&n.abs()` (Int) | YES |
| 6 | `&s.contains(sub)` (Bool) | YES |
| 7 | `&s.trim()` (Str — non-Copy) | NO — BorrowOfNonPlace + FIX |
| 8 | `&s.split(sep)` (List[Str] — non-Copy) | NO — BorrowOfNonPlace + FIX |

Rows 1-6 admissions; rows 7-8 confirm §2.5 catch preserved. Cascade risk: **minimal** — widens place-acceptance only; no existing rejection becomes acceptance. Existing `BorrowOfNonPlace` test sites at `crates/cobrust-types/tests/error_suggestion_corpus.rs:818` use synthetic-error construction and remain unchanged.

**Zero existing program changes meaning.** No current `.cb` source in the workspace uses `&recv.method()` form. The relaxation is forward-looking surface.

## 7. TEST + DEV PAIR plan

Per F28 strict-separation. Single-file changes; minimal ceremony.

### 7.1 TEST sprint (~20 min, sonnet)

Extend `crates/cobrust-types/tests/well_typed.rs` + `ill_typed.rs`:

- **Well-typed (5 cases)**: `w0052g_01_borrow_str_len` (`&s.len()` accepted as `Ref(Int)`), `w0052g_02_borrow_list_is_empty`, `w0052g_03_borrow_float_floor`, `w0052g_04_borrow_int_abs`, `w0052g_05_borrow_str_contains`.
- **Ill-typed (3 cases)**: `i0052g_01_borrow_str_trim_non_copy_rejected` (`&s.trim()` → `BorrowOfNonPlace` w/ FIX containing "bind ... to a local first"), `i0052g_02_borrow_str_split_non_copy_rejected`, `i0052g_03_borrow_free_fn_call_rejected` (defense-in-depth — parser §8 still rejects).

### 7.2 DEV sprint (~30 min, sonnet)

- Edit `synth_expr` `ExprKind::Borrow` arm at `check.rs:888-891` per §5 diff.
- Add `is_copy_primitive` helper (or inline `matches!`).
- Un-ignore `f30wit_method_03` at `method_dispatch_f30_witness.rs:241`.
- Run `cargo test --package cobrust-types -- w0052g i0052g`, `cargo test --package cobrust-mir f30wit_method_03`, and workspace-wide cascade enumeration.

### 7.3 Total

~20 min TEST + ~30 min DEV + ~30 min P10 5-gate review = **~1 day P10-direct PAIR**.

## 8. §2.5 compliance

Per CLAUDE.md §2.5 audit-teammate rubric:

- **Compile-time-catch wins**: `&s.trim()` (non-Copy return) STILL rejected with a structured `BorrowOfNonPlace::suggestion` carrying the exact let-bind-then-borrow rewrite pattern. The diagnostic tells the LLM the FIX (§2.5 Direction B "print the FIX, not just the diagnosis"). Free-fn-call borrow remains a hard reject at both parser and type-check layers.
- **Training-data overlap matched**: Rust `&v.len()`, `&n.abs()`, `&v.is_empty()` are canonical training surface across the Rust corpus. The relaxation admits exactly the shapes the LLM expects to write; admission matches the LLM prior.

## 9. Out of scope

- **`&recv.method_returning_non_copy()`**: deferred. `&s.trim()` rejects with FIX-text; Phase H+ may relax via NLL + scoped-temp.
- **`let r = &recv.method()` (let-binding to borrow of temp)**: deferred to M9 NLL. Current admission consumed at fn-arg-binding only.
- **Chained `&a.b.c.method()`**: field-chain still capped per ADR-0052a §8 single-field-depth rule.
- **`&mut recv.method()`**: Wave-2 ships shared-borrow-only per ADR-0052a §12.
- **Borrow of generic intrinsic**: `&list_new()` parses as free-fn-call, rejects per §4.3.

## 10. Consequences

### Positive

- Closes deferred `f30wit_method_03` test from ADR-0052f §11; un-ignoring is the single impl validation.
- Restores end-to-end empirical truth to ADR-0052f §3 prose at the type-check + MIR-lowering combined stage.
- Unblocks ADR-0052d round-2 example migration: callsites can write `&s.len()` for explicit-borrow §2.5 signaling.
- Single-file type-check change; minimal blast radius; reversible.
- Strengthens `BorrowOfNonPlace` diagnostic with structured FIX-text for the new non-Copy-method-return bucket.

### Negative

- Borrow arm grows from 4 lines to ~30 lines with explicit place-vs-method-call branching. `is_copy_primitive` helper is a Wave-2 narrower variant of MIR's `is_copy_type` (MIR includes `Ty::Ref(_)`; type-check excludes it to prevent `&&x` nested-borrow regression).
- Asymmetry: `&s.method()` admitted, `&foo(x)` rejected. Principled but LLM may need one round of compile-error feedback; §2.5 FIX-text mitigates.

### Neutral

- No HIR / parser / MIR / codegen change. The MIR rewrite map at `mir/src/lower.rs:2526` already resolves the method-form receiver under the borrow wrapper.
- §11-style "Cascade enumeration (post-spike)" addendum methodology applies — append subsection to §10 after impl merge.

## 11. Dispatch readiness

- **TEST**: ~20 min (sonnet). 5 `w0052g_*` + 3 `i0052g_*` extensions.
- **DEV**: ~30 min (sonnet). Single-arm edit at `check.rs:888-891` + un-ignore f30wit_method_03.
- **P10 review + merge**: ~30 min (5-gate green at DG per heavy-build offload).
- **Total**: ~1 day P10-direct PAIR.
- **Pre-dispatch checklist**: status `proposed`; HEAD anchor `25ee43f`; cross-ref ADR-0052f §11; `is_copy_primitive` scope = `Int | Float | Bool` (excludes `Ref(_)`).
- **Branch**: `feature/adr0052g-borrow-of-call-result-type-check`.
- **Merge target**: `main`. **Ratification**: on impl merge.
