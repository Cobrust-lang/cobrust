---
adr_id: "0037"
title: "@py_compat hard-bind — strict tier enforcement in translation pipeline"
status: proposed
date: 2026-05-10
authors: [review-claude, p7-cleanup-sprint]
supersedes: []
superseded_by: []
---

# ADR-0037 — @py_compat hard-bind

**Status**: proposed (reserved placeholder)

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

## Decision

Deferred to Phase F.1 post-M12 cleanup. This placeholder prevents the
ADR roster from showing a dangling gap between ADR-0036 and ADR-0038,
and prevents cross-references from being stale.

## Consequences

None until implemented. When implemented: the L2.behavior gate will
reject translations that fail the declared py_compat tier (e.g.
`strict` tier requires byte-identical oracle match; `numerical` tier
allows `rtol=1e-7` tolerance).
