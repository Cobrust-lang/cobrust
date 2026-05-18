---
doc_kind: index
last_verified_commit: e91caed
---

# Findings

Negative results, dead ends, and benchmark surprises live here. They
are first-class deliverables — capturing what *doesn't* work is as
valuable as capturing what does.

## How to add a finding

1. Create `<slug>.md` (kebab-case, descriptive).
2. Frontmatter:

   ```yaml
   ---
   doc_kind: finding
   finding_id: <slug>
   last_verified_commit: <sha>
   dependencies: [<stable-id>...]
   ---
   ```

3. Section structure (see `../conventions.md`):
   - **Hypothesis** — what we tried to prove
   - **Method** — what we did
   - **Result** — what happened
   - **Conclusion** — actionable takeaway
   - **Cross-references**

4. Update the index below.

## Index

| Finding ID | File | Status |
|---|---|---|
| `m1-fuzz-method` | [`m1-fuzz-method.md`](m1-fuzz-method.md) | M1 fuzz gate satisfied via proptest; one panic shrunk and fixed |
| `m5-m7-real-llm-validation` | [`m5-m7-real-llm-validation.md`](m5-m7-real-llm-validation.md) | M3 LLM Router validated against a real OpenAI-compatible endpoint; round-trip, cache replay, transport-failure isolation all green |
| `m13-sync-bridge-cost` | [`m13-sync-bridge-cost.md`](m13-sync-bridge-cost.md) | M13 sync-bridge architecture costs ~2.8× over pure-async tokio reference; ADR-0028 §F amends the gate from 0.7× to 0.3× per measured reality |
| `examples-literal-print-debt` | [`examples-literal-print-debt.md`](examples-literal-print-debt.md) | M12.x ADR-0027 omitted fizzbuzz/fib from binding deliverable list — examples still print canned strings rather than executing real algorithms; M11.1 sprint queued |
| `translator-real-vs-synthetic-status` | [`translator-real-vs-synthetic-status.md`](translator-real-vs-synthetic-status.md) | L0→L1→L2→L3 closed loop never run end-to-end with a real LLM on a real library; remediation sprint queued |
| `m12-x-while-if-codegen-regression` | [`m12-x-while-if-codegen-regression.md`](m12-x-while-if-codegen-regression.md) | Cranelift backend post-M12.x: `while`-loop with leading `if` produces empty stdout; M11.1 sprint queued to fix before fizzbuzz/fib rewrite |
| `multi-agent-cobrust-topology` | [`multi-agent-cobrust-topology.md`](multi-agent-cobrust-topology.md) | 6 recurring failure modes + SOPs from 100+ commits of 4-way parallel multi-agent worktree topology — externalised methodology per audit #6 |
| `m9-cross-arch-linux-x86_64-validation` | [`m9-cross-arch-linux-x86_64-validation.md`](m9-cross-arch-linux-x86_64-validation.md) | Linux x86_64 surfaced `infer_return_type` Ty::None float bug; macOS arm64 silent-wrong-value latent. P0 fix sprint dispatched (Task #41) |
| `codegen-i8-i64-mismatch-at-4-blocks` | [`codegen-i8-i64-mismatch-at-4-blocks.md`](codegen-i8-i64-mismatch-at-4-blocks.md) | review-claude Conway-toy stress: 4+ similar inline compute blocks → Cranelift verifier rejects `iadd.i8` with i64 operand. Bug 1 (narrow-type) real (Task #43); Bug 2 (silent miscompile) was MIS-DIAGNOSIS — CLI already exits 3 correctly (Task #42 closed) |
| `audit-1-codegen-pollution-quarantine-sop` | [`audit-1-codegen-pollution-quarantine-sop.md`](audit-1-codegen-pollution-quarantine-sop.md) | CTO守闸 fallback for SendMessage absence: in-flight audit #1 sub-agents running on codegen-polluted baselines; merge-time rejection + rerun sprint after fixes land |
| `audit-1-tomli-real-llm-result` | [`audit-1-tomli-real-llm-result.md`](audit-1-tomli-real-llm-result.md) | **Audit #1 PASS** — Opus authoritative: first end-to-end real-LLM translation of `tomli::parse_bool` (rich-prompt design) PASS 12/12 strict over 5 deterministic runs (ADR-0032). sonnet branch (`feature/audit-1-tomli-real-llm`) held as supplementary scaffolding showing PARTIAL-FAIL with bare-bones prompt — together they pin ADR-0035 (renumbered from 0033) prompt-design strategic decision |
| `audit-3a-stateful-prompt-design` | [`audit-3a-stateful-prompt-design.md`](audit-3a-stateful-prompt-design.md) | **Audit #3a PASS** — Opus authoritative: production `build_translation_prompt_rich` builder lifts the audit-1 design; stateful function `tomli::parse_int` (loop-driven state mutation) PASS 14/14 strict via real LLM. §1.2 mechanism-demonstrated → production-validated upgrade signal achieved (ADR-0036). audit-1 sonnet PARTIAL-FAIL retired. |
| `two-bugs-one-fix-option-c-pattern` | [`two-bugs-one-fix-option-c-pattern.md`](two-bugs-one-fix-option-c-pattern.md) | Reusable codegen methodology: when two surface bugs share root-cause family (`Ty::None` fallback default), upgrading to Option C (root primitive — `inferred_locals` fixed-point) closes both with one fix. Decision criteria for surface-vs-root choice articulated. |
| `m9-cross-arch-9ff481c-regression` | [`m9-cross-arch-9ff481c-regression.md`](m9-cross-arch-9ff481c-regression.md) | PARTIAL PASS — no new Linux-only regression in ~14 commits since last cross-arch validation. All 4 example binaries + Conway-toy 4-cell/5-cell pass bit-identical on x86_64. Pre-existing 2-test staleness in `cli_verifier_exit_corpus` (both archs equally) needs CTO cleanup. |
| `while-binop-eq-zero-condition-miscompile` | [`while-binop-eq-zero-condition-miscompile.md`](while-binop-eq-zero-condition-miscompile.md) | ✅ **closed_by_M11.3** @ `cfb7fd0` — review-claude LC 263 farm. `while <BinOp> == 0:` head silent miscompile; 24-hr 第三个 `while` codegen bug. Empirical fix in MIR (`lower_condition` shared root primitive); cmp-bit-identical stdout verified. ADR-0035 §"Layer correction" addendum records spike-codegen-fix-MIR pattern. |
| `msgpack-fuzz-190gib-allocation` | [`msgpack-fuzz-190gib-allocation.md`](msgpack-fuzz-190gib-allocation.md) | **P1 closed** — ARRAY_32/MAP_32 DoS fixed by double-bound prealloc cap (`saturating_sub + min(64KiB)`) in T1.1-cleanup sprint. |
| `m9-cross-arch-post-T1.1-cleanup-regression` | [`m9-cross-arch-post-T1.1-cleanup-regression.md`](m9-cross-arch-post-T1.1-cleanup-regression.md) | **PASS** — post-T1.1-cleanup sprint cross-arch validation on Ubuntu 22.04 x86_64; 2545/0/8 on both archs. Pre-fix: pyo3 0.22 API mismatch found + fixed (T1.C). |
| `B4-toml-recursion-depth` | [`B4-toml-recursion-depth.md`](B4-toml-recursion-depth.md) | **P0 closed** — `cobrust-tomli` `parse_array`/`parse_inline_table` unbounded recursion → SIGSEGV on adversarial deep-nested input; fixed via `State::depth` + `MAX_DEPTH=100` guard. |
| `B5-requests-body-cap` | [`B5-requests-body-cap.md`](B5-requests-body-cap.md) | **P0 closed** — `cobrust-requests` `from_reqwest` had no body size cap; `read_to_end` on adversarial server → OOM; fixed via `MAX_BODY_BYTES=64MiB` streaming cap + `HttpErrorKind::BodyTooLarge`. |
| `B6-msgpack-pos-overflow` | [`B6-msgpack-pos-overflow.md`](B6-msgpack-pos-overflow.md) | **P0 closed** — `cobrust-msgpack` `unpack_bin/float/str/uint` used plain `pos + length` without overflow check; 32-bit wrap-around bypasses bounds check; fixed via `checked_add` + `MsgErrorKind::OverflowSize`. |
| `m10-sha-pin-hallucination` | [`m10-sha-pin-hallucination.md`](m10-sha-pin-hallucination.md) | **CI hot-fix closed** — M10 Wave 2 sub-agent emitted 4 fake 40-char SHA pins for 3rd-party GitHub Actions; 13/14 CI jobs red on v0.1.0 tag; reverted at `4186c8e`; ADR-0042 closes F1.1 enforcement path. |
| `lc100-tier-a-summary` | [`lc100-tier-a-summary.md`](lc100-tier-a-summary.md) | **LC-100 Tier A Phase 3 triage** — 77/100 pass rate; 0 compile-fail; 3 distinct failure patterns (A: 8 misalignment / B: 1 list[str] gap / C: 15 corpus defects). Fix-pack potential: 99/100 with Pattern A + C closure (5-8 hr). Ramp recommendation: HOLD fix-pack then GO Tier B. ADR-0047 Phase 3 deliverable. |
| `lc100-pattern-a-rodata-literal-misalignment` | [`lc100-pattern-a-rodata-literal-misalignment.md`](lc100-pattern-a-rodata-literal-misalignment.md) | **8 LC-100 failures** — `print_no_nl(literal)` / `str_at(literal_var, i)` panic at `fmt.rs:194` because Cranelift passes raw `.rodata` byte pointer where runtime expects 8-byte aligned `*mut StringBuffer`. Fix candidates: F1 (raw-bytes runtime variant, preferred, 2-4 hr); F3 (`print_int_no_nl` intrinsic, complementary). |
| `lc100-pattern-b-list-of-str-gap` | [`lc100-pattern-b-list-of-str-gap.md`](lc100-pattern-b-list-of-str-gap.md) | **1 LC-100 failure (024 group-anagrams)** — `list[str]` type missing from Cobrust language surface; only `list[i64]` exists. BLOCK-severity for string-storing algorithms. Estimated ≥ 1 day opus-grade work; ADR-0048 proposal candidate. Defer to forward-looking ADR sprint, not LC-100 fix-pack. |
| `lc100-pattern-c-test-corpus-defects` | [`lc100-pattern-c-test-corpus-defects.md`](lc100-pattern-c-test-corpus-defects.md) | **15 LC-100 failures** — test.toml oracles mathematically inconsistent with algorithm description; .cb implementations are correct. 6 sub-classes (arithmetic miscount / algorithm contradiction / tree encoding / ambiguous spec / constraint violation / balance miscount). Fix: 1-2 hr corpus correction. F23 ADSD candidate (oracle authorship without independent verification). |
| `lc100-adsd-f-pattern-candidates` | [`lc100-adsd-f-pattern-candidates.md`](lc100-adsd-f-pattern-candidates.md) | **ADSD F22 + F23-A codification text** — F22 (coverage drive without bug-fix cadence) prevented by ADR-0047's time-cap + decision-gate; F23-A (oracle authorship without independent verification) primary precedent. F23-B (synthetic-distribution-drift) remains candidate pending audit-3a follow-up real-Python translation. Handoff to review-claude for ADSD repo codification. |
| `mf30-loop-scope-crosses-fn-boundary` | [`mf30-loop-scope-crosses-fn-boundary.md`](mf30-loop-scope-crosses-fn-boundary.md) | **P2 closed_by_cef71f3** — type checker accepted `break` / `continue` inside a nested `fn` whose outer scope sat in a loop body. Surfaced during ADR-0050a corpus authorship (b13/b14). Fixed via `check_fn` save/reset/restore of `loop_depth`, mirroring the `return_stack` discipline. MIR's defensive fallback would have caught it later with an opaque `Internal` error. |
| `0.1.0-beta-tomli-full-translation` | [`0.1.0-beta-tomli-full-translation.md`](0.1.0-beta-tomli-full-translation.md) | T1.1 production milestone — tomli full-library real-LLM translation 5/5 PASS; ADR-0039 + blog post |
| `0052a-wave1-dev-bidirectional-unify-cascade` | [`0052a-wave1-dev-bidirectional-unify-cascade.md`](0052a-wave1-dev-bidirectional-unify-cascade.md) | F31-candidate — ADR-0052a Wave 1 bidirectional `Ref(T) ↔ T` unify produces 142-failure AmbiguousType cascade; root cause + v3 one-way-coercion fix documented |
| `0052d-prereq-impl-blocker` | [`0052d-prereq-impl-blocker.md`](0052d-prereq-impl-blocker.md) | ADR-0052d-prereq DEV blocker — parser rejected `&s.method()` (f30wit_method_03); deferred to 0052f+0052g |
| `adr-cross-surface-bug-fix-scope-creep` | [`adr-cross-surface-bug-fix-scope-creep.md`](adr-cross-surface-bug-fix-scope-creep.md) | F29-candidate — cross-surface bug-fix scope creep under single-Opus PAIR retrofit |
| `adr-scope-reality-divergence` | [`adr-scope-reality-divergence.md`](adr-scope-reality-divergence.md) | F27-candidate — ADR scope-reality divergence on Phase F.3 batch frame |
| `adsd-pair-pattern-impl-gap` | [`adsd-pair-pattern-impl-gap.md`](adsd-pair-pattern-impl-gap.md) | F28-candidate — ADSD PAIR pattern is ceremonial under Claude Code single-layer sub-agent architecture |
| `comp-lowering-zero-sentinel-collision` | [`comp-lowering-zero-sentinel-collision.md`](comp-lowering-zero-sentinel-collision.md) | list-comprehension lowering still uses iter-protocol 0-sentinel collision; open pending Phase G consolidation |
| `f33-agent-self-disciplinary-rule-skip` | [`f33-agent-self-disciplinary-rule-skip.md`](f33-agent-self-disciplinary-rule-skip.md) | F33-candidate — agent self-disciplinary rule skip when judged low-risk |
| `fstring-hole-mir-type-dispatch` | [`fstring-hole-mir-type-dispatch.md`](fstring-hole-mir-type-dispatch.md) | f-string Str holes misdispatched through `__cobrust_fmt_int`; closed_by_2e9d456 |
| `lc100-str-use-after-move-regression-from-adr0050c` | [`lc100-str-use-after-move-regression-from-adr0050c.md`](lc100-str-use-after-move-regression-from-adr0050c.md) | LC-100 mass-regression from ADR-0050c Str=non-Copy; accepted as honest debt per Phase F.3 precedent |
| `lower-constant-str-zero-pointer-m9-stub` | [`lower-constant-str-zero-pointer-m9-stub.md`](lower-constant-str-zero-pointer-m9-stub.md) | `lower_constant(Constant::Str(_))` returned zero pointer for non-callsite paths (M9 stub); closed_by_65a5335 |
| `mf31-while-else-not-skipped-on-break` | [`mf31-while-else-not-skipped-on-break.md`](mf31-while-else-not-skipped-on-break.md) | MIR while-else clause is NOT skipped when `break` fires — contradicts ADR-0050a §"Semantics"; open |
| `predicate-flip-cascade-discovery-deficit` | [`predicate-flip-cascade-discovery-deficit.md`](predicate-flip-cascade-discovery-deficit.md) | F30-candidate — predicate-flip cascade discovery deficit |
| `f34-pre-candidate-numeric-anchor-degradation-high-churn` | [`f34-pre-candidate-numeric-anchor-degradation-high-churn.md`](f34-pre-candidate-numeric-anchor-degradation-high-churn.md) | F34 ratified (2026-05-18) — numeric anchor degradation in ADRs under high-churn surface files (Phase G batch; ~24 stale anchors in 2 ADRs after ~14 days) |
