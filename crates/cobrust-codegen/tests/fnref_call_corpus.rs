//! M11.2 user-defined fn call corpus (ADR-0034 §"Done means" #2).
//!
//! Targets the `Constant::FnRef(u32)` Call lowering gap in
//! `cranelift_backend.rs::lower_call`. Before ADR-0034, a user-defined
//! cross-fn call (e.g. `fib(n-1)` calling the same module's `fib`)
//! silently took the M9 stub `iconst(I64, 0); write_place(...)` path —
//! no real Cranelift `call` instruction was emitted, every recursive
//! invocation returned 0, and the verifier eventually rejected the IR
//! whenever the bogus i8/i64 type chain mismatched downstream.
//!
//! Bug shape: MIR's `lower_call` keeps the resolved fn-name local in
//! `Operand::Copy(local)` (per `cobrust-mir/src/lower.rs:1003-1011 + 1178`).
//! The intrinsic-rewrite pass (`cobrust-cli/src/build/intrinsics.rs:143,156`)
//! converts known callsites (polymorphic `print`, ADR-0064) to
//! `Constant::Str(runtime_symbol)` so the M11 codegen Str-callee branch
//! handles them. User-defined fn callees (`fib`, `is_even`, ...) remain
//! `Operand::Constant(Constant::FnRef(def_id))` — and the M9 stub fired
//! through to the I64-zero placeholder.
//!
//! ADR-0034 §"Decision" Option 3 closes this:
//!
//! 1. Per-`CraneliftCtx` `function_ids: HashMap<u32, FuncId>` is already
//!    populated at `declare_body` time (line 379) — i.e. forward
//!    declaration of every body's signature happens BEFORE any body's
//!    `define_body` runs (lines 56-64 in `emit`). So mutual recursion +
//!    self-recursion are already callable; only the per-body
//!    `lower_call` needs to consult `function_ids`.
//!
//! 2. New `lower_call` branch: when `func` is
//!    `Operand::Constant(Constant::FnRef(id))`, look up `function_ids[id]`,
//!    convert to a `FuncRef` for the in-progress builder via
//!    `declare_func_in_func`, lower args, emit `ins().call`, write
//!    return value to destination Place, jump to continuation.
//!
//! ## Cases (10 — all must pass post-fix)
//!
//! - `fnref_single_arg_recursive` — fib(n) = n if n<2 else fib(n-1)+fib(n-2)
//! - `fnref_multi_arg_recursive` — truncated Ackermann (3 levels)
//! - `fnref_zero_arg_recursive` — depth-counter (no args, mutates global-ish)
//! - `fnref_direct_recursion` — fib structural variant (smoke alongside fib)
//! - `fnref_mutual_recursion` — is_even / is_odd cross-call (forward decl gate)
//! - `fnref_chain_call` — a → b → c → leaf (no recursion)
//! - `fnref_inferred_locals_recursive_chain` — recursive fn whose return
//!   passes through a `Ty::None` temp (ADR-0033 + ADR-0034 interaction)
//! - `fnref_no_args_no_return` — `fn side_effect() -> i64` returning const
//! - `fnref_returns_call_of_other` — return another fn's result directly
//! - `fnref_negative_arg` — recurse with `n - 1` arithmetic (Ty::None
//!   operand chain through the `_bin` temp)
//!
//! Each case shells out via `Command::new(<cobrust binary>)`, builds the
//! `.cb` source, runs the produced executable, captures stdout, and
//! asserts byte-for-byte equality with the expected output. Pattern
//! mirrors `crates/cobrust-codegen/tests/while_if_corpus.rs`.

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
    // `CARGO_BIN_EXE_cobrust` is only set when the test runner is the
    // `cobrust-cli` package. From within `cobrust-codegen` we cannot
    // declare `cobrust-cli` as a dev-dependency (circular). Locate the
    // pre-built binary in the workspace target directory; Cargo always
    // builds debug first when running `cargo test --workspace`.
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

/// Write a `.cb` source file to a temp dir; return (guard, path).
/// F63 (2026-05-27): RAII tempdir.
fn write_temp(name: &str, contents: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir for source");
    let p = dir.path().join(format!("{name}.cb"));
    std::fs::write(&p, contents).expect("write temp .cb");
    (dir, p)
}

/// Build the source file with the `cobrust` binary; return (guard,
/// exe_path). Panics with a helpful message on failure.
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

/// Run a produced executable; return its stdout as a String.
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
// fnref_single_arg_recursive — fib(10) = 55 (the canonical case)
// =====================================================================

#[test]
fn fnref_single_arg_recursive() {
    let (_src_guard, src) = write_temp(
        "fnref_single_arg_recursive",
        "fn fib(n: i64) -> i64:\n\
         \x20\x20\x20\x20if n < 2:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return n\n\
         \x20\x20\x20\x20return fib(n - 1) + fib(n - 2)\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(fib(10))\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_single_arg_recursive", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "55\n", "fib(10) stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_multi_arg_recursive — truncated 2-arg recursion (Ackermann shape
//                              capped to avoid stack blow-up)
// =====================================================================

#[test]
fn fnref_multi_arg_recursive() {
    // Truncated Ackermann: ack_t(m, n) returns A(m, n) for small values
    // but caps recursion via the explicit base cases. ack_t(2, 2) = 7.
    let (_src_guard, src) = write_temp(
        "fnref_multi_arg_recursive",
        "fn ack_t(m: i64, n: i64) -> i64:\n\
         \x20\x20\x20\x20if m == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return n + 1\n\
         \x20\x20\x20\x20if n == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return ack_t(m - 1, 1)\n\
         \x20\x20\x20\x20return ack_t(m - 1, ack_t(m, n - 1))\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(ack_t(2, 2))\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_multi_arg_recursive", &src);
    let stdout = run(&exe);
    // A(2, 2) = 7.
    assert_eq!(stdout, "7\n", "ack_t(2,2) stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_zero_arg_recursive — counter() returns a constant via
//                            recursive call chain through depth=3
// =====================================================================

#[test]
fn fnref_zero_arg_recursive() {
    // depth_3() calls depth_2() calls depth_1() calls depth_0() returns
    // 42. This exercises zero-arg fn references at every level.
    let (_src_guard, src) = write_temp(
        "fnref_zero_arg_recursive",
        "fn depth_0() -> i64:\n\
         \x20\x20\x20\x20return 42\n\
         \n\
         fn depth_1() -> i64:\n\
         \x20\x20\x20\x20return depth_0()\n\
         \n\
         fn depth_2() -> i64:\n\
         \x20\x20\x20\x20return depth_1()\n\
         \n\
         fn depth_3() -> i64:\n\
         \x20\x20\x20\x20return depth_2()\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(depth_3())\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_zero_arg_recursive", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "42\n", "depth_3() stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_direct_recursion — fib structural variant (sums up to N)
// =====================================================================

#[test]
fn fnref_direct_recursion() {
    // sum_to(n) = n + sum_to(n-1) with base case sum_to(0) = 0.
    // sum_to(5) = 0+1+2+3+4+5 = 15. Variant of fib's shape: same self-
    // recursion + base case + arithmetic but different recurrence.
    let (_src_guard, src) = write_temp(
        "fnref_direct_recursion",
        "fn sum_to(n: i64) -> i64:\n\
         \x20\x20\x20\x20if n <= 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20return n + sum_to(n - 1)\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(sum_to(5))\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_direct_recursion", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "15\n", "sum_to(5) stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_mutual_recursion — is_even / is_odd cross-call.
//                          MANDATORY: verifies forward declaration
//                          enables BOTH directions even though is_odd
//                          appears textually after is_even.
// =====================================================================

#[test]
fn fnref_mutual_recursion() {
    // is_even(n) calls is_odd(n-1); is_odd(n) calls is_even(n-1). Both
    // base out at n == 0. Without forward declaration, is_even cannot
    // call is_odd because is_odd's FuncId would not yet exist. With
    // ADR-0034 §"Decision" pass-1 declare-then-define, this works.
    let (_src_guard, src) = write_temp(
        "fnref_mutual_recursion",
        "fn is_even(n: i64) -> i64:\n\
         \x20\x20\x20\x20if n == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return 1\n\
         \x20\x20\x20\x20return is_odd(n - 1)\n\
         \n\
         fn is_odd(n: i64) -> i64:\n\
         \x20\x20\x20\x20if n == 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20return is_even(n - 1)\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(is_even(4))\n\
         \x20\x20\x20\x20print(is_odd(4))\n\
         \x20\x20\x20\x20print(is_even(7))\n\
         \x20\x20\x20\x20print(is_odd(7))\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_mutual_recursion", &src);
    let stdout = run(&exe);
    // is_even(4) = 1, is_odd(4) = 0, is_even(7) = 0, is_odd(7) = 1.
    assert_eq!(
        stdout, "1\n0\n0\n1\n",
        "is_even/is_odd output mismatch: {stdout:?}"
    );
}

// =====================================================================
// fnref_chain_call — non-recursive call chain a → b → c → leaf.
//                    Tests forward declaration without the recursion
//                    surface.
// =====================================================================

#[test]
fn fnref_chain_call() {
    // leaf returns 1; c calls leaf; b calls c; a calls b; main calls a.
    // No recursion, but every fn references the next via FnRef. Ensures
    // the fix doesn't break linear chains.
    let (_src_guard, src) = write_temp(
        "fnref_chain_call",
        "fn leaf() -> i64:\n\
         \x20\x20\x20\x20return 1\n\
         \n\
         fn c() -> i64:\n\
         \x20\x20\x20\x20return leaf() + 10\n\
         \n\
         fn b() -> i64:\n\
         \x20\x20\x20\x20return c() + 100\n\
         \n\
         fn a() -> i64:\n\
         \x20\x20\x20\x20return b() + 1000\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(a())\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_chain_call", &src);
    let stdout = run(&exe);
    // 1 + 10 + 100 + 1000 = 1111.
    assert_eq!(stdout, "1111\n", "chain stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_inferred_locals_recursive_chain — MANDATORY ADR-0033 + ADR-0034
//   interaction regression guard. Recursive fn whose return value
//   passes through a `Ty::None`-declared `_bin` temp before the final
//   return. The fix must not regress ADR-0033's fixed-point inference.
// =====================================================================

#[test]
fn fnref_inferred_locals_recursive_chain() {
    // double_then_recurse(n): if n <= 0 return 0; return (2 * n) +
    // double_then_recurse(n - 1). The intermediate `(2 * n)` lowers
    // to a `_bin` temp typed `Ty::None`; the call's return passes
    // through `_callret` typed `Ty::None`; their sum lowers to
    // another `_bin` temp typed `Ty::None`. The chain depth stresses
    // ADR-0033's fixed-point.
    //
    // Expected: dbl(3) = 6 + 4 + 2 + 0 = 12.
    let (_src_guard, src) = write_temp(
        "fnref_inferred_locals_recursive_chain",
        "fn dbl_rec(n: i64) -> i64:\n\
         \x20\x20\x20\x20if n <= 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20return (2 * n) + dbl_rec(n - 1)\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(dbl_rec(3))\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_inferred_locals_recursive_chain", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "12\n", "dbl_rec(3) stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_no_args_no_return — fn returning a constant via FnRef call.
//                            Tests the no-arg dispatch path; Cobrust's
//                            top-level callable convention requires an
//                            explicit `-> i64` return type even for
//                            "side-effect" fns.
// =====================================================================

#[test]
fn fnref_no_args_no_return() {
    // side_effect prints a literal then returns 0. main calls it twice.
    // Verifies that a void-shaped (constant-i64-return) fn is callable
    // through FnRef.
    let (_src_guard, src) = write_temp(
        "fnref_no_args_no_return",
        "fn side_effect() -> i64:\n\
         \x20\x20\x20\x20print(\"side\")\n\
         \x20\x20\x20\x20return 0\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(side_effect())\n\
         \x20\x20\x20\x20print(side_effect())\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_no_args_no_return", &src);
    let stdout = run(&exe);
    // Two side-effect prints + two zero returns.
    assert_eq!(
        stdout, "side\n0\nside\n0\n",
        "side_effect stdout mismatch: {stdout:?}"
    );
}

// =====================================================================
// fnref_returns_call_of_other — return another fn's result directly,
//                                no intermediate temp binding. Stresses
//                                the MIR _callret-to-_0 short path.
// =====================================================================

#[test]
fn fnref_returns_call_of_other() {
    // produces_seven returns 7 directly; relay returns
    // produces_seven() unchanged. main prints relay().
    let (_src_guard, src) = write_temp(
        "fnref_returns_call_of_other",
        "fn produces_seven() -> i64:\n\
         \x20\x20\x20\x20return 7\n\
         \n\
         fn relay() -> i64:\n\
         \x20\x20\x20\x20return produces_seven()\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20print(relay())\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_returns_call_of_other", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "7\n", "relay stdout mismatch: {stdout:?}");
}

// =====================================================================
// fnref_negative_arg — recurse with `n - 1`-style arithmetic argument.
//                       Explicitly exercises the operand chain through
//                       a `_bin` Ty::None temp before the FnRef call.
// =====================================================================

#[test]
fn fnref_negative_arg() {
    // countdown(n) prints n, then recurses with countdown(n - 1) until
    // n <= 0. The `n - 1` argument lowers to a `_bin` temp typed
    // `Ty::None` whose actual value is i64. The new lower_call path
    // must read that value with the right type via inferred_locals.
    let (_src_guard, src) = write_temp(
        "fnref_negative_arg",
        "fn countdown(n: i64) -> i64:\n\
         \x20\x20\x20\x20if n <= 0:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20print(n)\n\
         \x20\x20\x20\x20return countdown(n - 1)\n\
         \n\
         fn main() -> i64:\n\
         \x20\x20\x20\x20let result: i64 = countdown(3)\n\
         \x20\x20\x20\x20print(result)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let (_exe_guard, exe) = build("fnref_negative_arg", &src);
    let stdout = run(&exe);
    // 3, 2, 1 printed during recursion; final return value 0 printed
    // by main.
    assert_eq!(
        stdout, "3\n2\n1\n0\n",
        "countdown(3) stdout mismatch: {stdout:?}"
    );
}
