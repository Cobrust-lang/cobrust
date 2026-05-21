//! Integration tests for `cobrust install` (ADR-0065 §3.3.3 CLI surface).
//!
//! These tests exec the produced `cobrust` binary so they cover argument
//! parsing, top-level dispatch, and error formatting end-to-end.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

fn cobrust_bin() -> PathBuf {
    // The `CARGO_BIN_EXE_<name>` env var is set by `cargo test` when running
    // integration tests against a binary target — points at the built `cobrust`.
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

#[test]
fn cobrust_install_help_exits_zero_and_lists_args() {
    let out = Command::new(cobrust_bin())
        .args(["install", "--help"])
        .output()
        .expect("spawn cobrust");
    assert!(out.status.success(), "install --help should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--version"),
        "help should mention --version flag"
    );
    assert!(
        stdout.contains("--registry-url"),
        "help should mention --registry-url flag"
    );
    assert!(
        stdout.contains("--dry-run"),
        "help should mention --dry-run flag"
    );
}

#[test]
fn cobrust_install_missing_version_exits_nonzero_with_suggestion() {
    let out = Command::new(cobrust_bin())
        .args(["install", "nonexistent-pkg"])
        .output()
        .expect("spawn cobrust");
    assert!(
        !out.status.success(),
        "install without --version must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("version required") || stderr.contains("suggestion"),
        "stderr must mention required version or suggestion; got: {stderr}"
    );
}

#[test]
fn cobrust_install_dry_run_against_mock_registry_succeeds() {
    // Spin up an in-process HTTP server that serves a one-entry wheel index.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let mock_url = format!("http://127.0.0.1:{}", addr.port());

    // Pick a triple that matches the test host so wheel_select returns
    // something.
    let triple = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let cpu_level = if cfg!(target_arch = "aarch64") {
        if cfg!(target_os = "macos") { "m1" } else { "neon" }
    } else {
        "v1"
    };

    let wheels_json = serde_json::json!([{
        "filename": format!("cobrust-hello-0.1.0-{triple}-{cpu_level}.tar.gz"),
        "triple": triple,
        "cpu_level": cpu_level,
        "sha256": "0".repeat(64),
        "cobrust_abi": "0.1",
        "size_bytes": 1024,
        "download_url": "https://example.com/dummy.tar.gz"
    }]);
    let body = serde_json::to_vec(&wheels_json).expect("encode");

    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let _ = ready_tx.send(());
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 8192];
        let _ = stream.read(&mut buf);
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n");
        let _ = stream.write_all(format!("Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
        let _ = stream.write_all(&body);
    });
    let _ = ready_rx.recv();

    let out = Command::new(cobrust_bin())
        .args([
            "install",
            "hello-cb",
            "--version",
            "0.1.0",
            "--registry-url",
            &mock_url,
            "--dry-run",
        ])
        .output()
        .expect("spawn cobrust");

    let _ = handle.join();

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "install --dry-run should exit 0; stderr={stderr} stdout={stdout}"
    );
    assert!(
        stderr.contains("dry-run"),
        "stderr should announce dry-run; got: {stderr}"
    );
}
