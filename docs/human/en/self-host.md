# Self-hosting the Type Checker (Phase H)

> **Status: proposed (impl pending)** — Phase H design is complete; implementation has not started.

## Goal

Translate `crates/cobrust-types` (type-checker core) from Rust into Cobrust,
achieving partial compiler self-hosting. This is the first deliverable stage of
the self-hosting roadmap in CLAUDE.md §4.4.

## Scope

| File | LOC | Wave | ADR |
|---|---|---|---|
| `ty.rs` (type universe) | ~220 | Wave 2 | ADR-0055a |
| `error.rs` + `lib.rs` (error enum + entry point) | ~346 | Wave 2 | ADR-0055b |
| `infer.rs` (inference + unification) | ~300 | Wave 2 | ADR-0055c |
| `check.rs` (bidirectional checker) | ~2402 | Wave 3 | ADR-0055d |
| Parity harness | — | Wave 1 | ADR-0055e |
| **Total** | **~3368** | | ADR-0055 |

## Phase overview

```
Wave 1: 0055e — parity harness (first; gates all subsequent waves)
Wave 2: 0055a + 0055b + 0055c — parallel (Tier-1: types / errors / inference)
Wave 3: 0055d — bidirectional checker (Tier-2; largest sub-sprint)
```

## Wall-time estimate

- Wave 1: ~1 week (parity harness)
- Wave 2: ~1 week (3 ADRs in parallel)
- Wave 3: ~1-2 weeks (`check.rs` ~2402 LOC; largest single sub-sprint in project history — see ADR-0055d §10.2)
- **Total: ~2.5 weeks**

## Current status

All Phase H ADRs (0055 + 0055a–0055e) are **proposed**.
Implementation will be dispatched to the DG workstation (2×RTX 3090) after
Wave 1 (0055e) is approved.

## Related ADRs

- [ADR-0055](../../agent/adr/0055-phase-h-self-host-type-checker.md) — frame ADR
- [ADR-0055a](../../agent/adr/0055a-ty-rs-cb-port.md) — `ty.rs` cb port
- [ADR-0055b](../../agent/adr/0055b-error-rs-lib-rs-cb-port.md) — `error.rs` + `lib.rs` cb port
- [ADR-0055c](../../agent/adr/0055c-infer-rs-cb-port.md) — `infer.rs` cb port
- [ADR-0055d](../../agent/adr/0055d-check-rs-cb-port.md) — `check.rs` cb port
- [ADR-0055e](../../agent/adr/0055e-phase-h-parity-harness.md) — parity harness
