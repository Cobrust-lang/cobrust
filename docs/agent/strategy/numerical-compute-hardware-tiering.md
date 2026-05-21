---
doc_kind: strategy
strategy_id: numerical-compute-hardware-tiering
title: "numerical compute — hardware tiering insight (CPU tier 0-3 + GPU paths)"
status: strategic-anchor
date: 2026-05-19
last_verified_commit: d2cbb8d
relates_to: [adr:0046, adr:0058b, adr:0007, adr:0011, adr:0028]
sourced_from: user insight 2026-05-19 (numerical-compute hardware tiering)
---

# Numerical Compute Hardware Tiering

> READ THIS FIRST before any numpy-cb, cobrust.gpu, or SIMD-dispatch sprint.

## Core insight (user 2026-05-19)

**Hardware tiering is a multi-dimensional problem.** The CPU instruction-set axis
and the heterogeneous-compute (GPU/accelerator) axis are orthogonal and must be
planned independently, but they share the same `cobrust build` surface and the same
ADR-0058b multi-target dispatch infrastructure.

The §2.5 LLM-first principle adds a third axis: **what the LLM user has to remember
at the keyboard**. The highest-priority UX goal is that the LLM can write
`cobrust install numpy-cb` and never think about SIMD.

---

## CPU instruction-set tiering (single-CPU dimension)

| Tier | Name | Mechanism | Binary overhead | Cobrust delta |
|---|---|---|---|---|
| **0** | Baseline | `x86-64-v1` / `armv8-a`; no SIMD | 0 | 0 — current `release.yml` tier-1 default |
| **1** | Runtime-dispatch multi-versioning | Same `.so` embeds SSE2 + AVX2 + AVX-512; startup `__builtin_cpu_supports` selects fastest | +50–100% binary size | ~200 LOC on top of ADR-0058b multi-target dispatch (Rust `multiversion` crate or `std::arch + cfg-if`) |
| **2** | Compile-time `--target-cpu=native` | `cobrust build --release --target-cpu=native`; LLVM targets current host CPU; zero dispatch overhead | 0 | ~20 LOC CLI flag exposed on top of ADR-0058b framework |
| **3** | Prebuilt multi-wheel distribution | `release.yml` matrix sub-targets: `-x86-64-v3` / `-haswell` / `-skylake-avx512`; package tool auto-detects CPU at install | per-wheel ~10% over Tier 0 | ADR-0046 matrix sub-target extension (~50 LOC YAML) + new package-tool ADR |

**Naming convention for Tier 3 wheels:**
`cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz`
Mirrors PyPI `manylinux` + wheel approach. `cobrust install <pkg>` detects CPU and
downloads the matching wheel.

---

## Heterogeneous compute (GPU / accelerator)

numpy itself does not do GPU compute; cupy / pytorch do. Cobrust has two paths:

### Path A — borrow existing runtimes (recommended first)

`cobrust.gpu` stdlib module binds `cuBLAS` / `rocBLAS` / Metal Performance Shaders
via FFI. The FFI boundary is isomorphic to numpy's BLAS FFI plus a device-context
layer. Low-risk; covers ~90% of user scenarios. No concurrency-runtime extension
needed.

### Path B — Cobrust IR → GPU kernel (deferred)

MIR lowered to NVPTX / AMDGPU / SPIR-V via inkwell (already supported per ADR-0058b
LLVM backend). Adds GPU target triple to the existing multi-target dispatch table.
~500–800 LOC of compiler work, but **requires ADR-0028 concurrency-runtime extension
for device-memory ownership semantics**. Not a single wave; plan carefully.

### WASM SIMD — browser-side numpy-cb

inkwell supports `wasm32-unknown-unknown` SIMD128. This is the §1.1 in-browser
positioning vs Pyodide. Cobrust AOT + no-GIL wins here because Python wrapper
overhead disappears. Include in Tier 3 wheel distribution as a WASM target.

### NEON / SVE — Apple M-series, Graviton, Ampere

`aarch64-apple-darwin` + `aarch64-unknown-linux-gnu` are already tier-1 targets.
`--target-cpu=apple-m1` / `--target-cpu=neoverse-v1` are available immediately via
Tier 2. SVE (variable-width vector extensions) is Phase K+ scope; defer until SVE
ABI stabilizes in inkwell.

---

## §2.5 LLM-first ranking

Ordered best → worst for LLM user experience:

| Rank | Experience | Why |
|---|---|---|
| **Best** | Tier 3: `cobrust install numpy-cb` — toolchain auto-detects CPU + downloads optimal wheel | Full `pip install numpy` training-data overlap; LLM writes nothing architecture-specific |
| **Middle** | Tier 1: `cobrust build --release` defaults to runtime-dispatch | Binary slightly larger; optimal everywhere; LLM unaware of SIMD |
| **Worst** | Tier 2 manual: LLM must write `--target-cpu=skylake-avx512` | LLM training data lacks this specific flag; error-prone |

**Operational rule**: Tier 1 is the `--release` default. Tier 2 is an advanced escape
hatch. Tier 3 is the long-run distribution story and the §2.5 optimum.

---

## Tier 1 status: SHIPPED 2026-05-19

Tier 1 runtime-dispatch multi-versioning landed on main via the
feature/tier1-runtime-dispatch branch merge. Delivered:

- `TargetSpec::runtime_dispatch: bool` (default `true` on `--release`).
- `--enable-runtime-dispatch [bool]` CLI flag (`cobrust build`).
- `llvm_backend::emit_multi_version_dispatch`: x86_64 emits `_v1_sse2` /
  `_v2_avx2` / `_v3_avx512` + dispatcher; aarch64 no-op.
- `runtime/cpu_features.c` — `__builtin_cpu_supports` helpers, no unsafe.
- 3 smoke tests in `tests/runtime_dispatch_smoke.rs`; TEST_EXIT=0 on DG.

Cascade addendum (honest re-scope):
- Hot-function selection deferred: wave-1 dispatches ALL top-level functions.
  Opt-out: `--enable-runtime-dispatch=false`.
- SVE multi-versioning (aarch64) deferred — see §NEON/SVE above.

## Tier 2 status: SHIPPED f900910

`--target-cpu=native` CLI flag landed on main via feature/tier2-target-cpu. Delivered:

- `TargetSpec::target_cpu: Option<String>` (default `None` = generic baseline).
- `--target-cpu <CPU>` CLI flag on `cobrust build`; accepted values: `"native"`,
  any LLVM CPU name (`"skylake"`, `"apple-m1"`, `"neoverse-v1"`, …).
- `build_target_machine` reads `spec.target_cpu` instead of hardcoded `"generic"`.
- Tier 1 and Tier 2 are mutually compatible: `--target-cpu=native --enable-runtime-dispatch=true`
  activates both layers simultaneously.
- 3 LLVM-gated smoke tests in `tests/tier2_target_cpu_smoke.rs`; 0/pass on Mac
  (LLVM not installed); Tier 1 smoke tests: zero regression (3/3 pass).

## Recommended landing order (updated)

1. ~~**Tier 1** runtime-dispatch~~ — **SHIPPED 2026-05-19**.
2. ~~**Tier 2** `--target-cpu` CLI flag~~ — **SHIPPED f900910** (merge SHA: 4e862bb).
3. ~~**Tier 3** multi-wheel prebuilt distribution
   (release.yml matrix extension ~50 LOC YAML + new package-tool ADR).~~
   **wave-1 SHIPPED** — release.yml 9-variant matrix landed; see ADR-0065 §7.1.
4. **GPU Path A** `cobrust.gpu` stdlib module wrapping cuBLAS / Metal / ROCm
   (new ADR; low-risk; covers most user demand).
5. **GPU Path B** Cobrust → NVPTX / SPIR-V direct codegen
   (last; blocked on ADR-0028 device-memory ownership extension; Phase K+ at earliest).

---

## Tier 3 status: wave-1 SHIPPED (Phase O wave-1)

ADR-0065 §7.1 wave-1 landed on main. Delivered:

- `release.yml` matrix extended to 9 wheel variants per tagged release:
  `-gnu-v1`, `-gnu-v3`, `-gnu-v4`, `-musl-v1`, `-musl-v3`,
  `-linux-neon`, `-linux-sve` (experimental), `-darwin-m1`, `-darwin-m2`.
- Each matrix entry carries `cpu_level`, `rustflags_extra`, `asset_suffix`.
- Archive naming: `cobrust-v{version}-{triple}-{cpu_level}.tar.gz` per ADR-0065 §3.2.
- RUSTFLAGS per-step env inherits `-D warnings` and appends `--target-cpu` flag.
- rust-cache key extended to include `cpu_level` to prevent cross-contamination.

Waves 2-4 remain queued per ADR-0065 §7:
- **wave-2**: `cobrust install` subcommand + CPU detection (~600 LOC).
- **wave-3**: `cobrust-registry` crate + CDN mirror (~350 LOC).
- **wave-4**: SHA/ABI hardening + smoke tests across 3 CPU classes.

Acceptance gate §5.1 (≥ 7 variants) is structurally satisfied by the 9-variant
matrix. Gates §5.2-§5.4 are wave-2/4 scope.

---

## Production benchmark v0.4.0 (2026-05-21)

Empirical baseline of stripped O3 `cobrust` binary across all 9 shipped
wheel variants. This is the real-world artifact downstream consumers
exercise, captured at the v0.4.0 cut (main HEAD `d2cbb8d`):

| CPU tier / target                  | Binary size  | Δ vs Tier-0 baseline |
|------------------------------------|--------------|----------------------|
| Tier-0  `x86_64-unknown-linux-gnu-v1`   | 14,814,368 B | — (baseline)         |
| Tier-3  `x86_64-unknown-linux-gnu-v3`   | 14,814,368 B | 0 B (no -fvectorize delta visible at the binary layer; per-function code-size payoffs cancel) |
| Tier-3  `x86_64-unknown-linux-gnu-v4`   | 14,814,368 B | 0 B (same as v3 — AVX-512 inlines bigger but cobrust binary has limited AVX-eligible hot loops at v0.4.0 scope) |
| Tier-0  `x86_64-unknown-linux-musl-v1`  | 14,885,688 B | +71,320 B (musl libc statically linked) |
| Tier-3  `x86_64-unknown-linux-musl-v3`  | 14,885,688 B | +71,320 B |
| Tier-0  `aarch64-unknown-linux-gnu-neon`| 11,288,368 B | -3,526,000 B (arm64 instruction density vs x86_64) |
| Tier-3  `aarch64-unknown-linux-gnu-sve` | 11,288,368 B | -3,526,000 B (SVE-scalable: opt-in but no SVE-eligible hot loops at v0.4.0) |
| Tier-3  `aarch64-apple-darwin-m1`       | 10,231,360 B | -4,583,008 B (Mach-O vs ELF + Apple Silicon code density) |
| Tier-3  `aarch64-apple-darwin-m2`       | 10,231,360 B | -4,583,008 B (M2 = M1 ISA superset; no native delta) |

**Key finding 1**: at v0.4.0 the cobrust binary itself contains few
SIMD-eligible hot loops, so the `-v3` / `-v4` / `-sve` CPU-tier flags
produce **byte-identical** stripped binaries vs their `-v1` / `-neon`
baselines on the same triple. The CPU-tier multiplexing pays off in
the **downstream numerical workload** (numpy-cb, cobrust.gpu), not
in the compiler binary itself. ADR-0065 wave-1's CPU-tier wheels
remain correct in design — they distribute the same compiler binary
across CPU tiers so that *user code* compiled with `--target-cpu=native`
(Tier-2) can target the host's best instruction set.

**Key finding 2**: cross-arch shows real density delta. `aarch64`
beats `x86_64` by ~3.5 MB (29% smaller); `darwin` beats `gnu` by
~1 MB additional (M-series Mach-O + Apple Silicon code density).
This is consistent with prior art (Rust compiler, Go toolchain).

**Key finding 3**: O3-vs-O0 production ratio (Mac aarch64 control)
**0.293 (70.7% reduction)** — materially better than the toy-fixture
median 0.584 (41.6% reduction). LTO + inlining + DCE compound at
scale. See ADR-0023 §A3 production-scale empirical close for the
full O0 baseline (34,960,800 B) and binding harness anchor
(`crates/cobrust-codegen/tests/binary_size_prodscale.rs`).

---

## Cross-references

- [[adr:0046]] — `release.yml` tier-1 platform contract; Tier 3 extends this matrix.
- [[adr:0058b]] — LLVM opt-level + multi-target dispatch; Tier 1 and Tier 2 build on this directly.
- [[adr:0007]] — L0-L3 translator pipeline (numpy-cb translation path).
- [[adr:0011]] — PyO3 build path; BLAS selection happens at the FFI layer, same as numpy's manylinux wheel ABI.
- [[adr:0028]] — concurrency runtime; device-memory ownership extension is a prerequisite for GPU Path B.
- `docs/agent/strategy/numpy-translation-architecture.md` — sibling doc; wrapper-first architecture insight.
