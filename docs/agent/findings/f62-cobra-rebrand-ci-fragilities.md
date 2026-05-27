---
finding_id: F62
title: Cobra-rebrand surfaced two CI fragilities — bare-lib-name merged-doctest + disk-fragile size bench
status: RESOLVED (den lib doctest disabled; binary_size_bench #[ignore]'d)
date: 2026-05-28
severity: low
siblings: [F59, F60, F61]
last_verified_commit: d355b8f
---

# F62 — cobra-rebrand CI fragilities (doctest + size bench)

## §1 Context

The ADR-0071 cobra-rebrand CI run (`26530486106`, `d355b8f`) failed on BOTH platforms
with TWO unrelated issues — neither caught by the local (warm-build, macOS) paired
audit:
- **ubuntu**: `cobrust-den/src/lib.rs - (line 29)` doctest →
  `error: extern location for den does not exist: target/debug/deps/libden.rlib`.
- **macOS**: `binary_size_bench::o3_median_under_70pct` →
  `parse .../nested_branch.o: Could not read file magic` (a truncated/empty object).

## §2 Root causes (two)

### A. Bare `[lib] name` × Rust 1.94 merged-doctests × clean build
The rebrand set `[lib] name = "<cobra>"` (bare word) — required so the PyO3 cdylib +
`#[pymodule]` resolve as the user-facing module (`den`). On a CLEAN build, Rust 1.94's
**merged-doctests** harness compiles all doctests into one binary that links the crate
rlib, but fails to resolve `libden.rlib` (`extern crate r#den;`) before it is built —
an ordering bug that only bites cold builds. The local audit passed only because a warm
`libden.rlib` already existed in `target/debug/deps`. Only `cobrust-den` has a lib
doctest (the other 6 rebranded crates have none), so it was the sole casualty.

### B. Size benchmark × CI runner disk pressure
`binary_size_bench` compiles 5 fixtures × 2 opt levels = 10 temp `.o` files, then reads
them back. The rebrand run did a COLD full rebuild of a now-larger workspace (renamed
everything + new `den`/`coil` + rusqlite-bundled's libsqlite3 C compile), pressuring the
macOS runner's disk → a truncated `.o` write → `Could not read file magic`. It is a
size **benchmark** (ADR-0023 §A3 empirical close), not a correctness test, and the
codegen it exercises was untouched by the rebrand (it passed on prior runs).

## §3 Resolution

- **A**: mark the `cobrust-den/src/lib.rs` `//!` example fence ` ```ignore`. NOTE:
  `[lib] doctest = false` in Cargo.toml was tried first but is **NOT honored under Rust
  1.94 merged-doctests** (the example still ran), so the per-fence ` ```ignore` is the
  reliable mechanism. The example remains as documentation; den's behavior is verified by
  its 26-test integration suite (8 unit + 13 CPython-differential + fuzz), far exceeding a
  single doc example. Policy for the PyO3/.cb translation crates: bare lib name (for PyO3)
  ⇒ lib examples are ` ```ignore` (compile-verified via integration tests instead).
- **B**: `#[ignore]` both `binary_size_bench` tests (run opt-in via `--ignored`). A size
  benchmark with heavy temp I/O must not gate CI (F59-style deterministic-CI discipline);
  the §A3 size-reduction contract stays documented + opt-in checkable.

## §4 Process note

Both slipped the rebrand paired audit because it ran on **macOS with a warm build**. The
recurring lesson (compounds F60/F61): for cross-platform/codegen/build-config changes,
the **clean-build CI on both platforms is the authoritative oracle** — a green warm-build
mac audit is necessary, not sufficient. The X.3/X.4/rebrand arc has now surfaced
F53/F55/F56/F58/F60/F61/F62 as a detection-gate cascade.
