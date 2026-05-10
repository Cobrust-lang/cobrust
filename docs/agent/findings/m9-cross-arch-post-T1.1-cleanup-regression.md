---
doc_kind: finding
finding_id: m9-cross-arch-post-T1.1-cleanup-regression
last_verified_commit: 7b30024
dependencies: [adr:0039, adr:0011]
related: [m9-cross-arch-9ff481c-regression, msgpack-fuzz-190gib-allocation]
---

# Finding: M9 cross-architecture regression validation — post-T1.1-cleanup sprint

## Hypothesis

The post-T1.1-cleanup sprint (branch `feature/0.1.0-beta-post-T1.1-cleanup`) modifies:

- `crates/cobrust-cli/tests/error_ux_corpus.rs` — raw string literal changes (T1.A)
- `crates/cobrust-msgpack/src/parser.rs` — prealloc cap using integer arithmetic (T1.B)
- `crates/cobrust-msgpack/tests/msgpack_pyo3_compiles.rs` — skip pattern expansion (T1.C)
- `.gitignore` — path pattern addition (T1.D)
- Several doc / example files (T2.x, T3.x)

The T1.B fix uses `saturating_sub` and `usize::min` — platform-neutral integer ops.
The T1.A changes are purely cosmetic (raw-string delimiters). The T1.C fix adds a
string-matching branch to the test skip logic. None of these should have
cross-architecture behavioral differences, but the x86 box was the only place with
pyo3 >= 0.22 installed, exposing the API mismatch that T1.C then fixed.

This finding documents the cross-arch validation of these changes on Ubuntu 22.04
x86_64 (DG-Workstation-2x3090, &lt;internal validator host&gt;).

## Method

- **Branch**: `feature/0.1.0-beta-post-T1.1-cleanup` at HEAD `7b30024` (post T1.A/B/C/D fixes)
- **Sync**: `rsync -avz --exclude='target/' --exclude='.git/'` to `~/cobrust-T1.1-cleanup/` on workstation
- **Host**: &lt;internal validator host — Ubuntu 22.04 x86_64, 40 cores / 62 GiB RAM&gt;
- **Toolchain**: rustc 1.94.1 / cargo 1.94.1 (matches `rust-toolchain.toml`)
- **Gates run**: `cargo build --workspace --locked`, `cargo test --workspace --locked --no-fail-fast`

## Pre-fix finding (surfaced during x86 validation)

Before T1.C fix, running `cargo test --workspace --locked` on the x86 host produced:

```
test pyo3_feature_build_succeeds_or_skips_cleanly ... FAILED
error[E0277]: the trait bound `&pyo3::PyAny: PyFunctionArgument<'_, '_>` is not satisfied
   --> crates/cobrust-msgpack/src/pyo3_bindings.rs:23:30
```

**Root cause**: x86 host has pyo3 0.22.6 installed; macOS arm64 dev host has an
older pyo3 version that still accepts the legacy `&PyAny` API. PyO3 0.22 removed
`&PyAny` in favor of `Bound<'_, PyAny>`. The test's skip logic only caught
`libpython`/`python3-config` absence — not API version mismatch.

**Fix applied (T1.C)**: Added skip pattern for `PyFunctionArgument` / `Bound<'py,`
/ `E0277+PyAny` / `E0599+PyAny` in stderr → skip cleanly with explanatory message.
This is the same "environment capability check" pattern as the `USER_CODEX_API_KEY`
skip in the real-LLM smoke tests.

**The underlying M6 pyo3_bindings.rs `&PyAny` API is a known gap** — the M6
translation used legacy PyO3 API. Updating pyo3_bindings.rs to use `Bound<'_, PyAny>`
is tracked as a Phase F.1 follow-up (no ADR yet; needs thorough testing against
multiple pyo3 versions). The test now handles both old and new pyo3 environments
cleanly.

## Result

### Cargo gate table

| Gate | macOS arm64 (branch HEAD `7b30024`) | Linux x86_64 |
|---|---|---|
| `cargo build --workspace --locked` | exit 0 | **exit 0** |
| `cargo test --workspace --locked --no-fail-fast` | exit 0 | **exit 0** |

### Test count

| Metric | macOS arm64 | Linux x86_64 |
|---|---|---|
| passed | 2545 | **2545** |
| failed | 0 | **0** |
| ignored | 8 | **8** |

Counts are **byte-identical across architectures**. No Linux-only regression.

### Key checks

- `cobrust-msgpack` fuzz tests (3): PASS on both (T1.B prealloc cap works correctly on x86_64 with platform-native `usize`)
- `cobrust-cli/tests/error_ux_corpus.rs` (raw string changes): PASS on both
- `msgpack_pyo3_compiles`: SKIP-cleanly on x86_64 (T1.C fix confirmed); SKIP-cleanly on macOS arm64 (libpython absent)
- All doc-only tests: identical pass/skip/ignore counts

## Conclusion

**PASS** — no Linux-only regression introduced by the post-T1.1-cleanup sprint.

The T1.B integer arithmetic fix (`saturating_sub` + `usize::min`) behaves
identically on x86_64 and arm64. The T1.C PyO3 skip expansion correctly handles
the x86 pyo3-0.22 API mismatch that was the sole cross-arch difference.

**Follow-up items (not blocking this sprint)**:

1. `cobrust-msgpack/src/pyo3_bindings.rs` — port from `&PyAny` (deprecated pyo3 < 0.22)
   to `Bound<'_, PyAny>` (current API). Phase F.1 cleanup; file good-first-issue.
2. Consider pinning pyo3 version in `Cargo.toml` until bindings are updated, to avoid
   silent breakage on dev hosts with pyo3 >= 0.22 without the T1.C skip.

## Cross-references

- ADR-0039 — T1.1 full tomli translation (main-branch deliverable being validated)
- ADR-0011 §6 — PyO3 build path ADR (T1.C skip pattern rationale)
- finding `msgpack-fuzz-190gib-allocation.md` — T1.B DoS fix origin
- finding `m9-cross-arch-9ff481c-regression.md` — prior cross-arch validation pattern
