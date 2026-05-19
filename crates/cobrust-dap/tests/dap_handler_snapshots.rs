//! ADR-0059b §6.1 snapshot tests — 5 canonical DAP requests →
//! response JSON shape.
//!
//! Each snapshot captures the wire-shape per ADR-0059b §3.2 so any
//! drift in the conversion path (capabilities, breakpoint metadata,
//! stack-frame layout, variable display) surfaces in CI review.
//! Snapshots exclude monotonic `seq` fields (test-stub serial) so the
//! diffs are stable across runs.
//!
//! These tests use `LldbDriver::test_stub(...)` — they do NOT spawn
//! lldb-18. The e2e smoke test in `dap_e2e_smoke.rs` covers the
//! real-lldb path on <self-hosted-runner>.

use cobrust_dap::{Adapter, LldbDriver, Request};

fn req(seq: i64, command: &str, args: serde_json::Value) -> Request {
    Request {
        seq,
        type_field: "request".to_string(),
        command: command.to_string(),
        arguments: Some(args),
    }
}

#[tokio::test]
async fn snapshot_initialize_response() {
    let adapter = Adapter::new();
    let request = req(
        1,
        "initialize",
        serde_json::json!({
            "clientID": "vscode",
            "clientName": "VS Code",
            "adapterID": "cobrust",
            "pathFormat": "path",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        }),
    );
    let response = adapter.dispatch(&request).await;
    // Snapshot the body shape (capabilities); response wrapping fields
    // (seq, request_seq) are stable but uninteresting.
    insta::assert_json_snapshot!("initialize_response_capabilities", response.body);
}

#[tokio::test]
async fn snapshot_set_breakpoints_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        2,
        "setBreakpoints",
        serde_json::json!({
            "source": { "name": "fib.cb", "path": "/tmp/fib.cb" },
            "breakpoints": [
                { "line": 7 },
                { "line": 12 },
            ],
        }),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("set_breakpoints_response", response.body);
}

#[tokio::test]
async fn snapshot_continue_response() {
    // Stub provides canned stdout for a breakpoint stop.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "process continue".to_string(),
        "Process 12345 stopped\n  * thread #1, queue = 'com.apple.main-thread', stop reason = breakpoint 1.1".to_string(),
    )]));
    let request = req(3, "continue", serde_json::json!({"threadId": 1}));
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("continue_response", response.body);
}

#[tokio::test]
async fn snapshot_stack_trace_response() {
    // Stub provides canned `thread backtrace` stdout with 2 frames.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread backtrace".to_string(),
        "* thread #1, stop reason = breakpoint 1.1\n  * frame #0: 0x100003ee4 fib`fib + 8 at fib.cb:8:5\n    frame #1: 0x100003f44 fib`main + 12 at fib.cb:12:5\n".to_string(),
    )]));
    let request = req(
        4,
        "stackTrace",
        serde_json::json!({
            "threadId": 1,
            "startFrame": 0,
            "levels": 20,
        }),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("stack_trace_response", response.body);
}

#[tokio::test]
async fn snapshot_variables_response_with_pretty_printer_output() {
    // Stub provides canned `frame variable` stdout including wave-1
    // pretty-printer summaries (List, Str, Int).
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "frame variable".to_string(),
        "(cobrust::List) xs = [1, 2, 3]\n(cobrust::Str) name = \"hello\"\n(int) n = 10\n"
            .to_string(),
    )]));
    let request = req(
        5,
        "variables",
        serde_json::json!({"variablesReference": 1000}),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("variables_response", response.body);
}
