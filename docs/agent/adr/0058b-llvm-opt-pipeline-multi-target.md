---
doc_kind: adr
adr_id: 0058b
parent_adr: 0058
title: "Phase K wave-2 — LLVM optimization pipeline + multi-target dispatch"
status: accepted
date: 2026-05-19
ratified_at: 72f4d27
last_verified_commit: 72f4d27
supersedes: []
superseded_by: []
relates_to: [adr:0058, adr:0058a, adr:0023, adr:0046]
discovered_by: P10 Phase K wave-2 second sub-sprint per ADR-0058 §"Sub-ADR roster"
ratification_path: P9 sub-ADR review; ratified on DEV landing of PassBuilder wire (`f3574b1`) + multi-target smoke (`4749f99`) + binary-size bench (`ea9edac`) + DG verify clean @ 72f4d27 (404 PASS / 0 FAILED, O3 median ratio 0.584)
---

# ADR-0058b: Phase K wave-2 — LLVM optimization pipeline + multi-target dispatch

## 1. Motivation

ADR-0058a (Phase K wave-1, accepted `3d60e63`) shipped MIR → LLVM IR lowering core
parallel to Cranelift. The wave-1 emitter constructs IR via `inkwell` but does
**not** run any optimization passes — every `OptLevel::Speed` / `OptLevel::SpeedAndSize`
request collapses to LLVM `-O0` per the §8 non-goals carve-out:

> Optimization pass pipeline (`OptLevel::Speed` / `OptLevel::SpeedAndSize`)
> stays at LLVM `-O0` until sub-ADR 0058b lands.

ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" pinned the empirical
close bar:

> Cranelift `-O0` baseline → LLVM `--release -O3` target:
> median size reduction ≥ 30%.

This bar has been **open** since M9. ADR-0058b is the empirical close: wire the
LLVM PassBuilder pipeline into `llvm_backend::emit`, validate `-O3` produces
≤70% of `-O0` size on a 5-fixture bench harness, and amend ADR-0023 §A3 to
RESOLVED status.

Wave-2 also closes the **multi-target dispatch** gap. ADR-0046 (release.yml
tier-1 contract, accepted `03c70f2`) + Strand #5 musl promotion expand the
tier-1 matrix to four target triples:

- `aarch64-apple-darwin` (Mac arm64)
- `aarch64-unknown-linux-gnu` (Linux arm64)
- `x86_64-unknown-linux-gnu` (Linux x86_64)
- `x86_64-unknown-linux-musl` (Linux x86_64 musl, Strand #5)

Wave-1's `build_target_machine` already accepts the triple via `spec.triple`
through `TargetTriple::create(&spec.triple.to_string())` + `Target::from_triple`.
The wave-1 path is **functionally correct** on all four triples; wave-2 codifies
this as a binding contract (tested per triple at `cargo test` time when the
underlying LLVM toolchain supports the cross-target).

## 2. §2.5 LLM-first design — neutral

Per ADR-0058 §2 "§2.5 ROI position", Phase K is §2.5-neutral. ADR-0058b
inherits that neutrality: optimization-pipeline quality and multi-target reach
are **product perf + cross-platform credibility**, neither §2.5 §A
(compile-time-catch-errors) nor §2.5 §B (training-data overlap). The LLM
writes Cobrust source identically regardless of whether the LLVM backend runs
opt passes. Wave-2 introduces no new `TypeError::*` variants — it consumes the
IR-construction pass from wave-1 and runs LLVM passes on top.

§2.5 audit: must not regress error UX (PassBuilder errors propagate as
`CodegenError::LlvmError(String)`, identical to wave-1's existing path).

## 3. Decision

### 3.1 Scope

Wave-2 extends `crates/cobrust-codegen/src/llvm_backend.rs` by:

- **Wiring LLVM PassBuilder** at the post-IR-construction / pre-object-emit
  hook point. Use `inkwell::passes::PassBuilderOptions::create()` + `Module::run_passes`
  per inkwell 0.9 + LLVM-18+ "new pass manager" API.
- **Honoring `spec.opt_level`** — translate `OptLevel::None` / `OptLevel::Speed`
  / `OptLevel::SpeedAndSize` into PassBuilder pipeline strings.
- **Multi-target dispatch contract** — codify all four tier-1 triples as
  supported `Target::from_triple` arguments; add per-triple smoke at the
  bench harness.
- **Binary-size bench harness** — 5 representative fixtures compiled at
  O0 + O3; assert O3 ≤ 70% of O0 size per ADR-0023 §A3.

### 3.2 Pipeline mapping (binding)

The `OptLevel` → PassBuilder string mapping uses the LLVM new-pass-manager
`default<O*>` pipeline:

| Cobrust `OptLevel` | LLVM PassBuilder pipeline | LLVM `OptimizationLevel` for TargetMachine |
|---|---|---|
| `OptLevel::None` | (no passes — `run_passes` skipped) | `OptimizationLevel::None` |
| `OptLevel::Speed` | `default<O2>` | `OptimizationLevel::Default` |
| `OptLevel::SpeedAndSize` | `default<O3>,default<Os>` | `OptimizationLevel::Aggressive` |

Wave-2 ships `default<O2>` for `OptLevel::Speed` (Phase K release-mode default
per ADR-0058 §5.2) and `default<O3>,default<Os>` for `OptLevel::SpeedAndSize`
(binary-size acceptance bar).

ADR-0023 §"Per-MIR-form lowering rules" + §"Public surface" enumerate only
three `OptLevel` variants. Wave-2 does **not** introduce a fourth (`O1`-only)
variant; the binding stays at the three documented variants. The
"O0/O1/O2/O3" framing in the dispatch prompt collapses to the three-variant
Cobrust public surface — wave-2 mirrors the public surface, not LLVM's
internal pipeline tier count.

### 3.3 PassBuilderOptions defaults (binding)

`PassBuilderOptions::create()` with **inkwell 0.9 defaults preserved**. Wave-2
does NOT manually flip:

- `set_verify_each(false)` (default `false` — verify runs only at `Module::verify()`
  in dev-mode per ADR-0058a §9.2).
- `set_loop_vectorization`, `set_loop_unrolling`, `set_loop_slp_vectorization`,
  `set_loop_interleaving` — left at defaults; the `default<O2>` / `default<O3>`
  pipeline already drives these per LLVM internal mapping.

Manual flipping is sub-ADR 0058b-followup if empirical O3 ratio fails to clear
the 70% bar on the 5-fixture bench. Wave-2 ships defaults first; bench-failure
diagnostics drive any follow-up.

### 3.4 Multi-target dispatch (binding)

`build_target_machine` already calls `Target::from_triple(&triple)` parametrically.
Wave-2 explicitly enumerates the four supported tier-1 triples in module docs
and adds a unit test that constructs a `TargetMachine` for each (skipping
unavailable cross-targets at runtime via `Target::from_triple` returning Err).

The wave-1 emit path is **functionally correct** for cross-targets when the
underlying LLVM 18 toolchain on the host supports them. macOS `brew install llvm@18`
ships with both `aarch64-unknown-linux-gnu` + `x86_64-unknown-linux-musl`
backend support compiled in (verified at sub-ADR entry); Linux `apt llvm-18-dev`
likewise. Wave-2 does NOT cross-link (linker delegation per ADR-0023
§"Linker delegation" stays at `cc`; cross-linking requires `cross` or sysroot
prep, which is `release.yml` matrix scope, not codegen scope).

Wave-2's multi-target deliverable is **object-file emission**: `.o` for ELF
triples (Linux gnu/musl), `.o` Mach-O for Darwin. Executable production for
non-host triples requires a host-native linker matching the target ABI, which
stays in `release.yml` / `cross` scope.

## 4. Non-goals (explicit)

- **No new MIR features.** Wave-2 consumes wave-1's IR-construction pass; it
  does not extend the MIR → LLVM lowering surface.
- **No JIT opt.** The JIT path (cobrust-jit, ADR-0056a §13) inherits PIC contract
  + opt-level settings independently; wave-2 does not touch
  `crates/cobrust-jit/src/lower.rs`.
- **No DWARF.** DWARF debug-info emission is sub-ADR 0058c.
- **No cross-link.** Linker dispatch stays at `cc` per ADR-0023 §"Linker
  delegation"; cross-target executables are `release.yml` + `cross`-tool scope.
- **No PassBuilder custom-pass plugin loading.** `set_inline_threshold` and
  similar advanced flags stay at defaults; future sub-ADR may revisit if O3
  bench fails to clear the bar.

## 5. Acceptance gate

Wave-2 dispatches and closes only when **all four** conditions hold:

- **`default<O2>` + `default<O3>,default<Os>` pipelines compile clean** on the
  30-fixture LLVM diff corpus from ADR-0058a (no regressions vs `-O0`).
- **5-fixture bench passes O3 ≤ 70% of O0 size** on at least one tier-1 host
  (DG-Workstation `x86_64-unknown-linux-gnu` is the dispatch-canonical host).
- **Multi-target `TargetMachine` construction succeeds** for all four tier-1
  triples on a host with full LLVM-18 backend support (DG-Workstation per
  apt `llvm-18-dev`; Mac per brew `llvm@18`).
- **cobrust-jit regression clean**: 12-fixture JIT corpus passes unchanged
  (wave-2 does not modify the JIT lowering surface).

## 6. Implementation plan

| Phase | Surface | LOC delta | Wall-time |
|---|---|---|---|
| Phase 1 | PassBuilder wire in `llvm_backend::build_target_machine` + `emit` post-define hook | ~80 | 3-4h |
| Phase 2 | Multi-target dispatch table + per-triple unit smoke | ~50 | 2-3h |
| Phase 3 | Binary-size bench harness `tests/binary_size_bench.rs` (5 fixtures × 2 opt levels) | ~300 | 2-3h |
| Phase 4 | DG verify (codegen 30 + jit 12 + bench 5 + baseline ~350) | n/a | 30min |
| Phase 5 | Dual-track docs (zh + en architecture.md; agent modules/codegen.md OptLevel pipeline rows) | ~120 | 1h |
| Phase 6 | Ratify ADR-0058b accepted + ADR-0023 §A3 RESOLVED amendment | ~40 | 30min |

**Total LOC**: ~590 (within ADR-0058 §"Dispatch readiness" sub-ADR row ~20h DEV
budget).

## 7. Risk register

### 7.1 inkwell 0.9 PassBuilder API stability

- **Risk**: `inkwell::passes::PassBuilderOptions` was added in inkwell 0.6;
  inkwell 0.9 exposes it stably. The `Module::run_passes(passes, &machine, options)`
  signature is stable per docs.rs/inkwell/0.9.0. If the lockfile-pinned inkwell
  revision drifts past 0.9, the API surface may shift.
- **Mitigation**: ADR-0058a §"Evidence" pinned the lockfile inkwell revision;
  wave-2 keeps the same pin. CI `release.yml` regenerates the lockfile on
  toolchain change and would surface drift at gate time.

### 7.2 `default<O3>,default<Os>` may regress vs `default<O3>` alone on some fixtures

- **Risk**: O3 + Os (size opt overlay) may bloat code-size on small fixtures
  where unrolling defeats size opt. Bench harness on small fixtures (hello.cb
  at ~50 lines compiles to a tiny binary) could see O3 *larger* than O0 if
  unrolling fires.
- **Mitigation**: bench harness asserts ≤ 70% **median** across the 5 fixtures,
  NOT per-fixture. If median fails, fall back to `OptLevel::SpeedAndSize` →
  `default<O3>` (drop the Os overlay). Recovery path is one-line edit to the
  pipeline string.

### 7.3 Multi-target `TargetMachine::from_triple` may fail on missing LLVM backend

- **Risk**: macOS `brew install llvm@18` may not include the `x86_64-unknown-linux-musl`
  backend in some distributions; DG-Workstation `apt llvm-18-dev` may similarly
  lack a backend.
- **Mitigation**: per-triple unit test uses `Target::from_triple(&triple).ok()`
  + skip-if-unavailable pattern; missing backends do NOT fail CI, they record
  a skip. The four-triple matrix is verified end-to-end in `release.yml` via
  cross-build runners, not in `cargo test`.

## 8. Sub-ADR boundary — what wave-2 SHIPS

Concrete deliverables:

- `crates/cobrust-codegen/src/llvm_backend.rs::emit` runs `Module::run_passes`
  with the pipeline string from `OptLevel` mapping (§3.2).
- `crates/cobrust-codegen/src/llvm_backend.rs::build_target_machine` keeps
  parametric triple dispatch (unchanged from wave-1; doc strengthened to
  enumerate the four tier-1 triples).
- `crates/cobrust-codegen/tests/binary_size_bench.rs` (NEW): 5-fixture bench
  harness. Asserts O3 ≤ 70% O0 size on median.
- ADR-0023 §A3 amendment: status RESOLVED at `<wave-2 close SHA>`; cites the
  empirical median ratio from the bench harness.

What wave-2 does NOT ship — explicit non-goals (§4):

- DWARF (sub-ADR 0058c).
- New MIR features.
- JIT opt-level changes.
- Cross-link.
- Manual PassBuilder flag tuning beyond `default<O*>` defaults.

## 9. Cascade enumeration

None anticipated. Wave-2 is **scope-narrow** — the PassBuilder wire is one
function call, multi-target dispatch is already correct in wave-1, and the
bench harness is a new file with no public-surface impact. If empirical O3
ratio fails the bar, recovery is in scope (§7.2 fall-back) without re-spec.

## 10. Evidence

- ADR-0058 (Phase K frame, accepted `9bf8d67`) — §5.2 OptLevel pipeline table;
  §"Sub-ADR roster" pins wave-2 as opt + multi-target sub-ADR.
- ADR-0058a (Phase K wave-1, accepted `3d60e63`) — §8 non-goals carved opt
  pipeline + multi-target out; §"Cascade enumeration" §14 documents impl
  ratifications wave-2 inherits.
- ADR-0023 (M9 codegen, accepted `ec680bc`) — §"LLVM `-O3` ≥ 30% smaller
  binary acceptance" pins the bar wave-2 empirically closes; §"Public surface"
  pins the three-variant `OptLevel` enum wave-2 maps to PassBuilder strings.
- ADR-0046 (release.yml tier-1 contract, current `c06f0bd`) — §"Tier-1
  platform contract" four-target list wave-2 codifies; Strand #5 musl
  promotion at `2b14cee`.
- `crates/cobrust-codegen/src/llvm_backend.rs` HEAD `c06f0bd` — wave-1
  emitter; lines 75-114 `emit()` post-define hook point where wave-2 inserts
  `run_passes`; lines 142-168 `build_target_machine` parametric triple dispatch.
- inkwell 0.9 docs — `inkwell::passes::PassBuilderOptions`,
  `inkwell::module::Module::run_passes(passes: &str, machine: &TargetMachine,
  options: PassBuilderOptions) -> Result<(), LLVMString>`.
- LLVM new pass manager — `default<O3>` pipeline; `opt -passes='default<O3>'`.

— P10 Phase K wave-2 dispatcher, 2026-05-19

## 11. Ratification — empirical close (2026-05-19)

DG-Workstation verify @ `cargo test -p cobrust-codegen --features llvm -p cobrust-jit`:

- **TEST_EXIT=0**, 404 tests PASS / 0 FAILED / 8 ignored.
- **codegen** (392): 12 unit (incl. 5 wave-2 inline added by 4749f99) + 2 binary_size_bench + 31 aggregate + 31 cast + 50 diff + 50 ill_formed + 16 object_layout + 10 release_smoke + 70 well_formed + 33 control_flow + 16 float_return + 10 fnref_call + 12 if_condition + 30 ref + 12 while_condition + 7 while_if.
- **cobrust-jit** (12): 1 unit + 11 jit_roundtrip. Wave-2 does not touch JIT lowering surface; regression clean.
- **POSTFLIGHT clean**: `/tmp/cobrust-*` count 0 → 0 across the bench run (one mid-run drop required PRE_TMP=434 cleanup pre-bench-rerun; final bench POST_TMP=0).

### 11.1 Binary-size empirical (ADR-0023 §A3 close)

Per-fixture O3/O0 object-file size ratio on DG-Workstation
`x86_64-unknown-linux-gnu` host:

| Fixture | O0 size | O3 size | Ratio |
|---|---|---|---|
| `hello` | 872 | 576 | **0.661** |
| `fizzbuzz` | 1408 | 760 | **0.540** |
| `fib` | 1192 | 696 | **0.584** |
| `dot_product` | 1056 | 640 | **0.606** |
| `nested_branch` | 1200 | 624 | **0.520** |

**Median ratio: 0.584** (41.6% size reduction). Clears the ADR-0023 §A3
≥ 30% bar (≤ 0.70 ratio). Sorted per-fixture ratios:
`[0.520, 0.540, 0.584, 0.606, 0.661]`.

The `default<O3>,default<Os>` pipeline + `OptimizationLevel::Aggressive`
TargetMachine combination is sound. §7.2 risk's `default<Os>` overlay
fall-back is NOT triggered.

### 11.2 ADR-0023 §A3 RESOLVED

ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" RESOLVED at
HEAD `72f4d27`. Median size reduction 41.6% on the 5-fixture bench
corpus exceeds the 30% bar by a 11.6-pp safety margin.

— P10 Phase K wave-2 ratifier, 2026-05-19
