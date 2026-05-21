---
doc_kind: adr
adr_id: 0065
name: 0065
title: "Tier 3 prebuilt multi-wheel distribution — per-CPU sub-target + cobrust install package tool"
status: partial (wave-1 shipped; waves 2-4 queued)
date: 2026-05-20
phase: Phase O (post-Phase-N packaging)
relates_to: [adr:0026, adr:0046, adr:0058b, adr:0058e]
discovered_by: "docs/agent/strategy/numerical-compute-hardware-tiering.md §Tier 3 strategic anchor"
---

# ADR-0065: Tier 3 Prebuilt Multi-Wheel Distribution

## 1. Motivation

Tier 1 (runtime-dispatch, SHIPPED) and Tier 2 (`--target-cpu=native`, SHIPPED)
complete the in-process CPU optimization story. Tier 3 is the **distribution
story**: LLM agents and human users should type `cobrust install numpy-cb` and
receive the optimal pre-compiled wheel for their host CPU — with no knowledge of
SIMD, sub-targets, or architecture-specific flags required.

The canonical prior art is `pip install numpy`: PyPI ships CPU-specific wheels
tagged by `manylinux2014_x86_64`, `manylinux_2_28_aarch64`, etc. `pip` inspects
the host, fetches the best match, verifies the SHA, and unpacks. Cobrust Tier 3
mirrors this behaviour exactly — maximizing `pip install numpy` training-data
overlap per §2.5.

Without Tier 3:

- LLM agents must embed `--target-cpu` flags in build scripts (§2.5 violation:
  low training-data overlap).
- Packages distributed as source tarballs require a local Rust toolchain + LLVM
  to be present on the install host.
- No SHA-verified, registry-indexed, version-pinned distribution exists for
  Cobrust packages.

Tier 3 fills all three gaps.

---

## 2. §2.5 LLM-first audit

### 2.1 Compile-time-catch

- Package tool validates wheel **SHA-256** against the registry index at install
  time; SHA mismatch → hard error before any bytes are written to disk.
- **ABI compatibility** is validated at install: the wheel's embedded
  `cobrust-abi` tag (triple + CPU level) must be compatible with the host ABI;
  incompatible wheel → error with suggestion before installation proceeds.
- Both validations surface as structured errors with fix suggestions
  (per §2.5 direction B: errors print the FIX, not just the diagnosis).

### 2.2 Training-data overlap

- `cobrust install numpy-cb` is syntactically identical to `pip install numpy`.
  LLM training corpora contain millions of `pip install <pkg>` invocations; the
  surface is maximally familiar.
- Wheel naming (`cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz`) mirrors
  PyPI wheel tag format (`numpy-1.24.0-cp311-manylinux_2_28_x86_64.whl`).
- CPU detection fallback (`v3` → `v1`) mirrors `pip`'s wheel-tag fallback
  (most-specific to least-specific), reducing the LLM's need to know about
  fallback semantics.

---

## 3. Scope

### 3.1 release.yml matrix sub-target extension

Extend the existing ADR-0046 tier-1 matrix (4 triples) with per-CPU sub-target
entries. Each `release.yml` matrix row adds a `cpu_level` field and a
`RUSTFLAGS` override. The full Tier 3 wheel matrix:

| Triple | CPU level | Wheel suffix | LLVM `--target-cpu` | Notes |
|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | `v1` | `-gnu-v1` | `x86-64` | Baseline; runs on all x86-64 |
| `x86_64-unknown-linux-gnu` | `v3` | `-gnu-v3` | `haswell` | Haswell+; AVX2 + FMA |
| `x86_64-unknown-linux-gnu` | `v4` | `-gnu-v4` | `skylake-avx512` | Skylake AVX-512; data-center / HPC |
| `x86_64-unknown-linux-musl` | `v1` | `-musl-v1` | `x86-64` | Static baseline |
| `x86_64-unknown-linux-musl` | `v3` | `-musl-v3` | `haswell` | Static AVX2 |
| `aarch64-unknown-linux-gnu` | `neon` | `-linux-neon` | `generic` (NEON mandatory in ARMv8) | All ARMv8-A |
| `aarch64-unknown-linux-gnu` | `sve` | `-linux-sve` | `neoverse-v1` | SVE; Graviton3 / Ampere Altra |
| `aarch64-apple-darwin` | `m1` | `-darwin-m1` | `apple-m1` | Apple M1 / M2 base |
| `aarch64-apple-darwin` | `m2` | `-darwin-m2` | `apple-m2` | Apple M2 Pro+ / M3+ |

Existing tier-1 ADR-0046 triples continue to ship unchanged as the **baseline
wheels** (`v1` / `neon` / `m1`). No tier-1 regression.

`release.yml` delta: approximately 50 LOC of YAML matrix expansion using
the existing `include:` strategy. Each new entry uses:

```yaml
- target: x86_64-unknown-linux-gnu
  os: ubuntu-latest
  use_cross: false
  install_musl_tools: false
  cpu_level: v3
  rustflags_extra: "-C target-cpu=haswell"
```

The `Build release binary` step gains:

```yaml
env:
  RUSTFLAGS: "-D warnings ${{ matrix.rustflags_extra }}"
```

The `Package tar.gz` step names the archive:

```
cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz
```

### 3.2 Wheel naming convention

Pattern: `cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz`

Examples:

```
cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz
cobrust-numpy-0.1.0-aarch64-apple-darwin-m1.tar.gz
cobrust-hello-0.1.0-x86_64-unknown-linux-musl-v1.tar.gz
```

Embedded `cobrust-wheel.toml` manifest inside each archive:

```toml
[wheel]
name = "numpy"
version = "0.1.0"
triple = "x86_64-unknown-linux-gnu"
cpu_level = "v3"
cobrust_abi = "0.1"           # semver-major ABI version
sha256 = "<hex>"              # SHA-256 of the binary payload
py_compat_tier = "numerical"  # @py_compat tag of the primary exported items
source_library = "numpy"
source_version = "1.24.0"
provenance_manifest = "provenance.toml"   # path inside archive
```

The `cobrust_abi` field allows the package tool to reject a wheel built for an
incompatible Cobrust ABI version at install time (before disk write).

### 3.3 `cobrust install <pkg>` package tool

The `cobrust install` subcommand (new CLI surface on `cobrust-cli`) performs
host-detected wheel selection and installation.

#### 3.3.1 CPU detection logic

| Host | Detection method | Fallback |
|---|---|---|
| Linux x86_64 | Parse `/proc/cpuinfo` flags: `avx512f` → `v4`; `avx2` → `v3`; else `v1` | `v1` |
| Linux aarch64 | Parse `/proc/cpuinfo` features: `sve` → `sve`; else `neon` | `neon` |
| macOS aarch64 | `sysctl -n hw.optional.avx512f` (absent on Apple Silicon); chip model from `sysctl -n machdep.cpu.brand_string` → `m2` if M2/M3+; `m1` otherwise | `m1` |
| macOS x86_64 | `sysctl -n machdep.cpu.features` (Intel Macs, legacy) | `v1` |
| Container / VM | CPU flags may be masked; detection is best-effort; `v1` / `neon` baseline always safe | baseline |

CPU detection is isolated in a standalone `cpu_detect::detect_cpu_level() -> CpuLevel` function
(pure read of `/proc/cpuinfo` or `sysctl`; no `cpuid` intrinsic required in pure Rust).

#### 3.3.2 Wheel selection algorithm

```
1. Detect host triple (Rust std::env::consts::ARCH + OS + ABI).
2. Detect CPU level via §3.3.1.
3. Query registry index for package + version: GET /index/<pkg>/<version>/wheels.json
4. Filter wheel list by exact triple match.
5. Sort by CPU level preference (v4 > v3 > v1; sve > neon; m2 > m1).
6. Select highest match. If none, select v1 / neon / m1 baseline.
7. Download wheel tarball.
8. Verify SHA-256 against index. On mismatch: error + suggestion.
9. Validate cobrust_abi compatibility. On mismatch: error + suggestion.
10. Extract to ~/.cobrust/packages/<pkg>/<version>/<triple>-<cpu_level>/.
11. Write lock entry to ~/.cobrust/packages.lock.
```

Error messages follow §2.5 direction B — every error includes a `suggestion:` field:

```
Error: SHA-256 mismatch for cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz
  expected: a1b2c3...
  got:      deadbeef...
  suggestion: re-run `cobrust install numpy --force` to re-download, or pin
              to a known-good version with `cobrust install numpy==0.0.9`
```

#### 3.3.3 CLI surface

```
cobrust install <pkg>[@<version>] [--cpu <level>] [--triple <triple>] [--dry-run]
cobrust install --list                    # list installed packages
cobrust install --update <pkg>            # update to latest compatible wheel
```

`--cpu` and `--triple` are escape hatches for advanced users (§2.5: expose but
do not require). Default: auto-detect.

### 3.4 Wheel registry shape

The Cobrust wheel registry is a lightweight index over HTTPS, mirrored to:

1. **Primary**: GitHub Releases (already used for compiler binaries per release.yml)
   — each tagged release uploads all wheels as release assets.
2. **CDN mirror**: Cobrust-controlled CDN (URL TBD; specified in `cobrust.toml`
   `[registry]` section) for bandwidth efficiency and SPOF mitigation (§6.3).

Index API shape (mirrors PyPI Simple API):

```
GET /index/<pkg>/                           → JSON list of versions
GET /index/<pkg>/<version>/wheels.json      → JSON list of wheel metadata
```

`wheels.json` entry shape:

```json
{
  "filename": "cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz",
  "triple": "x86_64-unknown-linux-gnu",
  "cpu_level": "v3",
  "sha256": "a1b2c3...",
  "cobrust_abi": "0.1",
  "size_bytes": 4194304,
  "upload_time": "2026-05-20T00:00:00Z",
  "download_url": "https://github.com/Cobrust-lang/cobrust/releases/download/v0.1.0/..."
}
```

The registry index is **static JSON** — no dynamic server required. It is
generated by the `release.yml` `release` job after all wheel artifacts are
uploaded. A `generate-index` step writes the JSON and uploads it as a release
asset + pushes to the CDN mirror.

---

## 4. Non-goals

- **No source-build replacement**: `cobrust build` remains the path for packages
  not in the wheel registry. Tier 3 complements, does not replace, source builds.
- **No PyPI integration**: publishing Cobrust packages to PyPI is a separate ADR.
  This ADR covers the Cobrust-native registry only.
- **No automatic transitive dependency resolution**: `cobrust install numpy-cb`
  installs `numpy-cb` and its direct deps only. A full resolver (SAT/PubGrub)
  is a future ADR.
- **No Windows wheel production** in Phase O: Windows is queued per ADR-0046
  §Amendment; Tier 3 wheels follow the same queue.
- **No GPU wheels** in this ADR: GPU sub-targets are heterogeneous-compute axis
  (GPU Path A/B per hardware tiering strategy); separate ADR.
- **No WASM wheels** in Phase O wave-1: WASM Tier 3 is noted as desirable in the
  strategy doc but requires dedicated packaging format work; defer to Phase O wave-3+.

---

## 5. Acceptance gate

### 5.1 Wheel matrix coverage

`release.yml` produces ≥ 5 distinct wheel variants per tagged release
(across the 4 tier-1 triples × 2-3 CPU levels each). Exact minimum count:
`v1 + v3 + v4 + musl-v1 + musl-v3 + neon + m1 = 7` variants for the
`cobrust-cli` + `cobrust-hello` smoke packages.

### 5.2 `cobrust install` smoke test

On 3 different CPU classes:
1. `x86_64-v1` host (baseline): `cobrust install hello-cb` → installs `-gnu-v1` wheel → `hello-cb` runs → exit 0.
2. `x86_64-v3` host (Haswell+): same command → installs `-gnu-v3` wheel → exit 0.
3. `aarch64-m1` host (Apple Silicon): same command → installs `-darwin-m1` wheel → exit 0.

### 5.3 SHA verification

Every wheel install run verifies SHA-256. Gate: inject a corrupted wheel into
the test cache; `cobrust install --force` returns exit code ≠ 0 with an error
message containing `SHA-256 mismatch` and `suggestion:`.

### 5.4 ABI compatibility gate

Install a wheel tagged `cobrust_abi = "9.0"` on a `cobrust_abi = "0.1"` host;
confirm hard error + `suggestion:` before disk write.

---

## 6. Risk register

### 6.1 Wheel storage cost

Each release produces ~9× more artifacts than the current 4-triple baseline.
GitHub Releases has no per-release size limit but total repo size softcap is
10 GB (advisory). Mitigation: large numerical wheels (numpy-cb) will be
significantly larger than the compiler binary; establish a per-wheel size limit
(target ≤ 50 MB compressed) in the packaging spec. Offload to CDN mirror for
packages exceeding 20 MB.

### 6.2 CPU detection edge cases

VMs (QEMU, KVM), containers (Docker), and WSL2 may mask or partially expose
CPU feature flags. `/proc/cpuinfo` in a Docker container running on a Haswell
host may report `avx2` absent if `--cpuset-cpus` restricts features.
Mitigation: detection is best-effort; always fall back to `v1` / `neon`
baseline. The fallback wheel is correct and runs on all hardware; the only cost
is sub-optimal performance. Document this explicitly in `cobrust install --help`.

### 6.3 Registry SPOF

A single registry endpoint (GitHub Releases only) is a SPOF. Mitigation per
§3.4: CDN mirror as secondary source. `cobrust.toml` `[registry]` allows
operator override of registry URL (self-hosted or air-gapped). `cobrust install`
tries primary then falls back to mirror automatically.

### 6.4 ABI mixing across CPU tiers

A package depending on `numpy-cb` may load both a `v3`-compiled `numpy-cb`
and a `v1`-compiled helper. Wheels must not call across CPU-level ABI boundaries
at the Rust `extern "C"` level (AVX2 calling convention is a superset of v1
on x86-64, but callee-saved registers may differ with AVX3 zeroupper semantics).
Mitigation: `cobrust_abi` tag embeds the CPU level; the package tool enforces
that all wheels in a dependency closure use the SAME cpu_level (or the
lowest-common-denominator baseline). Violation → install-time error with
suggestion to pin all deps to `--cpu v1`.

### 6.5 SVE ABI stability

SVE (scalable vector extensions) ABI in inkwell / LLVM is not fully stable for
ARM. Mitigation: `*-linux-sve` wheels are **opt-in** and marked `experimental`
in the index (`"stability": "experimental"`). `cobrust install` does not
auto-select `sve` unless `--cpu sve` is passed explicitly. SVE promotion to
stable deferred per strategy doc note.

---

## 7. Implementation plan (deferred to Phase O sprints)

### 7.1 Phase O wave-1: release.yml matrix expansion — SHIPPED

Status: **SHIPPED** (feature/tier3-w1-matrix → main, 2026-05-21).

Delivered:
- `build-tier1` matrix extended from 4 → 9 entries with `cpu_level`, `rustflags_extra`, `asset_suffix` fields.
- `Package tar.gz` step uses `asset_suffix` for ADR-0065 §3.2 naming: `cobrust-v{ver}-{triple}-{cpu_level}.tar.gz`.
- RUSTFLAGS per-step env overrides global env; inherits `-D warnings` + appends `target-cpu` flag.
- rust-cache key extended with `cpu_level` to prevent contamination across CPU variants.
- Actual delta: 85 LOC YAML (header comments expanded; all 9 entries documented inline).
- Note: `generate-index` step deferred to wave-3 (registry crate must exist first).
- Prerequisite: none (atop existing ADR-0046 matrix).

### 7.2 Phase O wave-2: `cobrust install` subcommand + CPU detection

- `cobrust-cli`: new `install` subcommand (~200 LOC argument parsing + flow).
- `crates/cobrust-pkg`: new `cpu_detect` module (~100 LOC); new `wheel_select`
  module (~150 LOC); new `registry_client` module (~150 LOC).
- SHA-256 verification using `sha2` crate (already in dependency graph via
  translation pipeline).
- ABI compatibility check on `cobrust_abi` semver-major.
- Total delta: approximately 600 LOC.
- Prerequisite: wave-1 (registry index must exist to test against).

### 7.3 Phase O wave-3: `cobrust-registry` crate spike + CDN mirror

- `crates/cobrust-registry`: static index generation tool (~300 LOC Rust).
- CDN mirror sync script (Bash, ~50 LOC).
- `cobrust.toml` `[registry]` section parser wired into `cobrust install`.
- Prerequisite: wave-2 (registry client shape must be known).

### 7.4 Phase O wave-4: SHA verification hardening + ABI tagging

- Enforce ABI dependency-closure check (§6.4).
- Mark SVE wheels as `experimental` in index generator.
- Integration smoke tests across 3 CPU classes (§5.2).
- Audit `cobrust-wheel.toml` provenance fields against ADR-0007 L1 manifest
  shape (translation provenance carried through wheel).
- Prerequisite: wave-3 (registry + CDN must be operational).

---

## 8. Cross-references

- [[adr:0026]] — Cobrust package format. Tier 3 wheel archives are a specialized
  distribution format built atop ADR-0026 package semantics.
- [[adr:0046]] — `release.yml` tier-1 matrix. Tier 3 extends this matrix with
  per-CPU sub-target rows; baseline tier-1 triples remain unchanged.
- [[adr:0058b]] — LLVM opt-pipeline + multi-target dispatch. Tier 3 wheel
  compilation uses the `--target-cpu` LLVM flag routing codified in ADR-0058b
  §3.2 + §3.4.
- [[adr:0058e]] — AOT Cranelift substrate delegation (Phase K Strand #4).
  Tier 3 wheels are AOT-compiled; the AOT substrate path from 0058e is the
  codegen substrate Tier 3 wheels traverse.
- `docs/agent/strategy/numerical-compute-hardware-tiering.md` §Tier 3 —
  strategic anchor for this ADR; §2.5 LLM-first ranking establishes Tier 3 as
  the long-run distribution optimum.
- `docs/agent/strategy/numpy-translation-architecture.md` — numpy-cb is the
  primary Tier 3 consumer package; the wrapper-first architecture's PyO3 boundary
  sits inside the wheel archive as the ABI surface.

---

## 9. Evidence

- `docs/agent/strategy/numerical-compute-hardware-tiering.md` §Tier 3 row:
  "ADR-0046 matrix sub-target extension (~50 LOC YAML) + new package-tool ADR".
  This ADR is that package-tool ADR.
- PyPI Simple API: https://peps.python.org/pep-0503/ — static index shape mirrored
  in §3.4.
- PyPI wheel tag spec: https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/
  — wheel naming convention mirrored in §3.2.
- x86-64 microarchitecture levels: https://gitlab.com/x86-psABIs/x86-64-ABI
  (v1/v2/v3/v4 as standardized CPU compatibility tiers).
- `cibuildwheel` CI wheel building model (reference implementation of
  multi-platform wheel matrix in CI).

— P9 Tech Lead, Phase O ADR author, 2026-05-20
