---
doc_kind: strategy
strategy_id: numerical-compute-hardware-tiering
title: "numerical compute — hardware tiering insight (CPU tier 0-3 + GPU paths)"
status: strategic-anchor
date: 2026-05-19
last_verified_commit: 2b9460c
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
3. **Tier 3** multi-wheel prebuilt distribution
   (release.yml matrix extension ~50 LOC YAML + new package-tool ADR).
4. **GPU Path A** `cobrust.gpu` stdlib module wrapping cuBLAS / Metal / ROCm
   (new ADR; low-risk; covers most user demand).
5. **GPU Path B** Cobrust → NVPTX / SPIR-V direct codegen
   (last; blocked on ADR-0028 device-memory ownership extension; Phase K+ at earliest).

---

## Cross-references

- [[adr:0046]] — `release.yml` tier-1 platform contract; Tier 3 extends this matrix.
- [[adr:0058b]] — LLVM opt-level + multi-target dispatch; Tier 1 and Tier 2 build on this directly.
- [[adr:0007]] — L0-L3 translator pipeline (numpy-cb translation path).
- [[adr:0011]] — PyO3 build path; BLAS selection happens at the FFI layer, same as numpy's manylinux wheel ABI.
- [[adr:0028]] — concurrency runtime; device-memory ownership extension is a prerequisite for GPU Path B.
- `docs/agent/strategy/numpy-translation-architecture.md` — sibling doc; wrapper-first architecture insight.
