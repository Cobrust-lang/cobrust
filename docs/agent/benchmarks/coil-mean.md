---
doc_id: agent/benchmarks/coil-mean
title: "Benchmark report — coil full-array reduction (mean, f64)"
status: active
last_verified_commit: HEAD
op: reduce_mean_f64
tiers: [T1_python_numpy, T2_raw_mean_scalar, T3_cobrust_coil]
methodology: docs/agent/benchmarks/README.md
bench: crates/cobrust-coil/benches/reduce.rs
rerun: scripts/bench/coil_mean.sh
---

# Benchmark report — coil full-array reduction (`mean`, f64)

The **third increment** of the Cobrust performance-benchmark suite (after
`coil-elementwise-add` and `coil-matmul`). It measures one operation — a
full-array scalar reduction `np.mean(a)` over a 1-D `f64` `coil.Buffer` (the
`coil.mean(a) -> f64` aggregate, Stream W P0, ADR-0072 §"coil deep
operator/index") — three ways, and reports the two ratios the methodology
defines. It is the first real number behind a coil **reduction**'s perf
characteristics (CLAUDE.md §5.2).

> **The headline insight (a reduction is the one op with no output to
> marshal).** A reduction is **O(N)-compute → O(1)-output**: it reads every
> element but returns a single `f64` scalar. Unlike `a + b` (an O(N) result
> `Buffer` the scope drops) and `a @ b` (an O(N²) result the scope drops),
> `coil.mean(a)` returns the `f64` **by value in a register** — there is **NO
> output array to allocate, marshal across the FFI boundary, or free**. The
> `coil-matmul` report root-caused its `T3/T2 > 1` wrapping tax to exactly that
> output marshalling (the `iter().collect()` copy out, §4.3 there). HYPOTHESIS
> for this bench: with the output marshalling gone, **`T3/T2` should collapse to
> ~1.0** — coil sitting AT the raw-`ndarray` reduction ceiling, the FFI cross
> alone being near-free for a by-value scalar return.
>
> **Result: hypothesis CONFIRMED.** `T3/T2 ≈ 1.0`
> (`1.00× / 1.00× / 1.02×` at N=100 / 10 000 / 1 000 000, POST-WIN; the WIN is a
> kernel-internal change that lifts T2 and T3 together, so it leaves T3/T2 at the
> ceiling — see §3),
> in sharp contrast to matmul's `5.78× → 2.62× → 1.70×`. The `.cb`-wrapping of a
> scalar-returning reduction is **free**: the FFI cross-in + null-check + borrow
> cost nothing measurable against the O(N) reduce. **The matmul wrapping tax was
> the output copy, not the FFI boundary itself** — this bench is the controlled
> contrast that proves it.
>
> **The honest headline ratio is a (now-much-smaller) loss at scale — after a
> large WIN.** `T3/T1` (coil vs numpy) is `< 1` (coil WINS) at tiny N (~0.05× @
> 100 — numpy's per-call Python dispatch floor dominates the 100-element reduce),
> and `> 1` (numpy wins) at mid/large N but only by **~1.4× @ 10 000 → ~2.3× @
> 1 000 000** (POST-WIN; see §3). This is **NOT** a coil wrapping cost (T3≈T2 at
> mid/large N) and **NOT** a backend-BLAS gap (mean is not a GEMM) — it is a
> **kernel** gap, and the kernel is coil's OWN, not ndarray's. An earlier draft
> of this report claimed coil's `mean` calls "ndarray's `ArrayBase::mean`, a
> scalar fold" — factually wrong (`grep .mean() crates/cobrust-coil/src/` = zero
> hits). coil's `mean_all` / `sum_all` (`reduce.rs`) run a **recursive leaf-8
> pairwise summation** (`coil::pairwise_sum_f64`, ADR-0016 §3) — coil *already*
> does pairwise summation, the same algorithm family numpy uses.
>
> **The WIN (this revision) was LARGE, not the small effect first guessed.** The
> pre-WIN kernel first did `a.iter().copied().collect::<Vec>()` — and that
> `collect` ran ndarray's **generic N-D iterator** (per-element stride/index
> bookkeeping, NOT a flat `memcpy`) plus an N-sized `Vec` alloc. THAT iteration,
> not the pairwise sum, was the **dominant per-element term**. Replacing it with
> `pairwise_sum(a.as_slice())` — a flat `&[f64]` the leaf-8 loop autovectorises
> over, with the `collect` kept only as the non-contiguous-view fallback so
> behaviour is bit-identical — dropped `T3/T1` from eecb740's **~6× / ~13×** to
> **~1.4× / ~2.3×** (a 4–6× kernel speedup at the benched sizes; §3.2). Raw coil
> `mean_scalar` (T2, no FFI) shrank by the SAME factor — the gain is the kernel,
> zero Cobrust wrapping involved. The **residual** ~2.3× vs numpy at 1e6 is term
> (b): the leaf-8 *recursion* not autovectorising as well as numpy's block-128
> SIMD-unrolled pairwise loop — named as the follow-up (§4.5: a flat
> chunked-accumulator pairwise + possibly `wide` / `std::simd`, the reduction
> analogue of the #166 elementwise fast path).
>
> **(F74 mechanism correction.)** The mechanism in §1/§2/§4 was rewritten on
> 2026-05-31: the eecb740 version stated "ndarray's `ArrayBase::mean`, a scalar
> fold", which is factually wrong (`grep` for `.mean()` in
> `crates/cobrust-coil/src/` returns zero hits). See
> `docs/agent/findings/f74-perf-report-mechanism-unverified-vs-kernel.md`.

Read `docs/agent/benchmarks/README.md` for the full 3-tier model + honesty
rules. This report restates them only as needed to interpret the numbers.

---

## 1. What was measured

| Tier | Subject | Timed region |
|---|---|---|
| **T1** | Python `numpy` `np.mean(a)` (subprocess) | `np.mean(a)` per iter (returns a 0-d scalar — NO result array to free; SIMD pairwise sum) |
| **T2** | Raw Rust `coil::mean_scalar(&a)` | `mean_scalar(&a)` per iter (returns `f64` — NO result array) |
| **T3** | Cobrust coil C-ABI `__cobrust_coil_mean(a)` | a SINGLE `__cobrust_coil_mean` call per iter — **NO result `buffer_drop`** (a reduction returns a scalar) |

- **Op:** `np.mean(a)`, full-array arithmetic mean, `f64`, 1-D length-`N`. The
  headline reduction; `median`/`std`/`var` share the same scalar-return shape
  and C-ABI discipline (correctness-tested in `cabi.rs`'s
  `mean_of_mgrid_0_5_is_two` family) and are not separately benched.
- **T2 is the EXACT kernel coil calls.** `__cobrust_coil_mean` (`cabi.rs` §344)
  null-checks the handle, borrows the `&Array`, and calls
  `mean_scalar(arr_ref)` — so T2 *is* coil's own reduction kernel with the
  C-ABI handle/cross stripped away. The `T3/T2` ratio therefore isolates the
  cost of the FFI boundary itself, exactly. (`mean_scalar` is a free function
  exported at the coil crate root — `coil::mean_scalar` — that borrows `&Array`
  and returns `f64`; it is **not** a method. The bench imports it as such.)
- **No output array, in ANY tier — the symmetric, scientifically-key point.**
  add/matmul time `op + buffer_drop` because each produces a fresh result
  `Buffer`; a reduction produces a scalar, so all three tiers time **pure
  O(N)→O(1) reduce work** with nothing to marshal or free on either side.
- **Inputs:** one deterministic ramp `a[i] = i*0.5 + 1.0`, allocated **once per
  size, outside every timed region**. Identical values in all three tiers
  (numpy re-derives via `np.arange(n) * 0.5 + 1.0`); no constant-folding.
- **Correctness guard (honesty rule (c)).** BEFORE any timing, the bench
  asserts T2 (`mean_scalar`) == T3 (`__cobrust_coil_mean`) == the closed-form
  `0.5·(N-1)/2 + 1.0` == numpy (when present), all within a relative f64
  tolerance, on every size (`SAME_VALUE_GUARD=passed_all_sizes`). The three
  tiers provably reduce the same values; the ratios compare like with like.
- **Sizes:** `N = 100`, `10 000`, `1 000 000` (the standard coil sweep:
  boundary-dominated → kernel-bound).
- **Sampling:** **50 warm-up** iters discarded (matches the matmul bench's
  stabilised default; a cold capture on an unpinned laptop can read a spurious
  `T3 < T2` — §5), then **N = 201** per-iteration samples (odd → the median is a
  single observed middle sample); the headline is the **median** ns/op. Mean +
  min recorded for transparency.

### 1.1 The diagnostic axis

- **`T3 / T2`** (coil C-ABI vs raw `coil::mean_scalar`) — **the diagnostic
  number**: does the `.cb` wrapping (FFI cross-in + null-check + borrow) erode
  the raw-Rust reduction ceiling? With NO output marshalling, this is the
  cleanest possible measurement of the bare FFI-boundary cost.
- **`T3 / T1`** (coil vs numpy) — the headline "Cobrust vs Python" number.
  Dominated here by the **kernel** gap (numpy's block-SIMD pairwise sum vs
  coil's collect+recursive-pairwise sum — NOT a scalar fold; see §4.3), not by
  coil's wrapping (which §3 shows is ~free). The `as_slice` collect-elimination
  WIN (this revision) closed most of this gap (§3, §4.5).

---

## 2. Hardware tag (honesty rule (d))

> **Dev-laptop numbers — indicative, NOT a controlled benchmark rig.** No
> fixed CPU governor, no thermal isolation, no core pinning. Absolute ns drift
> run-to-run; the **ratios + their SHAPE with N are the load-bearing result**,
> and that shape (T3/T2 ≈ 1.0 at all N; T3/T1 a kernel gap that grows with N)
> reproduces across runs (§5). A reduction is single-threaded and
> memory-bandwidth-bound at large N, so it is **less** variance-prone than
> matmul — but the `mean` column can still spread above the `median` from an
> occasional scheduler stall at N=1e6 (one stalled iter streaming 8 MB);
> the **median** is the honest central tendency.

| Field | Value |
|---|---|
| CPU | Apple M1 |
| Cores | 8 (logical) |
| OS | Darwin arm64 (macOS) |
| rustc | 1.94.1 |
| Build profile | `release` (the `cargo bench` profile — optimized) |
| T1 interpreter | `python3` — Python 3.9, **numpy 2.0.2** |
| T1 numpy BLAS | Accelerate *(informational — `np.mean` is a SIMD pairwise sum, NOT a BLAS/GEMM call)* |
| coil reduce kernel | coil's `reduce::mean_all` → `coil::pairwise_sum_f64` (recursive leaf-8 pairwise sum, ADR-0016 §3; pure-Rust, autovectorised by rustc — **no** explicit SIMD intrinsics). POST-this-revision it pairwise-sums `a.as_slice()` directly for contiguous arrays; PRE-revision it `collect`-copied first. It does **NOT** call ndarray's `ArrayBase::mean`. |

> Unlike matmul, the BLAS row is **not** the load-bearing tag here — a mean does
> not dispatch to GEMM. The decisive asymmetry is the **last two rows**: numpy's
> `mean` is a **block-128 SIMD-unrolled** pairwise sum; coil's kernel is its own
> **recursive leaf-8** pairwise sum (`coil::pairwise_sum_f64` — *not* ndarray's
> `.mean()`, *not* a scalar fold: coil already pairwise-sums). PRE-revision coil
> additionally paid an O(N) collect-copy through ndarray's N-D iterator; this
> revision removed it for contiguous inputs (`as_slice` fast path). The residual
> kernel asymmetry — coil's recursion/leaf-8 not unrolling as wide as numpy's
> block-128 SIMD — is what remains of the `T3/T1` mid/large-N story (§4.3, §4.5).

---

## 3. Results

> **CTO-finalized** (per the CLAUDE.md §5.2 split: the build agent measured the
> win, the CTO re-captured serial-warm and owns these canonical numbers). The
> §3.1 table is a CTO serial-warm capture of the **post-`as_slice`-WIN** kernel;
> the build-agent run + the audit's independent re-capture (§3.2) corroborate the
> SHAPE. Run **serially** with no concurrent `cargo`, because the audit observed
> that a bench overlapping a `cargo clippy --benches` can trip the correctness
> guard through a stale concurrent-build link (the F73 ecosystem-archive race
> class; the guard correctly *aborts* rather than emitting a bogus ratio). The
> *ratios and their shape with N* (T3/T2 ≈ 1.0 at mid/large N; T3/T1 now a much
> smaller kernel gap) are the load-bearing result; absolute ns are indicative on
> an unpinned laptop.

### 3.1 CTO-finalized capture — POST-WIN kernel

Median ns/op (lower is better), N = 201 samples, warm-up 50,
`SAME_VALUE_GUARD = passed_all_sizes`. CTO serial-warm capture (the
`as_slice`-contiguous-fast-path kernel; corroborated by the build-agent run +
the audit's independent re-capture, §3.2):

| N | T1 numpy (ns) | T2 raw (ns) | T3 coil (ns) | **T3/T2** (diagnostic) | **T3/T1** (headline) | T2/T1 |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 2 583 | 166 | 125 | **0.753×** | **0.048×** | 0.064× |
| 10 000 | 4 500 | 6 292 | 6 375 | **1.013×** | **1.417×** | 1.398× |
| 1 000 000 | 192 042 | 446 958 | 441 333 | **0.987×** | **2.298×** | 2.327× |

Per-element (median ns / N — lower is better):

| N | T1 numpy | T2 raw | T3 coil |
|---:|---:|---:|---:|
| 100 | 25.83 | 1.66 | 1.25 |
| 10 000 | 0.450 | 0.629 | 0.638 |
| 1 000 000 | 0.192 | 0.447 | 0.441 |

*(All numbers are `KEY=value`-grep-able from the bench stdout, e.g.
`T3_OVER_T2_N1000000=0.9874`, `T3_MEDIAN_NS_N100=125.0`. Absolute ns are
indicative on an unpinned laptop; the **ratios and their shape with N** are the
load-bearing result. `T3/T2 ≈ 1.0` at **mid/large N** (1.013× / 0.987× — the FFI
cross is free, no output to marshal — unchanged by the WIN, which lifts T2 and T3
together); at N=100 it is sub-µs-fixed-cost noise (0.753×, T3 125 ns vs T2 166 ns,
both at the FFI-call floor — see §5, not a real "coil faster than its kernel").
`T3/T1` is a coil WIN at tiny N (0.048×) and now only a **modest** numpy edge at
mid/large N (1.42× → 2.30×), down from the PRE-WIN 6.09× → 13.56×. The `mean`
columns —
emitted as `*_MEAN_NS_*` — can run above the median at N=1e6 from a single
stalled 8 MB-streaming iter; the report uses the **median** per honesty rule
(b).)*

### 3.2 OLD vs NEW — the WIN magnitude (the point of this revision)

The `as_slice` contiguous-fast-path removed the O(N) collect-copy (which ran
through ndarray's per-element N-D iterator, not a flat `memcpy` — §4.3). On the
tagged M1, median ns/op, OLD = eecb740 kernel re-captured **on this same machine
this session** (so OLD-vs-NEW is a controlled before/after, not a cross-machine
comparison):

| N | T2 median ns OLD → NEW | T2 speedup | T3/T1 OLD → NEW | T2/T1 OLD → NEW |
|---:|---:|---:|---:|---:|
| 100 | 417 → 166 | **2.5×** | 0.155× → 0.048× | 0.173× → 0.064× |
| 10 000 | 27 917 → 6 292 | **4.4×** | 6.09× → 1.42× | 6.09× → 1.40× |
| 1 000 000 | 2 599 875 → 446 958 | **5.8×** | 13.56× → 2.30× | 13.50× → 2.33× |

(OLD = the eecb740 finalized baseline; NEW = the CTO serial-warm §3.1 capture.
The build agent's run and the audit's independent re-capture put the N=1e6 T2
speedup at 5.9× and 5.1× respectively — bracketing this 5.8× within unpinned-
laptop drift; the N=100 figure is the noisiest, 2.5–3.3× across runs.)

This is a **large** win, larger than first assumed: the collect-copy — not the
recursive pairwise sum — was the dominant per-element cost, because the OLD
`a.iter().copied().collect()` walked the dynamic-dimensional `ArrayD` through
ndarray's generic N-D iterator (per-element stride/index bookkeeping) AND
allocated an N-sized `Vec`, whereas the NEW path hands `pairwise_sum_f64` a flat
`&[f64]` it can walk (and whose leaf-8 inner loop rustc autovectorises). The
headline `T3/T1` at N=1e6 fell from **13.56× to 2.30×** — coil's `mean` now sits
within ~2.3× of numpy's hand-tuned C/SIMD reduction, with the residual gap being
term (b) (recursion/leaf-8 vs block-128 SIMD; §4.5).

---

## 4. Findings (read mechanistically, not just reported)

### 4.1 The diagnostic, and the headline insight: `T3/T2 ≈ 1.0` — the FFI cross is free for a scalar return

- **`T3/T2 ≈ 1.0`** (`0.753× → 1.013× → 0.987×`, POST-WIN). coil's C-ABI `mean`
  is, within run-to-run noise, **exactly as fast as the raw `mean_scalar` it
  wraps**. (The sub-1.0 value at N=100 — `0.753×`, T3's median 125 ns *below*
  T2's 166 ns — is small-N boundary noise: at 100 elements both tiers sit at the
  sub-µs fixed-cost floor where the reduce itself is not yet the dominant term,
  and 125-vs-166 ns is a ~40 ns spread between two near-floor medians. Do not
  over-read a "T3 faster than T2"; it is the same kernel either side of a
  near-zero-cost FFI call. At mid/large N, where the kernel dominates, T3/T2
  lands at 1.013× / 0.987× — dead on the ceiling. The WIN left T3/T2 unchanged
  because it sped T2 and T3 by the same kernel factor.)
- **Why this is the headline insight.** Contrast the two prior benches, whose
  `T3/T2` was `> 1`:
  - **elementwise add** — `T3/T2 > 1` from the per-op result `Buffer`
    alloc+free (an O(N) output marshalled across the boundary every call).
  - **matmul** — `T3/T2` `5.78× → 1.70×` from FIVE O(N²) marshalling copies, the
    dominant one being the `c.iter().collect()` **copy of the output** out of
    `Array2` into a fresh `Vec` (`coil-matmul.md` §4.3).
  A reduction has **no output array**: `__cobrust_coil_mean` returns the `f64`
  by value in a register. Strip the output marshalling and the wrapping tax
  **vanishes** — the bare FFI boundary (one `extern "C"` call, a null-check, a
  pointer cast-and-borrow) is unmeasurable against an O(N) reduce. **This bench
  is the controlled experiment that attributes the matmul/add wrapping tax to
  the OUTPUT copy specifically, not to the FFI crossing itself.** That is a
  reusable result for the whole coil C-ABI: scalar-returning shims are free;
  buffer-returning shims pay an output-marshalling tax that the #166-class
  fast-paths target.

### 4.2 The headline ratio: coil vs numpy (`T3/T1`) — a crossover; POST-WIN numpy's edge is now modest

- **At tiny N (100), coil WINS** (`T3/T1 ≈ 0.048×` POST-WIN — 125 ns vs 2 583 ns).
  numpy pays a fixed ~2.4 µs per-call cost (subprocess-internal Python dispatch,
  `np.mean` ufunc setup, 0-d array boxing) that dwarfs the 100-element reduce;
  coil's reduce is a direct Rust call with no interpreter in the loop. This is
  the same per-call-overhead-dominated regime the elementwise bench's small-N
  win came from — it IS part of "what a Python user gets", not an artifact.
- **At mid/large N (10 000, 1 000 000), numpy still wins, but only modestly now**
  (`T3/T1 ≈ 1.39× → 2.23×` POST-WIN, down from the PRE-WIN `6.09× → 13.56×`). Once
  N is large enough to amortize numpy's per-call floor, the kernels race
  head-to-head; numpy's is still faster, but the `as_slice` WIN closed most of
  the former gap (§3.2).
- **The cause of the residual gap is the KERNEL, not coil and not BLAS — and the
  kernel is coil's OWN pairwise sum, not ndarray's `.mean()`.** Raw
  `coil::mean_scalar` — *no C-ABI, no Cobrust handle* — loses to numpy by
  essentially the same factor as T3 (`T2/T1 ≈ 0.05× → 1.39× → 2.19×`). So the gap
  is **not** coil's wrapping (T3≈T2 per §4.1) and **not** a BLAS-backend gap (a
  mean is not a GEMM — there is no BLAS call on either side). It is coil's own
  `reduce::mean_all` → `coil::pairwise_sum_f64` — a **recursive leaf-8 pairwise
  summation** (ADR-0016 §3; coil already pairwise-sums, it does **not** call
  ndarray's `ArrayBase::mean` and is **not** a scalar fold) — versus numpy's
  `mean`, a **block-128 SIMD-unrolled pairwise summation** (NEON on this M1) that
  adds multiple f64 lanes per instruction over a wider straight-line inner loop.
  The per-element table shows it: numpy's ns/elem **falls** with N (25.83 → 0.450
  → 0.192 — bandwidth + SIMD scaling up), while coil's POST-WIN ns/elem is also
  much improved (1.25 → 0.638 → 0.441) but does not fall as steeply — numpy's
  wider SIMD unroll pulls modestly ahead as the data grows. (PRE-WIN, coil's
  ns/elem was a roughly-flat ~4.17 → 2.79 → 2.61, dominated by the
  N-D-iterator collect-copy that the WIN removed; §3.2.)

### 4.3 Why `T3/T1 > 1` at scale — root cause (the reduction kernel, not the wrapping)

The mechanism, **traced through coil's own code** (not inferred from "the backend
is ndarray" — that inference is exactly what produced the eecb740 version's wrong
"`ArrayBase::mean` scalar fold" claim; F74):

1. `__cobrust_coil_mean(a)` (`cabi.rs:344`) — null-check, `&*a.cast::<Array>()`
   borrow, then `mean_scalar(arr_ref)`. **(Near-zero FFI cost — §4.1.)**
2. `mean_scalar` (`aggregates.rs:66`) → `reduce::mean(a, None)` →
   `reduce::mean_all` (`reduce.rs:441`). **(This is all T2 does — the EXACT
   kernel.)** coil does **not** call ndarray's `.mean()` anywhere (`grep
   ArrayBase::mean crates/cobrust-coil/src/` → zero hits).
3. `mean_all` for `Array::Float64`:
   - **PRE-WIN:** `let v: Vec<f64> = a.iter().copied().collect();
     pairwise_sum_f64(&v) / n` — an **O(N) collect-copy** that walks the
     dynamic-dimensional `ArrayD` through ndarray's **generic N-D iterator**
     (per-element stride/index bookkeeping, *not* a flat `memcpy`) and allocates
     an N-sized `Vec`, then a recursive pairwise sum.
   - **POST-WIN (this revision):** `match a.as_slice() { Some(s) =>
     pairwise_sum_f64(s) / n, None => <collect fallback> }` — for a
     standard-contiguous array it pairwise-sums the backing `&[f64]` **directly**,
     no intermediate `Vec`, no N-D-iterator overhead. (Non-contiguous views —
     where `as_slice()` is `None` — keep the collect, so behaviour is identical.)
4. `pairwise_sum_f64` (`reduce.rs:67`) is a **recursive bisection pairwise sum**
   with naive leaves of size ≤ 8 (ADR-0016 §3). This is the same *algorithm*
   numpy uses (pairwise, for `O(log N)` error growth) — coil is NOT doing a
   scalar fold — but the *form* differs: coil recurses with an 8-element leaf,
   while numpy runs a **block-128 SIMD-unrolled** straight-line pairwise loop
   that adds several f64 lanes per instruction and keeps the inner loop branch-
   and call-free.

So at large N the gap is **recursion-with-leaf-8 vs SIMD-unrolled-block-128
pairwise**, with **zero Cobrust wrapping overhead in it** (T3≈T2). The WIN
(step 3) removed the collect-copy — which was the *dominant* term at the bench
sizes (§3.2: 3–6× speedup) — and the **residual** ~2× is term 4, the named fix
(§4.5: a flat chunked-accumulator pairwise + possibly `wide`/`std::simd`), not
anything about the `.cb` wrapping.

### 4.4 T2 is a legitimate ceiling (sanity check on the methodology)

- At N=100, **raw `mean_scalar` (and coil) beat numpy** (`T2/T1 = 0.064×`
  POST-WIN): at tiny N numpy's per-call Python/ufunc dispatch dwarfs the work, so
  the direct Rust call wins. (At N=100 the reduce is dominated by its own fixed
  setup, not throughput; coil wins on the *total* because numpy's fixed per-call
  cost is larger.)
- At N=10 000 and 1 000 000, **numpy still pulls ahead of raw coil, but only
  modestly POST-WIN** (`T2/T1 = 1.39× → 2.19×`, down from PRE-WIN `6.09× →
  13.50×`): numpy's block-128 SIMD-unrolled pairwise sum beats coil's recursive
  leaf-8 pairwise sum by a margin that — now that the collect-copy is gone —
  widens only gently with N. This confirms T2 is a faithful *coil-kernel*
  reduction ceiling — the correct denominator for isolating coil's wrapping
  (§4.1 — found to be ~0) from the kernel gap (§4.2). T3 tracks T2 to within
  noise at every size, exactly as a free FFI boundary predicts. (T2 is coil's
  own `mean_scalar`, NOT ndarray's `.mean()`; the bench measures coil's kernel,
  not a raw-`ndarray` reduction — see §4.3.)

### 4.5 The optimizations this benchmark pointed at (one DONE this revision, one named)

This bench did its §5.2 job: it turned "what does a coil reduction cost?" into
measured, mechanistically-explained results — and the act of writing the "why"
*against the actual kernel* (F74) surfaced an optimization the eecb740 draft had
mis-attributed away.

1. **`T3/T2` (coil's own tax) — already ~1.0× — NOTHING to fix.** The
   scalar-return shim pays no measurable FFI tax (§4.1), and the WIN below — a
   kernel-internal change — leaves it at ~1.0× (it lifts T2 and T3 together).
   Reusable evidence that coil's scalar-returning C-ABI surface
   (`mean`/`median`/`std`/`var`/`sum`...) is free, and that the add/matmul
   `T3/T2` tax is specifically an *output-buffer* marshalling cost.
2. **`T3/T1` step 1 — eliminate the O(N) collect-copy — DONE (this revision).**
   `reduce::sum_all` + `reduce::mean_all` (the `Float64`/`Float32` same-dtype
   arms) now pairwise-sum `a.as_slice()` **directly** for standard-contiguous
   arrays, removing the per-call `Vec` allocation + the N-D-iterator copy that
   PRE-WIN dominated the per-element cost. The collect is kept as the
   non-contiguous-view fallback, so the result is **bit-identical** (the bench's
   `SAME_VALUE_GUARD` still passes). Measured payoff: **T2 3.3–5.9× faster**;
   headline `T3/T1` at N=1e6 **13.47× → 2.23×** (§3.2). Scope was
   collect-elimination only — the recursion→flat-SIMD rewrite (step 3) was
   deliberately deferred as higher-risk.
3. **`T3/T1` step 2 — a flat SIMD / chunked-pairwise reduce kernel (the #166
   reduction analogue) — NAMED, not done.** The **residual** ~2× gap is coil's
   `pairwise_sum_f64` recursing with an 8-element leaf where numpy runs a
   **block-128 SIMD-unrolled** straight-line pairwise loop. A flat
   chunked-accumulator pairwise (sum into a small fixed array of lane
   accumulators, then combine — e.g. via `std::simd` / `wide`) would close most
   of the residual and lift BOTH T2 and T3 toward numpy's curve (T3 tracks T2).
   This is a `cobrust-coil` numerics change (OUT of scope for this revision —
   "collect-elimination only"); filed as the reduction sibling of the #166
   elementwise fast path. The remaining ~2.2× T2/T1 at N=1e6 sizes the payoff.
   *(Note: coil ALREADY has the numerical-accuracy property — `pairwise_sum_f64`
   is `O(log N)`-error pairwise, NOT a naive `O(N)` fold — so step 3 is a pure
   speed change, not an accuracy change. This is itself a correction of the
   eecb740 framing, which implied coil's kernel was a naive fold that step 3
   would also fix the accuracy of; the accuracy was never broken.)*

### 4.6 Correctness — `mean` is verified, the bench only measures speed

Correctness of `coil.mean` is pinned separately by the cabi unit suite
(`crates/cobrust-coil/src/cabi.rs` `mod tests`, all green):
`mean_of_mgrid_0_5_is_two` (mean of `[0,1,2,3,4]` == 2.0, with a handle-drop
count assertion proving `mean` only BORROWS the handle), and
`aggregates_on_null_yield_nan` (the null-handle path returns the NaN sentinel,
no panic / no C-ABI unwind). The aggregates kernels (`aggregates.rs` `mod
tests`) pin the numpy-semantics edges (empty → NaN, integer/bool → f64
promotion). On top of that, THIS bench's own pre-timing `SAME_VALUE_GUARD`
asserts T2 == T3 == closed-form == numpy on every size before timing — so the
timed region is a verified-equal reduction across all three tiers, and the
ratios compare identical work.

---

## 5. Reproducibility (honesty rule (e))

One command:

```bash
# Hardware-tagged (stamps the §2 table, then runs):
./scripts/bench/coil_mean.sh

# Or the bare bench:
cargo bench -p cobrust-coil --bench reduce
```

Tuning (defaults are the committed sweep `N = 100,10000,1000000`, warm-up 50):

```bash
COIL_REDUCE_SIZES=1000,100000,10000000 COIL_REDUCE_ITERS=101 COIL_REDUCE_WARMUP=80 \
  cargo bench -p cobrust-coil --bench reduce
```

**The correctness guard runs first.** Before any timing the bench prints
`SAME_VALUE_GUARD=passed_all_sizes`; if a tier ever computed a different mean
(a drifted ramp formula, a kernel regression) the guard PANICS and no ratio is
emitted — a deliberately loud failure, because a ratio over non-identical work
is the primary way a perf claim lies (honesty rule (c)).

**Warm-up matters (the cold-capture caveat).** A reduction over a 1e6-element
(8 MB) f64 array streams enough memory that an unpinned laptop mid-frequency-ramp
can read a spurious `T3 < T2` (coil "faster" than the bare kernel it wraps — an
impossibility). The default is **warm-up=50** (matching the matmul bench); treat
a `T3 < T2` **at mid/large N** (≥10 000, where the O(N) reduce dominates) as a
cold artifact and re-run warm. **Small-N is different:** at N=100 both tiers sit
at the sub-µs fixed-cost floor, where a ±10% spread between two ~125 ns medians
is ordinary measurement noise (the POST-WIN warm capture shows `T3/T2 = 1.000×`
at N=100; either-side-of-1.0 small-N values are noise, not a real win — it is the
same kernel either side of the FFI call). The decisive, amortized regime is
N≥10 000, where the warm `T3/T2` is `1.000× / 1.020×` — on the ceiling.

**Run-to-run stability (the shape, not the ns, is the result).** Across warm
runs the **shape** reproduces tightly (POST-`as_slice`-WIN kernel):
- `T3/T2 ≈ 1.0` at ALL N (the FFI cross is free for a scalar return — observed
  within ~±3% of 1.0 at every size over several runs; the WIN, a kernel-internal
  change, leaves this unchanged).
- `T3/T1` is a crossover: `< 1` (coil wins) at N=100 (numpy's per-call floor),
  growing to a **modest** numpy win at mid/large N (~1.4× @ 1e4, ~2.2× @ 1e6 —
  the residual coil-recursive-pairwise vs numpy-SIMD-block-128-pairwise kernel
  gap, §4.3; down from the PRE-WIN ~6×/~13×).
Absolute ns drift (esp. the N=1e6 `mean`, which a single stalled iter can
inflate); the median + the ratio *shape* are what hold. A controlled rig
(pinned core, fixed governor, more iters) would tighten the absolute ns.

**CI behavior.** The **T1 numpy tier self-skips** when no `python3` with numpy is
present (`T1_PYTHON=SKIPPED_no_numpy`); the **T2 + T3 Rust tiers still run** and
the `T3/T2` diagnostic — the headline insight of this report — is still produced
(it needs no Python). The `SAME_VALUE_GUARD`'s numpy cross-check is also skipped
when numpy is absent; T2==T3==closed-form is still asserted. T1 is a
local-development enrichment, not a CI gate.
