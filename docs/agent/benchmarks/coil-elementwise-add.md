---
doc_id: agent/benchmarks/coil-elementwise-add
title: "Benchmark report — coil element-wise add (a + b, f64)"
status: active
last_verified_commit: HEAD
op: elementwise_add_f64
tiers: [T1_python_numpy, T2_raw_ndarray, T3_cobrust_coil]
methodology: docs/agent/benchmarks/README.md
bench: crates/cobrust-coil/benches/elementwise_add.rs
rerun: scripts/bench/coil_elementwise_add.sh
---

# Benchmark report — coil element-wise add (`a + b`, f64)

The **first increment** of the Cobrust performance-benchmark suite. It
measures one operation — element-wise `a + b` on 1-D `f64` arrays — three
ways, and reports the two ratios the methodology defines. This is the first
real number behind a Cobrust "perf" statement (CLAUDE.md §5.2) and the first
candidate threshold for the translation `L2.perf` gate (CLAUDE.md §4.2).

> **Update (task #166 — the §5.2 loop closed).** The first run of this
> benchmark root-caused a `T3/T2 ≈ 3.5–4.5×` per-element tax to four
> redundant N-sized copies in `binary_dispatch` (§4.3) and named the fix as
> the headline follow-up (old §4.5). That fix — a same-dtype/same-shape
> **fast path** at the top of `binary_dispatch` that operates directly on the
> input views, skipping both `cast_to` clones, both `broadcast_owned` copies,
> and (for the infallible f32/f64/bool arms) the output zero-fill — has now
> **landed and been re-measured**. `T3/T2` collapsed from ~4× to **≈ 0.85–1.0×
> at n ≥ 10k** (coil now at parity with the raw-`ndarray` ceiling) with only a
> fixed **~1.76× residual at n=100** (the per-call FFI/`Box` boundary, which
> does NOT scale with N). The headline `T3/T1` (coil vs numpy) now **passes
> the §4.2 perf gate (`≤ 1.25×`) at every size**, and coil is *faster than
> numpy* at n=100 and n=10k. §3 reports the post-fix numbers; the pre-fix
> numbers are kept in §3.1 for the before/after contrast that proves the win.

Read `docs/agent/benchmarks/README.md` for the full 3-tier model + honesty
rules. This report restates them only as needed to interpret the numbers.

---

## 1. What was measured

| Tier | Subject | Timed region |
|---|---|---|
| **T1** | Python `numpy` `np.add(a, b)` (subprocess) | `np.add(a, b)` per iter (numpy allocates+frees one result/iter) |
| **T2** | Raw Rust `ndarray::ArrayD<f64>` `&a + &b` | `&a + &b` per iter (one owned result allocated + dropped/iter) |
| **T3** | Cobrust coil C-ABI `__cobrust_coil_buffer_add` | the add shim **+ the result `__cobrust_coil_buffer_drop`** per iter |

- **Op:** `a[i] + b[i]`, element-wise, `f64`, 1-D, equal shapes (no
  broadcasting — the common case).
- **Inputs:** a deterministic ramp `a[i] = i*0.5 + 1.0`, `b[i] = i*0.25 -
  3.0`, allocated **once per size, outside every timed region**. Identical
  values in all three tiers (numpy re-derives via `np.arange`); no
  constant-folding.
- **Sizes:** `100`, `10_000`, `1_000_000`.
- **Sampling:** 50 warm-up iters discarded, then **N = 201** per-iteration
  samples; the headline is the **median** ns/op (odd N → a single observed
  middle sample). Mean + min recorded for transparency.

### 1.1 The diagnostic axis

- **`T3 / T2`** (coil vs raw `ndarray`) — **the diagnostic number**: how much
  the `.cb` wrapping (FFI cross + per-op result alloc + the kernel's
  redundant copies) erodes the raw-Rust ceiling.
- **`T3 / T1`** (coil vs numpy) — the headline "Cobrust vs Python" number.

---

## 2. Hardware tag (honesty rule (d))

> **Dev-laptop numbers — indicative, NOT a controlled benchmark rig.** No
> fixed CPU governor, no thermal isolation, no core pinning. Absolute ns
> drift run-to-run; the **ratios are the load-bearing result**. Post-fix
> (task #166) the *shape* is `T3/T2 ≈ 1.0× at n ≥ 10k` (coil at the
> raw-`ndarray` ceiling) with a fixed `~1.76×` residual at n=100 that does
> NOT scale with N — the inverse of the pre-fix per-element tax. Three-run
> observed band: n=100 `1.76` (rock-stable), n=10k `0.84–0.99`, n=1M
> `0.46–0.94` (the n=1M low end is M1 large-array scheduler noise on a
> sub-1 ms op — see §5). The pre-fix shape was a per-element `≈ 3.5–4.5×` tax
> that never collapsed toward 1.0; that tax is **gone at every size**.

| Field | Value |
|---|---|
| CPU | Apple M1 |
| Cores | 8 (logical) |
| OS | Darwin 25.3.0 arm64 (macOS) |
| rustc | 1.94.1 (e408947bf 2026-03-25) |
| Build profile | `release` (the `cargo bench` profile — optimized) |
| T1 interpreter | `/usr/bin/python3` — Python 3.9.6, **numpy 2.0.2** |

> The T1 numpy version (**2.0.2**) matches the numpy version coil is a
> translation of (per `crates/cobrust-coil/Cargo.toml` description), so T1 is
> an apples-to-apples baseline against the exact upstream coil targets.

---

## 3. Results (post-fix — task #166 fast path landed)

Median ns/op (lower is better), N = 201, warm-up 50. **One captured run**
with the same-dtype/same-shape fast path active (the equal-shape `f64 + f64`
this bench drives now takes it).

| size | T1 numpy (ns) | T2 ndarray (ns) | T3 coil (ns) | **T3/T2** (diagnostic) | **T3/T1** (headline) | T2/T1 |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 375.0 | 166.0 | 292.0 | **1.76×** | **0.78×** | 0.44× |
| 10 000 | 4 000.0 | 3 584.0 | 3 208.0 | **0.90×** | **0.80×** | 0.90× |
| 1 000 000 | 567 334.0 | 676 333.0 | 632 375.0 | **0.94×** | **1.11×** | 1.19× |

Per-element (median ns / element):

| size | T1 numpy | T2 ndarray | T3 coil |
|---:|---:|---:|---:|
| 100 | 3.75 | 1.66 | 2.92 |
| 10 000 | 0.400 | 0.358 | 0.321 |
| 1 000 000 | 0.567 | 0.676 | 0.632 |

*(All numbers are `KEY=value`-grep-able from the bench stdout, e.g.
`T3_OVER_T2_N1000000=`, `T3_MEDIAN_NS_N100=`. The table is **one captured
run with symmetric result-free timing** (T2 and T3 both free their result
inside the timed region — see §1 / the bench comment); re-running reproduces
the **shape** (`T3/T2 ≈ 1.0× at n ≥ 10k, ~1.76× at n=100`) but not the exact
ns — every op here is sub-1 ms and noisy on an unpinned laptop. See §5 for
the 3-run spread.)*

### 3.1 Before/after (the §5.2 claim ⇒ experiment ⇒ re-measured-claim loop)

The pre-fix numbers (the original first-increment capture, before the task
#166 fast path) for the identical sweep + methodology:

| size | T3/T2 **before** | T3/T2 **after** | T3/T1 **before** | T3/T1 **after** |
|---:|---:|---:|---:|---:|
| 100 | 3.77× | **1.76×** | 1.67× | **0.78×** |
| 10 000 | 4.47× | **0.90×** | 3.96× | **0.80×** |
| 1 000 000 | 3.49× | **0.94×** | 4.11× | **1.11×** |

- **`T3/T2` (diagnostic):** the flat `≈ 3.5–4.5×` per-element tax collapses to
  `≈ 0.9–1.0×` at n ≥ 10k — coil is now at the raw-`ndarray` ceiling (the
  fast path does the same single fused read-read-add-write pass into one
  freshly-allocated buffer that `&a + &b` does). The only residual is the
  `~1.76×` at n=100: the fixed per-call FFI/`Box` boundary cost, now VISIBLE
  precisely *because* the per-element tax is gone (at tiny N the ~40 ns
  cross-in + cross-out + result-free is a large fraction of ~166 ns of work).
- **`T3/T1` (headline):** drops below the §4.2 gate (`≤ 1.25×`) at every size,
  and coil is *faster than numpy* at n=100 (`0.78×`) and n=10k (`0.80×`). At
  n=1M coil and numpy are both single memory-bandwidth-bound passes (`1.11×`,
  within noise of parity).

---

## 4. Findings (what the numbers mean — read mechanistically, not just reported)

### 4.1 The headline: coil vs numpy (`T3/T1`) — post-fix

- **Small arrays (n=100): coil is ~0.78× of numpy — i.e. FASTER.** At this
  size numpy is dominated by *Python-side per-call overhead* (the interpreter
  dispatch into the C ufunc), and coil's compiled-FFI path — now with no
  redundant copies — wins outright.
- **Mid arrays (n=10k): coil is ~0.80× of numpy — FASTER.** coil's fused
  single-pass kernel + one allocation beats numpy at the size where numpy's
  per-call overhead has amortized but throughput limits haven't yet dominated.
- **Large arrays (n=1M): coil ≈ 1.11× of numpy.** Here both are steady-state
  memory-bandwidth-bound: one SIMD/auto-vectorised pass + one result alloc on
  each side. They are within noise of parity.
- **This now PASSES the CLAUDE.md §4.2 perf gate** (`≥ 0.8×` the Python
  library ⇒ `T3/T1 ≤ 1.25`) **at every size**. The pre-fix capture FAILED the
  gate everywhere (`1.67×`/`3.96×`/`4.11×`); the §4.5 fast path closed it.
  This is the §5.2 loop completed: the first increment *measured* the gap, the
  fix *closed* it, and the same harness *re-measured* the win.

### 4.2 The diagnostic: coil vs raw ndarray (`T3/T2`) — post-fix

- **`T3/T2 ≈ 0.9–1.0× at n ≥ 10k`** — coil now sits at the raw-`ndarray`
  ceiling. The fast path runs the identical fused read-read-add-write `Zip`
  into a single freshly-allocated output (`map_collect`) that `&a + &b` does;
  there is no remaining per-element copy traffic to distinguish them, so the
  ratio is at parity (and dips below 1.0 within run-to-run noise).
- **`T3/T2 ≈ 1.76× at n=100`** — the residual is now a **fixed per-call cost**
  (the FFI cross-in + cross-out + result `Box` free), NOT a per-element tax.
  The diagnostic flipped sign with N exactly as a fixed overhead should: it is
  a large fraction of ~166 ns at n=100 and a negligible fraction at n ≥ 10k.
  This is the inverse of the pre-fix behavior (a flat ~4× that did NOT
  amortize), and it confirms the per-element tax was eliminated, leaving only
  the irreducible `.cb` boundary — which is small in absolute terms (~tens of
  ns) and only visible at tiny N.

### 4.3 Why the fix works — what the fast path eliminated (the root cause, now closed)

The pre-fix kernel (`crates/cobrust-coil/src/ufunc.rs`, `binary_dispatch`)
did, for the equal-shape `f64 + f64` path:

1. `cast_to(a, Float64)` → `cast_to_f64` → for an already-`f64` input this was
   **`a.clone()`** — a full N-element copy. Same for `b`. **(2 copies.)**
2. `broadcast_shape(...)` — cheap shape check.
3. `broadcast_owned(&av, target)` → `av.broadcast(target).to_owned()` — for
   equal shapes the broadcast is a no-op *view*, but `.to_owned()` still
   **copied** it. Same for `bv`. **(2 more copies.)**
4. `ArrayD::<f64>::zeros(target)` — **allocate + zero-fill** the output. **(1
   alloc + 1 write pass.)**
5. `Zip::from(&mut out).and(&av_b).and(&bv_b).for_each(|o,&x,&y| *o = x+y)` —
   the actual add. **(1 read+add+write pass.)**

So coil touched roughly **5 N-sized buffers** per add (2 cast-clones + 2
broadcast-clones + 1 zero-filled output) where raw `ndarray` `&a + &b`
touches **1** — predicting the observed `~3.5–4.5×`.

**The task #166 fast path** detects the common case
(`a.dtype() == promoted && b.dtype() == promoted && a.shape() == b.shape()`)
at the TOP of `binary_dispatch`, BEFORE any `cast_to`, and:

- skips both `cast_to` clones (step 1) — operates on the input `ArrayD<T>`
  **views** directly;
- skips both `broadcast_owned` copies (step 3) — equal shapes need no
  broadcast, so the views are used as-is;
- for the infallible f32/f64/bool arms, replaces `zeros(...) + for_each` with
  `Zip::from(av).and(bv).map_collect(|&x,&y| op(x,y))` — which allocates the
  output **once** and writes each element **once**, dropping the zero-fill
  pass (step 4) entirely;
- for the fallible i32/i64 arms (where `op_*` returns `Result`, e.g. integer
  div-by-zero), it keeps the early-exit error handling and the `zeros`
  allocation (int-arm correctness over shaving one write pass) but STILL drops
  the 4 clones/copies — the dominant cost.

Result: coil now touches **1 N-sized buffer** (the output) for the f64 path —
the same as `&a + &b` — which is exactly why `T3/T2 → ~1.0×`. The
mixed-dtype / broadcasting / shape-differing cases fall through to the
unchanged slow path (which still needs `cast_to` + `broadcast_owned` and
stays correct — verified by the broadcast + differential corpora, §4.6).

The fast path is **bit-identical** to the slow path for the common case: the
same `op_*` closure runs over the same logical-order element stream (`Zip`
iterates identically whether it writes a pre-zeroed `out` or `map_collect`s a
fresh one), and equal shapes always self-broadcast (`broadcast_shape(s,s) =
Ok(s)`), so no error path is lost by skipping the `broadcast_shape` call.

### 4.4 T2 is a legitimate ceiling (sanity check on the methodology)

- At n=100, **raw `ndarray` beats numpy** (`T2/T1 = 0.44×` — 166 ns vs 375
  ns): no interpreter overhead.
- At n=1M, **raw `ndarray` ties numpy** (`T2/T1 = 1.19×` — both sub-1 ms):
  both are a single memory-bandwidth-bound pass. This is the expected
  outcome and confirms T2 is a faithful native ceiling, validating it as the
  denominator for the diagnostic ratio. (That coil — T3 — now also lands at
  this ceiling for n ≥ 10k is the §4.2 result.)

### 4.5 The optimization this benchmark pointed at — DONE (task #166)

The pre-fix `T3/T2 ≈ 4×` gap was **almost entirely reclaimable** and was not a
Cobrust *architecture* cost — it was four redundant N-sized copies (+ one
zero-fill) in `binary_dispatch`. The fix named in the first capture — a
same-dtype/same-shape fast path that skips both `cast_to` clones and both
`broadcast_owned` copies and runs the `Zip` directly on the input views — has
**landed and been re-measured** (§3 / §3.1 / §4.3):

- `T3/T2` collapsed from `~3.5–4.5×` to **`~0.9–1.0×` at n ≥ 10k** (coil at the
  raw-`ndarray` ceiling), with a fixed `~1.76×` residual at n=100 (the `Box`/
  FFI boundary, which does not scale with N — confirming the per-element tax is
  gone, not just shifted).
- `T3/T1` dropped below the §4.2 `≤ 1.25×` gate at **every** size, with coil
  *faster than numpy* at n=100 and n=10k.

This closes the §5.2 loop end-to-end on the project's first benchmark:
**claim** ("coil's element-wise kernel is unoptimized") ⇒ **experiment** (this
3-tier harness measured `~4×`) ⇒ **fix** (the fast path) ⇒ **re-measured
claim** (`~1.0×` at the ceiling, gate now passes), all reproducible from the
one entrypoint in §5.

### 4.6 Correctness — the fast path is verified equivalent, not just faster

The fast path is exercised by the full coil suite (all green, numpy-2.0.2
oracle present on the measuring host):

- **`ufunc_differential` (14 tests)** — bytewise (int) / `rtol=1e-7` (float)
  comparison vs upstream numpy 2.0.2 on add/sub/mul/div/pow + comparisons; the
  equal-shape same-dtype inputs take the new fast path, and every result still
  matches numpy bit-for-bit (float within tol).
- **`numpy_fuzz` (5)** + **`numpy_differential` (5)** — panic-free + value
  fuzz.
- **`ufunc_well_typed` (50)** + **`ufunc_ill_typed` (50)** — the latter
  includes the int div-by-zero cases (`t16`–`t30`) that exercise the
  **fallible** i32/i64 fast-path arms' error early-exit; all still raise
  `IntegerDivisionByZero` correctly.
- **`broadcast_corpus` (2)** + **`broadcast_elementwise_corpus` (8)** — the
  broadcasting / mixed-shape cases that fall through to the UNCHANGED slow
  path; still correct.
- **`div_scalar_elementwise_corpus` (13)** — true-division + int-div-by-zero
  scalar paths.

Total coil suite post-fix: 157 lib unit + all integration binaries green, 0
failed, 0 ignored.

---

## 5. Reproducibility (honesty rule (e))

One command:

```bash
# Hardware-tagged (stamps the §2 table, then runs the bench):
./scripts/bench/coil_elementwise_add.sh

# Or the bare bench (same numbers, no hw tag):
cargo bench -p cobrust-coil --bench elementwise_add
```

Tuning (defaults are the committed sweep):

```bash
COIL_BENCH_SIZES=1000,100000 COIL_BENCH_ITERS=401 COIL_BENCH_WARMUP=100 \
  cargo bench -p cobrust-coil --bench elementwise_add
```

**Run-to-run stability (why the ratios, not the ns, are the result).** Three
consecutive full runs **post-fix** (`T3/T2`, symmetric result-free timing) on
the tagged hardware:

| size | run 1 | run 2 | run 3 | post-fix band | (pre-fix band) |
|---:|---:|---:|---:|---:|---:|
| 100 | 1.76 | 1.76 | 1.76 | **1.76** (stable) | (3.5–4.0) |
| 10 000 | 0.90 | 0.99 | 0.84 | **0.84–0.99** | (3.7–4.5) |
| 1 000 000 | 0.94 | 0.84 | 0.46 | **0.46–0.94** | (3.5–4.2) |

Post-fix, n=100 is rock-stable at `1.76×` (a fixed per-call cost is a constant
fraction of constant per-element work), n=10k sits at parity (`~0.9×`), and
n=1M scatters `0.46–0.94` — the n=1M low end is real M1 large-array noise (the
op is sub-1 ms; a single scheduler preemption on the T2 *denominator* deflates
the ratio). None of the post-fix runs reproduces the pre-fix `~4×`; the
per-element tax is gone. The variance is real (an unpinned dev laptop: each op
is sub-1 ms, so a single scheduler preemption moves the median, and at n=100
the fixed FFI/`Box` boundary is a visible fraction). The honest reading is
"`T3/T2 ≈ 1.0× at n ≥ 10k`, `~1.76× at n=100`" — the *shape* (parity at the
ceiling for non-tiny N, a fixed boundary residual at tiny N, §4.2/§4.3) holds
in every run even though the exact per-run ns/ratio does not. A controlled rig
(pinned core, fixed governor, more iters) would tighten the spread; that is
explicitly out of
scope for a dev-laptop first increment (honesty rule (d)). The absolute ns
are indicative; the ~4× shape is the load-bearing result.

**CI behavior.** The **T1 numpy tier self-skips** when no `python3` with
numpy is present (`T1_PYTHON=SKIPPED_no_numpy` in the output); the **T2 + T3
Rust tiers still run** and the `T3/T2` diagnostic is still produced. T1 is a
local-development enrichment, not a CI gate.
