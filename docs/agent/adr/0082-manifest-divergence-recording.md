---
doc_kind: adr
adr_id: 0082
title: Manifest divergence recording — verification.divergences mirrors the L2.behavior gate
status: accepted
date: 2026-05-30
last_verified_commit: 36d86d0
supersedes: []
superseded_by: []
---

# ADR-0082: Manifest divergence recording

## Context

The M4 real-LLM tomli run (finding `M4-real-llm-tomli-2026-05-30`,
2026-05-30) closed the flagship closed-loop claim down to one residual
narrative-vs-reality gap, recorded as Follow-up #2. It has two distinct
defects:

- **(a)** the production `pipeline::translate` repair loop has never been
  exercised against a *real* differential oracle (the default
  `BehaviorVerifier` is `AcceptAll`→`Skip`, ADR-0040);
- **(b)** the provenance manifest's `verification.divergences` field was
  **hardcoded** `vec![]` at `pipeline.rs:807`, so even a *real*
  `TierVerifier` Reject — already fully wired (ADR-0052c) and already
  feeding the repair loop — left **no trace** in the manifest.

This ADR resolves **(b)**. Defect (a) — a concrete CPython
`OracleHarness` plus a hermetic full-loop integration test — is tracked
separately (translator task #159).

### Why (b) is a real honesty bug, not cosmetics

The manifest is the project's §5.2-Scientific deliverable: "every AI
translation output includes a verification manifest: oracle used, seeds,
inputs, **divergences**" (CLAUDE.md §5.2). A manifest that *always*
reports zero divergences is an F44-class "green that lies": it claims the
behavior gate observed perfect parity even when the repair loop demonstrably
caught and fixed a divergence on the way to convergence. The repair loop
already knew the divergence (it rendered a `GateFailure`, wrote a
diagnostic blob, and re-dispatched) — it simply threw the record away on
the success path, keeping it only for the *escalation-failure* report.

## Decision

`verification.divergences` MUST mirror exactly the set of `l2_behavior`
Rejects the repair loop observed-and-repaired during a successful
translation. Concretely:

1. `run_repair_loop` accumulates one rendered record per `l2_behavior`
   Reject (perf Rejects are excluded — they are not behavioral
   divergences) into an `observed_divergences: Vec<String>`, returned on
   `RepairLoopResult`.
2. `render_divergence(&GateFailure)` produces a terse one-line record:
   `function`, failing `input`, `expected`, `actual`, gate, and the
   re-dispatch attempt number. `expected`/`actual` payloads are clipped
   to 120 chars (`…(N more)`) so a structured oracle output cannot bloat
   the manifest.
3. `build_manifest` writes that vec into `verification.divergences`
   verbatim — the prior `vec![]` literal is gone.

### Invariants

- **Mirror, never fabricate.** A clean run where no `l2_behavior` Reject
  ever fired records `divergences: []`. (Negative tripwire test:
  `pipeline_is_deterministic_across_runs` asserts emptiness on the
  AcceptAll path.)
- **One Reject ⇒ one record.** Each behavioral Reject the loop repairs
  contributes exactly one record. (Positive test:
  `pipeline_repair_loop_recovers_when_attempt_2_canned` asserts
  `len == 1` naming the function, input, expected, actual, and gate.)
- **Escalation is out of scope.** When a function exhausts the
  escalation threshold the whole `translate` returns `Err` and no
  manifest is written; `divergences` is a property of the *success*
  path only (the divergences that were found *and fixed*).
- **Behavioral only.** `failed_gate == "l2_behavior"` is the filter;
  `l2_perf` Rejects do not enter `divergences`.

## Consequences

- The manifest now carries observable evidence of the repair loop's
  behavioral work — aligning `verification.divergences` with the same
  honest-verdict contract ADR-0040 imposed on the gate *strings*.
- No public API change: `VerificationSection.divergences` already
  existed and was already serialized; only its *population* changed.
  `build_manifest` / `run_repair_loop` / `render_divergence` are all
  private. Doc-coverage is unaffected (no new public item).
- Once defect (a) lands (task #159, a real CPython `OracleHarness`), a
  real-LLM tomli run that hits a genuine divergence will record it here
  automatically — no further wiring needed. This ADR is the prerequisite
  that makes that run's manifest honest.

## Evidence

- `crates/cobrust-translator/src/pipeline.rs` — `render_divergence`,
  `observed_divergences` accumulation in `run_repair_loop`, threaded
  through `RepairLoopResult` → `build_manifest`.
- Tests (both in `pipeline::tests`):
  - `pipeline_repair_loop_recovers_when_attempt_2_canned` — positive:
    one Reject ⇒ one record, content-checked.
  - `pipeline_is_deterministic_across_runs` — negative: no Reject ⇒
    empty.
- `cargo test -p cobrust-translator --lib` → 93 passed; `cargo clippy
  -p cobrust-translator --all-targets` → zero warnings (2026-05-30).
