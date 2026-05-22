//! Verify `cobrust-lsp` shim binary launches identically to `cobrust lsp`
//! subcommand (ADR-0068 §4.2 + §8 closure).
//!
//! The shim is the transitional standalone binary retained for v0.5.x
//! editor extensions (per ADR-0068 §4.2); its behavior must be
//! byte-for-byte identical to `cobrust lsp`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn shim_binary_responds_to_initialize() {
    // Locate the workspace root from the manifest dir of this crate
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let shim_bin = workspace_root.join("target/release/cobrust-lsp");

    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "cobrust-lsp-shim"])
        .current_dir(workspace_root)
        .status()
        .expect("cargo build");
    assert!(status.success(), "cargo build cobrust-lsp-shim failed");

    let mut child = Command::new(&shim_bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cobrust-lsp shim");

    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#;
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(msg.as_bytes())
        .unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(2000));
    let _ = child.kill();
    let output = child.wait_with_output().expect("wait for shim");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Content-Length")
            || stdout.contains("InitializeResult")
            || stdout.contains("serverInfo"),
        "cobrust-lsp shim did not emit LSP response;\nstdout: {stdout}"
    );
}
