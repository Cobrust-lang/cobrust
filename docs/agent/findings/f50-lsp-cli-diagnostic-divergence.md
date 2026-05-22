---
name: f50
status: RATIFIED
family: F-language
date: 2026-05-22
last_verified_commit: 07159ce
---

# F50 — `cobrust-lsp` / `cobrust check` diagnostic divergence on PRELUDE intrinsics

## §1 Context

Cobrust v0.6.0 + ADR-0068 §4.1 unified LSP entry point (`cobrust lsp` subcommand
and transitional `cobrust-lsp` shim binary). User reported on 2026-05-22 17:00:

- `cobrust check examples/fib.cb` exits `0` with stdout `ok`.
- `cobrust-lsp` (v0.6.0) emits `textDocument/publishDiagnostics` for the same file
  carrying `code: "lower-unknown-name", message: "unknown name 'print' at file#1@340..345"`.
- Cursor renders red squiggles on every `print(...)` call in every `.cb` file.
- The mismatch covers every PRELUDE intrinsic (`print`, `range`, `parse_int`,
  `list_*`, `str_*`, math, IO, LLM, `argv`, `input`, `input_no_prompt`, ...).

This is a F-language family bug (F47-sibling) and F35-sibling discipline violation
(the v0.6.0 release tagged `cobrust-lsp` as "full diagnostic parity with CLI"; the
landed reality diverged on every PRELUDE name).

## §2 Root cause

`crates/cobrust-cli/src/check.rs:36` prepends the synthetic PRELUDE source to user
input before invoking the frontend:

```rust
let source = format!("{}{user_source}", crate::build::PRELUDE);
let module = parse_str(&source, FileId::SYNTHETIC) // ...
```

The same prepend lives in `crates/cobrust-cli/src/build.rs:124` and `:570` (build
+ JIT paths). The PRELUDE declares signatures for every intrinsic visible to user
code; without it `print` / `range` / etc. resolve as undefined names in HIR lowering
and emit `LoweringError::UnknownName`.

The `cobrust-lsp` `Backend::compile_diagnostics` and `Backend::compile_diagnostics_with_session`
functions (`crates/cobrust-lsp/src/lib.rs:316`, `:361` pre-fix) called `parse_str(source, ...)`
directly without prepending PRELUDE. Every PRELUDE intrinsic surfaced as `lower-unknown-name`.

The completion-side prelude name table (`cobrust-lsp/src/completion.rs::prelude_items`)
existed and was correct; it was scoped to autocomplete and never reached the
diagnostic path. The two surfaces shipped from independent name tables.

## §3 Empirical divergence (sweep, 2026-05-22)

Full corpus sweep across `examples/**/*.cb` via
`cargo test -p cobrust-lsp --test diagnostic_parity_sweep -- --ignored`.
Report saved to `/tmp/lsp_sweep_report.md`.

| Phase | Total `.cb` files | LSP-diagnostic-emitting | Code histogram |
|---|---|---|---|
| Pre-fix | 144 | 144 (100%) | `lower-unknown-name`: 144 |
| Post-fix | 144 | 0 (0%) | (empty) |

Top names misclassified pre-fix (by frequency):
- `print` — every fixture using stdout
- `print_no_nl` — every fixture using non-newline stdout
- `parse_int` / `input` / `argv` — every leetcode-stress fixture
- `range` / `list_get` / `list_set` — every for-range / list-mutation fixture

CLI exit code on every fixture: `ok` (exit 0). LSP diagnostic count on every
fixture pre-fix: ≥1.

## §4 Detection rule

Added `crates/cobrust-lsp/tests/diagnostic_parity_smoke.rs` —
`lsp_emits_zero_diagnostics_for_well_typed_examples`. Runs on every `cargo test
-p cobrust-lsp` invocation; pins a curated 5-fixture subset (`hello`, `fib`,
`for_range`, `fizzbuzz`, `early_exit`). Any future PRELUDE intrinsic added to
`cobrust-frontend::PRELUDE` that fails to round-trip through the LSP pipeline
will fail this smoke test.

Companion `tests/diagnostic_parity_sweep.rs::lsp_cli_diagnostic_parity_sweep`
walks the entire `examples/**/*.cb` corpus and emits a markdown report to
`/tmp/lsp_sweep_report.md`. Gated with `#[ignore]` so the default unit run
stays fast; explicit `cargo test ... -- --ignored` produces the table.

These tests form the F50 CI gate against regression.

## §5 Resolution

Three atomic commits land the fix:

1. **Move PRELUDE to a shared location.** New module
   `crates/cobrust-frontend/src/prelude.rs` exposes
   `PRELUDE: &str`, `PRELUDE_BYTE_LEN: u32`, `PRELUDE_LINE_COUNT: u32` (the
   last two computed at compile-time via `const fn` so they can never drift
   from the literal). `crates/cobrust-cli/src/build.rs` re-exports
   `pub use cobrust_frontend::PRELUDE` so existing `crate::build::PRELUDE`
   call sites remain stable.

2. **Wire LSP to prepend PRELUDE.** `Backend::compile_diagnostics*` build
   `composed = format!("{PRELUDE}{user_source}")`, parse + lower + type-check
   against the composed source using a composed-source `LineMap`, then post-
   shift each emitted `Diagnostic.range.{start,end}.line` by `-PRELUDE_LINE_COUNT`
   and post-shift the embedded `file#K@N..M` byte offsets in each
   `Diagnostic.message` by `-PRELUDE_BYTE_LEN`. Diagnostics whose final
   `start.line` would underflow (i.e., the span lay inside the PRELUDE prefix)
   are filtered as a defensive measure.

3. **Regression tests.** Smoke + sweep harnesses added per §4.

`crates/cobrust-lsp/src/completion.rs::prelude_items` (the completion-side
name table) was already correct; no change required there.

## §6 Cross-references

- **ADR-0024** — Hello-world contract: defines `print` as the M10 intrinsic.
- **ADR-0050b** — `range(start, stop) -> list[i64]` as a real Cobrust fn body.
- **ADR-0057** — LSP framework; `cobrust-lsp` capabilities.
- **ADR-0057a** — `textDocument/publishDiagnostics` wire mapping.
- **ADR-0057b** — `textDocument/didChange` incremental + Session reuse.
- **ADR-0057c** — completion + hover (the prelude completion table that
  existed correctly but did not feed the diagnostic path).
- **ADR-0064** — `print_int` removed from source-face PRELUDE.
- **ADR-0068** — unified LSP entry point (`cobrust lsp` subcommand).
- **F35-sibling discipline** — claim vs landed reality (v0.6.0 advertised
  diagnostic parity; actual coverage diverged on PRELUDE names).
- **F47** — F-string user-fn `str` interp empty (same-day reporter, same F-language family).

## §7 Files touched

- `crates/cobrust-frontend/src/prelude.rs` (new; PRELUDE + constants + tests)
- `crates/cobrust-frontend/src/lib.rs` (module export)
- `crates/cobrust-cli/src/build.rs` (re-export from frontend)
- `crates/cobrust-lsp/src/lib.rs` (compose source, shift diagnostics, helper + tests)
- `crates/cobrust-lsp/tests/diagnostic_parity_smoke.rs` (new; F50 regression gate)
- `crates/cobrust-lsp/tests/diagnostic_parity_sweep.rs` (new; full-corpus sweep)
- `docs/agent/findings/f50-lsp-cli-diagnostic-divergence.md` (this file)

## §8 User-facing repro post-fix

After reinstalling `cobrust-lsp-shim` from `main` HEAD:

- Opening `examples/fib.cb` in Cursor: zero red squiggles. `print(...)` resolves.
- Opening `examples/for_range.cb`: zero squiggles. `range(...)` + `list_get`/`list_set`
  resolve.
- Opening a file with a real type error (e.g. `let x: i64 = "hello"`): one red
  squiggle on the assignment, code `type-mismatch`, line/character in user
  coordinates (not composed coordinates).
