---
finding_id: f75
title: "A 'deliberately slow' test fixture built from a discarded black_box busy-loop is DCE-flaky; make it slow BY CONSTRUCTION"
status: candidate
date: 2026-05-31
severity: medium
surface: crates/cobrust-translator/tests/production_loop_perf_gate.rs
siblings: [f36-fixture-name-vs-behavior-drift, f44-ci-cache-stale-green-false-pass, feedback_ci_killed_runner_flake]
rule_refs: [CLAUDE.md §5.3, CLAUDE.md §6 workflow-discipline]
---

# F75 — a timed-gate test's "slow" fixture must be slow by CONSTRUCTION, not by a best-effort optimizer barrier

## One-line

The L2.perf-gate test (`production_loop_perf_gate.rs`, #164) needs a
deliberately-SLOW translated function so the gate has something to Reject. The
first version made it slow with a ~5M-iteration `std::hint::black_box`
accumulator loop whose result was **discarded** (`black_box(acc); n + 1`).
`rustc -O` **intermittently dead-code-eliminated the whole loop** (acc was
ultimately unused), collapsing the "slow" `incr` to `n + 1` → median 0 ns →
ratio = +∞ → the slow emission **flakily PASSED** the gate.

## How it manifested

The build agent's own runs + one audit lens (gate-logic) saw the loop survive
and declared the test non-flaky. The **reliability audit lens** ran it 5× +
5× serially → **0/10 recorded the correct verdict** in those runs: the test's
own stdout printed `SLOW incr: cobrust=0ns cpython=84ns ratio=inf pass=true`
and the repair test got `repair_attempts=0` (expected 1).

**Mechanism, proven by wall-time correlation:**
- FAILED runs: SLOW median `0 ns` (or `41 ns`), test wall `0.5–0.7 s` → loop elided.
- PASSED runs: SLOW median `~10.5 ms`, test wall `6–7 s` → loop executed.

A genuine 10 ms loop cannot measure 0 ns; the 0 ns is proof the loop was
compiled away in that build. `std::hint::black_box` is documented as a
**best-effort** optimization barrier with **no correctness guarantee** — when
its output is discarded, LLVM may elide the producer.

## Fix

Make the slow path slow **by construction**, with a mechanism the optimizer
**cannot** touch:

```rust
pub fn incr(n: i64) -> i64 {
    std::thread::sleep(std::time::Duration::from_micros(200)); // syscall — un-elidable
    n + 1
}
```

`sleep` is a syscall, never DCE'd → ~200 µs/call DECISIVELY > CPython's ~84 ns
(ratio ~0.0004 ≪ 0.8, ~2000× margin) on EVERY build, deterministically.
Re-verified **10/10 PASS** with the slow side genuinely slow (not 0 ns).
(Alternatives that are also optimizer-proof: an observable side-effect such as
an `AtomicI64::store` of the loop result, or writing to a real sink. A sleep is
the simplest for a gate-MECHANISM test, which times per-call wall-time.)

A second, FUNCTIONAL bug the swap exposed: the repair test asserted the slow
body did not survive via `!final_emission.contains("5_000_000")` — after the
swap to `sleep` that string is gone, so the assertion was **vacuous** (always
true). Re-pinned to `!contains("sleep")` / `!contains("from_micros")` (the
fast `n + 1` body has neither, so it is now a real check). Sibling of F36
(fixture name/marker vs behavior drift).

## Lesson (the rules)

- **A "deliberately slow/expensive" test fixture must be slow by CONSTRUCTION.**
  Never rely on `std::hint::black_box` (or `volatile` reads, or "the optimizer
  probably won't remove this") to KEEP work the program otherwise discards —
  `black_box` is best-effort. Use a syscall (`sleep`), an observable
  side-effect (atomic store / I/O), or make the result genuinely flow into an
  externally-observed output.
- **A timed test's "I ran it N times green" is NOT a non-flaky proof when one
  lens got lucky.** The gate-logic lens declared non-flaky from runs where the
  loop happened to survive. Reliability needs (a) repeat under VARIED load, and
  (b) a MECHANISM check — here the wall-time/measured-ns correlation proved the
  loop was *elided*, not merely "a run failed". Bake a wall-time-vs-measured
  sanity assert into timed fixtures (a 10 ms loop reading 0 ns is impossible →
  fail loudly rather than silently mis-verdict).
- **When you change a fixture's body, re-audit every assertion that matched the
  OLD body by string** — a `contains("<old-marker>")` check silently goes
  vacuous (F36 family).

## Process note (the win)

The 2-lens workflow audit with **deliberately different scopes** caught this:
gate-logic lens (deep on the verdict logic, mutation-proved the Reject is
real) PASSED, reliability lens (ran it repeatedly + correlated wall-time)
FAILED. Neither alone would have shipped a reliable gate. Keep timed/perf
workflow audits split into a logic lens AND a reliability lens.
