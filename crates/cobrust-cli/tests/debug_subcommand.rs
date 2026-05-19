//! ADR-0059c Phase L wave-3 `cobrust debug` subcommand integration tests.
//!
//! Three tests per ADR-0059c §6:
//!
//! - §6.1 `debug_help_lists_subcommand` — verifies clap registers the new
//!   subcommand + that `--dap`, `--bp`, `--lldb-path` flags surface in
//!   `cobrust debug --help` output. Fast (no spawn cost beyond a single
//!   `--help` invocation).
//! - §6.2 `debug_dap_stdio_initialize_disconnect_handshake` —
//!   `#[ignore]`-gated end-to-end DAP handshake (Initialize +
//!   Disconnect) that proves `cobrust debug --dap` correctly stdio-
//!   forwards to the wave-2 `cobrust-dap` server. Mirrors
//!   `cobrust-dap/tests/dap_e2e_smoke.rs` but spawns the wave-3 CLI
//!   instead of `cobrust-dap` directly.
//! - §6.3 `debug_missing_source_in_interactive_mode_errors_clean` —
//!   verifies the §4.2 `DebugError::MissingSource` user-error path
//!   emits exit 1 + a hint-bearing stderr.

#![allow(clippy::missing_panics_doc)]
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

#[test]
fn debug_help_lists_subcommand() {
    // §6.1 — clap registration smoke.
    let out = Command::new(cobrust_binary())
        .args(["debug", "--help"])
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust debug --help");
    assert!(
        out.status.success(),
        "`cobrust debug --help` failed: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--dap"),
        "--help missing --dap flag; stdout={stdout}",
    );
    assert!(
        stdout.contains("--bp"),
        "--help missing --bp flag; stdout={stdout}",
    );
    assert!(
        stdout.contains("--lldb-path"),
        "--help missing --lldb-path flag; stdout={stdout}",
    );
}

#[test]
fn debug_missing_source_in_interactive_mode_errors_clean() {
    // §6.3 — `cobrust debug` (no args, no --dap) is a user error.
    // Per ADR-0059c §4.2 `DebugError::MissingSource` maps to exit
    // code 1 (USER_ERROR per ADR-0024).
    let out = Command::new(cobrust_binary())
        .arg("debug")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust debug");
    assert!(
        !out.status.success(),
        "`cobrust debug` (no args) should fail; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 (USER_ERROR); got {:?}; stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("source file required"),
        "stderr missing user hint; stderr={stderr}",
    );
}

#[test]
#[ignore = "spawns cobrust + cobrust-dap subprocess; run with --ignored on DG"]
fn debug_dap_stdio_initialize_disconnect_handshake() {
    // §6.2 — `cobrust debug --dap` forwards stdio to cobrust-dap.
    // Validate the round-trip handshake (Initialize → response →
    // Disconnect → clean exit). Mirrors
    // `cobrust-dap/tests/dap_e2e_smoke.rs` shape but spawns
    // `cobrust debug --dap` so we exercise the wave-3 CLI path.
    //
    // Per HARD-BANNED #1 (no new Cargo deps), JSON framing is built
    // by hand — DAP wire frames are tiny and the substring assertions
    // are sufficient for a handshake smoke (the wave-2 dap_e2e_smoke
    // test already exercises serde-parsed shape; wave-3 reuses that
    // contract here by string-matching the response body fragments).
    let mut child = Command::new(cobrust_binary())
        .args(["debug", "--dap", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cobrust debug --dap");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    // 1. Send Initialize. Hand-rolled JSON body so wave-3 carries no
    //    new serde_json dep.
    let init_body = r#"{"seq":1,"type":"request","command":"initialize","arguments":{"clientID":"wave3-e2e","pathFormat":"path"}}"#;
    write_dap_frame(&mut stdin, init_body);

    // 2. Read response body and assert success substrings.
    let response = read_dap_frame(&mut reader);
    assert!(
        response.contains("\"command\":\"initialize\""),
        "expected initialize response, got {response}",
    );
    assert!(
        response.contains("\"success\":true"),
        "Initialize response not success; got {response}",
    );

    // 3. Send Disconnect.
    let disc_body = r#"{"seq":2,"type":"request","command":"disconnect","arguments":{}}"#;
    write_dap_frame(&mut stdin, disc_body);
    let _ = read_dap_frame(&mut reader);

    drop(stdin);

    // 4. Wait for clean exit within 5s budget.
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().expect("try_wait") {
            Some(_) => break,
            None if start.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                panic!("cobrust debug --dap did not exit within 5s after Disconnect");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Helper: write a DAP `Content-Length`-framed JSON message.
fn write_dap_frame(stdin: &mut std::process::ChildStdin, body: &str) {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes()).expect("write header");
    stdin.write_all(body.as_bytes()).expect("write body");
    stdin.flush().expect("flush");
}

/// Helper: read one DAP `Content-Length`-framed JSON message body as
/// a `String`. Parses only the `Content-Length:` header line, ignores
/// other headers per DAP base protocol.
fn read_dap_frame<R: BufRead>(reader: &mut R) -> String {
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).expect("read line");
        assert!(n > 0, "EOF before header complete");
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().expect("parse content-length");
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).expect("read body");
    String::from_utf8(body).expect("utf8")
}
