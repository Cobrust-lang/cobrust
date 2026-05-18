---
doc_kind: adr
adr_id: 0058
parent_adr: 0054
title: "Phase K frame — LLVM Backend (release perf + cross-platform + DWARF prep for Phase L)"
status: proposed
date: 2026-05-18
last_verified_commit: 2a710d3
supersedes: []
superseded_by: []
relates_to: [adr:0054, adr:0023, adr:0046]
discovered_by: P10/user 2026-05-18 batch frame-author dispatch ("author all post-G frames in parallel")
ratification_path: P9 frame-ADR review; ratifies on first sub-ADR (0058a / 0058b / 0058c) dispatch
---

# ADR-0058: Phase K frame — LLVM Backend

## 1. Context

### 1.1 ADR-0023 deferral, ADR-0054 un-deferral

ADR-0023 (M9 codegen, accepted `ec680bc`) shipped Cranelift as the M9 default and pinned LLVM as `--features llvm` opt-in. The LLVM backend was **scaffolded but not implemented** at M9 — `crates/cobrust-codegen/src/llvm_backend.rs` exists behind the feature gate but the entry path under `Backend::Llvm` either errors `UnsupportedBackend` (no `--features llvm`) or hits a stub when the feature is on. ADR-0023 §"Per-MIR-form lowering rules" already enumerates the LLVM-column targets per MIR construct as a forward-compat contract, but no Phase F+ activation was scheduled.

ADR-0054 §"Phase K" un-defers this work. Post-Phase-G (v0.3.0 shipped), Phase K activates the LLVM column of ADR-0023's lowering table, lights up `inkwell` as the binding crate, and expands the tier-1 target matrix to ADR-0046's three rows.

### 1.2 Why Phase K (not Phase K+) sits at this slot

- **Cranelift stays canonical for `cargo build`** (dev path). Cranelift's sub-second incremental compile remains the default; ADR-0023 §"Backend feature flag layout" binding is preserved.
- **LLVM activates for `cargo build --release`** (prod path). ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" pins the bar; Phase K is the empirical close.
- **Phase L Debugger consumes Phase K's DWARF emission.** Phase L (ADR-0059) blocks on ADR-0058c (DWARF sub-ADR). The Phase K → L handoff is the gating dependency between the two phases.
- **Cross-platform credibility.** ADR-0046's tier-1 contract enumerates 3 targets (`aarch64-apple-darwin` + `aarch64-unknown-linux-gnu` + `x86_64-unknown-linux-gnu`); Cranelift's coverage at M9 is delivered on the first 2, "reachable" on the 3rd. Phase K promotes all 3 to LLVM-built release-tier delivery.

### 1.3 Constitutional anchors

- **CLAUDE.md §2.5** — LLM-first design principle. Phase K is §2.5-neutral; see §2 of this ADR.
- **CLAUDE.md §4.1** — pipeline diagram explicitly enumerates `Codegen (LLVM / Cranelift)` as the M9 backend. LLVM is on the constitutional roadmap, not bolt-on.
- **CLAUDE.md §5.3** — efficient: AOT default, JIT optional. Phase K AOT-only; Phase I (REPL JIT) is the JIT path.

## 2. §2.5 ROI position — neutral, ranked #4

Phase K is **§2.5-neutral**. Per ADR-0054 §2:

| Rank | Phase | Surface | §2.5 ROI |
|---|---|---|---|
| 1 | J | LSP server | highest (LLM-amplifier) |
| 2 | H | Self-host type checker | high (training-data overlap) |
| 3 | I | REPL JIT | medium (translation closed-loop) |
| **4** | **K** | **LLVM Backend** | **neutral (product credibility, not LLM-amplifier)** |
| 5 | L | Debugger | ~0 (human-facing) |

The LLM does **not** write Cobrust code differently because of LLVM. LLVM lowering quality affects:

- **Binary size / runtime perf** — numpy-tier workloads benchmark below numpy on Cranelift `-O0`.
- **Cross-platform reach** — ADR-0046 tier-1 contract gains delivery confidence.
- **Phase L unblock** — DWARF emission is shared with debugger.

None of these are §2.5 §A (compile-time-catch-errors) or §2.5 §B (training-data overlap) wins. Phase K is product-completeness work — shipped because release-mode credibility matters, not because LLM-authored translations get more correct.

This ranking is identical to ADR-0054 §2's Phase K row. The frame-ADR codifies it again for dispatch-time audit clarity.

## 3. Decision

Activate `Backend::Llvm` as a parallel implementation alongside `Backend::Cranelift` in `crates/cobrust-codegen/`. MIR shape (ADR-0020) is unchanged; both backends consume the identical `cobrust_mir::Module`. Lowering differs per ADR-0023 §"Per-MIR-form lowering rules" LLVM column.

Cranelift remains the dev-mode default. LLVM becomes the release-mode default once `--features llvm` is on. Both backends ship; neither replaces the other.

### 3.1 Feature flag binding

The existing `crates/cobrust-codegen/Cargo.toml` `[features] llvm = ["dep:inkwell"]` gate (per ADR-0023 §"Backend feature flag layout") is preserved. Phase K does **not** make `inkwell` a non-optional dependency. The dev `cargo build` path stays pure-Rust per ADR-0023's binding rationale.

### 3.2 Mode selection (binding)

| Build mode | Backend chosen | Rationale |
|---|---|---|
| `cargo build` (dev) | `Backend::Cranelift` | Sub-second incremental, pure-Rust dep tree (ADR-0023 §"Backend feature flag layout") |
| `cargo build --release` w/o `--features llvm` | `Backend::Cranelift` at `OptLevel::Speed` | Cranelift release fallback path (existing) |
| `cargo build --release --features llvm` | `Backend::Llvm` at `OptLevel::SpeedAndSize` | Phase K default for prod release |

The default-resolution logic in `crates/cobrust-codegen/src/target.rs` `Backend::default_for_profile()` is updated to consult `cfg!(feature = "llvm")` plus `OptLevel`.

## 4. LLVM dependency choice — `inkwell`

Bind `inkwell` (safe-Rust LLVM wrapper) at LLVM 18 if the target build host has LLVM 18 available, otherwise LLVM 17. Concretely:

- **Crate**: `inkwell = { version = "0.9", optional = true, features = ["llvm18-1"] }`. ADR-0023 §"Backend feature flag layout" already pins `inkwell = "0.9"` — the latest stable as of `1fbed82`. Phase K keeps this version and **adds** `features = ["llvm18-1"]` to activate LLVM 18; `llvm17-0` accepted as fallback. Phase K dispatch verifies the lockfile-pinned version at sub-ADR 0058a entry.

  > **Correction 2026-05-18 per audit `a8155e81cb212aaca` F1**: ADR-0023 comment treated `inkwell = "0.9"` as forward-compat placeholder; this was empirically incorrect (0.9 is latest stable on crates.io). The `llvm18-1` feature only exists on inkwell ≥ 0.6, making a downgrade to 0.5 immediately fatal to `cargo build`. Phase K keeps 0.9 + enables `llvm18-1` feature.
- **LLVM version**: 18.x preferred, 17.x acceptable fallback. CI matrix expands to verify both.
- **Pin via `Cargo.lock`**: the exact `inkwell` revision + LLVM features array land in `Cargo.lock`; sub-ADR 0058a captures the pin commit.
- **System LLVM**: macOS via `brew install llvm@18`; Linux via apt `llvm-18-dev` / `libpolly-18-dev`. Documented in `docs/human/{zh,en}/install.md` Phase K addition.

Bind-the-core principle (ADR-0012) applies: we do not reimplement LLVM. `inkwell` is the bound surface; `llvm-sys` is the per-feature fallback if `inkwell` coverage gaps surface (§10 risk register).

## 5. MIR → LLVM IR lowering

Function-by-function lowering. Each MIR `Body` lowers to an LLVM `FunctionValue` with the `extern "Cobrust"` calling convention (System V AMD64 on Linux x86_64; AAPCS64 on macOS arm64 + Linux arm64) per ADR-0023 §"Calling convention details (binding)".

### 5.1 Lowering table (excerpt — full table per ADR-0023 §"Per-MIR-form lowering rules" LLVM column)

| MIR construct | LLVM (inkwell) lowering |
|---|---|
| `Body` | `FunctionValue` with `LinkageType::External` |
| `LocalDecl` | stack `alloca` + load / store |
| `BasicBlock` | `BasicBlock` via `LLVMAppendBasicBlock` |
| `Statement::Assign` | `build_store(rvalue, place_ptr)` |
| `Terminator::Goto(b)` | `build_unconditional_branch(b)` |
| `Terminator::SwitchInt` | `build_switch` |
| `Terminator::Return` | `build_return(Some(&ret))` |
| `Terminator::Call` | `build_call(fn, &args, "call")` |
| `Terminator::Drop` | `build_call(_cobrust_drop_<TypeId>, &[place])` |

### 5.2 Optimization pass pipeline (binding)

| `OptLevel` | LLVM pass pipeline | Use |
|---|---|---|
| `OptLevel::None` (O0) | `default<O0>` | Dev parity with Cranelift; LLVM-side debugging |
| `OptLevel::Speed` (O2) | `default<O2>` | Phase K release-mode **default** |
| `OptLevel::SpeedAndSize` (O3) | `default<O3>` then `default<Os>` | Binary-size acceptance bar (ADR-0023 ≥30%) |
| Custom `-O3` opt-in | `default<O3>` (no `Os`) | Pure-perf benchmark mode |

`OptLevel::Speed` (O2) is the new Phase K release default. ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" applies under the `OptLevel::SpeedAndSize` path; ADR-0058b empirically closes the bar.

Cranelift's optimization tier stays at `OptLevel::None` (dev) and `OptLevel::Speed` (Cranelift's own optimizer, not LLVM-O2-equivalent). Cranelift is **not** an `-O3` competitor; that's exclusively the LLVM path.

## 6. Cross-platform target matrix

Phase K's tier-1 target deliverables match ADR-0046 §"Tier-1 platform contract" exactly:

| Triple | Object format | LLVM target | Status (Phase K close) |
|---|---|---|---|
| `aarch64-apple-darwin` | Mach-O | `arm64-apple-darwin` | delivered |
| `aarch64-unknown-linux-gnu` | ELF | `aarch64-unknown-linux-gnu` | delivered |
| `x86_64-unknown-linux-gnu` | ELF | `x86_64-unknown-linux-gnu` | delivered |
| `x86_64-apple-darwin` | Mach-O | `x86_64-apple-darwin` | reachable (post-K bonus, not gated) |
| `wasm32-unknown-unknown` | WASM | `wasm32-unknown-unknown` | out of scope (ADR-0023 §"Target triple matrix") |

LLVM target triple resolution uses `inkwell::targets::Target::from_triple` + `TargetMachine::create_from`. Sysroot detection delegated to `cc` (per ADR-0023 §"Linker delegation" binding). Cross-compilation between tier-1 targets uses `cross` (already wired in `release.yml` for `aarch64-unknown-linux-gnu`).

## 7. DWARF debug-info emission

LLVM has full DWARF v5 support via `inkwell::debug_info::DebugInfoBuilder` (wrapping LLVM `DIBuilder`). Phase K's DWARF emission contract:

- **DWARF lines**: every MIR statement carries its source `Span`; lowering emits `dbg.declare` / `dbg.value` intrinsics keyed by the span's line + column.
- **Variable info**: every `LocalDecl` emits a `DILocalVariable` entry; `extern "Cobrust"` parameters emit `DIFormalParameter` entries.
- **Type info**: every `cobrust_types::Ty` lowers to a `DIType` (primitives → `DIBasicType`; tuples / structs → `DICompositeType`).
- **Inlining**: deferred to post-Phase-K. Inlined-frame DWARF (`DILocation` inlined-at chain) is Phase-L+ if debugger demand surfaces it.

DWARF is emitted into the `.debug_info` / `.debug_line` ELF sections (Linux) or `__DWARF` segment (Mach-O). Both formats are `inkwell::module::Module::create_debug_info_builder` standard output.

Phase L Debugger (ADR-0059) consumes this DWARF via `lldb` / `gdb` / VS Code DAP — no Cobrust-specific bridging required. The bind-the-core principle (ADR-0012) extends: we emit standard DWARF, debuggers consume it.

## 8. Sub-ADR roster

Four ADRs land under the Phase K frame:

- **ADR-0058** (this ADR) — Phase K frame. Roadmap, ROI position, sub-ADR scope.
- **ADR-0058a** — MIR → LLVM IR lowering core (~1 week wall). Implements §5 lowering table; closes Cranelift / LLVM differential gate on the M9 "core 30" forms.
- **ADR-0058b** — Optimization pipeline + multi-target (~1 week wall). Implements §5.2 pass pipeline; closes ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" bar; expands tier-1 matrix per §6.
- **ADR-0058c** — DWARF debug-info emission (~1 week wall). Implements §7; produces the artifact Phase L consumes.

Total: 3 sub-ADRs after the frame. ADR-0054 §6.3 lists three sub-ADRs (0058a / 0058b / 0058c) under different scopes; this frame's roster supersedes the §6.3 hint with the §5 / §5.2 / §6 / §7 binding split codified above.

## 9. Wave plan — sequential, NOT parallelizable internally

```
ADR-0058 (this frame, proposed)
       │
       ▼
ADR-0058a (MIR → LLVM IR core)
       │
       │ blocks on: core IR lowering must compile + pass M9 diff gate
       ▼
ADR-0058b (Optimization + multi-target)
       │
       │ blocks on: 0058a's IR lowering operational on tier-1 host
       ▼
ADR-0058c (DWARF emission)
       │
       │ blocks on: 0058a's lowering keyed-by-Span infrastructure
       ▼
Phase L (ADR-0059) — Debugger
```

**Sequential, not parallel**, because:

- **0058b depends on 0058a**: optimization passes operate on emitted IR. No IR = no optimization to test.
- **0058c depends on 0058a**: DWARF intrinsics emit at IR-construction time, interleaved with `build_store` / `build_call`. The `DebugInfoBuilder` wires into the same `Builder` cursor that 0058a constructs. Can't add debug info without an IR-emission pass to attach it to.
- **0058b and 0058c could theoretically overlap** but in practice they touch overlapping codepaths (both modify the per-`Body` lowering pass), so dispatch ordering prefers serial.

Phase H + I (ADR-0055 + 0056) per ADR-0054 §9.1 can OVERLAP; Phase K cannot internally. The cost is wall-time depth without parallelization saving — ~3 weeks minimum, ~4 weeks with normal risk buffer.

## 10. Risk register

Three concrete risks tracked for Phase K dispatch:

### 10.1 LLVM API instability across versions

- **Risk**: `inkwell` major-version updates lag LLVM major-version releases by 3-6 months. LLVM 19 ships Q3 2026; `inkwell` 0.5.x supports through LLVM 18 only. If LLVM 18 is dropped from a tier-1 host distro before `inkwell` 0.6 ships, Phase K binding stalls.
- **Mitigation**: pin `inkwell` version + LLVM feature flag in `Cargo.lock` at sub-ADR 0058a; CI matrix pins LLVM 17 and 18 explicitly via apt / brew. If LLVM 19 forced by distro update, fall back to LLVM 17 (oldest supported) until `inkwell` catches up.

### 10.2 `inkwell` coverage gaps vs raw `llvm-sys`

- **Risk**: `inkwell`'s safe-Rust API does not cover every `llvm-sys` C-binding. DWARF inlined-frame chains and some LLVM intrinsics are partially-wrapped. Sub-ADR 0058c may hit a coverage gap during DWARF emission.
- **Mitigation**: identify gaps at 0058a entry (run `cargo doc --open` on `inkwell` against the §7 DWARF API surface; cross-check against `llvm-sys`). If gap surfaces, drop to `llvm-sys` for that specific feature; document the carve-out in sub-ADR 0058c. ADR-0012 bind-the-core principle permits per-feature fallback.

### 10.3 Linker integration (cross-target dispatch)

- **Risk**: Linker dispatch differs per target. `aarch64-unknown-linux-gnu` builds from a non-arm64 host (e.g. CI's x86_64 Linux runner) need `cross` + arm64 sysroot. `aarch64-apple-darwin` builds from Intel Mac (post-K bonus row) need cross-SDK. `cc` invocation per ADR-0023 §"Linker delegation" already handles native targets; cross-target adds matrix complexity.
- **Mitigation**: Phase K reuses `cobrust-cli`'s existing linker dispatch from M9. The `release.yml` v0.1.2 binding (ADR-0046 §"Tier-1 platform contract") already exercises `cross` for `aarch64-unknown-linux-gnu`; Phase K dispatch piggybacks on that proven path. Sub-ADR 0058b documents the per-target `cross` config.

## 11. Pre-dispatch acceptance gate

Phase K dispatches only when all four conditions hold:

- **Phase G fully closed**: v0.3.0 stable tag shipped (Wave-2 round-2 + ADR-0052d method-call-sugar impl accepted). Verified at HEAD `1fbed82` ✓ — v0.3.0 closure is the dispatch-readiness anchor.
- **Phase H + I non-blocking**: LLVM Backend is independent of self-host type checker (Phase H) and REPL JIT (Phase I). Phase K can dispatch in parallel with Phase H sub-ADR completion or after Phase I closes. Per ADR-0054 §9.2, Phase K is non-critical-path until Phase J ships; Phase K may begin earlier without blocking critical-path.
- **LLVM toolchain available on dispatch hosts**: Mac (homebrew `llvm@18` already installable; preflight at sub-ADR 0058a entry); DG-Workstation (apt `llvm-18-dev`; ssh preflight check). Both required before 0058a dispatch begins.
- **Phase J non-blocking**: LSP server (Phase J) does not require LLVM; LSP-driven IDE editing latency is fine under Cranelift `-O0`. Per ADR-0054 §9.1 "Phase K sequential after Phase J", Phase K may begin before Phase J closes if Phase G is closed; the §9.1 ordering is a default, not a blocker.

## 12. Compression-ratio note

ADR-0054 §8.4 calls out Phase K as ~2-3x compression vs ~4-8x for self-contained pure-Rust phases. The Phase K work is **external-system-bound**:

- **LLVM API surface** is documented but evolving. Cross-checking inkwell wrappers against LLVM IR semantics requires reading LLVM's own docs (and occasional LLVM C++ source) — slower than internal Rust work.
- **Linker integration** dispatches into `cc` / `lld` / `cross` toolchains, each with platform-specific environment dependencies. Failures bottleneck on external tool docs.
- **DWARF format** has 5+ revisions; LLVM emits a specific subset; debuggers consume different subsets per version.

Budget accordingly: **3-4 weeks wall agent-velocity** (vs ~2 months human-developer estimate). Compression ratio nearer the low end of ADR-0054 §8.4's "External-system-bound" row (~2-3x). Each sub-ADR (0058a / 0058b / 0058c) is ~1 week wall.

Risk acknowledged: if `inkwell` coverage gap (§10.2) surfaces, Phase K could slip into a 5th week. Buffer +1 week before triggering critical-path slip into Phase L.

## 13. Phase K × L handoff

Phase L Debugger (ADR-0059) blocks on Phase K's ADR-0058c sub-ADR landing:

- ADR-0058c emits DWARF v5 lines + variables + types.
- ADR-0059 consumes via `lldb` / `gdb` / VS Code DAP — standard DWARF consumers.
- No bespoke Cobrust ↔ debugger bridging required (bind-the-core, ADR-0012).
- Phase L dispatch readiness gate: ADR-0058c accepted + DWARF emission verified on at least one tier-1 target (per §6 matrix).

If Phase K slips, Phase L slips by the same delta. ADR-0054 §9 critical-path treats the Phase K → L sequence as sequential; this ADR preserves that binding.

## 14. Consequences

### 14.1 Positive

- LLVM `-O3` release-mode codegen unblocks numpy-tier perf workloads (ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary acceptance" empirical close).
- Tier-1 target matrix (ADR-0046) gains LLVM-backed delivery on all 3 rows.
- DWARF emission lands the Phase L Debugger gate; ADR-0059 unblocks immediately on 0058c acceptance.
- Cranelift dev-path preserved — no regression to `cargo build` sub-second incremental.

### 14.2 Negative

- 3-4 weeks wall agent-velocity (compression-ratio-bound at ~2-3x per §12).
- Two backend lowering paths to maintain long-term (already accepted at ADR-0023 §"Consequences"; Phase K does not re-litigate).
- System LLVM dep adds CI matrix complexity (LLVM 17 + 18 verification; `cross` cross-target sysroots).
- §2.5-neutral: Phase K consumes 3-4 weeks runway with no LLM-amplifier payoff (justified by product credibility + Phase L unblock, not LLM ergonomics).

### 14.3 Neutral

- `wasm32-unknown-unknown` remains out of scope (ADR-0023 §"Target triple matrix"); Phase K does not change the WASM deferral.
- `x86_64-pc-windows-msvc` stays in ADR-0046 "queued" tier; Phase K does not promote Windows.
- LLVM 18 vs 17 binding is reversible (lockfile pin); future LLVM 19 / `inkwell` 0.6 upgrade is a sub-ADR followup, not a frame revision.

## 15. Dispatch readiness — TEST / DEV hours, ~3-4 weeks total wall

- **ADR-0058a** (MIR → LLVM IR core): ~1 week wall. TEST ~10h (diff-gate corpus mirroring against Cranelift); DEV ~25h.
- **ADR-0058b** (Optimization + multi-target): ~1 week wall. TEST ~8h (per-target smoke + binary-size benchmark); DEV ~20h.
- **ADR-0058c** (DWARF emission): ~1 week wall. TEST ~6h (DWARF section presence + lldb-parse smoke); DEV ~18h.
- **Buffer**: +1 week for §10.2 inkwell coverage gap risk.

**Total**: ~3-4 weeks wall agent-velocity. Sequential per §9; no internal parallelization saving.

## 16. Why this ADR now

- **Phase G closed** at v0.3.0 (HEAD `1fbed82`). Post-G frame ADRs (H / I / J / K / L) author in parallel per user 2026-05-18 batch dispatch.
- **K is independent of H / I / J impl-wise**: LLVM Backend touches `crates/cobrust-codegen/`; Phase H touches `crates/cobrust-types-cb/` (new); Phase I touches `crates/cobrust-cli/src/repl.rs`; Phase J touches `crates/cobrust-lsp/` (new). Zero file-path overlap. Frame-author parallel dispatch is sound.
- **DWARF emission is the Phase L gate.** Authoring Phase K frame ex-ante codifies the 0058c → 0059 handoff before Phase L frame dispatch, preventing scope drift at Phase L authoring time.
- **ADR-0023 forward-compat contract activation deserves explicit frame.** ADR-0023 enumerated LLVM lowering per MIR form in 2026-04-30; Phase K is the un-deferral. A frame ADR makes the activation auditable.

## 17. Evidence

- ADR-0023 (M9 codegen, `ec680bc`) — §"Backend feature flag layout" `--features llvm` opt-in path; §"Per-MIR-form lowering rules" LLVM column; §"LLVM `-O3` ≥ 30% smaller binary acceptance" bar.
- ADR-0046 (release.yml tier-1 contract, current `1fbed82`) — §"Tier-1 platform contract" 3-target list (`aarch64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`).
- ADR-0054 (post-Phase-G roadmap, `bc10842`) — §"Phase K" un-defer (3-4w wall, ~2-3x compression external-bound); §2 §2.5 ROI rank table (Phase K #4 neutral); §9.1 sequential-after-Phase-J ordering.
- `crates/cobrust-codegen/src/lib.rs` lines 74-112 (HEAD `1fbed82`) — existing `Backend::Llvm` dispatch arm + `#[cfg(feature = "llvm")] pub mod llvm_backend`.
- `crates/cobrust-codegen/src/cranelift_backend.rs` lines 1-60 (HEAD `1fbed82`) — Cranelift backend reference; Phase K parallel-impl mirror, not replacement.
- CLAUDE.md §2.5 (HEAD `1fbed82`) — LLM-first design principle; Phase K §2.5-neutral.
- CLAUDE.md §4.1 — pipeline diagram `Codegen (LLVM / Cranelift)` anchor.
- `inkwell` crate documentation — <https://docs.rs/inkwell>; LLVM 18 binding via `llvm18-1` feature flag.

— P9 Tech Lead, 2026-05-18
