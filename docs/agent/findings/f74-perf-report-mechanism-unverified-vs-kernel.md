---
finding_id: f74
title: "Perf-report mechanism shipped confidently-WRONG, unverified against the kernel"
status: candidate
date: 2026-05-31
severity: medium
surface: docs/agent/benchmarks/coil-mean.md
introduced_commit: eecb740
detected_commit: HEAD
siblings: [f35-sibling-commit-msg-vs-diff-drift, f36-fixture-name-vs-behavior-drift, f44-ci-cache-stale-green-false-pass]
rule_refs: [CLAUDE.md §5.2]
---

# F74 — A perf report's MECHANISM is a CLAIM that requires KERNEL-READ verification, not inference from the backend crate's identity

## One-line

The `coil-mean.md` report (commit `eecb740`) shipped a confidently-stated but
**factually wrong** mechanistic explanation — "coil's reduce backend is
ndarray's `ArrayBase::mean`, a scalar fold" — because the Build agent and BOTH
audit lenses verified the *numbers'* honesty, the cabi symbol, and the
same-value guard, but **none read `reduce.rs` to verify the "why these numbers"
mechanistic claim**.

## What the report CLAIMED (eecb740)

Across §1, §2 (hardware-tag row "coil reduce backend"), §4.2, §4.3, §4.4 the
report asserted:

- coil's `mean` dispatches to **ndarray's `ArrayBase::mean()`**, described as a
  **"scalar `fold`"** — "one f64 add per element, no SIMD, no pairwise tree".
- The `T3/T1` loss at scale (~13× at N=1e6) was therefore root-caused to
  "ndarray-scalar-fold vs numpy-SIMD-pairwise-sum".

## What the kernel ACTUALLY does (verified by reading the code)

Trace `cabi → wrapper → backend`:

1. `__cobrust_coil_mean(a)` (`cabi.rs:344`) — null-check, borrow, then
   `mean_scalar(arr_ref)`.
2. `mean_scalar` (`aggregates.rs:66`) → `reduce::mean(a, None)`.
3. `reduce::mean(_, None)` → `mean_all` (`reduce.rs:441`).
4. `mean_all` for `Array::Float64` did (PRE-this-sprint):
   `let v: Vec<f64> = a.iter().copied().collect(); pairwise_sum_f64(&v) / n`.

So the real kernel is:

- an **O(N) collect-copy** into a fresh `Vec<f64>`, then
- a **recursive (leaf-8, ADR-0016 §3) pairwise summation** — `pairwise_sum_f64`
  (`reduce.rs:67`).

`grep` for `.mean()` / `ArrayBase::mean` across `crates/cobrust-coil/src/`
returns **zero hits**. ndarray's `.mean()` is **never called anywhere in coil**.
coil **already does pairwise summation** (the same algorithm family numpy uses);
it is NOT a scalar fold.

The true gap vs numpy is therefore:
- (a) the O(N) collect-copy — and crucially, that copy iterates via ndarray's
  generic **N-dimensional** iterator (`ArrayD::iter()` computes per-element
  stride/index bookkeeping, NOT a flat pointer bump) AND allocates an N-sized
  `Vec`; and
- (b) the *recursion / leaf-8* pairwise tree's autovectorisation vs numpy's
  block-128 SIMD-unrolled pairwise loop.

Both of these are completely different from the claimed "scalar fold", even
though the *direction* of the conclusion (numpy's SIMD pulls away at scale)
happened to land in the right ballpark — which is exactly why it slipped review:
**a plausible-and-direction-correct mechanism masks a factually-wrong one.**

The empirical sizing of (a) vs (b) is itself a lesson the eecb740 review could
not have produced *because it never read the kernel*: see "How big was the WIN,
actually" below — term (a) turned out to **dominate** at every benched size
(removing it is a 3–6× speedup), the opposite of what a naive "a bulk copy is
cheap" guess (including this finding's own first draft) assumed. Reading the
kernel is what surfaces that the "copy" runs through the N-D iterator, not a
`memcpy`.

## Why the gate missed it

The eecb740 dispatch + audit verified, correctly and thoroughly:
- (c) honesty rule: the `SAME_VALUE_GUARD` asserts T2==T3==closed-form==numpy
  before timing (mutation-proven real);
- the `__cobrust_coil_mean` cabi symbol exists and borrows-only;
- the numbers are warm-captured medians, hardware-tagged, reproducible.

Every one of those is a property of the *measurement apparatus* and the
*numbers*. **None of them touches the prose answer to "why are the numbers what
they are".** The mechanism sentence was inferred from "the backend crate is
ndarray, and ndarray-the-crate has an `ArrayBase::mean`" — a guess about which
backend API the wrapper calls, never checked against the wrapper's actual body.

## Lesson (the rule)

A perf report has two separable claim classes:

1. **The numbers** (apparatus honesty): warm/median/same-work/hardware-tagged/
   reproducible. The existing honesty rules (a)–(e) + the audit's mutation
   proofs cover this.
2. **The mechanism** ("why these numbers"): a causal claim about *which code
   path runs and what it does*. This is a CLAIM and must be **verified by
   reading the actual kernel the tier calls — trace `cabi → wrapper → backend`
   and read the function body**, not inferred from the backend crate's identity.

This is the direct sibling of the CLAUDE.md §5.2 instruction to write the
"why these numbers" section "by reading the kernel and counting its
allocations/passes". The eecb740 report wrote that section without reading the
kernel — it counted the allocations/passes of a *guessed* kernel
(`ArrayBase::mean`) rather than the *real* one (`collect` + recursive pairwise).

## Resolution

1. **Kernel WIN (this sprint):** `sum_all` + `mean_all` Float32/Float64
   same-dtype arms now sum directly over `a.as_slice()` for the contiguous case
   (eliminating the O(N) collect-copy); the collect is retained as the
   non-contiguous-view fallback so behaviour is bit-identical. Same
   `pairwise_sum_*` over the same elements. The measured speedup is **large**
   (T2 ~2.5×/4.4×/5.8× across N=100/10 000/1 000 000; headline `T3/T1` at N=1e6
   fell 13.56× → 2.30×) — term (a) DOMINATED, because that `collect` ran
   ndarray's generic N-D iterator (per-element stride/index bookkeeping), NOT a
   flat `memcpy`.
2. **Report corrected:** `coil-mean.md` §1/§2/§4 now state the TRUE mechanism
   (collect + recursive pairwise, NOT `ArrayBase::mean`/scalar-fold), name this
   WIN as removing the contiguous collect, and name the residual recursion/
   leaf-8-non-SIMD term as the follow-up (a flat chunked-accumulator pairwise +
   possibly `wide`/`std::simd`).
3. **Recurrence prevention:** `docs/agent/benchmarks/README.md` §2 gains honesty
   rule **(f)**: the mechanistic explanation MUST be verified by reading the
   actual kernel the tier calls (trace `cabi → wrapper → backend`), not inferred
   from the backend crate's identity; the audit reads the kernel and confirms
   the report's "why" matches the code.

## Honest note on the WIN's magnitude (a SECOND instance of this finding's own lesson)

This finding's *primary* value is the process correction, but the WIN it bundled
turned out **large**, not the "noise-level" effect first guessed. The controlled
re-bench (OLD eecb740 kernel vs NEW, same M1, same session) measured a **T2
speedup of ~2.5× / 4.4× / 5.8×** at N=100 / 10 000 / 1 000 000, dropping the
headline `T3/T1` at N=1e6 from **13.56× to 2.30×**. Term (a), the collect-copy,
*dominated* — not term (b).

That mis-prediction is itself a **second instance of exactly the failure this
finding documents**: the WIN-scoping dispatch (and this finding's own first
draft) *inferred* "a contiguous collect is a cheap bulk copy, so removing it is
noise-level" — without reading what `ArrayD::iter().copied().collect()` actually
does. It does **not** lower to a `memcpy`: it drives ndarray's generic
N-dimensional iterator, which recomputes per-element stride/index bookkeeping on
every element. *Measuring* the real kernel (here, the controlled OLD-vs-NEW
re-bench) is what corrected the guess — the same "verify against the kernel, do
not infer" discipline, applied to the optimization's own payoff prediction. The
collect elimination is correct, worth keeping, and large; the residual ~2.3× vs
numpy at N=1e6 is term (b) (recursion/leaf-8 vs numpy's block-128 SIMD), the
named follow-up.
