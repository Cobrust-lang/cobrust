---
doc_kind: finding
name: f38
title: Source-Surface Leakage of Codegen Internal Primitive
status: ratified
date: 2026-05-19
last_verified_commit: 46c0946
family: "F36/F37 source-fidelity"
resolution: adr:0064
related_findings: [f36, f37]
---

# F38: Source-Surface Leakage of Codegen Internal Primitive

## §1 Pattern

A codegen-internal primitive — named by type-shape (`<verb>_<type>`, e.g., `print_int`, `print_str`) — leaks into the source-face PRELUDE during a wave-1 demo sprint. It gets fossilized when subsequent waves do not audit the question: "is this name source-face API or codegen-internal symbol?"

The leak path:
1. Demo sprint needs to prove codegen works → quickest route is direct monomorphic names (`print_int`, `print_str`).
2. Demo lands, wave-1 closes, no cleanup ADR is authored.
3. Wave-2 onward sees the names in PRELUDE, writes examples against them, accumulates usage.
4. By the time an audit catches it, migration cost is non-trivial (50–100 call sites across examples, fixtures, skills).

This is not a logic bug. It is a **design surface contamination bug**: the internal implementation vocabulary bleeds into the user vocabulary.

---

## §2 Why It Is Debt

**§2.5 training-data-overlap violation (ADR-0051 binding):**

- LLMs trained on Python/Rust write `print(x)` — this is one of the highest-frequency call patterns in any Python corpus.
- `print_int(x)` does not appear in Python training data. It does not appear in Rust training data. It is a Cobrust-internal artifact.
- Result: LLM generates `print(x)` → source-face NameError → LLM confused by gap between prior and actual API → corrective loop is wasted tokens and latency.

**Migration cost grows quadratically with usage spread:**

Every new example file, fixture, LC-100 stress entry, or skill reference that uses `print_int` is another call site that will need updating. The longer the debt persists, the more expensive the cleanup sprint.

**Zero runtime benefit over polymorphic `print`:**

Static types are fully resolved by the time codegen runs. Monomorphization (`print_int` → `__cobrust_print_int`) can happen post-typecheck from a single polymorphic `print(x)` source call. The internal C-ABI symbols are unchanged; only the routing path changes.

---

## §3 Empirical

**Affected functions (Phase E demo era, Cobrust 2026-04):**

| Source-face name | Should be | Internal C-ABI symbol |
|---|---|---|
| `print_int` | `print` | `__cobrust_print_int` |
| `print_str` | `print` | `__cobrust_print_str` |
| `print_bool` | `print` | `__cobrust_print_bool` |
| `print_float` | `print` | `__cobrust_print_float` |

**Detection date:** 2026-05-19 user retrospective.

**Actual call-site count (ADR-0064 sprint):** 133 `.cb` call sites + ~200 Rust inline-source test strings refactored (prior agent commit `5e87e77`). Net source delta ~333 LOC.

**Sprint commit references:**
- `c73be4e` — PRELUDE table: remove `print_int`/`str`/`bool`/`float` source-face entries (§3.1)
- `b51b907` — polymorphic `print()` dispatch in `synth_call` + codegen monomorphization (§3.2-§3.3)
- `5e87e77` — mechanical refactor: 133 `.cb` call sites + Rust inline strings → `print()` (§3.4)
- `46c0946` — Phase 4 fix: `Ty::None` callret locals must dispatch to `__cobrust_println_int` not str-buf (§3.3 correctness)

**Ratified at:** `46c0946` (feature/0064-print-mono, rebased on `28176fa`)

---

## §4 Detection Rule (CI Gate Candidate)

For every function listed in the PRELUDE source-face table:

> If the function name matches the pattern `<verb>_<type>` where `<type>` ∈ `{int, str, bool, float, list, dict, set, tuple, ...}`, file an audit issue: "should this be polymorphic in source?"

This rule catches the class of leakage before it fossilizes. Candidate for a lint pass in CI post-ADR-0064 ratification.

Pseudocode for the check:
```
for name in PRELUDE.source_face_names:
    if re.match(r'^[a-z_]+_(int|str|bool|float|list|dict|set|tuple)$', name):
        emit_audit_warning(f"PRELUDE name '{name}' matches type-suffix pattern — verify it is source-face intentional")
```

---

## §5 Resolution Path

**ADR-0064** (`docs/agent/adr/0064-print-monomorphization-source-surface-cleanup.md`) is the direct response to this finding.

Implementation phases:
1. Remove `print_int` / `print_str` / `print_bool` / `print_float` from PRELUDE source-face table.
2. Add `print(x)` polymorphic dispatch — typecheck resolves type, codegen monomorphizes to `__cobrust_print_*`.
3. Mechanical refactor sprint (mirrors LC-100 stress &borrow 226-site batch pattern, commits b2618f3 + 8f63132).
4. Post-sprint: update this finding status to `ratified`, record final LOC numbers.

---

## §6 Status

`ratified` at commit `46c0946` (2026-05-20):
- Zero `print_int` / `print_str` / `print_bool` / `print_float` call-sites in any `.cb` source file under `examples/`. Confirmed via `grep -rEn "print_(int|str|bool|float)\(" examples/ --include="*.cb"` → empty.
- LC-100 12/12 maintained (including LC-05 merge_two_sorted_lists which caught a Ty::None dispatch bug fixed in Phase 4).
- 5+ integration tests passing for polymorphic `print` (`for_range_e2e.rs` ADR-0064 acceptance gate tests).
- `print_int(42)` in a `.cb` source file now produces `NameError: print_int is not defined` (removal confirmed at §3.1).
- 56/56 codegen diff corpus tests passing.

---

## §7 Related

| Finding | Relationship |
|---|---|
| F36 — fixture-name-vs-behavior drift | Same family: wave-1 demo-ware fossilizes without audit checkpoint; name promises X, behavior is Y |
| F37 — silent-rot-on-accepted-debt | Same family: accepted debt silently accumulates usage; no `#[ignore]` citation disciplines the debt boundary |
| ADR-0050e — PRELUDE-fn cleanup | Precedent: prior cleanup sprint that set the dispatch pattern this ADR reuses |
| ADR-0051 — LLM-first design | Constitutional binding: §2.5 training-data-overlap rule that this debt violates |
| LC-100 &borrow refactor (b2618f3 + 8f63132) | Mechanical sprint precedent: 226-site batch refactor; ADR-0064 Phase 3 mirrors this pattern |
| `docs/agent/findings/examples-literal-print-debt.md` | May overlap with Phase 3 call sites; check before sprint to avoid double-counting |
