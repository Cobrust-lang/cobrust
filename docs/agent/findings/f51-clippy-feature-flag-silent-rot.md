---
name: f51
status: RATIFIED
family: F44-sibling
date: 2026-05-25
last_verified_commit: 910279d
---

# F51 — `cargo clippy --features llvm` not run in CI → feature-gated lints silent-rot

## §1 Context

Discovered 2026-05-25 during ADR-0058g sub-wave-3 dispatch (author `a8275dacf32732746`). Sub-wave-2 test file `crates/cobrust-codegen/tests/llvm_wave3_list_runtime.rs` (landed `be4f074`) accumulated 4 clippy warnings under `cargo clippy -p cobrust-codegen --all-targets --features llvm`:

- `items_after_statements` × 3 (lines 621, 694, 745)
- `similar_names` × 1 (line 694 vicinity)

These warnings would normally fail CI under `-D warnings`. They did not, because:

**CI does not invoke `cargo clippy` with `--features llvm`.** The CI clippy job (`.github/workflows/ci.yml`) runs default-feature only. Feature-gated lints in code conditionally compiled under `#[cfg(feature = "llvm")]` (and test files that exercise that path) are invisible to CI.

## §2 Family

F44-sibling — "CI cache stale green != working" generalises to "CI scope incomplete != all-clean". Both F44 (cache invalidation) and F51 (feature flag scope) hide lints that surface only under different invocation conditions.

## §3 Detection rule

CI MUST exercise `cargo clippy --workspace --all-targets --features llvm -- -D warnings` as a blocking job, NOT just default features. Without this, any code under `#[cfg(feature = "llvm")]` (including tests gating on the feature) silently rots clippy warnings.

A pre-tag CI gate already exists for the runtime path (ADR-0069 §post-package smoke). F51 adds the discipline at the clippy linting tier.

## §4 Immediate resolution

`#![allow(...)]` module-level on the offending sub-wave-2 test file (commit 待 follow-up SHA),mirroring the sub-wave-3 author's same-day defensive pattern. Reasons cited per F51.

## §5 Systemic resolution (deferred)

Add `cargo clippy --features llvm` blocking job to `.github/workflows/ci.yml`. This is a CI workflow change requiring its own sprint (and tests across all `--features llvm` -gated code paths to ensure no surprises).

## §6 Cross-refs

- F44 — CI cache stale green
- F37 — silent rot on accepted debt
- F45a — LLVM wave-3 scope (parent context — feature-gated impl is where this lurks)
- F50 — LSP/CLI diagnostic divergence (sibling pattern: different invocation reveals different gaps)

## §7 Status

RATIFIED 2026-05-25 by ADR-0058g sub-wave-3 author empirical discovery + retroactive sub-wave-2 lint patch.
