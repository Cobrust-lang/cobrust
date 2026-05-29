---
doc_kind: adr
adr_id: 0079
title: coil deep-numerical strategy — the C/Fortran-backed numpy surface (linalg LAPACK / FFT / special functions) under the now-live RV+WASM cross-compile constraint; pure-Rust-first, ndarray-linalg opt-in, NEVER PyO3-reverse-bind for a shipped coil
status: draft
date: 2026-05-29
decision_owner: cto
last_verified_commit: 061fde9
relates_to: [adr:0012, adr:0013, adr:0016, adr:0017, adr:0018, adr:0072, adr:0075, adr:0077, adr:0078, "strategy:numpy-translation-architecture", "claude.md:§2.2", "claude.md:§2.4", "claude.md:§2.5", "claude.md:§5.3"]
---

# ADR-0079: coil deep-numerical strategy — LAPACK linalg / FFT / special functions under RV+WASM cross-compile

## 1. Context

ADR-0017 (M7.4) shipped coil's `linalg` surface — 8 ops (`matmul / dot / det /
solve / inv / svd / eigh / cholesky`) — as **pure-Rust kernels on `ndarray = "0.16"`**,
with `ndarray-linalg = "0.16"` declared as an **opt-in** dependency behind the
`linalg-backend` cargo feature (off by default). ADR-0077 began the `coil.Buffer`
operator/index/attribute `.cb` surface (`a + b`, `a[i]`, `a.shape`, `a.dot(b)`).
ADR-0078 established "wrap-the-crate over translate-the-Python" for the backend
surface and showed the per-crate FLAT/MEDIUM/DEEP tractability lens. ADR-0075
(proposed 2026-05-28) put **RISC-V + WebAssembly** cross-compilation on the v0.7.0
agenda, with `wasm32-wasip1` Phase-2 done-means **explicitly naming** a
"`.cb` numpy-on-coil small program runs under `wasmtime`" (ADR-0075 §3 Phase 2,
§6 Phase-2 done-means: `coil.eye(3).sum()` under `wasmtime`).

The `numpy-translation-architecture` strategy doc records the foundational insight:
**numpy ≈ 30% Python-wrapper (translate) + 50% C + 20% Fortran (BLAS/LAPACK — FFI,
NOT translate).** That doc's option was PyO3-reverse-bind to real numpy's C/Fortran
`.so`/`.dylib`. **That option does not apply to a *shipped* coil** — coil does NOT
FFI to numpy's C; it **reimplements** numpy on Rust `ndarray`. The deep-numerical
features numpy backs with C/Fortran libraries are exactly the surface this ADR
governs:

- **linalg** — `solve / inv / det / eig / svd / qr / cholesky / lstsq` (numpy →
  LAPACK: `*gesv`, `*getri`, `*geev`/`*syevd`, `*gesdd`, `*geqrf`, `*potrf`,
  `*gelsd`).
- **FFT** — `fft / ifft / rfft / irfft` (numpy → pocketfft, historically FFTPACK).
- **special functions + advanced random** — `gamma / beta / erf / bessel` and the
  distribution sampling numpy backs with C (`numpy.random` C generators,
  `scipy.special`-adjacent surface).

**This ADR is DESIGN ONLY (doc, zero src).** It decides the backend strategy for
the deep-numerical surface, designs the `.cb` `coil.linalg.*` / `coil.fft.*`
sub-namespace shape, produces the honest **portability matrix** ({feature group} ×
{native / RV / WASM} × {ndarray-linalg vs pure-Rust}) that is its most useful
artifact, and maps a Phase-1 first surface with an ADR-0077-§9-style implementation
map. It unblocks a future fill-in-the-blanks impl sprint and sets the precedent for
how every cross-target-sensitive numerical capability is wired.

### 1.1 The gap is NOT what the framing first suggests — verified at `061fde9`

The load-bearing finding from reading the source (NO-OVERCLAIM, §"Evidence"):

- **`solve / inv / det / svd / eigh / cholesky` already exist** in
  `crates/cobrust-coil/src/linalg.rs` (852 LOC, 8 `pub fn`s verified:
  `matmul`@175, `dot`@190, `det`@427, `solve`@464, `inv`@503, `cholesky`@532,
  `eigh`@575, `svd`@673), implemented **pure-Rust** (LU partial-pivot for
  det/solve/inv; one-sided Jacobi for svd; symmetric Jacobi for eigh), passing the
  ADR-0017 `rtol=1e-6` differential gate on cond ≤ 100 inputs. So "coil lacks
  solve/inv/det" is **false** — those ship today.
- **`ndarray-linalg` is declared-but-NOT-actually-wired.** The `linalg-backend`
  feature (`Cargo.toml:35`) gates `dep:ndarray-linalg`, but the only `#[cfg(feature
  = "linalg-backend")]` site in `linalg.rs` is a dead `_backend_marker() ->
  "ndarray-linalg"` stub (`linalg.rs:842-846`). **Turning the feature on does NOT
  today swap any kernel to LAPACK** — it merely pulls the dep into the build. The
  ADR-0017 §2 "when the feature is on, M7.4 swaps to ndarray-linalg kernels"
  sentence describes *intended* wiring that the impl never landed. This is recorded
  here as honest debt (it is not a regression — the pure-Rust path is correct; the
  acceleration path is simply unrealised).
- The **genuine gaps** are therefore:
  1. **linalg ops the ADR-0017 8-set explicitly deferred** (`qr / lstsq / pinv /
     norm / matrix_rank`; batched rank-3+; complex dtype; Householder/QR-based
     svd/eigh for N > 64 — the current Jacobi is O(N⁴), capped at N ≤ 64 per
     ADR-0017 §"Documented unstable cases").
  2. **FFT — entirely absent.** There is no `fft.rs`; no `fft`/`ifft`/`rfft` symbol
     anywhere in coil (verified: zero matches for `fft` in `src/`).
  3. **special functions — entirely absent** (`gamma`/`beta`/`erf`/`bessel`); coil
     `random.rs` covers `normal`/`uniform`/`integers`/`choice` (ADR-0018) but no
     `gamma`/`beta`/`poisson`/`binomial` distribution sampling and no special-fn
     surface.
  4. **the `.cb` sub-namespace surface** — `coil.linalg.solve(a, b)` /
     `coil.fft.fft(a)`. The ADR-0072 manifest + ADR-0077 operator chain expose
     coil's *flat* surface (`coil.zeros`, `a + b`); a *sub-namespace*
     (`coil.linalg.*`, `coil.fft.*`) has no manifest shape yet.
  5. **the cross-compile constraint that did not exist when ADR-0017 was written**
     (ADR-0075 is 2026-05-28; ADR-0017 is 2026-04-30) — and it is load-bearing,
     because the obvious "accelerate linalg with ndarray-linalg" answer is exactly
     the answer that **breaks the RV/WASM targets** (§4).

### 1.2 Current-mechanism map (verified at `061fde9`)

How coil's surface reaches `.cb` today, so §8's implementation map names real seams:

- **Crate** — `crates/cobrust-coil` (`Cargo.toml` verified): `crate-type =
  ["rlib", "cdylib", "staticlib"]`; deps `ndarray = "0.16"`, `ndarray-linalg =
  "0.16"` (optional, gated `linalg-backend`), `rand`/`rand_pcg`/`rand_distr`
  (ADR-0018 random), `pyo3` (optional). Features: `default = []`, `pyo3`,
  `linalg-backend`, `linalg-openblas-static` (= `linalg-backend` +
  `ndarray-linalg/openblas-static`), `linalg-intel-mkl-static`.
- **Rust API** — `lib.rs` re-exports `linalg::{matmul, dot, det, solve, inv, svd,
  eigh, cholesky, SvdResult, EighResult}` (verified `lib.rs:122`). These are
  Rust-internal; the `.cb` chain reaches them only through `cabi.rs` shims.
- **`.cb` surface** — `cabi.rs` (39 629 bytes) carries the `__cobrust_coil_*`
  C-ABI shims (16 manifest rows at ADR-0077's `936f13c`; the P0 increment added
  `mgrid/ogrid/broadcast_to/mean/median/std/var/split`). ADR-0077 §1 confirms the
  manifest model is a **flat** `module → fn` map keyed off `COIL_BUFFER_ADT`; there
  is **no namespacing** of the symbol space — every coil fn is `coil.<name>` at the
  `.cb` source level.
- **Manifest** — `crates/cobrust-types/src/ecosystem.rs` (`COIL_BUFFER_ADT` block +
  `lookup_handle_method`, per ADR-0077 §1.1). The manifest row carries `(params,
  ret, py_compat_tier)`; ADR-0077 §1.1 verified `synth_bin` rejects `Adt+Adt`
  today, so any new typed surface touches typecheck.
- **Build/link** — ADR-0072 8/8: `libcoil.a` static-linked per-import by `cobrust
  build`; `__cobrust_coil_*` recognised by the coil-prefix arm in
  `cobrust-cli/src/build/intrinsics.rs`. **Cross-target**: ADR-0075 §5 makes
  `libcoil.a` itself a cross-build target (`cargo build -p cobrust-coil
  --target=<triple>`) — which is exactly why a coil feature that pulls a
  non-cross-compilable dep is a cross-build hazard (§4).

## 2. Decision (summary)

| # | Question | Decision |
|---|---|---|
| Q1 | linalg-acceleration backend | **Pure-Rust default stays the SHIPPED, cross-portable path; `ndarray-linalg` becomes a genuinely-wired (today it is a dead stub, §1.1) *native-only* opt-in via `linalg-backend`, NEVER a default, NEVER on RV/WASM.** Reject direct C/Fortran FFI (Option b — re-derives ndarray-linalg's cabi work). Reject pure-Rust-only-forever (Option c — leaves a perf gap on big native workloads). **Categorically reject PyO3-reverse-bind to real numpy (Option d)** for a *shipped* coil — it re-introduces a Python runtime dependency, defeating coil's whole reason to exist + breaking RV/WASM + ADR-0078's "Cobrust IS Rust" thesis. |
| Q2 | FFT backend | **Pure-Rust `rustfft` (complex) + `realfft` (real-to-complex) — verified pure-Rust with explicit WASM support (§3.2).** No FFTW FFI. FFT is the *cleanest* new numerical surface precisely because the best Rust crate is portable to all three targets, so FFT does NOT carry the linalg cross-compile tension. |
| Q3 | special functions / advanced random | **Pure-Rust `statrs::function` (gamma/beta/erf — verified, libm-backed) for special fns; extend coil `random.rs` (`rand_distr`) for gamma/beta/poisson/binomial distributions.** **Bessel is an honest gap** — absent from `statrs`, `special`, and the surveyed crates (§3.3); deferred to a sub-ADR (either `puruspe`, a vetted Bessel crate, or a pure-Rust port). |
| Q4 | `.cb` sub-namespace shape (`coil.linalg.*`, `coil.fft.*`) | **Manifest-namespaced symbol, NOT a sub-module handle, NOT a new manifest schema.** `coil.linalg.solve` resolves to a flat runtime symbol `__cobrust_coil_linalg_solve` via a `(module_path, fn)` manifest key; the `.cb` `coil.linalg` is a **dotted name in the import-manifest namespace**, mirroring numpy's `np.linalg.*` shape (§5). Reject a `coil.linalg`-as-handle (a value you bind) — there is no state to hold; it is a namespace, not an object. |
| Q5 | cross-compile interaction (the load-bearing constraint) | **The feature graph must guarantee: `default` features cross-compile to native + RV + WASM with zero system deps. `linalg-backend` (and its BLAS sub-features) are native-x86_64-only and MUST be rejected (clear diagnostic) on RV/WASM targets — they pull `ndarray-linalg`, which is x86_64-only + needs system BLAS/LAPACK (§3.1, §4).** Pure-Rust linalg/FFT/special is the *only* path that satisfies ADR-0075 Phase-2's "coil under wasmtime" done-means. |
| Q6 | Phasing + Phase-1 first surface | **Phase 1 = wire the EXISTING `det / solve / inv` through the `.cb` `coil.linalg.*` namespace** (the highest-value, most-tractable, already-implemented, fully-portable first surface — zero new numerical kernels, pure `.cb`-surface + manifest-namespace work). FFT (rustfft), then `qr / lstsq`, then special fns, then the genuinely-wired ndarray-linalg native acceleration, then eig (non-symmetric) / big-N svd are later phases (§7). |

## 3. The gap surface — C/Fortran-backed numpy features, grouped (NO-OVERCLAIM §3)

Each row: numpy's C/Fortran backer, the Rust crate that covers it (verified against
docs.rs at authoring time per ADSD §4), and coil's current state.

### 3.1 linalg group (numpy → LAPACK)

| numpy op | LAPACK routine (real) | coil today (`061fde9`) | Rust crate for acceleration |
|---|---|---|---|
| `solve(a, b)` | `*gesv` (LU solve) | **EXISTS** (pure-Rust LU, `linalg.rs:464`) | `ndarray-linalg::Solve` |
| `inv(a)` | `*getrf` + `*getri` | **EXISTS** (pure-Rust `solve(a, I)`, `:503`) | `ndarray-linalg::Inverse` |
| `det(a)` | `*getrf` (LU, ∏diag) | **EXISTS** (pure-Rust, `:427`) | (ndarray-linalg has no first-class det in 0.16 docs — §"Evidence"; compute via LU) |
| `cholesky(a)` | `*potrf` | **EXISTS** (pure-Rust, `:532`) | `ndarray-linalg::cholesky` |
| `eigh(a)` (symmetric) | `*syevd`/`*heevd` | **EXISTS** (pure-Rust Jacobi, N ≤ 64, `:575`) | `ndarray-linalg::Eigh` |
| `svd(a)` | `*gesdd`/`*gesvd` | **EXISTS** (pure-Rust Jacobi, N ≤ 32, `:673`) | `ndarray-linalg::SVD` |
| `eig(a)` (non-symmetric) | `*geev` | **MISSING** | `ndarray-linalg::Eig` |
| `qr(a)` | `*geqrf` + `*orgqr` | **MISSING** | `ndarray-linalg::QR*` |
| `lstsq(a, b)` | `*gelsd` | **MISSING** | `ndarray-linalg::least_squares` |
| `pinv / norm / matrix_rank` | (composed) | **MISSING** | composed / `ndarray-linalg` norms |

**Crate verified (§"Evidence"):** `ndarray-linalg` 0.16 "leverages LAPACK's routines
using the bindings provided by blas-lapack-rs/lapack" — it **requires a system
BLAS/LAPACK backend; it cannot work without Fortran/C BLAS**, and (per the GitHub
README) **only supports x86_64**, with backend features `openblas-static` /
`intel-mkl-static` / `netlib`. The library-author guidance is explicit: a library
should NOT link a backend (forcing it on downstream). coil's `linalg-openblas-static`
/ `linalg-intel-mkl-static` sub-features (`Cargo.toml:36-37`) do exactly that
linking — acceptable only because they are opt-in.

### 3.2 FFT group (numpy → pocketfft / FFTPACK)

| numpy op | numpy backer | coil today | Rust crate |
|---|---|---|---|
| `fft / ifft` (complex) | pocketfft (C++) | **MISSING** | `rustfft` 6.4.1 — **pure Rust, no C deps, explicit WASM support** (`FftPlannerWasmSimd`); forward+inverse via `FftDirection` |
| `rfft / irfft` (real) | pocketfft | **MISSING** | `realfft` 3.5.0 — pure-Rust wrapper on `rustfft`, real→complex + complex→real |
| `fft2 / fftn` (multi-dim) | pocketfft | **MISSING** | composed over `rustfft` per-axis |

**Crate verified (§"Evidence"):** `rustfft` 6.4.1 is "a high-performance FFT library
written in pure Rust", "no C dependencies", with a dedicated `FftPlannerWasmSimd` for
WASM. `realfft` 3.5.0 wraps it for real-valued data. **FFT is the group with the
best portability story** — the leading crate is pure-Rust AND wasm-aware.

### 3.3 special functions + advanced random (numpy/scipy → C)

| Surface | numpy/scipy backer | coil today | Rust crate |
|---|---|---|---|
| `gamma / lgamma` | C (`cephes`-lineage) | **MISSING** | `statrs::function::gamma` (verified present) |
| `beta` | C | **MISSING** | `statrs::function::beta` (verified present) |
| `erf / erfc` | C | **MISSING** | `statrs::function::erf` (verified present) |
| `factorial` | C | **MISSING** | `statrs::function::factorial` (verified present) |
| **`bessel` (j/y/i/k)** | C (`cephes`) | **MISSING** | **HONEST GAP** — absent from `statrs`, absent from the `special` crate (verified: `special` has Gamma/Beta/Error/Elliptic/LambertW, NO Bessel); candidate `puruspe` needs vetting (§9) |
| `gamma / beta / poisson / binomial` sampling | numpy C RNG | **MISSING** (coil has normal/uniform/integers/choice per ADR-0018) | `rand_distr` (Gamma/Beta/Poisson/Binomial — already a coil dep) |

**Crate verified (§"Evidence"):** `statrs::function` provides `gamma`, `beta`, `erf`,
`factorial` modules (Bessel NOT listed); the `special` crate is `libm`-backed (Gamma /
Beta / Error / Elliptic / LambertW, no Bessel). **Bessel is the one numerical surface
with no clean verified pure-Rust crate in this survey** — recorded as a sub-ADR gap,
NOT hand-waved.

## 4. The decision — linalg backend strategy (≥3 options weighed, the load-bearing §)

The question: should coil's deep linalg (and its acceleration) be (a) ndarray-linalg,
(b) direct C/Fortran FFI, (c) pure-Rust-only, or (d) PyO3-reverse-bind to real numpy?
The **cross-compile constraint (ADR-0075, now live) is the tie-breaker.**

### Option (a) — ndarray-linalg (Rust bindings to system BLAS/LAPACK)

Already an optional coil feature (`linalg-backend`, §1.2). Needs a system BLAS/LAPACK,
or the bundled `openblas-static` (Fortran toolchain) / `intel-mkl-static` (~300MB
vendor blob; Intel license — ADR-0017 §2 rejected it as a *default*).

- **Correctness: best (1.0).** LAPACK is the numerical gold standard; numpy itself
  uses it. eig/svd/qr for large N are battle-tested where the pure-Rust Jacobi is
  O(N⁴)-capped.
- **Build-portability: worst.** Verified: ndarray-linalg **only supports x86_64** and
  **cannot work without a system Fortran/C BLAS**. `openblas-static` needs a Fortran
  toolchain (not portable); `intel-mkl-static` needs network + is x86-only + carries
  a vendor license.
- **§2.5: neutral.** `coil.linalg.solve(a, b)` shape is backend-independent — the LLM
  writes the same `.cb` whether the kernel is pure-Rust or LAPACK.
- **Cross-compile (the decisive axis): FAILS RV + WASM.** `ndarray-linalg` is x86_64
  only; it does NOT cross-compile to `riscv64gc-unknown-linux-gnu` nor
  `wasm32-wasip1`. Selecting it on a cross-target either fails the build (no x86 BLAS
  for the target) or — worse — silently links a host-arch blob. ADR-0075 §5 cross-
  builds `libcoil.a` per-target; a `linalg-backend`-on cross-build is a hard failure.

### Option (b) — direct C/Fortran FFI to BLAS/LAPACK/FFTW

Write coil's own `extern "C"` bindings to system `liblapack`/`libblas`/`libfftw3`.

- **Correctness: best (1.0)** — same LAPACK/FFTW numpy uses.
- **Build-portability: worst + most work.** All of (a)'s cross-compile failure, PLUS
  hand-writing + maintaining the cabi (ndarray-linalg already did this work). Strictly
  dominated by (a) for native and by (c) for cross.
- **Rejected:** re-derives ndarray-linalg's binding layer for no portability gain;
  more `unsafe`; more `Cargo.lock`/system-dep surface (F64-adjacent risk).

### Option (c) — pure-Rust reimplementation (the SHIPPED default, today's reality)

What coil ships *now* for the 8-op subset (LU + Jacobi). Extend with `rustfft`/
`realfft`/`statrs` (all pure-Rust) for FFT/special.

- **Correctness: good, with documented bounds.** Passes `rtol=1e-6` on cond ≤ 100,
  N ≤ 64 (ADR-0017 gate). **Honest limit:** Jacobi svd/eigh is O(N⁴) — inadequate for
  large N, and there is no pure-Rust non-symmetric `eig` in coil today (the hardest to
  reimplement well — the QR-algorithm with shifts is genuinely involved).
- **Build-portability: BEST.** Pure-Rust + `libm` cross-compiles to native + RV + WASM
  with zero system deps. This is the ONLY option that satisfies ADR-0075 Phase-2's
  "coil under wasmtime" done-means.
- **§2.5: identical** to (a) at the `.cb` surface.
- **Performance: the gap.** Pure-Rust LU at small N is competitive (ADR-0017 §2); at
  large N + for eig/svd, LAPACK wins. This is the one real cost — and it is bounded by
  documenting the N-cap + offering (a) as a *native* opt-in.

### Option (d) — PyO3-reverse-bind to real numpy's C/Fortran

The `numpy-translation-architecture` strategy doc's option: call numpy's
`multiarray.so` / LAPACK from Cobrust via PyO3, exactly as CPython does.

- **CATEGORICALLY REJECTED for a *shipped* coil.** (1) It re-introduces a **Python
  runtime + a numpy install** as a *runtime* dependency of every `.cb` binary that
  touches linalg — defeating coil's entire reason to exist (a no-Python-runtime numpy)
  and contradicting ADR-0078 §2's "Cobrust IS Rust → direct native link" thesis.
  (2) It does NOT cross-compile to RV/WASM (no CPython under `wasm32-wasip1`; ADR-0075
  excludes even network libs, let alone an embedded interpreter). (3) The strategy
  doc's PyO3-reverse-bind is the right tool for the *translation* project (importing
  real numpy's wrapper Python while FFI'ing its C) — it is the **wrong tool for a
  standalone reimplemented coil that ships as a static `.a`.** The two are different
  artifacts; this ADR governs the shipped coil, and (d) is out of scope for it.

### Decision + justification

**Adopt (c) as the SHIPPED default + cross-portable path; keep (a) as a genuinely-wired
native-only opt-in (`linalg-backend`); reject (b) and (d).**

The justification is the cross-compile tie-breaker made load-bearing: ADR-0075 turned
RV/WASM from hypothetical to a v0.7.0 done-means that *names coil*. A backend that does
not cross-compile cannot be the default for a library whose `.a` is cross-built
per-target. Pure-Rust is the only option that ships on all three targets. ndarray-linalg
remains the **correct native accelerator** for the perf-bound large-N workloads where
pure-Rust Jacobi is inadequate — but ONLY as an opt-in that is *rejected at build-config
time* on RV/WASM (Q5). This mirrors ADR-0017 §2's original "pure-Rust default, opt-in
acceleration" call, but now (i) makes the opt-in *actually wired* (it is a dead stub
today, §1.1), and (ii) hard-binds the opt-in to native-only because the cross-targets
did not exist when ADR-0017 was written.

## 5. The `.cb` sub-namespace surface + elegance (Q4)

numpy groups deep ops under sub-namespaces: `np.linalg.solve(a, b)`,
`np.fft.fft(a)`, `np.random.normal(...)`. §2.5 (maximize-training-data-overlap) says
coil's `.cb` surface should match: `coil.linalg.solve(a, b)`, `coil.fft.fft(a)`.

### How a sub-namespace maps to the ADR-0072 import-manifest

Today the manifest is a **flat** `coil → { fn → sig }` map (§1.2; ADR-0077 §1). A
two-level name (`coil.linalg.solve`) needs a decision on the manifest shape.

- **(Option Q4-a — manifest-namespaced flat symbol) [CHOSEN].** Extend the manifest
  *key* from `(module, fn)` to a dotted `(module_path, fn)` where `module_path` is
  `coil.linalg` / `coil.fft`. The `.cb` resolver already walks dotted attribute access
  (`coil.zeros` is `Attr(coil, zeros)`); `coil.linalg.solve` is
  `Attr(Attr(coil, linalg), solve)` — the resolver gains a rule: an `Attr` whose base
  resolves to a *known sub-namespace of an imported ecosystem module* is itself a
  namespace, and the leaf `Attr` resolves to a flat runtime symbol
  `__cobrust_coil_linalg_solve`. **No sub-module handle, no value to bind, no new ADT.**
  This is the minimal extension and keeps the symbol space flat at the C-ABI
  (`__cobrust_coil_linalg_*` / `__cobrust_coil_fft_*` — a new prefix sibling of the
  existing `__cobrust_coil_*` recognised by `intrinsics.rs`).
- **(Option Q4-b — `coil.linalg` is a handle you bind)** — `let la = coil.linalg; la.solve(a, b)`.
  Rejected: there is **no state** in a namespace; making it a bindable handle invents a
  drop-eligible object for nothing, violates §5.1 (newtypes only where invariants
  exist), and diverges from numpy (you don't bind `np.linalg` either — `from numpy import linalg`
  is rare; `np.linalg.solve` is the idiom).
- **(Option Q4-c — flatten to `coil.linalg_solve`)** — a single underscore-joined flat
  name. Rejected: it is NOT what numpy/LLMs write (`np.linalg.solve`, dotted), so it
  loses §2.5 training-overlap for a trivial implementation saving; the dotted resolver
  rule (Q4-a) is small.

**Decision: Q4-a — dotted namespace resolves to a flat per-namespace-prefixed runtime
symbol; `coil.linalg` / `coil.fft` are namespaces, not handles.** This is a new but
small manifest capability (a sub-namespace table) and it generalizes: any future
ecosystem module wanting numpy-style sub-namespaces (`coil.random.*` — coil already has
the random *kernels* from ADR-0018 but exposes them flat) reuses the same rule.

### Elegance — the footgun ledger (drop numpy's legacy)

Per the elegance law (no legacy footguns), coil's deep surface DROPS numpy's
accumulated mistakes:

| numpy footgun | Cobrust coil decision |
|---|---|
| `np.matrix` (legacy 2-D matrix class with `*` = matmul) | **DROPPED.** Only `Buffer` exists; `*` is elementwise (ADR-0077 Q1), `@`/`.dot`/`coil.linalg.*` is matmul. numpy itself deprecates `np.matrix`. |
| `np.linalg.solve` broadcasting-stacked silent batch | **Phase-1 rejects rank-3+ with a clear diagnostic** (ADR-0017 already does); batched is an explicit later phase, not a silent surprise. |
| `np.fft` default `norm=None` (un-normalised, asymmetric ifft scaling — a classic bug source) | **coil.fft makes the `norm` choice explicit in the surface** (`coil.fft.fft(a)` documents its normalization; a `norm=` is a later keyword-arg, NOT a silent default that bites round-trips). |
| `eig` returning complex eigenvalues from a real matrix silently | **Honest typing:** real-only coil (ADR-0017 §3) means non-symmetric `eig` (which genuinely produces complex results) is **deferred to the Complex-dtype tier** (ADR-0021) rather than silently lying about the result type. |
| `numpy.random` global mutable RNG state (`np.random.seed` mutates a process-global) | **Already dropped** by ADR-0018 (coil random is a `Generator` newtype, no global state); the deep-random extension (gamma/poisson) inherits this. |
| integer-input linalg silent f64 upcast | **Already dropped** by ADR-0017 §3 (`LinalgDtypeUnsupported` instead of silent promote). |

## 6. The portability matrix — the ADR's most useful artifact (Q5)

{feature group} × {native-x86_64 / RISC-V (riscv64gc-linux) / WASM (wasm32-wasip1)} ×
{ndarray-linalg vs pure-Rust}. Facts verified §3 + §"Evidence". `✓` = works, `✗` =
does not cross-compile / unsupported, `~` = works but with a documented limit.

### 6.1 Pure-Rust path (coil `default` features)

| Feature group | native-x86_64 | RISC-V (rv64-linux) | WASM (wasm32-wasip1) | Limit |
|---|---|---|---|---|
| linalg `solve/inv/det/cholesky` (LU) | ✓ | ✓ | ✓ | none for small N |
| linalg `svd/eigh` (Jacobi) | ~ | ~ | ~ | N ≤ 64 (O(N⁴)); ADR-0017 documented |
| linalg `eig` (non-symmetric) | ✗ | ✗ | ✗ | NOT IMPLEMENTED (any path); deferred |
| linalg `qr/lstsq/pinv/norm` | ✗ | ✗ | ✗ | NOT IMPLEMENTED; Phase-3+ pure-Rust |
| FFT `fft/ifft` (`rustfft`) | ✓ | ✓ | ✓ (+ WASM-SIMD) | pure-Rust, wasm-aware (§3.2) |
| FFT `rfft/irfft` (`realfft`) | ✓ | ✓ | ✓ | pure-Rust wrapper on rustfft |
| special `gamma/beta/erf` (`statrs`) | ✓ | ✓ | ✓ | libm-backed, portable |
| special `bessel` | ✗ | ✗ | ✗ | no verified pure-Rust crate (§3.3); sub-ADR |
| advanced random `gamma/poisson/...` (`rand_distr`) | ✓ | ✓ | ✓ | dep already present |

### 6.2 ndarray-linalg path (coil `linalg-backend` feature ON)

| Feature group | native-x86_64 | RISC-V (rv64-linux) | WASM (wasm32-wasip1) | Note |
|---|---|---|---|---|
| linalg `solve/inv/det/cholesky/svd/eigh/qr/lstsq/eig` | ✓ (with system BLAS or `*-static`) | ✗ | ✗ | ndarray-linalg is **x86_64-only + needs system Fortran/C BLAS** (§3.1, verified) |
| (everything else — FFT/special) | n/a | n/a | n/a | ndarray-linalg covers linalg ONLY; FFT/special always take the pure-Rust path |

### 6.3 The matrix's verdict (one sentence)

**Pure-Rust is the universal floor (works on all three targets for everything except
the genuinely-unimplemented eig/qr/lstsq + the bessel gap); ndarray-linalg is a
native-x86_64-only accelerator for big-N/eig linalg and is HARD-EXCLUDED from RV/WASM
builds** — so the only way ADR-0075 Phase-2's "coil under wasmtime" survives is the
pure-Rust path, and that is why it stays the shipped default.

## 7. Phasing + Phase-1 first surface (Q6)

Each phase: scope, done-means, layers, portability note.

### Phase 1 (≤1-2 day) — wire EXISTING `det / solve / inv` through `coil.linalg.*`

**Why this first (highest-value × most-tractable × most-portable):** the kernels
**already exist + pass the rtol gate** (§1.1); the only work is the `.cb`-surface +
manifest-namespace + cabi wiring. Zero new numerical code; fully pure-Rust; works on
all three targets. `solve` is the single most-used numpy linalg op (`np.linalg.solve`
is THE linear-systems idiom an LLM reaches for); `det`/`inv` are its natural cohort and
share the LU machinery. This is the precise analogue of ADR-0077's "ship the tractable
mechanism first" and ADR-0078's "lowest-risk chain extension first."

**Scope:** `coil.linalg.solve(a, b) -> Buffer`, `coil.linalg.det(a) -> f64`,
`coil.linalg.inv(a) -> Buffer` — the dotted-namespace resolver (Q4-a) + three cabi
shims wrapping the *existing* `linalg::{solve, det, inv}` Rust fns.

**Done-means:** a `.cb` program
`let a = coil.array_f64(...); let b = coil.ones(3); let x = coil.linalg.solve(a, b);
coil.print_buffer(x); let d = coil.linalg.det(a); print(d)` type-checks, resolves
`coil.linalg.solve` → `__cobrust_coil_linalg_solve`, links, runs, prints the solved
vector + determinant, exits 0, every Buffer dropped exactly once
(`coil::cabi::DROP_COUNT`). **Cross-target done-means (the ADR-0075 tie-in):** the SAME
program cross-builds with `--target=wasm32-wasip1` and runs under `wasmtime` (no system
BLAS, pure-Rust path). ≥3 negatives: `coil.linalg.solve(a)` (arity — clear diagnostic),
`coil.linalg.solveX(...)` (unknown sub-namespace member — clear diagnostic),
non-square `a` (runtime `LinalgShapeError` per ADR-0017).

### Phase 2 — FFT (`coil.fft.fft / ifft / rfft / irfft` via rustfft/realfft)

**Scope:** add `rustfft` + `realfft` deps; `fft.rs` module + `coil.fft.*` namespace +
shims. **Done-means:** `coil.fft.fft(a)` round-trips against numpy `rtol=1e-6`;
`ifft(fft(a)) == a`; cross-builds + runs under wasmtime. **Portability:** ✓ all three
targets (rustfft is wasm-aware). **NEW dep → stage `Cargo.lock` (F64).**

### Phase 3 — `coil.linalg.qr / lstsq` (pure-Rust) + the genuinely-wired ndarray-linalg native opt-in

**Scope:** (a) pure-Rust `qr` (Householder) + `lstsq` (via qr) — portable; (b) FINALLY
wire `linalg-backend` so the feature actually swaps `solve/inv/svd/eigh/qr/lstsq` to
ndarray-linalg kernels (today it is a dead stub, §1.1), gated native-x86_64-only with a
build-config rejection on RV/WASM (Q5). **Done-means:** qr/lstsq pass rtol gate
pure-Rust on all targets; `--features linalg-backend` on x86_64 passes the same gate via
LAPACK + the perf bench shows the accel; `--features linalg-backend
--target=wasm32-wasip1` is REJECTED with a clear `coil linalg-backend is native-x86_64
only; RV/WASM use the pure-Rust path` diagnostic.

### Phase 4 — special functions (`coil.special.gamma/beta/erf` via statrs) + advanced random

**Scope:** `statrs::function` for gamma/beta/erf; extend coil `random.rs` with
`rand_distr` Gamma/Beta/Poisson/Binomial. **Done-means:** special fns rtol-match scipy/
numpy; distribution sampling passes a KS-test gate (ADR-0018 precedent). **Bessel
EXCLUDED** (§9 sub-ADR). **Portability:** ✓ all three (libm/rand_distr).

### Phase 5+ (deferred, sub-ADRs §9) — non-symmetric `eig`, big-N svd/eigh, batched, complex

Householder+QR-shift `eig`, Householder-based svd/eigh for N > 64, batched rank-3+,
Complex-dtype tier (ADR-0021). These are the genuinely-hard numerical kernels; each its
own sub-ADR.

## 8. Phase-1 implementation map (fill-in-the-blanks for the impl sprint)

Mirrors ADR-0077 §9 / ADR-0078 §6.1. Line anchors at `061fde9`; the impl sprint
re-greps the named functions. Phase-1 scope = `coil.linalg.{solve, det, inv}` over the
EXISTING kernels.

| Layer | File | Function / site | Edit |
|---|---|---|---|
| **Manifest** | `crates/cobrust-types/src/ecosystem.rs` | the `COIL_BUFFER_ADT` block + `lookup_handle_method` (ADR-0077 §1.1) | add a **sub-namespace table** `lookup_coil_subnamespace(path, fn) -> Option<EcoSig>` (or a dotted-key extension of the existing coil fn map): rows `("coil.linalg","solve") -> __cobrust_coil_linalg_solve` (params `[Buffer, Buffer]`, ret `coil_buffer_ty()`), `("coil.linalg","det") -> ..._det` (ret `Ty::Float`), `("coil.linalg","inv") -> ..._inv` (ret `Buffer`); tier `Numerical { rtol: 1e-6 }` (ADR-0017 gate; ADR-0052c `PyCompatTier`) |
| **Typecheck** | `crates/cobrust-types/src/check.rs` | `synth_expr` Attr arm (the `Attr(Attr(coil, linalg), solve)` resolution) | dotted-namespace rule: an `Attr` whose base is a known coil sub-namespace (`coil.linalg`) resolves the leaf via `lookup_coil_subnamespace`; arity/type-check the call args as for any ecosystem fn |
| **MIR** | `crates/cobrust-mir/src/lower.rs` | `try_lower_ecosystem_call` @2011 + `emit_ecosystem_call` @2075 (anchors at 061fde9; impl sprint re-greps the named fns) | `coil.linalg.solve(a, b)` retargets to `Constant::Str("__cobrust_coil_linalg_solve")` via `emit_ecosystem_call` — **no new mechanism**, the sub-namespace leaf is just a different symbol string; reuse the borrow-2-return-fresh-handle path verbatim |
| **Codegen** | `crates/cobrust-codegen/src/llvm_backend.rs` | the coil extern block (ADR-0077 §1.1 @2854-2895) | add extern decls `__cobrust_coil_linalg_solve` / `_inv` (`ptr,ptr->ptr` / `ptr->ptr`), `_det` (`ptr->f64`) |
| **CLI build** | `crates/cobrust-cli/src/build/intrinsics.rs` | the `__cobrust_coil_*` prefix recognizer (ADR-0077 §9) | confirm `__cobrust_coil_linalg_*` matches the existing coil prefix (likely already prefix-matched on `__cobrust_coil_`; verify the new sub-prefix is covered) |
| **Runtime** | `crates/cobrust-coil/src/cabi.rs` | new shims, mirror `broadcast_to` (ADR-0077 §1.1 @262) | `__cobrust_coil_linalg_solve`(borrow 2 Buffers → call `crate::linalg::solve(&a, &b)?` → fresh box, or `__cobrust_panic` on `LinalgShapeError`/`SingularMatrix`); `_inv` (borrow 1 → `linalg::inv`); `_det` (borrow 1 → `linalg::det`, return the scalar `f64` from the 0-d Array — mirror ADR-0077 Q2's "0-d → `f64`" honesty) |
| **Runtime — ZERO new numerical code** | `crates/cobrust-coil/src/linalg.rs` | `solve`@464 / `det`@427 / `inv`@503 | **unchanged** — they already exist + pass the gate; Phase-1 only *wires* them. This is why Phase-1 is the cheapest deep-numerical proof |
| **Cargo** | `crates/cobrust-coil/Cargo.toml` | — | **no new dep for Phase-1** (pure-Rust kernels already there). FFT/special phases add deps + must **stage `Cargo.lock`** (F64) |
| **Build cross** | (ADR-0075 §5 cross-build of `libcoil.a`) | — | Phase-1 done-means includes the wasm32-wasip1 cross-run; **no `linalg-backend`** in default features keeps the cross-build clean (Q5) |
| **Tests** | `crates/cobrust-coil/src/cabi.rs` `#[cfg(test)]` + a new CLI E2E | mirror `broadcast_to_round_trip` + a `coil_linalg_namespace_e2e.rs` | drop-once assertions + the §7 Phase-1 done-means program + the wasmtime cross-run (gate on wasmtime available, ADR-0075 §5 harness) |
| **Docs** | `docs/{agent,human/zh,human/en}` coil specs | add the `coil.linalg.*` namespace rows | per CLAUDE.md §3.3 sync rule, in the impl commit |

**Honest difficulty read:** Phase-1 is **lower-risk than ADR-0077's Phase-1**, because
(1) the numerical kernels already exist + pass the gate (zero new math, the hardest
part is free), (2) it rides the ADR-0077/0073 ecosystem-call chain verbatim at MIR/
codegen (the *only* new compiler-internals work is the dotted sub-namespace resolution
rule in typecheck — a small, bounded edit), and (3) it adds no new dep so no
`Cargo.lock` risk. The one genuinely-new capability is the **sub-namespace manifest
shape** (Q4-a) — and it is the strategic payoff, because it unblocks `coil.fft.*`,
`coil.special.*`, and a future `coil.random.*` with the same rule.

## 9. §2.5 analysis — does the `.cb` surface match what an LLM writes?

| Surface | numpy idiom | Cobrust shape | §2.5 overlap | Forced divergence |
|---|---|---|---|---|
| `coil.linalg.solve(a, b)` | `np.linalg.solve(a, b)` | identical (Ph1) | **1.0** | none (`coil` vs `np` is the import alias, invariant across the whole library) |
| `coil.linalg.det(a)` | `np.linalg.det(a)` → 0-d scalar | `-> f64` (Ph1) | **~0.95** | result `f64` not 0-d numpy scalar (ADR-0077 Q2 precedent; benign) |
| `coil.linalg.inv(a)` | `np.linalg.inv(a)` | identical (Ph1) | **1.0** | none |
| `coil.fft.fft(a)` | `np.fft.fft(a)` | identical (Ph2) | **1.0** | normalization made explicit (drops numpy's silent `norm=None` footgun, §5) |
| `coil.fft.rfft(a)` | `np.fft.rfft(a)` | identical (Ph2) | **1.0** | none |
| `coil.special.gamma(x)` | `scipy.special.gamma(x)` | `coil.special.gamma(x)` (Ph4) | **~0.9** | namespace `coil.special` vs `scipy.special` (coil unifies; LLM recovers from one diagnostic) |
| `coil.linalg.eig(a)` | `np.linalg.eig(a)` | **deferred** (real-only; complex result) | **0.0 (deferred)** | honest: real coil can't lie about complex eigenvalues (§5; ADR-0021 Complex tier) |
| `coil.linalg.qr / lstsq` | `np.linalg.{qr,lstsq}` | Ph3 | **1.0 when shipped** | none |

**Aggregate:** the Phase-1 shipped surface (`coil.linalg.{solve,det,inv}`) scores
**~0.98 training-data overlap** — `np.linalg.solve` is verbatim numpy. FFT (Ph2) is
~1.0. The §2.5 deficit is concentrated in the **deferred** non-symmetric `eig` (which
needs the Complex tier to be honest) and the **bessel gap** (no clean crate) — both
recorded as sub-ADRs, neither hand-waved.

**Compile-time-catch (§2.5) ledger:** the surface is call-shaped, so arity/type/unknown-
member errors are **compile-time-caught** at the manifest layer (the strong signal — a
`coil.linalg.solveX` typo or a wrong-arg-count is a type error, not a runtime crash).
The intrinsic deficit (ADR-0077 Q4, inherited): **shape-correctness is runtime-only**
(a non-square `a` for `solve` is a runtime `LinalgShapeError`, not a type error) —
because Cobrust handles carry no shape in the type. This is the same honest limit
ADR-0077 §11 + ADR-0078 §8 record, not a new one.

## 10. Precedent — the first cross-target-gated numerical backend + the first ecosystem sub-namespace

Two precedents this ADR sets:

- **Cross-target-gated feature.** This is the first ADR where a cargo *feature*
  (`linalg-backend`) is **hard-bound to a target arch** (native-x86_64-only, rejected on
  RV/WASM). The pattern — "the pure-Rust path is the universal floor; an accelerated
  path is an opt-in that the build-config layer rejects on unsupported targets" —
  generalizes to any future numerical accelerator (a GPU/cuBLAS path per the
  hardware-tiering strategy doc; a SIMD-intrinsic path). ADR-0075 §4 Q4 (pointer-width)
  + the `available_on: Vec<TargetMatcher>` manifest growth (ADR-0075 §7 risk 4) are the
  natural home for encoding "this feature/module is available on these targets."
- **Ecosystem sub-namespace.** `coil.linalg` / `coil.fft` is the first *dotted
  sub-namespace* under an ecosystem module (every prior handle — den/strike/pit/dora/
  coil — exposed a flat `module.fn` surface). The Q4-a resolver rule (a dotted `Attr`
  whose base is a known sub-namespace resolves to a prefixed flat symbol) is reusable by
  any module mirroring a Python library with sub-modules (`coil.random.*`, a future
  `pandas`-like `frame.*`). The §2.5 payoff: numpy/scipy's exact dotted idioms become
  expressible.

## 11. Open questions for sub-ADRs

Each deferred surface warrants its own design pass when reached:

- **Non-symmetric `eig` + the Complex-dtype tier (ADR-0021 activation).** `np.linalg.eig`
  on a real matrix produces complex eigenvalues; an honest coil needs the Complex dtype
  *before* `eig` can ship without lying. Sub-ADR: does coil add a `Complex64`/`Complex128`
  dtype (ADR-0021 sketched it) + a pure-Rust QR-algorithm-with-shifts eig, and/or wire
  ndarray-linalg's `Eig` (native-only)? The hardest pure-Rust numerical kernel in the set.
- **Bessel functions — the crate gap.** No verified pure-Rust Bessel in `statrs`/`special`
  (§3.3). Sub-ADR: vet `puruspe` (a pure-Rust special-fn crate claiming Bessel) for
  correctness + license + cross-compile, OR port a `cephes`-lineage Bessel to pure Rust,
  OR declare `bessel` `@py_compat(none)` with a documented gap.
- **Big-N svd/eigh (Householder).** The current Jacobi is O(N⁴), N-capped (ADR-0017).
  Sub-ADR: pure-Rust Householder-tridiagonal + implicit-QR for portable big-N, vs the
  native ndarray-linalg path. Pure-Rust keeps RV/WASM; the native path is the perf escape.
- **The wasm-BLAS gap (is there EVER a wasm linalg accelerator?).** ndarray-linalg is
  x86_64-only. Sub-ADR: is there a wasm-targetable BLAS (a pure-Rust `nalgebra`/`faer`
  path, or a wasm-SIMD micro-BLAS) that beats the current pure-Rust LU on wasm without
  breaking portability? (`faer` is a pure-Rust dense-linalg crate worth a tractability
  survey here — it may be a portable accelerator that dominates the Jacobi path on ALL
  targets, potentially making the ndarray-linalg native-opt-in unnecessary.)
- **Multi-dim FFT (`fft2`/`fftn`) + the `norm=` keyword.** Composed-over-rustfft per-axis;
  needs the keyword-arg marshalling ADR-0077 Q5 deferred (the `norm=` parameter). A
  bounded follow-up once FFT Phase-2 + kwarg-marshalling both land.
- **`coil.random.*` sub-namespace + advanced distributions.** coil has the random kernels
  (ADR-0018) but exposes them flat; the Q4-a sub-namespace rule could re-home them under
  `coil.random.*` to match `np.random.*`, plus the gamma/beta/poisson/binomial extension.
- **The `available_on` manifest field (shared with ADR-0075).** Encoding "linalg-backend
  is native-x86_64-only" mechanically (so the build-config layer can reject it on RV/WASM
  with a clear diagnostic) needs the `available_on: Vec<TargetMatcher>` field ADR-0075 §7
  risk 4 flags. This ADR is the second consumer (after ADR-0075's pit/strike network-libs
  exclusion) — a shared sub-ADR could land the field once.

## 12. Consequences

- **Positive:** decides the deep-numerical backend with the cross-compile constraint
  made load-bearing (the honest §6 portability matrix is the artifact); keeps coil's
  shipped path pure-Rust + universally cross-portable (satisfies ADR-0075 Phase-2's
  "coil under wasmtime" done-means); recommends a Phase-1 that ships the most-used numpy
  linalg ops (`solve/det/inv`) with **zero new numerical code** (they exist + pass the
  gate); establishes the reusable dotted-sub-namespace `.cb` surface (`coil.linalg.*`)
  matching numpy's exact idiom; and names the FFT story as the *cleanest* numerical
  surface (rustfft is pure-Rust + wasm-aware).
- **Negative / accepted:** (1) the **honest stub** — `linalg-backend` is declared but
  NOT actually wired today (§1.1); Phase-3 finally wires it (native-only). (2) Pure-Rust
  svd/eigh stays O(N⁴) N-capped until the Householder sub-ADR; non-symmetric `eig` +
  `qr`/`lstsq`/`pinv` are unimplemented (Phase-3+). (3) **Bessel has no clean pure-Rust
  crate** (§3.3) — a real gap, sub-ADR'd not hidden. (4) Shape-correctness stays
  runtime-only (inherited ADR-0077 §11 §2.5 deficit).
- **Risk — manifest drift:** the new sub-namespace table joins the hand-maintained
  manifest (ADR-0072 §5 R4 accepted debt; generation still deferred).
- **Risk — cross-build feature footgun:** if a user sets `linalg-backend` on an RV/WASM
  cross-build, it MUST fail with a clear diagnostic, not silently link a host blob (Q5).
  The `available_on` field (§11) is the mechanical guard; until it lands, the build-config
  layer + docs carry the warning.
- **Risk — Cargo.lock staging (F64):** FFT (Phase-2) + special (Phase-4) add deps;
  **stage `Cargo.lock`** or `--locked` CI cluster-fails build/clippy/test.
- **Risk — over-claim:** this ADR explicitly corrects the framing that "coil lacks
  solve/inv/det" (§1.1 — they exist) and that "`linalg-backend` accelerates" (§1.1 — it
  is a dead stub). A future audit must not re-introduce either over-claim.
- **Follow-up:** ratify draft→accepted when the Phase-1 `coil.linalg.{solve,det,inv}`
  impl sprint lands + passes the §7 done-means (including the wasmtime cross-run) +
  a paired ADSD audit; open the §11 sub-ADRs (non-symmetric `eig`/Complex tier + the
  `faer` portable-accelerator survey first, as they reshape the whole backend story).

## 13. Evidence

- **Source ground truth (verified at `061fde9`):**
  - `crates/cobrust-coil/Cargo.toml` — features `linalg-backend` (= `dep:ndarray-linalg`),
    `linalg-openblas-static`, `linalg-intel-mkl-static`; `ndarray = "0.16"`,
    `ndarray-linalg = "0.16"` (optional), `rand`/`rand_pcg`/`rand_distr`.
  - `crates/cobrust-coil/src/linalg.rs` — 852 LOC; 8 `pub fn`s (`matmul`@175 / `dot`@190
    / `det`@427 / `solve`@464 / `inv`@503 / `cholesky`@532 / `eigh`@575 / `svd`@673);
    pure-Rust LU + Jacobi; `_backend_marker()`@842 the ONLY `#[cfg(feature =
    "linalg-backend")]` site (a dead stub — proves the accel is unwired).
  - `crates/cobrust-coil/src/lib.rs:122` — `linalg::{...}` re-exports; no `fft`/`special`
    module (verified absent).
- **ADRs:** ADR-0017 (M7.4 linalg — pure-Rust default + opt-in `ndarray-linalg`, the
  decision this ADR extends + corrects); ADR-0013 (M7.0 ndarray foundation / crate
  layout); ADR-0016 (M7.3 reductions / error-variant pattern); ADR-0018 (M7.5 random /
  `Generator` newtype, no global RNG); ADR-0021 (Complex-dtype tier — prerequisite for
  honest `eig`); ADR-0072 (ecosystem-import chain / flat manifest); ADR-0075 (RV+WASM
  enablement — the cross-compile constraint, §3/§5/§6/§7); ADR-0077 (coil operator/index/
  attr — the chain mechanism + §2.5/§9/§11 structure mirrored here); ADR-0078 (wrap-the-
  crate thesis + FLAT/DEEP tractability lens).
- **Strategy doc:** `docs/agent/strategy/numpy-translation-architecture.md` (the
  30%/50%/20% insight + the PyO3-reverse-bind option this ADR scopes OUT for a shipped
  coil); `docs/agent/strategy/numerical-compute-hardware-tiering.md` (CPU/GPU tiering,
  the home for a future GPU accelerator under the same cross-target-gated pattern).
- **Crate facts (verified against docs.rs / GitHub at authoring time, 2026-05-29):**
  - `ndarray-linalg` 0.16 — "leverages LAPACK's routines using the bindings provided by
    blas-lapack-rs/lapack"; **requires system BLAS/LAPACK, cannot work without Fortran/C
    BLAS**; **x86_64-only** (GitHub README); backends `openblas-static` /
    `intel-mkl-static` (Intel license) / `netlib`; traits `Solve`/`Inverse`/`Eig`/`Eigh`/
    `SVD`/`QR*`/`cholesky`, `least_squares` module; no first-class `det` in 0.16 docs.
    https://docs.rs/ndarray-linalg/latest/ndarray_linalg/ ,
    https://github.com/rust-ndarray/ndarray-linalg/blob/master/README.md
  - `rustfft` 6.4.1 — "high-performance FFT library written in pure Rust", "no C
    dependencies", **explicit WASM support** (`FftPlannerWasmSimd`); forward+inverse via
    `FftDirection`; complex-to-complex. https://docs.rs/rustfft/latest/rustfft/
  - `realfft` 3.5.0 — pure-Rust wrapper on `rustfft`; real-to-complex forward + complex-
    to-real inverse. https://docs.rs/realfft/latest/realfft/
  - `statrs::function` — `gamma`/`beta`/`erf`/`factorial` modules present; **Bessel NOT
    listed**. https://docs.rs/statrs/latest/statrs/function/index.html
  - `special` crate — `libm`-backed (`libm ^0.2`); traits Gamma/Beta/Error/Elliptic/
    LambertW/Primitive; **Bessel NOT listed**. https://docs.rs/special/latest/special/
  - `rand_distr` (already a coil dep) — Gamma/Beta/Poisson/Binomial distributions.
- **Constitution:** CLAUDE.md §2.2 (no Python runtime / `Result`-default / no silent
  coercion — grounds the Option-d rejection + the footgun ledger §5); §2.4
  (`@py_compat numerical(rtol)` — the linalg tier); §2.5 (LLM-first: `np.linalg.solve`
  training-overlap §9; compile-time-catch ledger); §5.1 (elegant — namespace-not-handle
  Q4-b rejection); §5.3 (efficient — the pure-Rust-vs-LAPACK perf trade §4).
- **Findings:** F64 (dev-dep Cargo.lock staging — every new-dep phase §7).
- **External numpy/LAPACK refs:** numpy linalg
  (https://numpy.org/doc/stable/reference/routines.linalg.html), numpy fft
  (https://numpy.org/doc/stable/reference/routines.fft.html), LAPACK reference
  (https://www.netlib.org/lapack/lug/).
