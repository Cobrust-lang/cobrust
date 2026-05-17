---
adr_id: "0037"
title: "@py_compat hard-bind — strict tier enforcement in translation pipeline"
status: superseded
date: 2026-05-10
authors: [review-claude, p7-cleanup-sprint]
supersedes: []
superseded_by: ["0052c"]
---

# ADR-0037 — @py_compat hard-bind

**Status**: superseded by ADR-0052c (Wave-2 activation, 2026-05-17 at HEAD `0418eae`)

## Context

Reserved for Phase F.1.x. The `@py_compat` tag system (defined in the
constitution §2.4) currently exists in the stdlib annotation layer but
is not enforced as a hard gate in the L2 verification loop. When a
translated function's PROVENANCE carries a `@py_compat(strict)` or
`@py_compat(numerical(rtol=1e-7))` declaration, the behavior gate
must enforce that tier's acceptance threshold — not just pass/fail on
the existing oracle.

## Cross-references

- finding: `docs/agent/findings/translator-real-vs-synthetic-status.md`
  §"Actionable consequences" #1 — source of this reserved slot.
- ADR-0038 §Cross-references: references ADR-0037 for this binding.
- Constitution §2.4 — `@py_compat` tier definitions.
- ADR-0052c — Wave-2 activation that supersedes this placeholder.

## Decision

**Superseded by ADR-0052c** (Wave-2 activation, 2026-05-17 at HEAD
`0418eae`). The full design is now in ADR-0052c §3-§7:

- `FunctionSpec.py_compat: String` → `FunctionSpec.py_compat: PyCompatTier`
  enum migration (ADR-0052c §4)
- `TierVerifier` impl `BehaviorVerifier` dispatches per-tier verdict
  (ADR-0052c §5)
- Tier-aware prompt construction in `translate.rs` (ADR-0052c §6)
- Per-tier router routing (`translate_strict` → consensus,
  `translate_numerical` → cost) (ADR-0052c §7)

This placeholder remains for cross-reference stability; consult
ADR-0052c for the binding contract.

## Consequences

Superseded. See ADR-0052c §13 for the full consequences enumeration
(positive / negative / neutral) and the Wave-2 cascade addendum
ratified at HEAD `0418eae`.
