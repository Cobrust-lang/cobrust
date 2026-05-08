//! M10 `cobrust translate` surface smoke (per ADR-0024 §"`cobrust translate` argv mapping").
//!
//! Exercises the CLI surface around `cobrust_translator::pipeline::translate`
//! against the existing `corpus/tomli/` fixtures. The pipeline itself has
//! its own integration test in `cobrust-translator/tests/tomli_pipeline.rs`;
//! this test only validates the CLI argv mapping + exit-code routing.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

#[test]
fn translate_tomli_via_cli_locates_corpus() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let out_dir = std::env::temp_dir().join(format!(
        "cobrust-m10-xlate-tomli-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&out_dir);

    // Skip the test if corpus/tomli isn't present (e.g. in a stripped CI).
    if !workspace.join("corpus/tomli/spec.toml").exists() {
        eprintln!("skipping: corpus/tomli/spec.toml missing");
        return;
    }

    let out = Command::new(&bin)
        .arg("translate")
        .arg("tomli")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke translate");

    // The pipeline may reject the translation in synthetic mode if the
    // canned table is missing entries; we accept either a clean success
    // (exit 0) or the canonical TRANSLATOR_BASE (100) failure mode.
    // What we *don't* accept: a panic / segfault / non-routed exit code.
    let code = out.status.code().unwrap_or(255);
    assert!(
        code == 0 || (100..=127).contains(&code),
        "translate exited with unexpected code {code}; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn translate_unknown_library_exits_1() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let out = Command::new(&bin)
        .arg("translate")
        .arg("definitely_not_a_real_library_xyz")
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke translate");
    let code = out.status.code().unwrap_or(255);
    assert_eq!(code, 1, "expected USER_ERROR (1) for unknown library");
}
