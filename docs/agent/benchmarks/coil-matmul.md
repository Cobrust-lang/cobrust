---
doc_id: agent/benchmarks/coil-matmul
title: "Benchmark report — coil matrix multiply (a @ b, f64)"
status: active
last_verified_commit: HEAD
op: matmul_f64_NxN
tiers: [T1_python_numpy, T2_raw_ndarray, T3_cobrust_coil]
methodology: docs/agent/benchmarks/README.md
bench: crates/cobrust-coil/benches/matmul.rs
rerun: scripts/bench/coil_matmul.sh
---

# Benchmark report — coil matrix multiply (`a @ b`, f64)

The **second increment** of the Cobrust performance-benchmark suite (after
`coil-elementwise-add`). It measures one operation — matrix multiplication
`a @ b` on square `N x N` `f64` matrices (the `.cb` `@` operator on
`coil.Buffer`, ADR-0077 §"@-operator") — three ways, and reports the two
ratios the methodology defines. It is the first real number behind the
`@`-operator's perf characteristics (CLAUDE.md §5.2).

> **Honest headline up front (no fabricated win).** Matmul is a story about
> **backends**, and coil currently loses the headline ratio:
>
> - **`T3/T1` (coil vs numpy) is `> 1` at non-trivial N** (`~1.9× @ 16 → ~12×
>   @ 64 → ~12× @ 256`). numpy `@` dispatches to **BLAS** (Apple Accelerate on
>   the measuring host); coil's default backend is `ndarray`'s **own pure-Rust
>   GEMM** (`Array2::dot`, NO BLAS unless the `linalg-backend` feature is on).
>   This gap is **ndarray-GEMM-vs-BLAS** — it is NOT a cost of coil's
>   `@`-operator wiring (raw `ndarray` loses to numpy by the same backend gap,
>   `T2/T1 ≈ 7×` at n=256) — and it is the concrete motivation for **#157** (a
>   pure-Rust BLAS-class linalg, e.g. `faer`).
> - **`T3/T2` (coil vs its own ndarray ceiling) is `> 1` but SHRINKS toward the
>   ceiling as N grows** (`~5.8× @ 16 → ~2.6× @ 64 → ~1.7× @ 256`). This is
>   coil's wrapping cost — coil's `linalg::matmul` makes **five O(N²)
>   marshalling copies** around the GEMM (§4.3). Because that tax is O(N²)
>   against an O(N³) GEMM, it **AMORTIZES** (`O(N²)/O(N³) = O(1/N)`): dominant
>   at tiny N (5.8×), small at large N (down to ~1.7× and still falling) — the
>   same fixed-overhead-amortizing shape the elementwise bench showed. It still
>   names a real follow-up (§4.5, the matmul analogue of task #166's fast path)
>   that would push T3/T2 toward 1.0× at ALL N — most visibly at small N.
>
> We report both gaps honestly and claim no win coil does not have. §3 has the
> numbers; §4 reads them mechanistically; §4.3/§4.5 root-cause + name the fix.

Read `docs/agent/benchmarks/README.md` for the full 3-tier model + honesty
rules. This report restates them only as needed to interpret the numbers.

---

## 1. What was measured

| Tier | Subject | Timed region |
|---|---|---|
| **T1** | Python `numpy` `a @ b` (subprocess) | `a @ b` per iter (numpy allocates+frees one result/iter; `@` = BLAS GEMM) |
| **T2** | Raw Rust `ndarray::Array2<f64>` `a.dot(&b)` | `a.dot(&b)` per iter (one owned result allocated + dropped/iter) |
| **T3** | Cobrust coil C-ABI `__cobrust_coil_buffer_matmul` | the matmul shim **+ the result `__cobrust_coil_buffer_drop`** per iter |

- **Op:** `a @ b`, matrix multiplication, `f64`, square `N x N @ N x N → N x N`.
  The headline `@` case; matrix-vector and 1-D cases share the same kernel and
  are correctness-tested in `coil_matmul_e2e.rs` (not separately benched).
- **T2 is the EXACT kernel coil calls.** coil's `matmul` 2-D·2-D arm
  (`crates/cobrust-coil/src/linalg.rs`) builds two `Array2<f64>` and calls
  `Array2::dot` — so T2 *is* coil's own GEMM with the marshalling stripped
  away. The `T3/T2` ratio therefore isolates coil's marshalling overhead
  exactly; the `T2/T1` ratio isolates the ndarray-vs-BLAS backend gap.
- **Inputs:** two deterministic row-major ramps `a[i] = i*0.5 + 1.0`,
  `b[i] = i*0.25 - 3.0` (`i` the row-major flat index), reshaped to `(N, N)`,
  allocated **once per size, outside every timed region**. Identical values in
  all three tiers (numpy re-derives via `np.arange(N*N).reshape(N,N)`); no
  constant-folding.
- **Sizes:** `N = 16`, `64`, `256` (16×16 … 256×256 — the latter is ~33.5M f64
  multiply-adds per matmul).
- **Sampling:** **50 warm-up** iters discarded (raised from 10 — matmul on a
  multi-threaded BLAS/GEMM laptop needs a longer warm-up to clear the CPU
  frequency-ramp; a cold capture produced an impossible `T3 < T2`, see §5),
  then **N = 51** per-iteration samples; the headline is the **median** ns/op
  (odd N → a single observed middle sample). Mean + min recorded for
  transparency. (Fewer iters than the elementwise bench's 201 because each
  matmul is O(N³).)

### 1.1 The diagnostic axis

- **`T3 / T2`** (coil vs raw `ndarray`) — **the diagnostic number**: how much
  the `.cb` wrapping (FFI cross + per-op result alloc + coil's matmul
  marshalling copies) erodes coil's own backend ceiling.
- **`T3 / T1`** (coil vs numpy) — the headline "Cobrust vs Python" number.
  Dominated here by the backend gap (BLAS vs ndarray-GEMM), not by coil.

---

## 2. Hardware tag (honesty rule (d))

> **Dev-laptop numbers — indicative, NOT a controlled benchmark rig.** No
> fixed CPU governor, no thermal isolation, no core pinning. Absolute ns drift
> run-to-run; the **ratios + their SHAPE with N are the load-bearing result**,
> and that shape (T3/T2 shrinks toward the ndarray ceiling; T3/T1 is a large,
> N-growing BLAS gap) reproduces across runs (§5). Matmul is more variance-prone
> than elementwise because BLAS (T1) and ndarray-GEMM (T2) are multi-threaded
> and contend on a shared laptop — visible as wide `mean` vs `median` spreads at
> n≥64; the **median** is the honest central tendency, and a 50-iter warm-up is
> required (a cold capture is not trustworthy — §5).

| Field | Value |
|---|---|
| CPU | Apple M1 |
| Cores | 8 (logical) |
| OS | Darwin arm64 (macOS) |
| rustc | 1.94.1 |
| Build profile | `release` (the `cargo bench` profile — optimized) |
| T1 interpreter | `python3` — Python 3.9, **numpy 2.0.2** |
| T1 numpy BLAS | **Accelerate** (Apple's tuned BLAS — `numpy.__config__`) |
| coil linalg backend | `ndarray` pure-Rust `Array2::dot` (**no** `linalg-backend`/BLAS feature) |

> The decisive tag rows are the **last two**: numpy is on Accelerate BLAS, coil
> is on ndarray's own GEMM. That single asymmetry is the whole `T3/T1` headline.

---

## 3. Results

Median ns/op (lower is better), N = 51, warm-up 50. A representative WARM run
on the tagged hardware (the *shape* — not the exact ns — is the result; see §5
for the run-to-run band).

| N (NxN) | T1 numpy (ns) | T2 ndarray (ns) | T3 coil (ns) | **T3/T2** (diagnostic) | **T3/T1** (headline) | T2/T1 |
|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 166 | 375 | 2 167 | **5.78×** | **1.86×** | 0.32× |
| 64 | 3 459 | 15 541 | 40 667 | **2.62×** | **11.76×** | 4.49× |
| 256 | 109 000 | 767 042 | 1 300 417 | **1.70×** | **11.93×** | 7.04× |

Per-FLOP (median ns / N³ multiply-adds — lower is better; each tier's GEMM
efficiency independent of problem size):

| N | T1 numpy | T2 ndarray | T3 coil |
|---:|---:|---:|---:|
| 16 | 0.285 | 0.0916 | 0.529 |
| 64 | 0.0132 | 0.0593 | 0.155 |
| 256 | 0.0065 | 0.0457 | 0.0775 |

*(All numbers are `KEY=value`-grep-able from the bench stdout, e.g.
`T3_OVER_T2_N256=`, `T3_MEDIAN_NS_N64=`. Absolute ns are indicative on an
unpinned laptop; the **ratios and their shape with N** are the load-bearing
result. `T3/T2` shrinks (5.78→2.62→1.70 — the marshalling tax amortizes);
`T3/T1` is a large BLAS gap that holds (~12× at n≥64). The `mean` columns —
emitted as `*_MEAN_NS_*` — are wide for T1/T2 at n≥64 due to scheduler stalls
on the multi-threaded BLAS/GEMM paths; the report uses the **median** per
honesty rule (b).)*

---

## 4. Findings (read mechanistically, not just reported)

### 4.1 The headline: coil vs numpy (`T3/T1`) — numpy wins, and the gap is BLAS

- **`T3/T1` is `> 1` at non-trivial N** (`1.86× @ 16 → 11.76× @ 64 → 11.93× @
  256`). numpy is faster — dramatically so once the matrices are large enough
  for BLAS to shine. (At n=16 the gap is only ~1.9× because numpy's per-call
  Python + BLAS-dispatch overhead is comparable to the work; by n=64 BLAS
  dominates and the gap jumps to ~12×, then holds.)
- **The cause is the backend, not coil.** Raw `ndarray::Array2::dot` — *no
  Cobrust anywhere* — ALSO loses to numpy, by a widening margin: `T2/T1 = 0.32×
  → 4.49× → 7.04×`. At n=16 ndarray actually *beats* numpy (0.32× — no BLAS
  setup overhead); by n=256 ndarray's simpler pure-Rust GEMM is ~7× slower than
  Accelerate's blocked, vectorised, multi-threaded BLAS. That ~7× IS the
  ndarray-GEMM-vs-BLAS gap, with zero Cobrust involved.
- **Why this is the right honest framing:** coil's `@` operator is a thin
  wrapper over `ndarray::Array2::dot`. It can never beat numpy while its backend
  is ndarray-GEMM and numpy's is BLAS — *no matter how perfectly the `.cb`
  wiring is done*. The headline gap is a **backend** decision, and it is exactly
  the motivation for **#157** (a pure-Rust BLAS-class linalg such as `faer`,
  which closes the ndarray-vs-BLAS gap *without* a C BLAS dependency). Until
  #157, the `@` operator is correct and ergonomic but not BLAS-competitive at
  large N — and we say so.

### 4.2 The diagnostic: coil vs raw ndarray (`T3/T2`) — coil's marshalling tax, which AMORTIZES

- **`T3/T2` is `> 1` but SHRINKS toward the ceiling as N grows** (`5.78× → 2.62×
  → 1.70×`). This is coil's wrapping cost — and, like the elementwise bench, it
  **amortizes**: dominant at tiny N, small (and still falling) at large N.
- **It is a fixed/low-order overhead amortizing against the O(N³) GEMM.** At
  n=16 the FFI floor + the five O(N²) marshalling copies (§4.3) dominate the
  tiny 4096-FLOP GEMM → 5.78×. As N grows the O(N³) GEMM grows faster than the
  O(N²) marshalling, so the ratio falls: `O(N²)/O(N³) = O(1/N)`. By n=256 the
  marshalling is down to a ~1.7× residual and still dropping. The per-FLOP table
  confirms it: coil's ns/FLOP **falls** (`0.529 → 0.155 → 0.0775`), converging
  toward T2's (`0.0916 → 0.0593 → 0.0457`) — coil is paying a *shrinking* tax
  per FLOP, not a growing one.
- **(Correction note.)** An earlier capture of this report read the trend
  backwards (claimed T3/T2 *grew* to 4.25×) — that was a cold-warm-up,
  throttled n=256 outlier (§5). The reproducible warm shape is the amortizing
  one above, which is also what §4.3's own `O(N²)/O(N³)` math predicts.

### 4.3 Why `T3/T2 > 1` — the root cause (the matmul marshalling chain)

coil's `matmul` (`crates/cobrust-coil/src/linalg.rs`) does, for the equal-shape
`f64 @ f64` 2-D·2-D path, around the SAME `Array2::dot` that T2 calls directly:

1. `coerce_pair_f64(a, b)` → `to_f64(a)` = **`arr.iter().copied().collect()`**
   — a full O(N²) copy of `a` into a fresh `Vec<f64>`. Same for `b`. **(2 O(N²)
   copies.)**
2. `matmul_f64`: **`Array2::from_shape_vec((m,k), a.to_vec())`** — `a.to_vec()`
   copies the just-collected vec AGAIN into the `Array2`. Same for `b`. **(2
   more O(N²) copies.)**
3. `a_mat.dot(&b_mat)` — the actual O(N³) GEMM. **(This is all T2 does.)**
4. `c.iter().copied().collect()` → the output `Vec<f64>`. **(1 O(N²) copy out.)**
5. `float_array_from_f64` → consumes the vec (no copy).

So coil touches roughly **5 N²-sized buffers** per matmul where raw `ndarray`
`a.dot(&b)` touches **0** extra. This is an O(N²) tax on top of an O(N³) GEMM,
so it **amortizes** (the ratio → ~1.0× as N → ∞, observed dropping 5.78×→1.70×
across 16→256). It is the SAME class of redundant-copy overhead the elementwise
bench found (task #166) — here on the `linalg::matmul` path, which has **not**
had the equivalent same-dtype fast-path optimization applied. The fast path
(§4.5) would reclaim it at ALL N — most dramatically at small N (where it is
the 5.78× dominant cost), and shaving the residual ~1.7× at large N too.

### 4.4 T2 is a legitimate ceiling (sanity check on the methodology)

- At n=16, **raw `ndarray` beats numpy** (`T2/T1 = 0.32×` — 375ns vs 1166ns):
  at tiny N the Python/BLAS per-call dispatch overhead dwarfs the work, so
  ndarray's lighter path wins.
- At n=64 and n=256, **numpy (BLAS) pulls far ahead of raw `ndarray`**
  (`T2/T1 = 4.49× → 7.04×`): Accelerate's blocked/threaded GEMM beats ndarray's
  simpler GEMM by a widening margin as the matrices grow. This is the expected
  ndarray-vs-BLAS outcome and confirms T2 is a faithful *ndarray-backend*
  ceiling — the correct denominator for isolating coil's wrapping (§4.2) from
  the backend gap (§4.1). The gap is large (~7× at n=256) → the #157 motivation
  is strong.

### 4.5 The optimizations this benchmark points at (NOT done — named, with evidence)

This bench did its §5.2 job: it turned "what does `@` cost?" into two measured,
mechanistically-explained gaps, each with a named fix.

1. **`T3/T2` (coil's own tax) — a matmul fast path (the #166 analogue).** The
   five O(N²) copies in `linalg::matmul` (§4.3) are largely reclaimable for the
   common same-dtype 2-D·2-D case: detect `a.dtype() == b.dtype() == Float64`
   (or `Float32`) and BOTH rank-2 at the top of `matmul`, borrow the input
   `ArrayD<T>` as a 2-D view (`.into_dimensionality::<Ix2>()`), call `.dot` on
   the views, and wrap the owned `Array2` result **without** the
   `to_f64`/`to_vec`/`collect` round-trips. That would drop T3/T2 toward ~1.0×
   at ALL N (the tax already amortizes; the fast path removes it outright, most
   visibly killing the 5.78× small-N cost). A `cobrust-coil` numerics change
   (OUT of scope for the `@`-operator wiring task that produced this report —
   "zero new numerics"); filed as the matmul sibling of the #166 elementwise
   fast path.
2. **`T3/T1` (the backend gap) — #157 (pure-Rust BLAS-class linalg).** Even a
   perfectly-marshalled coil matmul (fix 1 → T3 ≈ T2) would still lose to numpy
   by the `T2/T1` factor (up to ~7× at n=256), because ndarray-GEMM is not BLAS.
   Closing THAT requires a BLAS-class backend. **#157** (e.g. `faer` — pure-Rust,
   no-C-dependency, BLAS-competitive) is the path: it lifts BOTH T2 and T3 toward
   numpy's Accelerate-BLAS curve. This benchmark is the concrete, reproducible
   evidence motivating #157 — and the ~7× T2/T1 gap shows the payoff is large.

### 4.6 Correctness — `@` is verified, the bench only measures speed

Correctness of the `@` operator is pinned separately by the end-to-end suite
(`crates/cobrust-cli/tests/coil_matmul_e2e.rs`, all green): `[[1,2],[3,4]] @
[[5,6],[7,8]] == [[19,22],[43,50]]` (the exact product, ruling out
element-wise / transpose / swapped-operand bugs), the matrix-vector case
`[[1,2],[3,4]] @ [5,6] == [17,39]`, `a @ eye(2) == a`, the explicit-borrow form
`&a @ &b`, the runtime shape-mismatch TRAP `(2,3)@(2,2)` (clean abort, no UB / no
C-ABI unwind), and the two typecheck rejections `a @ scalar` / `scalar @ a`
(each with a §2.5 fix-printing diagnostic). The underlying `Array::matmul` kernel
additionally passes the ADR-0017 `rtol=1e-6` differential gate vs numpy. The
bench inputs (conformable square matrices) never hit the trap path, so the timed
region is pure kernel + marshalling.

---

## 5. Reproducibility (honesty rule (e))

One command:

```bash
# Hardware-tagged (stamps the §2 table incl. numpy's BLAS backend, then runs):
./scripts/bench/coil_matmul.sh

# Or the bare bench:
cargo bench -p cobrust-coil --bench matmul
```

Tuning (defaults are the committed sweep `N = 16,64,256`, warm-up 50):

```bash
COIL_MATMUL_SIZES=32,128,512 COIL_MATMUL_ITERS=101 COIL_MATMUL_WARMUP=80 \
  cargo bench -p cobrust-coil --bench matmul
```

**Warm-up matters (the cold-capture trap).** Matmul drives multi-threaded BLAS
(T1) and ndarray-GEMM (T2) that contend on an unpinned laptop and need the CPU
at steady frequency. A *cold* capture (the old warm-up=10 default) produced an
**impossible `T3 < T2`** (coil "faster" than the bare `Array2::dot` it wraps) —
the box was mid-frequency-ramp. The default is now **warm-up=50**; treat any run
where `T3 < T2` as a cold artifact and re-run warm.

**Run-to-run stability (the shape, not the ns, is the result).** Across warm
runs the **shape** reproduces tightly:
- `T3/T2` SHRINKS with N — observed `~5.5–5.9× @ 16`, `~2.6–2.9× @ 64`,
  `~1.57–1.70× @ 256` over several runs (the marshalling tax amortizing against
  the O(N³) GEMM).
- `T3/T1` is a large, N-growing BLAS gap — observed `~1.7–1.9× @ 16`, `~11–12× @
  64`, `~10.5–12× @ 256` (numpy-BLAS wins once N is non-trivial).
Absolute ns drift (esp. the n=256 `mean`, which a single stalled iter can
inflate 2–3×); the median + the ratio *shape* are what hold. A controlled rig
(pinned core, fixed governor, more iters) would tighten the absolute ns.

**CI behavior.** The **T1 numpy tier self-skips** when no `python3` with numpy is
present (`T1_PYTHON=SKIPPED_no_numpy`); the **T2 + T3 Rust tiers still run** and
the `T3/T2` diagnostic is still produced. T1 is a local-development enrichment,
not a CI gate.
