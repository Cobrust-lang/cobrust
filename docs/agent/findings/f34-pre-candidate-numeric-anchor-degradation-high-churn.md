---
doc_kind: finding
finding_id: F34
title: "Numeric-anchor degradation in ADRs under high-churn surface files"
date: 2026-05-18
status: ratified (2026-05-18)
discovered_by: project-wide Tier-2 review-claude ab88ae5a4ec1ab490
adsd_family: F1-Sediment (doc-tree decay sub-family)
related_findings: F27, F33
---

# Summary

ADRs in Phase G batch (0052a-g + 0052b-prereq + 0054-0058) cite numeric line anchors against high-churn source files (`crates/cobrust-types/src/check.rs` grew 60-80% during Phase G; `crates/cobrust-cli/src/error_ux.rs` grew from ~547 to ~1194 LOC).

Result: every numeric `file:line` anchor cited in those ADRs drifted >100 lines within ~2 weeks of being written. Project-wide Tier-2 review (2026-05-18) found:

- ADR-0052b: ~16 stale `check.rs:NNN` + 6 stale `error_ux.rs:NNN`
- ADR-0052d-prereq: `check.rs:920` → actual L1008 (Δ +88)
- ADR-0052g: anchors recently-pinned at `1fbed82` still valid by 4-day delta

Total: ~24 stale anchors in 2 ADRs after ~14 days.

# Mechanism

- Author authors ADR at write-time T0 + cites `file:LINE` against HEAD-at-T0.
- File grows continuously (Phase G batch adds variant + impl arms in same files).
- At T0+N days, every `LINE` cited in ADR drifts by file's growth + insertions above LINE.
- F27 (verified-at-HEAD) catches drift on FOLLOW-UP audit BUT only for the immediate author dispatch — silent drift accumulates between audits.

# Why this is sediment-shaped (F1 sub-family)

The doc-tree decays AS the codebase grows. The decay is invisible (no compile error, no runtime failure). Reader follows the anchor + sees adjacent code that's plausibly-related, doesn't notice the drift.

# Promotion 2026-05-18

**Second corroborator confirmed**: Phase H batch ADRs 0055c + 0055d explicitly adopted symbol-anchor convention throughout (e.g., `check.rs::Ctx::synth_expr`, `check.rs::Ctx::synth_call` over numeric `check.rs:NNN` form) per audit `af22fcdedbd1976d5` Lane 2. The audit GO-WITH-FINDINGS verdict on 0055c + 0055d documents adoption as load-bearing design decision for ADR longevity — matching the exact mechanism described in §Mechanism above.

Two-Phase evidence (Phase G first-instance + Phase H explicit adoption) satisfies the second-corroborator requirement. F34 promoted from pre-candidate to ratified.

# Proposed mitigation

Two options:

**Option A — symbol anchors over numeric** (preferred):
Prefer `check.rs::TypeError::ImplicitTruthiness arm` over `check.rs:1532`. Symbol survives line-number drift. Conventional in Rust-doc culture (rustdoc cross-refs are symbol-based).

**Option B — automated anchor lint** (heavy):
CI script grep ADR file:line citations + verify each line in current source matches a known pattern. High false-positive rate. Reject as over-engineered for current scale.

**Recommendation**: Adopt Option A as ADR convention going forward; existing numeric anchors stay until next audit cycle (Tier-2 sweep at v0.4.0 ship).

# Cross-references

- F27 — verified-at-HEAD discipline (catches at audit time, not preventive)
- F33 — rule-skip pattern (related: numeric-anchor-write may be rule-skip of symbol convention)
- ADR convention amendment: future sub-ADRs should default to symbol-style anchors for `check.rs`, `error_ux.rs`, `lower.rs`, `cranelift_backend.rs` (the 4 highest-churn files of Phase G)
