//! ADR-0073 second proof — end-to-end `.cb` source → compile → link →
//! run → stdout-assert for the `hood` ecosystem-import wiring (click,
//! CLI commands — the SEVENTH ecosystem module, second to exercise the
//! `.cb`↔Rust callback marshalling chain after pit).
//!
//! Generalizes the proven flat-intrinsic chain to a second callback
//! shape: the `.cb` source defines a top-level `fn handle_greet() -> i64:`
//! whose pointer is materialised at codegen via `Constant::FnRef`,
//! crosses the C ABI as a raw fn pointer, and is invoked from Rust
//! through the trampoline in `cobrust-hood/src/cabi.rs`. The handler
//! prints via stdout; the test captures stdout via `std::process::Command`
//! + asserts the printed line + exit code.
//!
//! ```text
//! `import hood` + `hood.Command(name, help)` + `cmd.handler(handle_greet)` +
//! `cmd.run()` + a top-level `fn handle_greet() -> i64:`
//!   → cobrust-types ecosystem manifest (typecheck, EcoParam::Callback)
//!   → cobrust-mir lowering (Constant::FnRef for the callback arg)
//!   → cobrust-codegen fn-pointer materialisation via function_ids
//!   → cobrust-hood C-ABI shims (libhood.a) + trampoline closure
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → handler prints to stdout + returns i64 0
//!   → test asserts stdout == "hello from hood\n" + exit 0
//! ```
//!
//! Pattern (mirrors `ecosystem_strike_e2e.rs` + `pit_pong_e2e.rs`,
//! minus the network harness — hood is local-only): compile a `.cb`
//! program to an exe, run it with `std::process::Command`, assert
//! stdout + exit.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable and return its path. The
/// caller spawns + asserts.
fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

/// Compile the `.cb` "greet" program, run it, assert stdout +
/// exit code. The example mirrors `examples/hood_cmd/main.cb`.
#[test]
fn test_e2e_hood_cmd_handler_round_trip() {
    let source = concat!(
        "import hood\n",
        "\n",
        "fn handle_greet() -> i64:\n",
        "    print(\"hello from hood\")\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let cmd = hood.Command(\"greet\", \"Print a friendly greeting\")\n",
        // handler() returns Ty::Int sentinel zero — let _ = ... discards.
        "    let _ = cmd.handler(handle_greet)\n",
        "    let _ = cmd.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe).output().expect("spawn hood example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "binary exit non-zero ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("hello from hood"),
        "stdout must contain `hello from hood`; got:\n{stdout}\nstderr:\n{stderr}",
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "exit code must be 0; got {:?}",
        out.status.code(),
    );
}

// =====================================================================
// Negative type-check cases — ADR-0073 §5 R4 callback gate. Hood's
// callback shape is `fn() -> i64` (zero positional args; i64 return);
// any other shape is rejected by the SHARED `check_callback_arg` path
// (same gate that protects pit's `fn(Request) -> Response` callbacks).
// We ship 3 cases here: wrong-arity, wrong-return-type, and lambda.
// =====================================================================

/// Compile-only helper — returns (success?, stderr).
fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Wrong-arity handler — takes a positional arg where hood's callback
/// slot expects zero positional args.
#[test]
fn test_neg_hood_callback_rejects_wrong_arity_fn() {
    let (ok, stderr) = try_build(concat!(
        "import hood\n",
        "\n",
        "fn bad_handler(extra: i64) -> i64:\n",
        "    return extra\n",
        "\n",
        "fn main() -> i64:\n",
        "    let cmd = hood.Command(\"greet\", \"help\")\n",
        "    let _ = cmd.handler(bad_handler)\n",
        "    return 0\n",
    ));
    assert!(!ok, "1-arg handler must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("callback") || stderr.contains("signature"),
        "stderr must mention callback / signature mismatch; got:\n{stderr}"
    );
}

/// Wrong-return-type handler — returns `str`, callback slot expects
/// `i64`.
#[test]
fn test_neg_hood_callback_rejects_wrong_return_type() {
    let (ok, stderr) = try_build(concat!(
        "import hood\n",
        "\n",
        "fn bad_return() -> str:\n",
        "    return \"oops\"\n",
        "\n",
        "fn main() -> i64:\n",
        "    let cmd = hood.Command(\"greet\", \"help\")\n",
        "    let _ = cmd.handler(bad_return)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "str-returning handler must be rejected; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("callback") || stderr.contains("signature"),
        "stderr must mention callback / signature mismatch; got:\n{stderr}"
    );
}

/// Lambda where a top-level `fn` name is required. Shape gate fires
/// in `check_callback_arg` (the same gate pit uses).
#[test]
fn test_neg_hood_callback_rejects_lambda() {
    let (ok, stderr) = try_build(concat!(
        "import hood\n",
        "fn main() -> i64:\n",
        "    let cmd = hood.Command(\"greet\", \"help\")\n",
        "    let _ = cmd.handler(lambda: 0)\n",
        "    return 0\n",
    ));
    assert!(!ok, "lambda callback must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("CallbackArgMustBeFnName") || stderr.contains("callback"),
        "stderr must mention callback / fn name; got:\n{stderr}"
    );
}
