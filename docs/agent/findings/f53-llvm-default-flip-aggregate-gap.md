---
finding_id: f53
title: ADR-0070 §X.3 LLVM-default flip BLOCKED — `lower_aggregate(List | FormatString)` stubs surfaced 26+ silent regressions
status: RESOLVED 2026-05-26 (LLVM `lower_aggregate_list` + `lower_aggregate_format_string` implementations landed mirroring Cranelift references; 36/36 workspace integration test regressions resolved; F45a §amendment ratified)
date: 2026-05-26
last_verified_commit: 4aa38da (pre-fix; resolution commit references this finding by SHA in the F53 amendment section §7)
related_findings:
  - f45a (LLVM wave-3 scope — sibling; F45a CLOSED 2026-05-25 missed the
    `lower_aggregate` LLVM gap because sub-wave-5 only declared the
    runtime extern functions, not the codegen sites that call them;
    F45a §8 amendment lands in same commit as F53 resolution)
  - f44 (CI cache stale-green — F44 sibling; X.2 sweep methodology
    falsely declared LLVM stable because the 144-program corpus
    excluded the workspace `tests/` directory tests that drive
    `cobrust build`)
  - f37 (silent-rot on accepted-debt — F37 sibling; this finding catches
    a 30+ test surface that was passing under Cranelift via default
    backend selection, masking the LLVM gap until X.3's default flip
    attempt)
adr_refs:
  - 0070 §X.3 (UNBLOCKED 2026-05-26 for prerequisites 1 + 2 in §3 — list
    + FormatString aggregate codegen sites now implemented;
    prerequisites 3 + 4 + 5 in §3 remain follow-up scope per F53 §7)
  - 0058g sub-wave-5 (F45a closure — declared runtime externs but did
    not implement the codegen aggregate lowering callers; F45a §8
    amendment closure 2026-05-26)
---

# Finding F53 — ADR-0070 §X.3 LLVM-default flip BLOCKED: `lower_aggregate(List | FormatString)` stubs

## 1. Summary

The X.3 flip attempt on 2026-05-26 set `crates/cobrust-codegen/Cargo.toml`
to `default = ["llvm"]` and made `Backend::default_for_dev()` return
`Backend::Llvm` when the `llvm` feature is active. The local `cargo test
--workspace` sweep then revealed **30+ test failures** across at least
three test files driven by `cobrust build` (debug profile, no
`--release`):

- `crates/cobrust-cli/tests/f64_e2e.rs` — 10 failures, 6 panics in
  `lower_binop` / `lower_cast` (float-as-i64 stack slot path), 4
  silent-empty-stdout regressions in fstring `:.Nf` paths
- `crates/cobrust-cli/tests/cli_stdin_argv_e2e.rs` — 6 failures, all
  LLVM module-verify errors (`Call parameter type does not match
  function signature` on `__cobrust_list_len` / `__cobrust_list_get` /
  `__cobrust_str_clone` extern calls)
- `crates/cobrust-cli/tests/list_str_e2e.rs` — 20 of 33 failures
  (60% pass-rate inversion); all hit the `lower_aggregate(List, _)`
  null-return stub
- `crates/cobrust-cli/tests/fstring_user_fn_str_corpus.rs` — 6 of 6
  failures (100%) on `lower_aggregate(FormatString, _)` null-return
  stub

X.3 was rolled back same-session per F35-sibling discipline (no flip
without GREEN evidence) and re-opened as BLOCKED pending the
prerequisites in §3.

## 2. Root cause (taxonomy)

Three independent root causes were intertwined; the first two were
fixed in the X.3 attempt and retained as LLVM-correctness wins. The
third is the blocker.

### 2.1 LLVM extern-call IntValue → PointerType coercion (FIXED in attempt)

`crates/cobrust-codegen/src/llvm_backend.rs` extern-call lowering
(~line 3075) only coerces narrow-int operands via `build_int_z_extend`.
When the callee's signature param is `PointerType` and the lowered
operand is `IntValue` (MIR encodes list / heap-string values as i64
stack-slot encodings of host pointers), LLVM's strict module verifier
rejects the call. Mirror Cranelift's defensive int→ptr coercion: emit
`build_int_to_ptr` when the callee param type is PointerType.

### 2.2 LLVM extern-call IntValue → FloatType coercion (FIXED in attempt)

Same call site, FloatType branch. MIR's `Rvalue::BinaryOp` allocates
its result as `Ty::None` → i64 (`cobrust-mir/src/lower.rs:1945`), so a
float arithmetic chain produces an i64-typed `_bin` slot holding the
f64 bit-pattern. When this flows into a runtime helper with `f64`
signature, the LLVM verifier rejects. Bitcast i64→f64 at the call site.

Companion fix in `lower_binop`: when `is_float` differs between the two
operands (one IntValue from a binop chain, one FloatValue from a
constant rhs), bitcast both to f64. And companion fix in `lower_cast`:
fall through defensively when the operand's LLVM type disagrees with
the cast direction (mirrors Cranelift `lower_cast` 2023-2055).

Companion fix in `lower_binop`: float NotEq used `FloatPredicate::ONE`
(ordered not-equal — returns false on NaN operands). Per IEEE 754
parity + Cranelift `FloatCC::NotEqual` semantics, use `UNE` (unordered
not-equal). This is a long-standing LLVM bug uncovered by
`f64e16_nan_not_equal_to_itself`.

### 2.3 LLVM `lower_aggregate(_, _)` is a stub returning null (BLOCKER, UNFIXED)

`crates/cobrust-codegen/src/llvm_backend.rs:3898-3908`:

```rust
fn lower_aggregate(
    &mut self,
    _kind: &AggregateKind,
    _operands: &[Operand],
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    // Wave-1 stub — Aggregate lowering for List/Dict/Set/Tuple/Record
    // requires the stdlib runtime helpers (`__cobrust_list_new`,
    // `__cobrust_dict_new`, etc.) which land in M11 + sub-ADR 0058b.
    // Matches the Cranelift backend's mid-M9 stub posture at the
    // wave-1 ratification SHA.
    Ok(self.emitter.opaque_ptr_ty.const_null().into())
}
```

The wave-1 stub was carried forward through F45a sub-wave-5's "fmt
helpers RESOLVED" closure. F45a sub-wave-5 declared the runtime
externs (`__cobrust_fmt_float_prec`, etc.) but never implemented the
**callers** of those externs in `lower_aggregate(FormatString, _)`.

The Cranelift backend implements `lower_aggregate_format_string`
(`cranelift_backend.rs:1882-2050`, ~170 lines) + `lower_aggregate_list`
(`cranelift_backend.rs:1653+`) + `lower_aggregate_dict` /
`lower_aggregate_tuple` / `lower_aggregate_set`. None of these paths
were ported to LLVM IR emission.

## 3. Prerequisites for re-flipping (X.3a sprint scope)

1. **Implement LLVM `lower_aggregate(List, _)`**. Mirror Cranelift's
   `lower_aggregate_list`: `__cobrust_list_new(elem_size, len)` → for
   each operand, `__cobrust_list_append(buf, materialise(v))`. ~80
   lines of inkwell IR emission.

2. **Implement LLVM `lower_aggregate(FormatString, _)`**. Mirror
   Cranelift's `lower_aggregate_format_string` (1882-2050). The
   FMTSPEC sentinel detection + per-type dispatch table (str / int /
   float / float-with-prec / bool / repr) is the bulk of the work.
   ~170 lines.

3. **Implement LLVM `lower_aggregate(Dict | Set | Tuple, _)`**.
   Lower priority than 1+2 (few existing tests exercise these via
   `cobrust build` driver) but required for full Cranelift parity.

4. **Re-baseline X.2 sweep methodology**. The 144-program corpus
   needs to be extended (or paired) with the workspace `tests/`
   `.rs`-integration test files that drive `cobrust build`. Today's
   X.2 sweep is "language-level" only; what's required for X.3 is
   "compiler-level + integration-test-level" GREEN.

5. **Cite this finding in the re-flip ADR commit**.

## 4. Honest-debt inventory

| Surface | Test count | Failure mode | Root cause |
|---|---|---|---|
| `cli_stdin_argv_e2e.rs` | 6/15 | LLVM verifier: ptr param mismatch | 2.1 (FIXED) |
| `f64_e2e.rs` (panics) | 6/33 | inkwell panic at into_float / into_int | 2.2 (FIXED) |
| `f64_e2e.rs` (fstring) | 4/33 | empty stdout — null `_fstr` slot | 2.3 (BLOCKED) |
| `list_str_e2e.rs` | 20/33 | null list pointer at runtime | 2.3 (BLOCKED) |
| `fstring_user_fn_str_corpus.rs` | 6/6 | empty stdout — null `_fstr` slot | 2.3 (BLOCKED) |
| **TOTAL gap** | **42** | — | 6/42 fixable inline; 36/42 require port work |

Of the 6/42 inline-fixable surfaces, all are retained as commits
(LLVM-correctness wins even without the X.3 flip). The 36/42
aggregate-stub surface defines the X.3a sprint.

## 5. Methodology lesson (F37-sibling)

F37 was "silent rot on accepted-debt" — `accepted_as_honest_debt` MUST
cite a `#[ignore = "..."]` URN. F53 extends F37 to **stability sweeps**:
a sweep-corpus that omits substantial real-world test paths can
falsely declare a backend "production-ready". The X.2 sweep on 144
.cb programs returned GREEN, but the workspace's 30+ integration tests
that drive `cobrust build` directly were never part of the sweep.

Methodology lesson: **stability sweeps must enumerate ALL paths that
the would-be default backend is reachable from**, not a curated
corpus that may dodge weak surfaces.

## 6. Cross-refs

- ADR-0070 §X.3 (status flipped `proposed` → `blocked` until F53's
  prerequisites in §3 land)
- F45a sub-wave-5 (sibling — declared runtime externs but did not
  implement the codegen aggregate-lowering callers; F45a closure
  prematurely RESOLVED)
- F44 (CI cache stale-green — the X.2 sweep's "GREEN" status had the
  same flavour of partial coverage)
- F37 (silent rot on accepted-debt — F53 generalises the rule to sweep
  methodology)
- F35-sibling (commit-message-vs-diff-drift discipline — X.3 rollback
  honored this; no false "flip landed" claim)

## 7. Resolution (2026-05-26)

### 7.1 What landed

`crates/cobrust-codegen/src/llvm_backend.rs::lower_aggregate` no longer
returns a blanket `opaque_ptr_ty.const_null()` stub for every aggregate
kind. Two callers implemented (mirror of the Cranelift references cited
in §2.3):

- **`lower_aggregate_list`** mirrors `cranelift_backend.rs:1674-1739`.
  Allocates via `__cobrust_list_new(elem_size=8, len)`, then for each
  operand decides between three materialisation cases (str literal →
  `materialize_str_buffer`; Str-typed local → `__cobrust_str_clone`;
  else → `lower_operand` direct → coerce to i64), then populates the
  slot via `__cobrust_list_set(buf, i, val_i64)`. ~120 lines.
- **`lower_aggregate_format_string`** mirrors
  `cranelift_backend.rs:1882-2020`. Allocates via `__cobrust_str_new()`,
  walks operands with an FMTSPEC: sentinel-aware loop. Static `Str`
  segments push via `__cobrust_str_push_static`. Holes dispatch per
  MIR-declared `Ty` (Str → fmt_str via str_ptr + str_len; Float →
  fmt_float / fmt_float_prec when followed by FMTSPEC:; Bool → fmt_bool
  with i1→i64 z_extend; Int → fmt_int with width-aware s_extend;
  PointerValue → fmt_repr). ~170 lines.

Two helper methods added:
- `coerce_value_to_i64` — mirrors Cranelift `coerce_to_i64`
  (cranelift_backend.rs:3031-3050) for the C-ABI i64 argument shape.
- `coerce_value_to_ptr` — defensive int→ptr conversion mirroring the
  existing `lower_call` int→ptr coercion at `llvm_backend.rs:3088-3108`.

Aggregate kinds NOT in this sprint (`Dict` / `Set` / `Tuple` / `Record` /
`Adt`) retain the wave-1 stub return path. The §3 prerequisite #3 lands
in a follow-up sprint.

### 7.2 Empirical verification (mac arm64, `--release --features cobrust-codegen/llvm`)

| Surface | Pre-fix (per §4) | Post-fix | Notes |
|---|---|---|---|
| `cli_stdin_argv_e2e.rs` | 6/15 fail | **15/15 PASS** | F53 §2.1 + §2.3 closure |
| `f64_e2e.rs` | 10/33 fail | **33/33 PASS, 2 ignored** | 2 ignored are pre-existing M-F.3.3 stretch (`f64e30` log NaN, `f64e31` pow NaN); unrelated to F53 |
| `list_str_e2e.rs` | 20/33 fail | **31/33 PASS, 2 ignored** | 2 ignored are pre-existing LC-100 (`f3ls22` use-after-move, `f3ls23` partial-iter drop); unrelated to F53 |
| `fstring_user_fn_str_corpus.rs` | 6/6 fail | **6/6 PASS** | F53 §2.3 closure |
| **Total F53 regressions** | **42 of 42 fail** | **36 of 36 RESOLVED** | The 6 F53 §2.1 + §2.2 surfaces stayed-fixed pre-sprint; this sprint resolves the 36 §2.3 surfaces |

Regression check on existing wave-3 corpora:
- `llvm_wave3_list_runtime` (--features llvm): 5/5 PASS
- `llvm_wave3_fmt_iter_math_str` (--features llvm): 14/14 PASS
- `llvm_wave3_llm_router` (--features llvm): 6/6 PASS

Lint check:
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- `cargo clippy --workspace --all-targets --features cobrust-codegen/llvm -- -D warnings` clean (F51 sibling: `manual_let_else` lints surfaced + fixed pre-commit)
- `cargo fmt --all -- --check` clean

### 7.3 Out of scope (deferred per F53 §3)

The remaining F53 §3 prerequisites NOT addressed by this sprint:
- §3 #3: `lower_aggregate(Dict | Set | Tuple, _)` LLVM implementation
  (Cranelift parity reference: `cranelift_backend.rs:1741-1881`)
- §3 #4: re-baseline the X.2 sweep methodology to enumerate workspace
  `tests/`-level integration paths
- §3 #5: re-flip ADR commit citing this finding (X.3 sprint scope, lands
  separately once §3 #3 + #4 also land)

X.3 flip itself remains BLOCKED on the remaining prerequisites. F53 §7
documents the 2-of-5 progression.

### 7.4 F35-sibling claim discipline

This sprint's commit message + ADR amendment + F53 §7 + F45a §8 are
co-scoped. Claim shape:

- **Claimed**: `lower_aggregate(List)` + `lower_aggregate(FormatString)`
  LLVM implementations + 36 regression resolutions + F45a §amendment
  cross-ref.
- **NOT claimed**: full F53 prerequisite closure (§3 #3 + #4 + #5
  remain), X.3 default flip, Dict/Set/Tuple aggregate parity.

Source-of-truth for the closure: the four corpora results in §7.2 +
the three Wave-3 corpus regression checks.
