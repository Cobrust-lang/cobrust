//! ADR-0057a §3 + F50 — LSP / CLI diagnostic parity smoke harness.
//!
//! Per finding `f50-lsp-cli-diagnostic-divergence.md` (2026-05-22): the
//! `cobrust check` CLI prepends a synthetic PRELUDE source (declaring
//! `print`, `range`, `parse_int`, ...) before invoking the frontend,
//! while the LSP `Backend::compile_diagnostics*` paths fed user source
//! directly. As a result, every `.cb` file in `examples/` that called
//! a PRELUDE intrinsic (`print(...)`, `range(...)`, ...) lit up red in
//! Cursor while `cobrust check` returned `ok`.
//!
//! This test pins the parity. For each fixture file:
//!   - CLI path: parse + HIR-lower + type-check with PRELUDE prepended
//!     (matching `crates/cobrust-cli/src/check.rs:36`).
//!   - LSP path: `Backend::compile_diagnostics(source, &line_map)`.
//!
//! Assertion: the LSP diagnostic count for every file must be `0` (the
//! CLI returns `ok` for all fixtures listed). If a future fixture is
//! intentionally ill-typed, gate it on a separate `ILL_TYPED_FIXTURES`
//! list with explicit expected-error codes.
//!
//! Wave-1 enforces a hard subset of representative `.cb` examples;
//! Wave-2 extends to the full `examples/**/*.cb` corpus.

use std::path::PathBuf;

use cobrust_lsp::{Backend, LineMap};

/// Curated fixture subset covering the highest-frequency PRELUDE intrinsics:
/// `print` (every fixture), `range` (`for_range`), arithmetic / cmp
/// (`fib`, `fizzbuzz`), early-return (`early_exit`).
const PARITY_FIXTURES: &[&str] = &[
    "examples/hello.cb",
    "examples/fib.cb",
    "examples/for_range.cb",
    "examples/fizzbuzz.cb",
    "examples/early_exit.cb",
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at `crates/cobrust-lsp/`; the workspace
    // root is two levels up.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

#[test]
fn lsp_emits_zero_diagnostics_for_well_typed_examples() {
    let root = workspace_root();
    let mut failures: Vec<String> = Vec::new();

    for rel in PARITY_FIXTURES {
        let path = root.join(rel);
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{rel}: cannot read fixture: {e}"));
                continue;
            }
        };
        let line_map = LineMap::from_source(&source);
        let diags = Backend::compile_diagnostics(&source, &line_map);
        if !diags.is_empty() {
            let codes: Vec<String> = diags
                .iter()
                .filter_map(|d| match &d.code {
                    Some(tower_lsp::lsp_types::NumberOrString::String(s)) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            let messages: Vec<String> = diags.iter().map(|d| d.message.clone()).collect();
            failures.push(format!(
                "{rel}: LSP emitted {} diagnostic(s) but CLI reports ok\n  codes: {:?}\n  msgs: {:?}",
                diags.len(),
                codes,
                messages,
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "LSP / CLI diagnostic parity broken for {} fixture(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
