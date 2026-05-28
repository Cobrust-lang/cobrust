//! ADR-0076 Phase 1 first proof — end-to-end `.cb` source → compile →
//! link → run → stdout-assert for the `dora` ecosystem-import wiring
//! (dora-rs robotics dataflow runtime bridge — the NINTH ecosystem
//! module, third to exercise the `.cb`↔Rust callback marshalling chain
//! after pit and hood).
//!
//! Generalizes the proven flat-intrinsic chain to a third callback
//! shape: the `.cb` source defines a top-level
//! `fn detect(event: dora.Event) -> i64:` whose pointer is materialised
//! at codegen via `Constant::FnRef`, crosses the C ABI as a raw fn
//! pointer, and is invoked from Rust through the SYNTHETIC trampoline
//! in `cobrust-dora/src/cabi.rs`. The trampoline allocates a canned
//! Event Box (`id="camera"`, `data_str="frame_001"`) before invoking
//! the .cb fn and frees it on return (ADR-0073 §2 D6 Rust-owned Event).
//! The handler prints via stdout; the test captures stdout via
//! `std::process::Command` + asserts the printed line + exit code.
//!
//! ```text
//! `import dora` + `dora.Node("detector")` + `dora.node(detect)` +
//! `node.run()` + a top-level `fn detect(event: dora.Event) -> i64:`
//!   → cobrust-types ecosystem manifest (typecheck, EcoParam::Callback)
//!   → cobrust-mir lowering (Constant::FnRef for the callback arg)
//!   → cobrust-codegen fn-pointer materialisation via function_ids
//!   → cobrust-dora C-ABI shims (libdora.a) + synthetic trampoline
//!     (canned Event injection)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → handler reads event via `event.data_str()` + prints + returns i64 0
//!   → test asserts stdout contains "got frame: frame_001" + exit 0
//! ```
//!
//! Pattern (mirrors `hood_cmd_e2e.rs` + `coil_hello_e2e.rs` — synchronous
//! local-only programs, no network harness): compile a `.cb` program to
//! an exe, run it with `std::process::Command`, assert stdout + exit.
//!
//! Phase 2 follow-up (tracked in
//! `docs/agent/findings/f68-dora-phase1-followups.md`) replaces the
//! explicit `let _ = dora.node(detect)` registration with the
//! `@dora.node(inputs=[...], outputs=[...])` decorator desugar (extends
//! ADR-0074 for module-receiver decorators).

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

/// Compile-only helper for the negative cases — returns `(success?,
/// stderr)`.
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

/// Compile the `.cb` "hello dora" program, run it, assert stdout +
/// exit code. The example mirrors `examples/dora_hello/main.cb`.
#[test]
fn test_e2e_dora_hello_synthetic_runtime_round_trip() {
    let source = concat!(
        "import dora\n",
        "\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let frame: str = event.data_str()\n",
        "    print_no_nl(\"got frame: \")\n",
        "    print(frame)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = dora.node(detect)\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe).output().expect("spawn dora example");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "binary exit non-zero ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("got frame: frame_001"),
        "stdout must contain `got frame: frame_001`; got:\n{stdout}\nstderr:\n{stderr}",
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "exit code must be 0; got {:?}",
        out.status.code(),
    );
}

// =====================================================================
// Negative type-check cases — ADR-0073 §5 R4 callback gate. Dora's
// callback shape is `fn(dora.Event) -> i64`; any other shape is rejected
// by the SHARED `check_callback_arg` path (same gate that protects pit's
// `fn(Request) -> Response` and hood's `fn() -> i64` callbacks).
// =====================================================================

/// Wrong-arity handler — takes ZERO positional args where dora's
/// callback slot expects ONE positional `dora.Event` arg.
#[test]
fn test_neg_dora_callback_rejects_zero_arity_fn() {
    let (ok, stderr) = try_build(concat!(
        "import dora\n",
        "\n",
        "fn bad_handler() -> i64:\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = dora.node(bad_handler)\n",
        "    return 0\n",
    ));
    assert!(!ok, "zero-arg handler must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("callback") || stderr.contains("signature"),
        "stderr must mention callback / signature mismatch; got:\n{stderr}"
    );
}

/// Wrong-return-type handler — returns `str`, callback slot expects
/// `i64`.
#[test]
fn test_neg_dora_callback_rejects_wrong_return_type() {
    let (ok, stderr) = try_build(concat!(
        "import dora\n",
        "\n",
        "fn bad_return(event: dora.Event) -> str:\n",
        "    return \"oops\"\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = dora.node(bad_return)\n",
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
