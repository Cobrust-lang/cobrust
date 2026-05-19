//! ADR-0059b §6.2 e2e smoke test.
//!
//! Spawns `cobrust-dap` as a child process + drives the load-bearing
//! subset of a DAP handshake: Initialize -> response shape check ->
//! Disconnect. Wave-2 e2e scope per ADR-0059b §6.2 (full Launch +
//! SetBreakpoints + Continue + StackTrace + Variables against a real
//! `cobrust build --debug examples/fib.cb`-produced binary requires
//! lldb-18 hosting + a real Cobrust executable; the runnable subset
//! is the stdio handshake.
//!
//! Gating per ADR-0059b §6.2:
//! - `#[ignore]` so default `cargo test` runs skip this (snapshot
//!   tests cover the protocol shape contract).
//! - DG runs via `cargo test -p cobrust-dap -- --ignored`.
//!
//! F37 explicit feature-incomplete deferral: the FULL e2e flow
//! (Launch → SetBreakpoints → Continue → StackTrace → Variables)
//! depends on:
//! - A `cobrust build --debug` binary on the host (per ADR-0058c).
//! - lldb-18 reliably spawning under `cargo test` (broken on Mac per
//!   ADR-0059b §7.4).
//! Wave-2 ships the stdio-handshake subset; the full flow is a
//! Phase L wave-3+ followup test once the `cobrust debug` CLI lands.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};

#[tokio::test]
#[ignore = "spawns cobrust-dap subprocess; run with --ignored on DG"]
async fn e2e_dap_initialize_disconnect_handshake() {
    // 1. Locate the cobrust-dap binary (assume `cargo build` already
    //    ran in the test runner; CI dispatches `cargo test -p
    //    cobrust-dap --no-run` first then `cargo test ... --ignored`).
    let bin = env!("CARGO_BIN_EXE_cobrust-dap");

    // 2. Spawn cobrust-dap as a child.
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cobrust-dap");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    // 3. Send Initialize request.
    let init = serde_json::json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {
            "clientID": "e2e-test",
            "pathFormat": "path",
        }
    });
    write_dap_message(&mut stdin, &init).await;

    // 4. Read response and verify success + capability shape.
    let response = read_dap_message(&mut reader).await;
    assert_eq!(response["command"], "initialize");
    assert_eq!(response["success"], true);
    let body = &response["body"];
    assert_eq!(body["supportsTerminateRequest"], true);
    assert_eq!(body["supportsConditionalBreakpoints"], false);

    // 5. Send Disconnect — the loop should return Ok and child should
    //    exit gracefully.
    let disconnect = serde_json::json!({
        "seq": 2,
        "type": "request",
        "command": "disconnect",
        "arguments": {}
    });
    write_dap_message(&mut stdin, &disconnect).await;
    let _disc_response = read_dap_message(&mut reader).await;

    // 6. Child process should exit on its own; force-kill if it doesn't
    //    within a short window.
    drop(stdin);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
    let _ = child.kill().await;
}

async fn write_dap_message(stdin: &mut ChildStdin, msg: &serde_json::Value) {
    let body = serde_json::to_vec(msg).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes()).await.unwrap();
    stdin.write_all(&body).await.unwrap();
    stdin.flush().await.unwrap();
}

async fn read_dap_message<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> serde_json::Value {
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
