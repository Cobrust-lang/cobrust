//! M10 hello-world smoke test (per ADR-0024 §"Hello-world contract").
//!
//! Drives the full CLI end-to-end:
//!
//! - `cobrust build examples/hello.cb` → produces a host executable
//! - running the executable → prints exactly `hello, world\n` + exits 0
//!
//! ADR-0019 §"M10 — CLI driver" pinned this as a binding done-means.

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

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn cobrust_binary() -> PathBuf {
    // Cargo writes the binary to `target/debug/cobrust` for tests under
    // its own cargo invocation; we locate it via CARGO_BIN_EXE.
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

#[test]
fn hello_world_compiles_and_runs() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let hello_cb = workspace.join("examples/hello.cb");
    assert!(hello_cb.exists(), "examples/hello.cb missing");

    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-m10-hello-{}-{}",
        std::process::id(),
        line!()
    ));
    let exe_path = exe_dir.join("hello_smoke");

    // 1. Build.
    let build_output = Command::new(&bin)
        .arg("build")
        .arg(&hello_cb)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust build");
    assert!(
        build_output.status.success(),
        "cobrust build failed: stdout={} stderr={}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );
    assert!(exe_path.exists(), "build produced no exe at {exe_path:?}");

    // 2. Run the produced executable + capture stdout.
    let run_output = Command::new(&exe_path)
        .output()
        .expect("invoke produced executable");
    assert!(
        run_output.status.success(),
        "hello binary exited non-zero: {:?} stderr={}",
        run_output.status.code(),
        String::from_utf8_lossy(&run_output.stderr)
    );
    let stdout = String::from_utf8_lossy(&run_output.stdout).to_string();
    assert_eq!(
        stdout, "hello, world\n",
        "expected `hello, world\\n` on stdout, got {stdout:?}"
    );
}

#[test]
fn cobrust_run_propagates_hello_world() {
    // `cobrust run examples/hello.cb` builds + executes, propagating stdout.
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let output = Command::new(&bin)
        .arg("run")
        .arg("examples/hello.cb")
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust run");
    assert!(
        output.status.success(),
        "cobrust run failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.contains("hello, world"),
        "expected `hello, world` in stdout, got {stdout:?}"
    );
}
