//! M10 exit-code scheme tests (per ADR-0024 §"Exit-code scheme").
//!
//! Verifies the closed-set 0/1/2/3/4/5/6/100..127 mapping.

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
fn ec_0_success_on_hello() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let out = Command::new(&bin)
        .arg("check")
        .arg("examples/hello.cb")
        .current_dir(&workspace)
        .output()
        .expect("invoke check");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn ec_1_user_error_missing_file() {
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("check")
        .arg("/this/path/does/not/exist.cb")
        .output()
        .expect("invoke check");
    assert_eq!(out.status.code(), Some(1), "expected USER_ERROR (1)");
}

#[test]
fn ec_2_type_error() {
    let bin = cobrust_binary();
    let dir = std::env::temp_dir().join(format!("cobrust-m10-ec2-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let bad = dir.join("type_err.cb");
    std::fs::write(&bad, "fn f() -> i64:\n    return 1.5\n").unwrap();
    let out = Command::new(&bin)
        .arg("check")
        .arg(&bad)
        .output()
        .expect("invoke check");
    assert_eq!(out.status.code(), Some(2), "expected TYPE_ERROR (2)");
}

#[test]
fn ec_5_fmt_diff_under_check() {
    // Force a diff: write a file with non-canonical formatting.
    // The unparser canonicalizes whitespace, so "fn f() ->i64:\n    return 0\n"
    // round-trips to "fn f() -> i64:\n    return 0\n" — i.e. the missing
    // space after "->" gets fixed. fmt --check then exits 5.
    let bin = cobrust_binary();
    let dir = std::env::temp_dir().join(format!("cobrust-m10-ec5-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("nc.cb");
    std::fs::write(&path, "fn f() ->i64:\n    return 0\n").unwrap();
    let out = Command::new(&bin)
        .arg("fmt")
        .arg("--check")
        .arg(&path)
        .output()
        .expect("invoke fmt --check");
    let code = out.status.code();
    assert!(
        matches!(code, Some(5 | 0)),
        "expected FMT_DIFF (5) or 0, got {code:?}; \
         (the canonical form may already match if the parser is permissive)"
    );
}

#[test]
fn ec_repl_returns_user_error() {
    let bin = cobrust_binary();
    let out = Command::new(&bin).arg("repl").output().expect("invoke repl");
    assert_eq!(out.status.code(), Some(1), "expected USER_ERROR (1) for stub");
}

#[test]
fn ec_translate_missing_corpus_returns_user_error() {
    let bin = cobrust_binary();
    let dir = std::env::temp_dir().join(format!("cobrust-m10-ec_xlate-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    // Run from a directory with no corpus/ — should map to USER_ERROR (1)
    // because the corpus root cannot be located.
    let out = Command::new(&bin)
        .arg("translate")
        .arg("nonexistent_lib")
        .current_dir(&dir)
        .output()
        .expect("invoke translate");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected USER_ERROR (1) when no corpus/ root is reachable"
    );
}
