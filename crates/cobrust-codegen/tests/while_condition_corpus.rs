//! M11.3 while-condition corpus (ADR-0035 §"Done means" #4).
//!
//! Twelve `while`-head condition cases that exercise the shared
//! `lower_condition` root primitive (extracted from `cobrust-mir/src/lower.rs`
//! per ADR-0035). Before the fix, `while <BinOp> == 0` (and similar
//! non-trivial-LHS shapes) miscompiled — the SwitchInt was emitted in the
//! while header while the cond's final assigns lived in a downstream
//! divassert-target block, so each iteration read a stale zero value and
//! the body never entered. See
//! `docs/agent/findings/while-binop-eq-zero-condition-miscompile.md`.
//!
//! Each case shells out to the `cobrust` binary, builds + runs the program,
//! captures stdout, and asserts equality (cmp-bit-identical). Sibling
//! `if`-head cases live in `if_condition_corpus.rs` and exercise the same
//! shared primitive's behaviour in `if` heads to verify no regression of
//! the M11.1 `if`-codegen acceptance gate.

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

fn write_temp(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-m11-3-while-cond-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{name}.cb"));
    std::fs::write(&p, contents).expect("write temp .cb");
    p
}

fn build(name: &str, src_path: &Path) -> PathBuf {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-m11-3-while-cond-exe-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&exe_dir);
    let exe_path = exe_dir.join(name);
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
    exe_path
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
// Case 1 — `while_binop_mod_eq_zero` (the LC 263 trigger)
//
// `while n % 2 == 0` — non-trivial BinOp on LHS of `==`. Pre-M11.3 the
// while header emitted SwitchInt(_bin) while `_bin` was assigned in a
// downstream div-assert successor, so the body never entered. With the
// shared `lower_condition` primitive, both heads route through the same
// pattern: terminate `cond_end_block`, NOT the original starting block.
// =====================================================================

#[test]
fn while_binop_mod_eq_zero() {
    let src = write_temp(
        "while_binop_mod_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 6\n\
         \x20\x20\x20\x20while n % 2 == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = 9999\n\
         \x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_mod_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "loop\n9999\n", "case 1 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 2 — `while_binop_mod_ne_zero`
//
// `while n % 2 != 0` — same shape with `!=` instead of `==`. Verifies
// the primitive doesn't accidentally collapse `!=` into a different
// operand chain.
// =====================================================================

#[test]
fn while_binop_mod_ne_zero() {
    let src = write_temp(
        "while_binop_mod_ne_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 7\n\
         \x20\x20\x20\x20while n % 2 != 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = 8\n\
         \x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_mod_ne_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "loop\n8\n", "case 2 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 3 — `while_binop_add_eq_zero`
//
// `while a + b == 0` — non-modulo BinOp on LHS. No div-assert involved,
// so the trigger is not specifically the assert chain — it's any
// non-trivial BinOp wrapped in a `==`/`!=` comparator. Pre-fix, this
// shape was equally broken because `lower_expr` returns to the wrong
// block when the BinOp's eval emits any chaining.
// =====================================================================

#[test]
fn while_binop_add_eq_zero() {
    let src = write_temp(
        "while_binop_add_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = -3\n\
         \x20\x20\x20\x20let b: i64 = 3\n\
         \x20\x20\x20\x20while a + b == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20a = 1\n\
         \x20\x20\x20\x20print(a)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_add_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "loop\n1\n", "case 3 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 4 — `while_binop_sub_ne_zero`
//
// `while a - b != 0` — sub instead of add, `!=` instead of `==`.
// =====================================================================

#[test]
fn while_binop_sub_ne_zero() {
    let src = write_temp(
        "while_binop_sub_ne_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 5\n\
         \x20\x20\x20\x20let b: i64 = 3\n\
         \x20\x20\x20\x20while a - b != 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20a = 3\n\
         \x20\x20\x20\x20print(a)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_sub_ne_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "loop\n3\n", "case 4 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 5 — `while_binop_mul_eq_zero`
//
// `while a * b == 0` — multiply BinOp.
// =====================================================================

#[test]
fn while_binop_mul_eq_zero() {
    let src = write_temp(
        "while_binop_mul_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 0\n\
         \x20\x20\x20\x20let b: i64 = 7\n\
         \x20\x20\x20\x20while a * b == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20a = 1\n\
         \x20\x20\x20\x20print(a)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_mul_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "loop\n1\n", "case 5 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 6 — `while_binop_div_eq_zero`
//
// `while a / b == 0` — div BinOp; exercises div-assert chain in the cond
// itself (`b != 0` assert before computing the div) plus the outer
// `<BinOp> == 0` shape.
// =====================================================================

#[test]
fn while_binop_div_eq_zero() {
    let src = write_temp(
        "while_binop_div_eq_zero",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 1\n\
         \x20\x20\x20\x20let b: i64 = 5\n\
         \x20\x20\x20\x20while a / b == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20a = 100\n\
         \x20\x20\x20\x20print(a)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_div_eq_zero", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "loop\n100\n", "case 6 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 7 — `while_compare_lt`
//
// `while n < 10` — happy path that worked pre-M11.3. Acts as a regression
// guard: the shared primitive must not regress simple-comparator while
// heads.
// =====================================================================

#[test]
fn while_compare_lt() {
    let src = write_temp(
        "while_compare_lt",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 0\n\
         \x20\x20\x20\x20while n < 3:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"x\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n + 1\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_compare_lt", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "x\nx\nx\n", "case 7 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 8 — `while_compare_eq`
//
// `while n == 5` — direct `==` of leaf locals (no BinOp on LHS). The
// LHS `n` is a Place, not a BinOp, so the cond chain has no auxiliary
// blocks. Must continue working post-fix.
// =====================================================================

#[test]
fn while_compare_eq() {
    let src = write_temp(
        "while_compare_eq",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20while n == 5:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"five\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = 0\n\
         \x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_compare_eq", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "five\n0\n", "case 8 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 9 — `while_through_temp` (probe 1 workaround from the finding)
//
// `let m = n % 2; while m == 0:` — explicit pre-computation into a
// named local. The finding's probe matrix uses this as the workaround
// that succeeds pre-fix; it must continue to succeed post-fix to confirm
// the primitive does not regress simple-leaf cond chains.
// =====================================================================

#[test]
fn while_through_temp() {
    let src = write_temp(
        "while_through_temp",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 4\n\
         \x20\x20\x20\x20let m: i64 = n % 2\n\
         \x20\x20\x20\x20while m == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"step\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20m = 1\n\
         \x20\x20\x20\x20print(\"done\")\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_through_temp", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "step\ndone\n", "case 9 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 10 — `while_nested_binop`
//
// `while (a + b) % c == 0:` — nested BinOp inside the LHS of `==`,
// adding chain depth >= 2 inside the cond. Verifies the primitive
// survives multi-step chains (orthogonal to ADR-0033's
// `inferred_locals` fixed-point — that handles the operand_ty side;
// this case checks the block-flow side).
// =====================================================================

#[test]
fn while_nested_binop() {
    let src = write_temp(
        "while_nested_binop",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: i64 = 4\n\
         \x20\x20\x20\x20let b: i64 = 2\n\
         \x20\x20\x20\x20let c: i64 = 3\n\
         \x20\x20\x20\x20while (a + b) % c == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"hit\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20a = 5\n\
         \x20\x20\x20\x20print(a)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_nested_binop", &src);
    let stdout = run(&exe);
    // First iter: (4+2)%3 = 0 → enter; a=5; back to header.
    // Second iter: (5+2)%3 = 7%3 = 1 → exit.
    assert_eq!(stdout, "hit\n5\n", "case 10 stdout mismatch: {stdout:?}");
}

// =====================================================================
// Case 11 — `while_binop_with_function_call` (ADR-0034 interaction)
//
// `while fact(n) == 0:` — the LHS is a user-fn call. Pre-M11.2
// (ADR-0034) user-fn calls didn't lower; post-M11.2 they do, but the
// call lowers via a separate Terminator::Call which itself emits a
// successor block. The shared `lower_condition` primitive must
// correctly capture `cond_end_block` after the call returns.
// =====================================================================

#[test]
fn while_binop_with_function_call() {
    let src = write_temp(
        "while_binop_with_function_call",
        "fn step(x: i64) -> i64:\n\
         \x20\x20\x20\x20return x - 1\n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 3\n\
         \x20\x20\x20\x20while step(n) > 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"tick\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = n - 1\n\
         \x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_binop_with_function_call", &src);
    let stdout = run(&exe);
    // step(3)=2>0 ✓ tick, n=2; step(2)=1>0 ✓ tick, n=1; step(1)=0 → exit.
    assert_eq!(
        stdout, "tick\ntick\n1\n",
        "case 11 stdout mismatch: {stdout:?}"
    );
}

// =====================================================================
// Case 12 — `while_condition_through_inferred_locals_chain`
// (ADR-0033 interaction)
//
// `while -(n - 5) == 0:` — UnaryOp wrapped around a BinOp inside `==`.
// Both `_bin` and `_un` synthetic temps carry `Ty::None` declared types;
// ADR-0033's `inferred_locals` fixed-point resolves both to I64. M11.3's
// `lower_condition` must work in concert with that resolution: the cond
// chain emits {_bin = Sub, _un = Neg, _eq = Eq}, all in
// `cond_end_block`. SwitchInt reads `_eq`, which the inferred_locals
// pass typed as I8 (correct for an Eq result).
// =====================================================================

#[test]
fn while_condition_through_inferred_locals_chain() {
    let src = write_temp(
        "while_condition_through_inferred_locals_chain",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let n: i64 = 5\n\
         \x20\x20\x20\x20while -(n - 5) == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20print(\"loop\")\n\
         \x20\x20\x20\x20\x20\x20\x20\x20n = 99\n\
         \x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("while_condition_through_inferred_locals_chain", &src);
    let stdout = run(&exe);
    // -(5-5)=0 → enter, n=99; -(99-5) = -94 ≠ 0 → exit.
    assert_eq!(stdout, "loop\n99\n", "case 12 stdout mismatch: {stdout:?}");
}
