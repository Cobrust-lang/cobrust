//! M11.3 if-condition sibling corpus (ADR-0035 §"Done means" #5).
//!
//! Sibling-of-`while_condition_corpus.rs` — same 12 condition shapes,
//! but exercised in `if` heads instead of `while` heads. Verifies the
//! shared `lower_condition` root primitive (extracted in
//! `cobrust-mir/src/lower.rs` per ADR-0035) does not regress `if`-head
//! behaviour. Pre-M11.3, `lower_if` already used the correct
//! `cond_end_block` pattern; post-M11.3 it routes through the same
//! helper as `lower_loop`'s While arm. Behaviour must remain bit-
//! identical to pre-fix `if`-head output for every shape.

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
#![allow(clippy::single_char_pattern)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::derivable_impls)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn cobrust_binary() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root from CARGO_MANIFEST_DIR");
    let debug_bin = workspace.join("target/debug/cobrust");
    if debug_bin.exists() {
        return debug_bin;
    }
    let release_bin = workspace.join("target/release/cobrust");
    if release_bin.exists() {
        return release_bin;
    }
    PathBuf::from("cobrust")
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

/// F63 (2026-05-27): RAII tempdir.
fn write_temp(name: &str, contents: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir for source");
    let p = dir.path().join(format!("{name}.cb"));
    std::fs::write(&p, contents).expect("write temp .cb");
    (dir, p)
}

fn build(name: &str, src_path: &Path) -> (TempDir, PathBuf) {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let exe_dir = tempfile::tempdir().expect("create tempdir for exe");
    let exe_path = exe_dir.path().join(name);
    let out = Command::new(&bin)
        .arg("build")
        .arg(src_path)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust build");
    assert!(
        out.status.success(),
        "cobrust build failed for {name}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (exe_dir, exe_path)
}

fn run(exe_path: &Path) -> String {
    let out = Command::new(exe_path)
        .output()
        .expect("invoke produced executable");
    assert!(
        out.status.success(),
        "binary {} exited non-zero ({:?})\nstderr={}",
        exe_path.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// =====================================================================
// Case 1 sibling — `if_binop_mod_eq_zero`
// =====================================================================

#[test]
fn if_binop_mod_eq_zero() {
    let (_src_guard, src) = write_temp(
        "if_binop_mod_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 6\n\
         \x20\x20\x20\x20if n % 2 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_mod_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-1 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 2 sibling — `if_binop_mod_ne_zero`
// =====================================================================

#[test]
fn if_binop_mod_ne_zero() {
    let (_src_guard, src) = write_temp(
        "if_binop_mod_ne_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 7\n\
         \x20\x20\x20\x20if n % 2 != 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_mod_ne_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-2 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 3 sibling — `if_binop_add_eq_zero`
// =====================================================================

#[test]
fn if_binop_add_eq_zero() {
    let (_src_guard, src) = write_temp(
        "if_binop_add_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = -3\n\
         \x20\x20\x20\x20let b: i64 = 3\n\
         \x20\x20\x20\x20if a + b == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_add_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-3 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 4 sibling — `if_binop_sub_ne_zero`
// =====================================================================

#[test]
fn if_binop_sub_ne_zero() {
    let (_src_guard, src) = write_temp(
        "if_binop_sub_ne_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 5\n\
         \x20\x20\x20\x20let b: i64 = 3\n\
         \x20\x20\x20\x20if a - b != 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_sub_ne_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-4 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 5 sibling — `if_binop_mul_eq_zero`
// =====================================================================

#[test]
fn if_binop_mul_eq_zero() {
    let (_src_guard, src) = write_temp(
        "if_binop_mul_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 0\n\
         \x20\x20\x20\x20let b: i64 = 7\n\
         \x20\x20\x20\x20if a * b == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_mul_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-5 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 6 sibling — `if_binop_div_eq_zero`
// =====================================================================

#[test]
fn if_binop_div_eq_zero() {
    let (_src_guard, src) = write_temp(
        "if_binop_div_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 1\n\
         \x20\x20\x20\x20let b: i64 = 5\n\
         \x20\x20\x20\x20if a / b == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_div_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-6 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 7 sibling — `if_compare_lt`
// =====================================================================

#[test]
fn if_compare_lt() {
    let (_src_guard, src) = write_temp(
        "if_compare_lt",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 2\n\
         \x20\x20\x20\x20if n < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_compare_lt", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-7 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 8 sibling — `if_compare_eq`
// =====================================================================

#[test]
fn if_compare_eq() {
    let (_src_guard, src) = write_temp(
        "if_compare_eq",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20if n == 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_compare_eq", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-8 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 9 sibling — `if_through_temp`
// =====================================================================

#[test]
fn if_through_temp() {
    let (_src_guard, src) = write_temp(
        "if_through_temp",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 4\n\
         \x20\x20\x20\x20let m: i64 = n % 2\n\
         \x20\x20\x20\x20if m == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_through_temp", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-9 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 10 sibling — `if_nested_binop`
// =====================================================================

#[test]
fn if_nested_binop() {
    let (_src_guard, src) = write_temp(
        "if_nested_binop",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 4\n\
         \x20\x20\x20\x20let b: i64 = 2\n\
         \x20\x20\x20\x20let c: i64 = 3\n\
         \x20\x20\x20\x20if (a + b) % c == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_nested_binop", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-10 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 11 sibling — `if_binop_with_function_call`
// =====================================================================

#[test]
fn if_binop_with_function_call() {
    let (_src_guard, src) = write_temp(
        "if_binop_with_function_call",
        "fn step(x: i64) -> i64:\n\
         \x20\x20\x20\x20return x - 1\n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 3\n\
         \x20\x20\x20\x20if step(n) > 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_binop_with_function_call", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-11 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 12 sibling — `if_condition_through_inferred_locals_chain`
// =====================================================================

#[test]
fn if_condition_through_inferred_locals_chain() {
    let (_src_guard, src) = write_temp(
        "if_condition_through_inferred_locals_chain",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20if -(n - 5) == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"yes\")\n\
         \x20\x20\x20\x20else:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"no\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("if_condition_through_inferred_locals_chain", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "yes\n", "if-12 stdout mismatch: {stdout:?}");
}
