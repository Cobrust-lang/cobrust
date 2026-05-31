---
doc_id: faer-adoption-survey
title: "faer adoption survey — pure-Rust BLAS-class linalg as coil's portable accelerator"
status: survey
last_verified_commit: bba2bcd
governs: ADR-0079 §11 + #157
date: 2026-05-31
relates_to: [adr:0079, adr:0017, adr:0075, issue:157, issue:166]
sourced_from: "#157 / ADR-0079 §11 faer tractability survey — two research agents (faer external docs.rs/crates.io + coil internal source)"
---

# faer Adoption Survey

> **This is a RECOMMENDATION for the CTO to act on, NOT an authoritative
> implementation spec.** API signatures, perf numbers, and cross-target support
> claims below are sourced from external docs/benchmarks (cited) and from a
> source read of coil at HEAD `bba2bcd`. Anything not read line-by-line off a
> rustdoc signature or a measured chart is marked `[UNVERIFIED]`. Do not treat
> faer API shapes here as a spec — confirm against `docs.rs/faer/0.24.0` before
> writing the spike. The §6 RISKS list carries every uncertainty forward.

---

## 1. The question + bottom-line recommendation

**The question (ADR-0079 §11, #157):** Can `faer` — a pure-Rust, BLAS-competitive
dense-linalg crate — be a SINGLE portable accelerator across `native + RISC-V +
WASM`, (a) closing coil's measured ~12× matmul gap vs numpy-on-BLAS
(`docs/agent/benchmarks/coil-matmul.md`), and (b) making the `native-x86_64-only`
`ndarray-linalg` opt-in unnecessary so it can be retired?

**BOTTOM LINE — RECOMMENDATION: ADOPT for the gemm path first, behind a spike;
treat full-op + full-target adoption as the goal but gate it on the spike's
measured numbers.** faer is genuinely pure-Rust (no BLAS/LAPACK/C/Fortran), MIT,
actively maintained, and its author claims matmul parity with OpenBLAS (a bit
under MKL) — placing it in the same tier as numpy's Accelerate/BLAS that coil's
ndarray-GEMM is NOT in; that is the right root-cause fix for the gap, and it
simultaneously satisfies ADR-0079's "default features cross-compile to native +
RV + WASM with zero system deps" constraint that `ndarray-linalg` structurally
cannot. We do **not** recommend a full big-bang swap-everything adoption on this
survey alone, because (i) we could not retrieve an exact faer-vs-Accelerate
matmul number on arm64 — the regime coil's gap was measured in — and (ii)
faer's wasm32 / riscv64 builds were not independently verified; both are
spike-gated below (§6).

---

## 2. coil's current linalg (and where the matmul gap actually is)

**Default build = 100% pure-Rust on `ndarray = "0.16"`. There is NO BLAS in the
default build.** (`crates/cobrust-coil/Cargo.toml:25` `default = []`.)

The opt-in accel chain exists but is **declared, not wired**:

- `linalg-backend = ["dep:ndarray-linalg"]` (Cargo.toml:35) pulls `ndarray-linalg
  = "0.16"` (Cargo.toml:73, `optional = true`).
- `linalg-openblas-static` (Cargo.toml:36), `linalg-intel-mkl-static`
  (Cargo.toml:37) layer the system-BLAS sub-features on top.
- **VERIFIED at HEAD `bba2bcd`:** the only `cfg(feature = "linalg-backend")` site
  in `src/` is `linalg.rs:842` — a dead `_backend_marker() -> "ndarray-linalg"`
  stub (`#[allow(dead_code)]`). A grep for any real `ndarray_linalg::` usage in
  `src/` returns **ZERO** matches. Turning the feature ON today pulls the dep
  into the build but swaps NO kernel to LAPACK; the pure-Rust path runs
  regardless. ADR-0079 §1.1 records this as honest debt (not a regression):
  "Turning the feature on does NOT today swap any kernel to LAPACK."

**The 8 linalg ops (ADR-0017 closed set) are ALL pure-Rust** (`src/linalg.rs`):

| Op | Implementation | Note |
|---|---|---|
| `matmul` / `dot` | scalar loops for rank-1; **2-D·2-D arm builds `Array2` then calls `Array2::dot`** (`matmul_f64` l.262, `matmul_f32` l.339) | plain `ndarray`'s built-in GEMM, **NOT** `ndarray-linalg` |
| `det` (l.427) | pure-Rust LU partial-pivot (`lu_decompose_f64` l.361) | ndarray-linalg 0.16 has no first-class det anyway (ADR-0079 §3.1) |
| `solve` (l.464) | pure-Rust LU + fwd/back subst (`lu_solve_f64` l.400) | |
| `inv` (l.503) | pure-Rust `solve(a, I)` column-by-column | |
| `cholesky` (l.532) | pure-Rust Cholesky–Banachiewicz | `NotPositiveDefinite` on non-PD |
| `eigh` (l.575) | pure-Rust cyclic Jacobi (`JACOBI_MAX_SWEEPS=100`) | **N ≤ 64 cap** (ADR-0017) |
| `svd` (l.673) | pure-Rust eigh(AᵀA) then U = A·V/σ | inherits the N-cap |

**Where the matmul gap actually is — `ndarray`'s built-in GEMM, not
`ndarray-linalg`.** The `.cb` path is `a @ b` → cabi shim
`__cobrust_coil_buffer_matmul` (`cabi.rs:550`) → `Array::matmul` (`array.rs:501`)
→ `linalg::matmul` (`linalg.rs:175`) → the 2-D arm's `a_mat.dot(&b_mat)`
(`linalg.rs:262`). The cabi doc comment states it verbatim: "uses `ndarray`'s
`Array2::dot` for the 2-D·2-D case, and is NOT BLAS by default."

The benchmark report `docs/agent/benchmarks/coil-matmul.md` confirms and
decomposes the gap (VERIFIED numbers from that file):

- **T3/T1 (coil vs numpy):** `~1.9× @16 → ~12× @64 → 11.93× @256` — numpy `@`
  dispatches to BLAS (Apple Accelerate on the measuring host); coil's default is
  ndarray's own pure-Rust GEMM.
- **T2/T1 (raw ndarray vs numpy) — the BACKEND gap:** `0.32× @16 → 4.49× @64 →
  7.04× @256`. (At N=16 raw ndarray *beats* numpy; once N is non-trivial,
  Accelerate's blocked/threaded/vectorized BLAS pulls ~7× ahead.) **This ~7× IS
  the ndarray-GEMM-vs-BLAS gap — it is NOT a cost of coil's `@`-operator
  wiring.**
- **T3/T2 (coil vs raw ndarray) — coil's own marshalling tax:** the ~5 O(N²)
  copy round-trips (coil-matmul.md §4.3), which amortize against the O(N³) GEMM
  (T3/T2 = 1.70× @256) and shrink toward 1.0 as N grows. This is the **#166**
  same-dtype fast-path's territory, **orthogonal to backend choice** (see §4).

So the headline gap is squarely a **backend** problem, and that is precisely
what coil-matmul.md §4.5 #2 names #157 / faer as the fix for.

---

## 3. faer: API, purity, perf-vs-BLAS, maturity/license

Sourced from `docs.rs/faer/0.24.0`, the crates.io API, faer's README and
benchmark page, and an author HN thread (all cited in §"Sources"). **Confidence
flags are explicit.**

### 3.1 API surface
Centered on the owning `Mat` type + borrowed `MatRef`/`MatMut` views,
decomposition structs, and the `Solve`/`SolveCore` traits.

- **Matmul** — operator-overloaded `*`. From the crate docs (verbatim example
  confirmed): `let c = &a * &b;` is the GEMM path (operands by reference to avoid
  moves; matrices built via `mat![...]`, `Mat::from_fn(r,c,|i,j| ...)`,
  `Mat::zeros(...)`). Under the hood `*` dispatches to faer's `gemm`/`nano-gemm`
  kernels (also directly reachable via `faer::linalg::matmul::matmul`).
- **Solve `A·x = b`** — build a decomposition then `.solve(&b)` (the `Solve`
  trait). Docs recommend `a.partial_piv_lu().solve(&b)` (general square system),
  `a.llt(faer::Side::Lower).solve(&b)` for SPD. faer's docs explicitly steer
  toward `Solve` over forming an explicit inverse.
- **Decompositions** (`faer::linalg::solvers`): `PartialPivLu`, `FullPivLu`,
  `Llt`, `Lblt`, `Ldlt`, `Qr`, `ColPivQr`, `Svd`, and eigensolvers
  `SelfAdjointEigen`, `Eigen`, `GeneralizedEigen`. Constructors are methods on
  `Mat`/`MatRef`: `.partial_piv_lu()`, `.llt(side)`, `.qr()`, `.svd()`,
  `.self_adjoint_eigen(side)`, `.eigen()`, etc. — **note this covers the ADR-0079
  DEFERRED ops** coil lacks today (`qr`, non-symmetric `eig`, and SVD/eigh
  without the N≤64 Jacobi cap).
- **Det / Inv** — `Mat` exposes `.determinant()` and `.inverse()` (determinant
  defaults through partial-pivot LU).

`[UNVERIFIED]` The exact `.solve()` signature and the precise receiver overloads
for `Mat::determinant()` / `Mat::inverse()` (whether on `Mat` directly vs on a
decomposition) were summarized from docs prose, not read line-by-line off the
rustdoc signatures. Confirm before the spike.

### 3.2 Purity — YES, genuinely pure Rust, no BLAS/LAPACK/C/Fortran
**This is the decisive finding and it is strongly corroborated:**

1. **Dependency tree (decisive).** crates.io for faer 0.24.0 lists required
   non-dev deps: `bytemuck`, `dyn-stack`, `equator`, `faer-traits`, `gemm`,
   `generativity`, `libm`, `nano-gemm`, `num-complex`, `num-traits`, `pulp`,
   `reborrow`; optional `log`, `npyz`, `private-gemm-x86`, `rand`, `rand_distr`,
   `rayon`, `spindle`. **ZERO** occurrence of blas/lapack/openblas/mkl/intel/
   netlib/cblas/lapacke/accelerate/blas-src. Matmul is the pure-Rust
   `gemm`/`nano-gemm` crates (same author).
2. **README:** implemented "in pure rust."
3. **Author (HN):** algorithms "implement[ed] from scratch"; a commenter noted
   "This does not seem to depend on BLAS/LAPACK."

**SIMD approach:** hand-written SIMD via the `pulp` crate (same author), **NOT**
`std::simd` — runtime CPU-feature dispatch / multiversioning over its own vector
abstraction (`f32x4`, `f64x2`, …). pulp covers **x86/x86-64 AND aarch64 (incl.
Apple Darwin)** with NEON / AVX2 / AVX-512 runtime dispatch — exactly coil's two
primary targets (Apple-silicon arm64 + x86_64). MSRV `rust 1.84.0`, no nightly
on the std path.

`[UNVERIFIED — CRITICAL]` faer's **wasm32 / RISC-V** builds were NOT independently
verified. Purity + pulp's portable dispatch strongly *imply* it cross-compiles
far better than `ndarray-linalg` (which is x86_64-only + needs system BLAS), but
no green `wasm32-wasip1` / `riscv64gc` build of faer 0.24 was confirmed, and
pulp's WASM-SIMD / RISC-V-V coverage is unconfirmed (may fall back to scalar).
This is the single biggest open question for ADR-0079's RV+WASM mandate.

### 3.3 Performance vs BLAS — SELF-REPORTED, directionally strong, no measured arm64 number
**Clearly separate self-reported from independent: ALL faer perf figures below
are self-reported by the library author on the author's own hardware. No
independent benchmark was found.**

- **Author's matmul claim (HN, the most concrete citable statement):** faer
  matmul is "usually faster, or even with openblas, slower than mkl on my
  desktop." → faer matmul ≈ OpenBLAS (sometimes faster), somewhat below MKL —
  i.e. **BLAS-class, not a toy GEMM.**
- **Official benchmark page** (`faer.veganb.tw/benchmarks/`) has per-CPU result
  sets (AMD Ryzen 7 8745HS; Intel Xeon Gold 6146) vs nalgebra, ndarray, MKL,
  OpenBLAS, eigen across matmul/LU/Cholesky/QR/SVD/eig. **HONESTY FLAGS:** (a)
  it is a JS-rendered SPA — the exact per-size GFLOPS/ms matmul tables could NOT
  be scraped, so no concrete matmul number at N≈1024/4096 was read off the
  chart; (b) all numbers are self-reported on the author's hardware, x86_64
  only (Ryzen/Xeon), **NOT vs Accelerate on arm64** — which is the host coil's
  ~12× gap was measured against.
- One concrete self-reported outlier (full-pivot LU, NOT matmul — shown only to
  evidence the kernels are seriously optimized): faer 27 ms vs MKL 186 ms @
  n=1024; faer 6.11 s vs MKL 15.70 s @ n=4096.
- Author caveat: benchmarking all libs in one process suffers OpenMP-vs-rayon
  threadpool contention noise.

**Bottom line on the 12× gap:** numpy/Accelerate is in the OpenBLAS/MKL tier;
faer matmul being "≈ OpenBLAS, a bit under MKL" puts faer in that same BLAS
class, which coil's ndarray-GEMM backend is NOT. So faer is the kind of backend
that **narrows toward parity and likely closes most** of the gap — but with no
measured faer-vs-Accelerate(arm64) figure, the residual could be ~1× (full
close) or a small multiple (1.5–3×). Treat the close as **well-supported in
direction and tier, not as a measured number.**

### 3.4 Maturity / license
- **Version 0.24.0** (latest, 2026-01-26). **License MIT** (single-license MIT
  across 0.22.x→0.24.0). Compatible with the repo's Apache-2.0-OR-MIT bar (MIT
  alone satisfies "Apache-2.0 OR MIT compatible"). **License nuance:** faer
  ships MIT-only, not the dual — coil would consume an MIT dep; CTO should
  confirm MIT-only inbound is acceptable for the project's licensing posture
  (per ADR-0001).
- **Actively maintained:** 2.83M total downloads, 1.29M recent (~45% recent);
  brisk cadence (0.22.x Apr 2025 → 0.23.x Sep 2025 → 0.24.0 Jan 2026). Source
  repo on Codeberg. **Single maintainer = bus-factor risk.**
- **API stability:** pre-1.0 (0.x); minor bumps CAN and DID break the API
  (0.22→0.23→0.24 in <1yr). Adopters must pin and budget periodic migration. No
  1.0 stability guarantee yet.

---

## 4. Verdict on the matmul gap + integration cost

**VERDICT: PLAUSIBLY YES — faer would close most of the ~12× matmul gap, because
it attacks the gap at its root.** coil-matmul.md attributes the headline to
"ndarray-GEMM-vs-BLAS" (the ~7× T2/T1 backend gap), NOT to coil's wiring.
Swapping in a BLAS-tier pure-Rust kernel (faer ≈ OpenBLAS, the same tier as
numpy's Accelerate) replaces the non-BLAS-class backend that *is* the cause.
coil's bench tops out at N=256; the #157 framing's larger-N regime (N≈1000²) is
exactly where faer's blocked/threaded GEMM shines — favoring a strong close.

**CONFIDENCE: directionally strong, NOT a measured number.** No
faer-vs-Accelerate(arm64) matmul figure was retrieved (§3.3 / §6). Honestly: the
gap "narrows toward parity, likely closes most" — residual unproven.

**Two coil-specific cautions:**

1. **The T3/T2 marshalling tax is ORTHOGONAL to backend choice.** faer fixes
   T2/T1 (backend), not T3/T2 (coil's ~5 O(N²) copies, coil-matmul.md §4.3/§4.5).
   Both fixes are needed for full numpy parity. **faer would actually ADD a
   marshalling edge** (ndarray↔faer hop, below) unless a faer-native fast path
   (the #166 analogue) keeps buffers in faer form.
2. faer would let coil **drop the optional `ndarray-linalg`/OpenBLAS/MKL features
   entirely** and satisfy ADR-0079's zero-system-dep cross-compile constraint —
   faer's biggest strategic win over `ndarray-linalg` (x86_64-only + needs
   system Fortran/C BLAS).

**INTEGRATION COST: MODERATE.** coil's numerics live in ndarray
`ArrayD<T>`/`Array2<T>` (`linalg.rs` uses `ndarray::{Array1, Array2, ArrayD,
IxDyn}`); faer wants its own `Mat`/`MatRef`. A faer gemm path needs:

```
ArrayD/Array2  ──(2-D view)──▶  MatRef (faer is COLUMN-major; ndarray default
                                  is ROW-major → pass a transposed view, build
                                  via Mat::from_fn, use a column-major-aware
                                  ctor, or the optional faer-ext interop crate)
       │                                          │
       │                                    &a * &b  (faer GEMM)
       ▼                                          │
   Array2/ArrayD  ◀──(copy back the Mat result)──┘
```

This is the **same class** of O(N²) boundary copy coil already pays
(coil-matmul.md §4.3) — it adds an ndarray↔faer hop but amortizes against the
O(N³) GEMM. **Layout (row- vs column-major) is the one real correctness footgun
to get right.** `[UNVERIFIED]` the ndarray↔faer interop crate (`faer-ext` /
ndarray feature) exact name + version compatibility with coil's `ndarray 0.16`
was not verified — confirm before committing.

---

## 5. Retirement impact + the cross-platform win

If faer (pure-Rust, BLAS-class, cross-portable) replaced the `ndarray-linalg`
opt-in, the following would be **RETIRED** (per ADR-0079 §2-Q5, §6, §10–§11):

**Flags / deps retired (Cargo.toml l.35–37, 73):**
- `linalg-backend` feature + its `dep:ndarray-linalg` — the whole opt-in tier
  collapses if faer is good enough to be the *default* accelerator (ADR-0079 §11:
  faer "potentially making the ndarray-linalg native-opt-in unnecessary"). The
  dead `linalg.rs:842` marker stub goes with it.
- `linalg-openblas-static` (l.36) — retires the OpenBLAS Fortran-toolchain
  requirement.
- `linalg-intel-mkl-static` (l.37) — retires the ~300 MB Intel MKL vendor blob +
  Intel license + network-fetch dependency (ADR-0079 §4 Option a).
- The optional `ndarray-linalg = "0.16"` dep (l.73) + its transitive system
  BLAS/LAPACK/Fortran requirement.

**Constraints retired:**
- The **native-x86_64-only** hard binding (ADR-0079 Q5, §6.2): `ndarray-linalg`
  is x86_64-only + needs system BLAS, HARD-EXCLUDED from RV/WASM. faer being
  pure-Rust removes the need to reject the accelerator on `riscv64gc-linux` /
  `wasm32-wasip1`.
- The build-config rejection diagnostic ("coil linalg-backend is native-x86_64
  only; RV/WASM use the pure-Rust path", §7 Phase-3) + the `available_on:
  Vec<TargetMatcher>` manifest guard (§10–§11) needed purely to fence the
  x86-only feature.
- The "two-tier" portability story (§6.1 pure-Rust floor vs §6.2 native-only
  ndarray-linalg).

**Resulting cross-platform story.** Today (ADR-0079 §6.3): "Pure-Rust is the
universal floor; ndarray-linalg is a native-x86_64-only accelerator … HARD-
EXCLUDED from RV/WASM." **With faer:** a SINGLE pure-Rust accelerated path could
be BLAS-competitive AND cross-compile to native-x86_64 + RISC-V + WASM with zero
system deps — collapsing the two-tier matrix into ONE accelerated floor. This
would simultaneously (a) close the ~7–12× BLAS gap (coil-matmul.md), (b)
potentially **lift the O(N⁴)/N≤64 Jacobi cap** on `svd`/`eigh` (faer ships
blocked decompositions + a non-Jacobi eigensolver), and (c) satisfy ADR-0075
Phase-2's "coil under wasmtime" done-means without an x86-only escape hatch.

**`[UNVERIFIED — do NOT assert]`** faer's actual cross-target support (esp.
wasm32-wasip1, RISC-V), its real BLAS-relative perf on arm64, and whether its op
coverage (eig/qr/lstsq/svd/eigh) is correctness-equivalent to LAPACK must be
verified against faer's own docs/benchmarks. ADR-0079 §11 itself frames faer as
needing "a tractability survey" — its suitability is an OPEN question this doc
narrows but does not settle.

---

## 6. Recommendation + next-step plan + RISKS

### 6.1 Recommendation
**ADOPT faer for the gemm path first, behind a verification spike; sequence the
full-op + full-target adoption as the goal, gated on the spike's measured
numbers.** Rationale: (a) it is the root-cause fix for the *backend* gap
coil-matmul.md actually measured; (b) it is the only surveyed path that also
satisfies ADR-0079's zero-system-dep cross-compile mandate (which `ndarray-linalg`
structurally cannot); (c) it opens the deferred ops (`qr`, non-symmetric `eig`)
and the un-capped big-N `svd`/`eigh`. We stop short of a big-bang full swap on
this survey alone because the arm64 matmul residual and the wasm32/RISC-V builds
are unverified (§6.3).

### 6.2 Concrete next-step plan (if ADOPT — recommended)

**Op priority (highest payoff → lowest):**
1. **`matmul` / `dot`** — the headline gap; smallest API surface (`&a * &b`);
   the bench (`cargo bench -p cobrust-coil --bench matmul`) already exists to
   prove or disprove the close.
2. **`svd` / `eigh`** — the biggest correctness+perf case: removes the O(N⁴)
   N≤64 Jacobi cap (`linalg.rs:573-574`) via faer's blocked decompositions.
3. **`solve` / `inv` / `det` / `cholesky`** — pure-Rust LU is competitive at
   small N; faer wins at large N. Lower urgency; migrate after 1–2 land.
4. **Deferred ops** (`qr`, non-symmetric `eig`, `lstsq`, `pinv`) — net-new
   capability faer enables; scope as follow-up sub-ADRs, not the first spike.

**The spike (one ADR-0079 §11 sub-ADR, time-boxed):**
- Add `faer = "0.24"` as a coil dep behind a temporary `linalg-faer` feature
  (do NOT yet retire `ndarray-linalg` — run them side-by-side until the gates
  pass). Stage `Cargo.lock` in the same commit (F64 — dev-dep lockfile staging
  miss).
- Implement the ArrayD↔faer marshalling helper (§4) with explicit row/col-major
  handling; wire `matmul` through `&a * &b`.
- **Differential-test gate (L2 behavior, mandatory):** run the existing
  pure-Rust `matmul` and the new faer `matmul` against each other AND against
  numpy on ≥1000 fuzzed shapes/dtypes (f32, f64), rtol per the M7.4 `rtol=1e-6`
  gate (`@py_compat(numerical)`). Layout-correctness is the #1 thing this gate
  must catch.
- **Bench gate:** re-run `cargo bench -p cobrust-coil --bench matmul`; record T1
  (numpy), T2 (ndarray), **T2′ (faer)**, T3 (coil) at the existing sizes; report
  the faer-vs-Accelerate(arm64) residual in `docs/agent/benchmarks/coil-matmul.md`
  — **this is the number this survey could not retrieve and is the gating
  decision input** (target: ≥ 0.8× of numpy per CLAUDE.md §5.2 perf gate /
  ADR-0079, ideally closing most of the ~12×).
- **Cross-target gate (the ADR-0079 differentiator):** attempt a green
  `wasm32-wasip1` + `riscv64gc-unknown-linux-gnu` build of coil with the
  `linalg-faer` feature ON (cross-build via GH Actions CI per the all-CI-heavy-
  build policy — Mac stays single-crate). A green RV/WASM build is what
  justifies retiring the `ndarray-linalg` two-tier story (§5).
- **Decision point:** if matmul closes most of the gap AND RV/WASM builds are
  green → promote faer to the default accelerator and open the retirement
  sub-ADR (drop l.35–37 + l.73). If matmul closes but RV/WASM is scalar-only →
  ADOPT faer native + keep pure-Rust as the RV/WASM floor (still retires
  `ndarray-linalg`'s OpenBLAS/MKL Fortran tax, a net win). If matmul does NOT
  close on arm64 → DEFER, document the negative result in `docs/agent/findings/`.

### 6.3 RISKS / UNCERTAINTIES (carry-forward for the CTO — verify before promoting)
1. **`[CRITICAL]` No arm64-vs-Accelerate matmul number.** coil's 12× gap was
   measured vs numpy-on-Accelerate (Apple M-series); faer's published benches
   are x86_64 (Ryzen/Xeon) vs MKL/OpenBLAS, NOT vs Accelerate on arm64. The
   residual after adopting faer on Apple silicon is **unproven** — could be
   ~parity or a small multiple. The §6.2 bench gate resolves this.
2. **`[CRITICAL]` faer wasm32 / RISC-V cross-compile NOT verified.** ADR-0079
   cares about RV+WASM. Purity + pulp portability strongly suggest it builds
   there, but no green `wasm32-wasip1` / `riscv64` build of faer 0.24 was
   confirmed; pulp's WASM-SIMD / RISC-V-V story is unconfirmed (may fall back to
   scalar). The §6.2 cross-target gate resolves this — and it is the linchpin of
   the §5 retirement claim.
3. **Exact matmul perf is SELF-REPORTED.** All faer perf figures are the author's
   on the author's hardware; no independent benchmark found, and the official
   benchmark SPA's exact matmul GFLOPS tables were not scrapable. CTO should open
   `faer.veganb.tw/benchmarks/` in a browser to read the actual matmul curve.
4. **ndarray↔faer interop crate (`faer-ext`) name + version compat with `ndarray
   0.16` unverified.** Layout conversion (faer column-major vs ndarray row-major)
   is THE integration footgun; the cost estimate assumes an O(N²) boundary copy.
5. **API churn / bus-factor.** faer is pre-1.0 (0.22→0.24 in <1yr); the
   cited solvers/`Solve`/`Mat`-method API is current as of 0.24 but may shift on
   the next minor bump. Single maintainer.
6. **Exact `.solve()` / `Mat::determinant()` / `Mat::inverse()` signatures** were
   read from docs prose, not line-by-line rustdoc — confirm before the spike.
7. **License posture.** faer is MIT-only (not Apache-2.0-OR-MIT). Compatible with
   the repo bar, but confirm MIT-only inbound is acceptable (ADR-0001).

---

## Sources

External (faer):
- `https://docs.rs/faer/0.24.0/faer/index.html` (matmul `&a * &b` example verbatim)
- `https://docs.rs/faer/latest/faer/linalg/solvers/index.html` (Solve/decompositions)
- `https://docs.rs/pulp/latest/pulp/` (runtime SIMD dispatch; x86-64 + aarch64; not std::simd)
- `https://crates.io/api/v1/crates/faer` (v0.24.0, MIT, rust_version 1.84.0, downloads 2.83M / recent 1.29M, repo Codeberg)
- `https://crates.io/api/v1/crates/faer/0.24.0/dependencies` (full dep list — NO BLAS/LAPACK; gemm, nano-gemm, pulp, libm, …)
- `https://raw.githubusercontent.com/.../faer-rs/main/README.md` (pure Rust; MSRV 1.84.0; links benchmark page)
- `https://faer.veganb.tw/benchmarks/` (SELF-REPORTED, JS SPA — exact matmul tables NOT scrapable; AMD Ryzen + Intel Xeon, x86_64)
- `https://news.ycombinator.com/item?id=40143669` (author: matmul "usually faster, or even with openblas, slower than mkl"; "implement from scratch"; full-piv-LU outlier; rayon-vs-openmp caveat)

Internal (repo, read at HEAD `bba2bcd`):
- `docs/agent/benchmarks/coil-matmul.md` (the ~12× T3/T1 gap; T2/T1=7.04× @256; #157 motivation)
- `docs/agent/adr/0079-coil-deep-numerical-strategy.md` (#157 / §11 open question; ndarray-linalg x86_64-only)
- `crates/cobrust-coil/src/linalg.rs` (8 pure-Rust ops; dead `cfg(linalg-backend)` stub l.842; zero `ndarray_linalg::` usage)
- `crates/cobrust-coil/src/cabi.rs` (`__cobrust_coil_buffer_matmul` l.550) + `src/array.rs` (`Array::matmul` l.501)
- `crates/cobrust-coil/Cargo.toml` (l.25 `default=[]`; l.35–37 opt-in chain; l.73 optional `ndarray-linalg`)
