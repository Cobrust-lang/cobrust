---
finding_id: F61
title: §X.4 UnsupportedTarget probe used a platform-divergent real arch (xtensa) — green on macOS, red on ubuntu CI
status: RESOLVED (use Architecture::Unknown — no LLVM backend on any platform)
date: 2026-05-27
severity: low
siblings: [F58, F59, F60]
last_verified_commit: 27562c5
---

# F61 — `ill_003`/`ill_004` UnsupportedTarget probe was platform-divergent

## §1 Context

The §X.4 sprint (Cranelift AOT removal) repointed `codegen_ill_formed.rs`
`ill_003`/`ill_004` (the `CodegenError::UnsupportedTarget` regression probes) from
the old SPARC64 triple to `xtensa-unknown-none-elf` — a flagged judgment call,
because LLVM 18 *does* register SPARC + RISC-V (so the old probe no longer errored).
The macOS author run + the mac-only paired audit both passed. CI then failed on
`cargo test (ubuntu-latest)`: `assert ... contains("xtensa")` and the
`matches!(UnsupportedTarget)` assert both failed.

## §2 Root cause

`Xtensa` is an LLVM **experimental** target. Ubuntu's `apt llvm-18` build registers
it; macOS's `brew llvm@18` does not. So `Target::from_triple("xtensa-…")`:
- **macOS** → no Xtensa backend → `Err` → `UnsupportedTarget` (tests pass).
- **ubuntu** → Xtensa registered → `Ok` → emit proceeds, no error → `unwrap_err()`
  panics / the `contains("xtensa")` assert fails (tests red).

Any *real* architecture is a poor "guaranteed-unsupported" probe — the registered
target set varies by LLVM build. A mac-only audit structurally cannot catch this.

## §3 Resolution

Force the host triple's architecture to `target_lexicon::Architecture::Unknown`
(`unsupported_triple()` helper). An `unknown` arch has NO LLVM backend in ANY build,
so `Target::from_triple` rejects it deterministically on every platform →
platform-invariant `UnsupportedTarget` coverage. `ill_004` now asserts the error
message carries the (deterministic) triple string rather than a hard-coded arch name.

## §4 Process note (compounds F60)

Two §X.4 follow-on findings (F60 doc-coverage, F61 xtensa) both slipped the §X.4
paired audit because it ran on macOS only. Lesson: for codegen/target changes,
(a) platform-sensitive assertions must use platform-invariant inputs, and (b) the
ubuntu CI run is the authoritative oracle — a GREEN mac audit is necessary, not
sufficient. The X.3/X.4 LLVM-default + Cranelift-removal arc has now surfaced
F53/F55/F56/F58/F60/F61 as a detection-gate cascade (all real latent gaps).
