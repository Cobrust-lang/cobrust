---
doc_kind: finding
finding_id: two-bugs-one-fix-option-c-pattern
last_verified_commit: b4808e0
discovered_during: ADR-0033 codegen Ty::None inference root-cause investigation
related: codegen-i8-i64-mismatch-at-4-blocks, m9-cross-arch-linux-x86_64-validation
dependencies: [adr:0033, adr:0023]
---

# Finding: Two surface bugs, one root-primitive fix — the Option C pattern

## 1. Hypothesis

Two seemingly independent codegen bugs (Bug A surfaced cross-arch on
float-via-temp arithmetic; Bug B surfaced via Conway-toy stress test on 4+
similar inline compute blocks with `% 2`) are actually two faces of the same
`Ty::None` operand_ty fallback default.

Corollary: a single fix at the root primitive — threading an
`inferred_locals: &HashMap<LocalId, ir::Type>` through `operand_ty` /
`rvalue_ty` and running `infer_local_types` to fixed-point — closes both
bugs with one ~30-50 LOC change and no MIR-layer rewiring.

## 2. Method

### Bug A diagnostic chain

1. **Cross-arch validation gate** (`docs/agent/findings/m9-cross-arch-linux-x86_64-validation.md`):
   - macOS arm64: 4 float tests (`p008`, `p017`, `p018`, `p019`) compile and
     produce wrong values silently (AAPCS64 truncation path tolerates the bogus
     type pair).
   - Linux x86_64 (<internal Linux x86_64 validator host>): same 4 tests panic inside Cranelift
     `CvtFloatToSintSeq::cvtt_op` — `(Size64, Size8)` not handled.
2. **Root-cause trace**:
   - MIR lowering assigns `Ty::None` to synthetic temps `_un` / `_bin` /
     `_callret` (`cobrust-mir/src/lower.rs` lines ~1190, 1239, 1251).
   - `infer_local_types` correctly resolves `_un: Ty::None` → `F64` (the rvalue
     type of a negated float constant).
   - `infer_return_type` calls `rvalue_ty(Use(Copy(_un)))` → `operand_ty(Copy(_un))`.
   - `operand_ty` for `Copy` looked up `body.locals[p.local].ty` (the *declared*
     type `Ty::None`), not the inferred map. `cranelift_scalar_ty(Ty::None)` =
     `Some(I8)` — **wrong for a float result**.
   - Cranelift emits `fcvt_to_sint_sat(I8, F64)`; x86_64 panics, arm64 silently
     miscompiles.
3. **Pre-fix Option A sketch** (from the finding): "patch `infer_return_type`
   to consult `inferred_locals` for `Copy/Move` of a `Ty::None` local." But
   ADR-0033 §"Options considered" notes this leaves depth-2 chains (`fr14` /
   `fr15` / `fr16` corpus cases) still miscompiling.

### Bug B diagnostic chain

1. **Conway-toy stress test** (review-claude audit window, 2026-05-09):
   - Out-of-workspace `Conway-cobrust-toy/src/main.cb`, 4-cell version (straight-line code, no `while`):
     - 20 mutable `i64` locals, arithmetic-only, repeated `% 2` patterns.
     - Cranelift verifier: `inst441 (v520 = iadd.i8 v515, v518): arg 1 has type i64, expected i8`.
   - Binary search over cell count: 1 / 2 / 3 cells pass; 4+ cells fail.
   - Smoke run on 4-cell repro with `s = 30` → stdout `5` (expected `3`).
2. **Independence of `while` loop confirmed**: bug fires on straight-line code
   — loop-phi SSA narrowing hypothesis ruled out. The `% 2` pattern's values
   bound to `{0, 1}` but declared `: i64` hinted at over-eager narrowing
   somewhere in the chain.
3. **Shared symptom observed**: error type `I8`, declared type `i64`; same
   `operand_ty` fallback mismatch shape as Bug A at greater chain depth.

### Three-option analysis from ADR-0033

| Option | Description | Gap left open |
|---|---|---|
| **A — surgical** | Patch `infer_return_type` to check `inferred_locals` for the return-chain `Copy/Move` hop | Depth-2 chains (`-(-x)`, `(a+b)*c`) still miscompile; corpus `fr14-fr16` still fail; Bug B untouched |
| **B — MIR refactor** | Rewire `lower.rs` so `_bin` / `_un` / `_callret` carry resolved `Ty` at MIR time (ADR-0020 invariant) | 5+ lowering sites touched; cascading risk across 2,430+ test baseline; largest blast radius |
| **C — root primitive (chosen)** | Thread `inferred_locals: &HashMap<LocalId, ir::Type>` through `operand_ty` / `rvalue_ty`; run `infer_local_types` to fixed-point so all chain depths resolve | O(n_locals × n_iters) inference cost — bounded at ~2-3 iters; negligible in practice |

### Empirical post-merge verification at HEAD `540ed65`

- `cobrust build /tmp/conway_4cell_repro.cb` → `BUILD_EXIT=0` (was Cranelift
  verifier reject before).
- Run output: `3` (was `5`).
- Conway-toy 5-cell version: also `BUILD_EXIT=0`, output correct.
- macOS arm64: `cargo test --workspace --locked` → 1,783 pass / 0 fail;
  float corpus 16/16; named tests `p008/p017/p018/p019` all PASS.
- Linux x86_64 (workstation): `codegen_well_formed` 60/60; `float_return_corpus`
  16/16; 4 named cross-arch tests all PASS.

## 3. Result

**Option C closed Bug A and Bug B with one fix.**

| Metric | Before | After |
|---|---|---|
| `p008_const_float_neg` Linux x86_64 | PANIC `unreachable!()` | PASS |
| `p017_fadd` Linux x86_64 | PANIC | PASS |
| `p018_fsub` Linux x86_64 | PANIC | PASS |
| `p019_fmul` Linux x86_64 | PANIC | PASS |
| Float corpus macOS arm64 (16 cases) | 13/16 (depth-2 chains wrong) | 16/16 |
| Conway-toy 4-cell `BUILD_EXIT` | 1 (verifier reject) | 0 |
| Conway-toy 4-cell stdout | `5` | `3` |
| Conway-toy 5-cell | same fail | PASS |
| Workspace total (macOS arm64) | 1,767 pass | 1,783 pass (+16) |
| Linux x86_64 `codegen_well_formed` | 56/60 | 60/60 |

## 4. Reusable lesson — decision criteria for surface-patch vs. root-primitive upgrade

This section is the primary deliverable. It frames the judgment as a
decision flow, not a prescription. An outside engineer facing a new codegen
bug they have never seen should be able to apply it.

---

### 4.1 Early signals that two surface bugs share a root cause

Investigate root-cause consolidation when **three or more** of the following
hold simultaneously:

1. **Same error type in both bugs.** Bug A: `I8` return for a float. Bug B:
   `iadd.i8` with `i64` operand. Both produce `I8` unexpectedly in a context
   where the Cobrust source declared a wider type. Same Cranelift type token = same
   inference path.
2. **Same failing predicate in the shared layer.** Both bugs ultimately fail
   inside or immediately downstream of `operand_ty`. The predicate
   "`operand_ty` returns `Some(I8)` for a local that should be wider" is the
   exact shared trigger.
3. **Same trigger pattern in source.** Both involve a `Ty::None`-typed synthetic
   temp appearing in a chain of two or more assignments. Bug A: `_0 =
   Copy(_un)` where `_un: Ty::None`. Bug B: `_k = BinaryOp(Mod, Copy(_j),
   Const(2))` where `_j: Ty::None` from a prior `BinaryOp` chain.
4. **Threshold behavior or depth dependency.** Bug B appears at ≥ 4 blocks
   (chain depth ≥ 2 in the inference graph). Bug A appears on depth-2 chains
   (`fr14-fr16`). Same depth sensitivity = same missing fixed-point.
5. **One architecture surfaces the bug fatally, another silently.** When an
   ABI-sensitive downstream step (Cranelift x86_64 `CvtFloatToSintSeq`)
   rejects a type pair, but the equivalent arm64 path tolerates it, the wrong
   type was already present in both — only the platform-specific emitter
   differs. This cross-arch asymmetry is a strong signal that the wrong type
   originates in a shared, architecture-agnostic layer (here: `operand_ty`).

### 4.2 Counter-signals that suggest independent bugs (do not upgrade to Option C)

Stop and treat bugs as independent when **any** of the following hold:

1. **Different compiler layer.** One bug is in lexer / parser, the other in
   codegen. They cannot share a root primitive: the layers do not share state.
2. **Different type token or different semantic family.** One bug produces
   `I8`, the other produces `F64` where `I64` is expected. Different narrow
   types from different inference paths — unlikely to share a root cause.
3. **Different trigger pattern.** One bug only fires on `match` arms; the
   other only fires on `while` loops with phi nodes. No shared MIR shape = no
   shared root.
4. **One bug is alignment / ABI related, the other is type-narrowing.** ABI
   bugs (struct layout, calling-convention field ordering) and type-inference
   bugs are orthogonal subsystems even when both produce wrong code.
5. **Fixes in independent code regions do not regress each other.** Apply
   Option A for Bug A; run tests; Bug B is unchanged. That is empirical
   confirmation they are independent.

### 4.3 Investigation moves that disambiguate

When the early signals are ambiguous, use these moves in order:

1. **Binary search the trigger.** Bug B used cell count {1, 2, 3, 4, 5}. The
   threshold at 4 immediately links it to a depth-dependent step (fixed-point
   iteration or chain length). If the threshold is 1 (always fails) or
   threshold-independent, rule out depth-sensitivity as the shared mechanism.
2. **Cross-arch comparison.** Run the failing test on a second architecture
   (workstation, CI matrix). If Bug A panics on x86_64 but produces wrong
   output silently on arm64, the bug is in the ISA-agnostic layer. If Bug B
   also only manifests on one arch, they share the same ISA-agnostic layer.
3. **Minimal-repro extraction with stripped scope.** Bug B's original
   hypothesis was loop-phi narrowing. Extracting the straight-line version
   (no `while`) proved the phi hypothesis false and isolated the bug to
   arithmetic-chain depth — the same dimension as Bug A.
4. **Apply Option A surgically, then re-run Bug B's repro.** If Option A
   fixes Bug A AND Bug B without touching Bug B's codepath, they were always
   the same root. If Bug B survives Option A, the bugs are independent and
   both need separate patches.
5. **Read the shared callsite chain.** `infer_return_type` → `rvalue_ty` →
   `operand_ty`. Both bugs terminate at `operand_ty`'s `Copy/Move` arm. When
   two bugs share a terminal callsite in the stack trace, the fix almost always
   belongs at or above that callsite.

### 4.4 Costs of upgrading from Option A (surgical) to Option C (root primitive)

Upgrade is not free. Weigh the following before choosing Option C:

| Cost | Magnitude in ADR-0033 case | General guidance |
|---|---|---|
| **More code touched** | 30-50 LOC; 4 function signatures changed (`operand_ty`, `rvalue_ty`, `infer_return_type`, `define_body`) | Proportional to how many consumers of the root primitive exist. Map them before committing. |
| **Wider review surface** | Every caller of `infer_local_types` and `operand_ty` must be audited for the new parameter | Audit mechanically: `grep -n "operand_ty\|rvalue_ty"`. If > 20 call sites, consider a wrapper that defaults to `&HashMap::new()` for callsites that cannot be threaded easily. |
| **Risk of regression in unrelated tests** | Low: the change is additive (inferred map consulted first; declared type is fallback). Tests that previously passed continue to pass because declared types for non-`Ty::None` locals are never in the inferred map. | Higher risk when: the root primitive is mutable (not a read-only lookup), or when fallback behavior changes for previously-working locals. |
| **Inference time increase** | O(n_locals × n_chain_depth) per body; bounded at 2-3 iterations in practice; sub-millisecond. | Matters if the function is on a hot path called O(n_functions) times. Profile before choosing Option C for a hot primitive (e.g. in a type-checker that runs per-expression). |
| **Architectural debt deferred** | Option C does not fix ADR-0020's invariant violation (`LocalDecl.ty` should be fully resolved at MIR). Option B remains the correct long-term direction. | Document this explicitly in the ADR (ADR-0033 §Consequences does). Option C buys time; it is not a permanent substitute for Option B. |

### 4.5 Decision flowchart

```
New codegen bug reported
         │
         ▼
Does this bug produce a wrong Cranelift type token?
─── No ──► Likely ABI / alignment / linking; treat independently.
         │
        Yes
         │
         ▼
Search codebase: does any currently-open bug share (a) same wrong type token
AND (b) same failing callsite (e.g. operand_ty, rvalue_ty)?
─── No ──► No consolidation signal. Apply Option A (surgical patch) and retest.
         │
        Yes
         │
         ▼
Apply Option A surgically to Bug A. Run Bug B's repro WITHOUT touching
Bug B's codepath. Does Bug B also pass?
─── Yes ──► They share a root. Option A happened to cover enough depth.
            Ship Option A, document the consolidation, add a depth-2
            corpus case to prevent regression.
         │
         No
         │
         ▼
Does a minimal binary-search repro of Bug B show a chain-depth threshold?
─── No ──► Bugs are independent at different depths; Option A insufficient
            but still ship it for Bug A; open a separate ticket for Bug B.
         │
        Yes (threshold ≥ 2)
         │
         ▼
Is the shared root primitive a simple read-only lookup (like operand_ty)?
─── No (mutable, hot path, > 20 call sites) ──► Consider Option B
            (architectural layer fix). Scope the blast radius carefully.
         │
        Yes
         │
         ▼
Enumerate all consumers of the root primitive.
Are there ≤ 10 direct call sites to update?
─── No ──► Design a wrapper/default-arg pattern, then apply Option C.
         │
        Yes
         │
         ▼
► CHOOSE OPTION C. Thread the inferred map. Run fixed-point. Verify all
  existing tests pass. Add corpus cases for each depth that was previously
  broken. Document in ADR: "Option B is long-term cleanup; this closes P0."
```

## 5. Negative comparison — what would have happened under Option A or B

### Option A outcome

- Bug A (float-via-temp, depth-1 chains) closes: `infer_return_type` now
  consults `inferred_locals` for the direct `Copy(_un)` hop.
- Bug B survives: Bug B's chain is depth-2 at minimum (`_j = BinaryOp(…)`;
  `_k = BinaryOp(Mod, Copy(_j), …)`). `infer_local_types` with a single pass
  cannot see `_k`'s type because `_j` is still `Ty::None` in iteration 1 of
  a non-fixed-point pass. `operand_ty(Copy(_j))` → `I8` → `iadd.i8`
  verifier reject.
- **Operational cost of Option A**: two ADRs instead of one (ADR-0033 for Bug
  A; ADR-0034-bis for Bug B). Conway-toy stress-test remains broken in
  production at HEAD `540ed65`. The third-party audit finding
  `codegen-i8-i64-mismatch-at-4-blocks` would remain `status: open`. A second
  dispatch sprint (est. 40-80 min) would have been required.

### Option B outcome

- Both bugs close (MIR-layer fix is the architecturally correct long-term
  direction).
- Cost: 5+ lowering sites in `cobrust-mir/src/lower.rs` must be updated to
  carry HIR-resolved types into `_bin` / `_un` / `_callret` creation. Each
  site requires threading the HIR type context through the lowering call
  chain.
- Blast radius: the entire test baseline (2,430+ tests at the time) must be
  re-verified. Cross-arch regression is possible if any lowering site
  introduces a type-promotion rule that differs from what the MIR consumer
  expects.
- **Verdict**: over-engineering for a P0 fix sprint. Option B remains the
  correct post-P0 cleanup (ADR-0033 §Consequences records this explicitly).
  The cost-benefit ratio inverts when there are 10+ `Ty::None`-related bugs
  or when the project reaches MIR stabilisation.

## 6. Code reference

All references are to
`crates/cobrust-codegen/src/cranelift_backend.rs`.

| Site | Lines | Role |
|---|---|---|
| `inferred_locals` declaration + first use | 408–410 | `define_body` materializes the map once via `infer_local_types`; the same map drives `body_signature_with`, `infer_return_type`, and every Variable declaration. |
| `infer_local_types` function | 302–360 | Fixed-point loop. `max_iters = candidates.len() + 1` (defensive bound). Each iteration re-evaluates every `Ty::None` local via `rvalue_ty(..., &out)`; stops when `out.len()` is unchanged. |
| `operand_ty` — `Copy/Move` arm | 263–278 | The root fix: `inferred.get(&p.local)` checked first; `body.locals[p.local].ty` only as fallback. Comment at lines 264-271 explains the chain-depth invariant. |
| `rvalue_ty` threading | 227–253 | Accepts `inferred: &HashMap<LocalId, ir::Type>`; delegates to `operand_ty` for `Use`, `BinaryOp` operand, `UnaryOp` operand. |
| `infer_return_type` threading | 196–218 | Same `inferred` parameter; calls `rvalue_ty` for the return local's RHS so depth-1 copy chains (`_0 = Copy(_un)`) now resolve to the inferred type. |
| `extern_funcs` branch as template | 562–566, 876–897 | Pattern for threading a context map (`extern_funcs`) through the `EmitCtx` struct to all `lower_call` consumers — the same pattern applied to `inferred_locals` in `define_body` (lines 408–514). |

## 7. Cross-references

- **ADR-0033** (`docs/agent/adr/0033-codegen-float-return-fix.md`) — the fix's
  design: Option A / B / C trade-offs, acceptance gates, consequences.
- **ADR-0023** — M9 codegen ABI + type matrix (the surface this fix lives on).
- **finding `m9-cross-arch-linux-x86_64-validation`** — Bug A origin: Linux
  x86_64 panic + macOS arm64 silent-wrong-value diagnostic.
- **finding `codegen-i8-i64-mismatch-at-4-blocks`** — Bug B origin: Conway-toy
  review-claude stress test; binary search to 4-block threshold; conclusion
  updated post-merge to record empirical closure by ADR-0033 Option C.
- **ADR-0019** §"Scientific" (constitution §5.2) — discipline under which this
  finding is filed: every design decision with evidence, negative results
  documented, not hidden.
- **finding `multi-agent-cobrust-topology`** — topology context: model-tier
  rules (Opus for hard/strategic; sonnet for mechanical-with-judgment); P9
  two-phase dispatch SOP that framed the fix-sprint sequencing.
