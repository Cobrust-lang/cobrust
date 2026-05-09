---
doc_kind: index
last_verified_commit: dfba6e9
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
| `two-bugs-one-fix-option-c-pattern` | [`two-bugs-one-fix-option-c-pattern.md`](two-bugs-one-fix-option-c-pattern.md) | Reusable codegen methodology: when two surface bugs share root-cause family (`Ty::None` fallback default), upgrading to Option C (root primitive — `inferred_locals` fixed-point) closes both with one fix. Decision criteria for surface-vs-root choice articulated. |
