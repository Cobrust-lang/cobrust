#![allow(
    clippy::unwrap_used,
    reason = "test code; unwrap on test invariants is acceptable"
)]
//! Smoke test for `cobrust dap` subcommand (ADR-0068 §8 closure).
//!
//! Spawn `cobrust dap` with stdio piped, send a synthetic DAP `initialize`
//! request (version 1 protocol), verify the server responds with an
//! `Initialized` event or `initialize` response.
//!
//! DAP uses `Content-Length:` headers + JSON body, similar to LSP.
//! Does NOT exercise full DAP flow — only verifies subcommand wire-up to
//! `cobrust_dap::run()` is end-to-end functional.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn cobrust_dap_subcommand_responds_to_initialize() {
    // Locate the workspace root from the manifest dir of this crate
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let cobrust_bin = workspace_root.join("target/release/cobrust");

    // Build cobrust (release profile to match wheel)
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "cobrust-cli"])
        .current_dir(workspace_root)
        .status()
        .expect("cargo build");
    assert!(status.success(), "cargo build cobrust-cli failed");

    // Spawn `cobrust dap` with stdio piped
    let mut child = Command::new(&cobrust_bin)
        .arg("dap")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cobrust dap");

    // Synthetic DAP initialize request
    let body = r#"{"seq":1,"type":"request","command":"initialize","arguments":{"clientID":"smoke-test","adapterID":"cobrust"}}"#;
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(msg.as_bytes())
        .unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();

    // Give it 2s to respond
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let _ = child.kill();
    let output = child.wait_with_output().expect("wait for cobrust dap");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Expect at least `Content-Length` header, `initialized` event, or
    // `initialize` response body
    assert!(
        stdout.contains("Content-Length")
            || stdout.contains("initialized")
            || stdout.contains("initialize"),
        "cobrust dap subcommand did not emit DAP response;\nstdout: {stdout}"
    );
}
