//! F68 — end-to-end `.cb` source → compile → link → run → stdout-assert
//! for the MODULE-RECEIVER ecosystem decorator `@dora.node(inputs=[...],
//! outputs=[...])`.
//!
//! This extends ADR-0074's decorator desugar (which recognised only
//! HANDLE-receiver decorators — `@app.route(...)` where the receiver is a
//! let-bound `pit.App`) to recognise MODULE-receiver decorators, where the
//! decorator's receiver is an ecosystem MODULE ALIAS (`import dora`) rather
//! than a let-bound handle. The HIR post-pass forks on the receiver's
//! resolved `DefKind`:
//!
//! - `DefKind::ImportAlias` of a known ecosystem module + a
//!   module-decoratable method (`node`) → synthesise a MODULE-FN call
//!   `dora.node(<handler>)` at `main`'s prologue (this file).
//! - `DefKind::LetBinding` → the pre-existing handle-method synth
//!   `app.route(...)` (see `decorator_pit_e2e.rs`).
//!
//! The synthesised `dora.node(detect)` is byte-identical to the explicit
//! form proven in `dora_hello_e2e.rs`, so it reuses the WHOLE ADR-0073
//! callback chain (MIR `Constant::FnRef` → codegen fn-ptr → `libdora.a`
//! synthetic trampoline) with ZERO new compiler infra below HIR. The
//! `inputs=`/`outputs=` port metadata is declarative — validated as
//! list-of-str literals at the desugar layer, then DROPPED (Phase 1's
//! synthetic `dora.node` manifest row takes only the callback slot).
//!
//! ```text
//! `import dora` + `@dora.node(inputs=["camera"], outputs=["detections"])`
//!   over `fn detect(event: dora.Event) -> i64:` + `node.run()`
//!   → cobrust-hir module-receiver decorator desugar (F68):
//!       synth `dora.node(detect)` at main's prologue
//!   → cobrust-types `try_synth_ecosystem_call` Case 1 (module free-fn) —
//!       validates the callback slot via the SHARED `check_callback_arg`
//!   → cobrust-mir lowering (Constant::FnRef for the callback arg)
//!   → cobrust-codegen fn-pointer materialisation via function_ids
//!   → cobrust-dora C-ABI shims (libdora.a) + synthetic trampoline
//!   → handler reads event via `event.data_str()` + prints + returns i64 0
//!   → test asserts stdout contains "got frame: frame_001" + exit 0
//! ```
//!
//! Pattern (mirrors `dora_hello_e2e.rs` + `decorator_pit_e2e.rs`): compile
//! a `.cb` program to an exe, run it, assert stdout + exit; negative cases
//! compile-only and assert the diagnostic.

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

// =====================================================================
// POSITIVE — the F68 canonical decorator form (mirrors
// `examples/dora_hello/main.cb`).
// =====================================================================

/// `@dora.node(inputs=["camera"], outputs=["detections"])` over a
/// top-level `fn detect(event: dora.Event) -> i64:` desugars to a
/// synthetic `dora.node(detect)` register-call, installs the handler, and
/// the synthetic runtime fires it once on `node.run()`.
#[test]
fn test_e2e_dora_decorator_form_round_trip() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"detections\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let frame: str = event.data_str()\n",
        "    print_no_nl(\"got frame: \")\n",
        "    print(frame)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
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

/// The BARE module-receiver form `@dora.node` (no call args, no IO
/// metadata) also desugars to `dora.node(detect)` and runs. Exercises the
/// bare-form recognition branch of `is_ecosystem_decorator_shape`.
#[test]
fn test_e2e_dora_decorator_bare_form_round_trip() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let frame: str = event.data_str()\n",
        "    print_no_nl(\"got frame: \")\n",
        "    print(frame)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe).output().expect("spawn dora bare-form");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "bare-form binary exit non-zero");
    assert!(
        stdout.contains("got frame: frame_001"),
        "bare-form stdout must contain `got frame: frame_001`; got:\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — shape + signature gates. The module-receiver decorator must
// reuse ADR-0074's scope gate (module-scope only) and ADR-0073's shared
// `CallbackSignatureMismatch` gate (same gate that protects pit/hood),
// plus the F68-new `@dora.node` shape gates (positional/kwarg/list).
// =====================================================================

/// A module-receiver decorator on a NESTED fn is rejected at HIR with the
/// "ecosystem decorators must be at module scope" diagnostic (ADR-0074 §2
/// Q1 — the same gate that rejects a nested `@app.route`).
#[test]
fn test_neg_dora_decorator_on_nested_fn_rejected() {
    let (ok, stderr) = try_build(concat!(
        "import dora\n",
        "\n",
        "fn outer() -> i64:\n",
        "    @dora.node(inputs=[\"camera\"], outputs=[\"detections\"])\n",
        "    fn inner(event: dora.Event) -> i64:\n",
        "        return 0\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "nested-fn module-receiver decorator must be rejected; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("module scope"),
        "stderr must mention module-scope requirement; got:\n{stderr}"
    );
}

/// `@dora.node` over a handler whose signature doesn't match
/// `fn(dora.Event) -> i64` is rejected by the SHARED callback gate with
/// `CallbackSignatureMismatch` — proving the synthesised `dora.node(...)`
/// routes through `try_synth_ecosystem_call`'s module-fn arm +
/// `check_callback_arg` (NOT a no-op decorator).
#[test]
fn test_neg_dora_decorator_wrong_handler_signature_rejected() {
    let (ok, stderr) = try_build(concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"detections\"])\n",
        "fn bad_handler() -> i64:\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "zero-arity handler under @dora.node must be rejected; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("CallbackSignatureMismatch")
            || stderr.contains("callback")
            || stderr.contains("signature"),
        "stderr must mention callback / signature mismatch; got:\n{stderr}"
    );
}

/// `@dora.node` rejects a POSITIONAL decorator arg — the handler is the
/// decorated `fn`, not a decorator arg. F68-new shape gate.
#[test]
fn test_neg_dora_decorator_positional_arg_rejected() {
    let (ok, stderr) = try_build(concat!(
        "import dora\n",
        "\n",
        "@dora.node(detect)\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "positional decorator arg must be rejected; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("no positional args"),
        "stderr must explain the handler is the decorated fn; got:\n{stderr}"
    );
}

/// `@dora.node` rejects a non-list `inputs=` value — IO ports must be a
/// list of string literals. F68-new shape gate.
#[test]
fn test_neg_dora_decorator_non_list_inputs_rejected() {
    let (ok, stderr) = try_build(concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=\"camera\", outputs=[\"detections\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "non-list `inputs=` value must be rejected; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("list of string literals"),
        "stderr must explain inputs/outputs must be list-of-str; got:\n{stderr}"
    );
}
