#![allow(
    clippy::format_push_string,
    clippy::manual_let_else,
    reason = "test corpus-builder style; readability over micro-optim"
)]
//! ADR-0057a §3 + F50 — full-corpus sweep of LSP / CLI diagnostic
//! parity across every `.cb` fixture in `examples/`.
//!
//! Companion to `diagnostic_parity_smoke.rs`. The smoke test pins a
//! curated 5-fixture subset; this sweep walks the entire `examples/`
//! tree and reports a per-file table. Marked `#[ignore]` by default so
//! the LSP unit run stays fast (~30s); invoke explicitly with:
//!
//! ```bash
//! cargo test -p cobrust-lsp --test diagnostic_parity_sweep -- --ignored --nocapture
//! ```
//!
//! Emits a markdown-style report to stdout and to `/tmp/lsp_sweep_report.md`
//! so the F50 finding can cite the exhaustive divergence baseline.

use std::path::{Path, PathBuf};

use cobrust_lsp::{Backend, LineMap};

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

fn walk_cb_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_cb_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("cb") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "full-corpus sweep; explicit invocation only"]
fn lsp_cli_diagnostic_parity_sweep() {
    let root = workspace_root();
    let examples_dir = root.join("examples");
    let mut files: Vec<PathBuf> = Vec::new();
    walk_cb_files(&examples_dir, &mut files);
    files.sort();

    let mut report = String::new();
    report.push_str("# LSP / CLI diagnostic parity sweep\n\n");
    report.push_str(&format!(
        "Total `.cb` files under `examples/`: {}\n\n",
        files.len()
    ));
    report.push_str("| File | LSP diag count | First code | First message |\n");
    report.push_str("|---|---|---|---|\n");

    let mut total = 0usize;
    let mut divergent = 0usize;
    let mut code_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    for path in &files {
        let rel = path.strip_prefix(&root).unwrap_or(path);
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let line_map = LineMap::from_source(&source);
        let diags = Backend::compile_diagnostics(&source, &line_map);
        total += 1;
        let (code, msg) = if let Some(d) = diags.first() {
            let code = match &d.code {
                Some(tower_lsp::lsp_types::NumberOrString::String(s)) => s.clone(),
                _ => "<no-code>".to_string(),
            };
            *code_counts.entry(code.clone()).or_default() += 1;
            (code, d.message.clone())
        } else {
            ("<ok>".to_string(), String::new())
        };
        if !diags.is_empty() {
            divergent += 1;
        }
        report.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            rel.display(),
            diags.len(),
            code,
            msg.chars().take(80).collect::<String>().replace('|', "\\|"),
        ));
    }

    report.push_str(&format!(
        "\nTotal: {total}, LSP-diagnostic-emitting: {divergent}\n"
    ));
    report.push_str("\n## Diagnostic code histogram\n\n");
    for (code, count) in &code_counts {
        report.push_str(&format!("- `{code}`: {count}\n"));
    }

    let out_path = std::path::Path::new("/tmp/lsp_sweep_report.md");
    let _ = std::fs::write(out_path, &report);
    println!("{report}");
    println!("(report also written to {})", out_path.display());
}
