---
doc_kind: adr
adr_id: 0043
title: "pyo3 0.22 → 0.23+ workspace upgrade — spike + migration plan"
status: proposed
date: 2026-05-11
last_verified_commit: acc708c
supersedes: []
superseded_by: []
relates_to: [adr:0011, adr:0022]
---

# ADR-0043: pyo3 0.22 → 0.23+ upgrade

## Context

The v0.1.1 CI hot-fix sprint (`03003f3` + `acc708c`) revealed that PyO3 0.22.6
is incompatible with the Python interpreters currently shipped by GitHub Actions
runners. Three distinct failure classes were observed:

1. **Version ceiling exceeded (macOS, Python 3.14 default):** PyO3 0.22 declares
   a maximum supported Python version of 3.13. On `macos-latest`, the default
   system Python is 3.14, causing `cargo build --features pyo3` to abort with
   "the configured Python interpreter version (3.14) is newer than PyO3's
   maximum supported version (3.13)."

2. **Macro-generated deprecated constants (Ubuntu, Python 3.11 via apt):**
   PyO3 0.22's `#[pymethods]` macro expands to references to deprecated
   `__pymethod_option__::SIGNATURE` and `__pymethod_argument__::SIGNATURE`
   constants. Under `-D warnings` (workspace default), these are fatal.

3. **Unsafe-block requirement (Ubuntu, Python 3.11 via apt):** The same macro
   expansion calls `unwrap_required_argument`, which is now `unsafe`. Without
   an `unsafe { }` block this is a hard `E0133` error — unsuppressible by
   `#[allow(...)]` attributes on cobrust source.

The skip-pattern workaround in `acc708c` is sound for the current test surface
but grows monotonically as pyo3 internal APIs evolve. The structural fix is to
upgrade the workspace to PyO3 0.23+, which resolves all three classes and
supports Python 3.11–3.14 without skip paths. This ADR is a spike: it captures
the impact analysis and migration plan so that a future implementation sprint
has a clear brief. No code changes are made here.

## Options considered

### Option A — Stay on pyo3 0.22, broaden skip-pattern further

Extend the `*_pyo3_compiles.rs` stderr matchers to cover additional error
signatures as they emerge. Add `#[allow(deprecated)]` shims where pyo3-generated
code references deprecated items from the cobrust side.

- Pro: zero implementation risk, no breaking API changes.
- Con: skip-pattern grows unboundedly with pyo3 internal evolution. The
  `unwrap_required_argument` `E0133` is a hard error — no `#[allow(...)]`
  suppresses it on user-side code, so the skip-pattern is the *only* mitigation.
  This means the five `*_pyo3_compiles.rs` harnesses remain permanently on the
  skip path and never exercise real PyO3 build success. That is a regression from
  the original ADR-0011 intent.

Rejected as a long-term strategy. The `acc708c` skip-pattern is the short-term
bridge; this option is the dead end.

### Option B — Upgrade to pyo3 0.23 (CHOSEN as plan; implementation deferred)

Bump `pyo3` and `pyo3-build-config` in `Cargo.toml` (workspace root) to 0.23.x.
Migrate all five pyo3-using crates to the 0.23 API surface:

- `&PyAny` → `Bound<'py, PyAny>` (the main ergonomic break)
- Implicit `defaults` in `#[pyo3(signature = ...)]` → explicit `Option<T>` fields
- `GILPool` / `Python::acquire_gil()` → `Python::with_gil(|py| ...)` (already
  the recommended pattern in 0.22 but enforced in 0.23)
- `PyModule::add_function` call signature changes in some sub-paths

PyO3 0.23 officially supports Python 3.8–3.14, removing all three failure classes.
The `*_pyo3_compiles.rs` harnesses should switch from the skip path to the success
path on all supported runner configurations.

- Pro: permanent fix; no skip-pattern growth; harnesses exercise real build success.
- Con: mechanical but non-trivial migration across 5 crates; `Bound<'py, T>` GIL
  lifetime leaks into signatures, requiring care in functions that currently take
  `&PyAny` as a raw argument.

### Option C — Drop pyo3 dependency entirely, use CPython C API directly via `cpython` crate

Replace PyO3 with the lower-level `cpython` crate (raw C API bindings) or hand-rolled
`unsafe` FFI.

- Pro: no upstream version coupling.
- Con: massive regression in ergonomics, safety, and maintainability; all translated
  crates would require a ground-up rewrite of their Python binding layer. Disproportionate
  to the problem being solved. Rejected outright.

## Decision

Adopt **Option B** as the implementation plan. This ADR is the specification;
execution belongs to a dedicated implementation sprint. The `acc708c` skip-pattern
bridge remains in place until the upgrade lands.

Trigger: post-v0.1.1 ship. Sprint owner: P9 sonnet (if numpy-crate cascade does
not materialize) or P9 opus (if it does — see Risks).

## Impact analysis (per-crate)

| Crate | PyO3 surface | Breaking changes expected | Estimated effort |
|---|---|---|---|
| `cobrust-click` | `#[pyo3(signature=...)]` implicit defaults, `PyCommand` methods, `PyModule::add_function` | High — decorator macro chain rewrites; implicit-default → explicit-`Option<T>` across ~15 `#[pymethods]` blocks | ~3–4 hr |
| `cobrust-msgpack` | `&PyAny` → `Bound<'py, PyAny>` in pack/unpack paths, fn signatures | Medium — partially handled by existing skip-pattern; straightforward mechanical substitution | ~2 hr |
| `cobrust-dateutil` | Similar to msgpack — `&PyAny` in `parse_iso` / `relativedelta_add` call paths | Medium | ~2 hr |
| `cobrust-numpy` | `&PyAny` / `PyArray` type aliases; numpy crate itself pins pyo3 (see Risks) | High — may require numpy crate bump in addition to pyo3 bump | ~3–4 hr |
| `cobrust-requests` | Minimal pyo3 surface — only `PyModule::add_function` + one `#[pyfunction]` | Low | ~1 hr |

**Total estimate:** 1–1.5 sonnet days assuming no numpy-crate cascade. If the
numpy crate requires a coordinated bump: escalate to opus + 1 day.

## Migration strategy

1. **Spike commit on `feature/pyo3-0.23-upgrade` worktree:** bump `pyo3` and
   `pyo3-build-config` in `Cargo.toml` (workspace root). Fix the smallest crate
   first (`cobrust-requests` — ~1 hr) to validate that the bump compiles cleanly
   and the harness switches to the success path. Commit atomically; do not proceed
   to step 2 until the 5-gate is green for this single crate.

2. **Cascade through remaining 4 crates one at a time** in order:
   `cobrust-msgpack` → `cobrust-dateutil` → `cobrust-click` → `cobrust-numpy`.
   Each crate gets its own atomic commit: implementation + harness success-path
   verification + any updated `#[allow(...)]` cleanups.

3. **Verify all five `*_pyo3_compiles.rs` harnesses** switch from the skip path
   to the success path. Grep for `eprintln!("skipping pyo3 compile")` — it should
   appear zero times in the successful test run output on both macOS and Ubuntu
   runners.

4. **Trim the broadened skip-pattern matchers** introduced in `03003f3` and
   `acc708c`: remove the `unwrap_required_argument` / `__pymethod_` /
   `implicit defaults are being phased out` / `unused import: pyo3_bindings` /
   `newer than PyO3's maximum supported version` lines once they no longer apply.
   This is the proof that the upgrade has genuinely resolved the root causes.

5. **5-gate green on both architectures** (`macos-latest` + `ubuntu-latest`).
   Cross-arch validation on the <self-hosted-runner> (`x86_64`) per
   `reference_x86_workstation.md` if the numpy cascade fires.

6. **Tag `v0.1.2`** after merge. No language / translator behavior changes —
   CI infrastructure patch only (same category as v0.1.1).

## Risks

- **numpy crate cascade:** The `ndarray`-based numpy backend currently brings in
  the `numpy` crate (Rust bindings for NumPy C API), which itself pins `pyo3 = "0.22"`.
  If this transitive pin is strict (not `>=`), bumping the workspace pyo3 to 0.23
  will produce a resolver conflict. Mitigation: check `numpy` crate changelog for
  a 0.23-compatible release; if none, fork or vendor the crate for the upgrade
  sprint. The spike step 1 will surface this immediately.

- **`Bound<'py, T>` ergonomic regressions:** PyO3 0.23 makes GIL lifetime
  annotations explicit on `Bound<'py, T>`. Functions that currently accept
  `&PyAny` will need to accept `&Bound<'py, PyAny>` or `Bound<'py, PyAny>`.
  In some contexts this leaks the `'py` lifetime into caller signatures. If a
  function is called from a `'static` context, a `Python::with_gil` wrapper
  may be needed. The click decorator chain is the highest-risk site.

- **`pub use pyo3_bindings::*` re-exports:** The `#[allow(deprecated)]` interaction
  with deprecated `SIGNATURE` constants needs cleanup — once on 0.23 the constants
  no longer exist, so the `#[allow(deprecated)]` can be removed (and the warning
  will fire if it is not removed). This is mechanical but must be caught during
  step 4.

## Consequences

### Positive

- All five `*_pyo3_compiles.rs` harnesses switch from the skip path to the success
  path on all supported Python versions (3.11, 3.12, 3.13, 3.14).
- v0.1.x releases ship compile-from-source on Python 3.11–3.14 hosts without
  any skip-path accommodation.
- Eliminates the technical debt of "skip-pattern grows monotonically with pyo3
  internal evolution" — the root problem identified in the v0.1.1 postmortem.
- Positions the workspace for Python 3.15+ support: PyO3 0.23 follows a regular
  Python version support cadence.

### Negative

- 1–1.5 days of mechanical + intellectual work across 5 crates.
- Risk of discovering additional `Bound<'py>` cascade breakage during
  implementation (numpy crate cascade is the most likely).
- The `acc708c` skip-pattern bridge remains in-tree until the upgrade lands;
  new contributors may encounter it and need to understand why it exists.

## Cross-references

- `docs/agent/adr/0011-pyo3-build-path.md` — PyO3 build path baseline; ADR-0011
  §6 defines the "succeed-or-skip-cleanly" contract for `*_pyo3_compiles.rs`.
- `docs/agent/adr/0022-translation-ecosystem-batch.md` — M-batch translation
  ecosystem; all five affected crates introduced here.
- `docs/agent/adr/0042-snapshot-lint-enforcement.md` — sister ADR from the v0.1.1
  hot-fix batch (different surface, same sprint).
- `docs/agent/findings/m10-sha-pin-hallucination.md` — postmortem for the
  hallucinated SHA incident that accompanied the pyo3 drift in v0.1.1.
- Finding `pyo3-0.22-vs-runner-python-drift` — empirical evidence (to be written
  as a formal finding during the implementation sprint; commit messages `03003f3`
  and `acc708c` serve as the interim record).

## When this ADR fires

This ADR moves from `proposed` to `accepted` when the implementation sprint begins.
The sprint owner should create a worktree at `feature/pyo3-0.23-upgrade`, follow
the 6-step migration strategy above, and update this ADR's `status` and
`last_verified_commit` fields in the same atomic commit that closes step 6.
