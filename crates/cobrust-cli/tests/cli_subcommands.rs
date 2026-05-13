//! M10 subcommand smoke tests (per ADR-0024 §"Subcommand contracts").
//!
//! Exercises build / run / check / fmt / new on small inputs.

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

struct TempSource {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

fn write_temp(name: &str, contents: &str) -> TempSource {
    let dir = tempfile::tempdir().expect("create temp source dir");
    let path = dir.path().join(format!("{name}.cb"));
    std::fs::write(&path, contents).expect("write temp .cb");
    TempSource {
        _temp_dir: dir,
        path,
    }
}

#[test]
fn s01_build_returns_zero() {
    // A function-only program builds cleanly to an object file even though
    // the M9 codegen stub does not produce real callable behavior.
    let bin = cobrust_binary();
    let src = write_temp("s01_build_zero", "fn main() -> i64:\n    return 0\n");
    let src = &src.path;
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src)
        .arg("--emit")
        .arg("obj")
        .arg("-o")
        .arg(src.with_extension("o"))
        .arg("--quiet")
        .output()
        .expect("invoke build");
    assert!(
        out.status.success(),
        "build failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(src.with_extension("o").exists());
}

#[test]
fn s02_check_ok() {
    let bin = cobrust_binary();
    let src = write_temp("s02_check_ok", "fn f(x: i64) -> i64:\n    return x + 1\n");
    let src = &src.path;
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src)
        .output()
        .expect("invoke check");
    assert!(
        out.status.success(),
        "check failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ok"), "expected 'ok', got {stdout:?}");
}

#[test]
fn s03_check_type_error_exits_2() {
    let bin = cobrust_binary();
    // Mismatch: declared return is i64, body returns a literal float.
    let src = write_temp("s03_check_type_error", "fn f() -> i64:\n    return 1.5\n");
    let src = &src.path;
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src)
        .output()
        .expect("invoke check");
    assert!(!out.status.success(), "expected failure for type error");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected TYPE_ERROR (2), got {:?}",
        out.status.code()
    );
}

#[test]
fn s04_fmt_check_clean_canonical() {
    // The unparser is its own oracle: the canonical form is whatever
    // `parse → unparse` produces. Round-tripping that twice must be
    // a fixed point.
    let bin = cobrust_binary();
    let src_path = write_temp("s04_fmt_clean", "fn f() -> i64:\n    return 0\n");
    let src_path = &src_path.path;
    // First, fmt rewrites into canonical form.
    let out1 = Command::new(&bin)
        .arg("fmt")
        .arg(&src_path)
        .output()
        .expect("invoke fmt");
    assert!(
        out1.status.success(),
        "fmt rewrite failed: stderr={}",
        String::from_utf8_lossy(&out1.stderr)
    );
    // Second, fmt --check on the canonical form should pass.
    let out2 = Command::new(&bin)
        .arg("fmt")
        .arg("--check")
        .arg(&src_path)
        .output()
        .expect("invoke fmt --check");
    assert!(
        out2.status.success(),
        "fmt --check on canonical form should be clean: stderr={}",
        String::from_utf8_lossy(&out2.stderr)
    );
}

#[test]
fn s05_new_scaffolds_package() {
    let bin = cobrust_binary();
    let dir = tempfile::tempdir().expect("create temp package dir");
    let dir = dir.path();
    let out = Command::new(&bin)
        .arg("new")
        .arg("my_app")
        .arg("--path")
        .arg(&dir)
        .output()
        .expect("invoke new");
    assert!(
        out.status.success(),
        "new failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pkg = dir.join("my_app");
    assert!(pkg.is_dir(), "package dir not created");
    let toml = pkg.join("cobrust.toml");
    assert!(toml.is_file(), "cobrust.toml not created");
    let toml_contents = std::fs::read_to_string(&toml).expect("read toml");
    assert!(
        toml_contents.contains("[package]"),
        "expected [package] table, got {toml_contents}"
    );
    assert!(
        toml_contents.contains("name = \"my_app\""),
        "expected name=\"my_app\", got {toml_contents}"
    );
    let main_cb = pkg.join("src/main.cb");
    assert!(main_cb.is_file(), "src/main.cb not created");
}

#[test]
fn s06_help_lists_subcommands() {
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("--help")
        .output()
        .expect("invoke --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in [
        "build",
        "run",
        "check",
        "fmt",
        "translate",
        "new",
        "test",
        "repl",
    ] {
        assert!(
            stdout.contains(sub),
            "expected '--help' to mention `{sub}`; got {stdout}"
        );
    }
}

#[test]
fn s07_run_hello_world_end_to_end() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let out = Command::new(&bin)
        .arg("run")
        .arg("examples/hello.cb")
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke run");
    assert!(out.status.success(), "run failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello, world"), "got {stdout:?}");
}
