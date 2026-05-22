//! ADR-0059f Phase L wave-4 — 22 integration + snapshot tests.
//!
//! Coverage matrix per ADR-0059f §5:
//! - 5 evaluate (§3.1)
//! - 4 conditional bp (§3.2)
//! - 4 multi-thread (§3.3)
//! - 3 exception bp (§3.4)
//! - 6 snapshot via insta
//!
//! All tests use `LldbDriver::test_stub(...)` — no lldb-18 spawn. The
//! existing `lldb_driver_integration_e2e.rs` covers the real-lldb path
//! when `lldb-18` is on PATH (ignored on Mac by default).

use cobrust_dap::{Adapter, LldbDriver, Request};

fn req(seq: i64, command: &str, args: serde_json::Value) -> Request {
    Request {
        seq,
        type_field: "request".to_string(),
        command: command.to_string(),
        arguments: Some(args),
    }
}

// =====================================================================
// §3.1 evaluate — 5 tests
// =====================================================================

#[tokio::test]
async fn evaluate_simple_arithmetic_expression() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "expression --".to_string(),
        "(int) $0 = 5\n".to_string(),
    )]));
    let request = req(
        1,
        "evaluate",
        serde_json::json!({"expression": "2 + 3", "context": "repl"}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    assert_eq!(body["result"], "5");
    assert_eq!(body["type"], "int");
}

#[tokio::test]
async fn evaluate_field_access_via_dot() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "expression --".to_string(),
        "(cobrust::Str) $0 = \"hello\"\n".to_string(),
    )]));
    let request = req(
        2,
        "evaluate",
        serde_json::json!({"expression": "p.name", "context": "watch"}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    assert_eq!(body["result"], "\"hello\"");
    assert_eq!(body["type"], "cobrust::Str");
}

#[tokio::test]
async fn evaluate_bool_test_expression() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "expression --".to_string(),
        "(bool) $0 = true\n".to_string(),
    )]));
    let request = req(
        3,
        "evaluate",
        serde_json::json!({"expression": "i > 10"}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    assert_eq!(body["result"], "true");
    assert_eq!(body["type"], "bool");
}

#[tokio::test]
async fn evaluate_lookup_undefined_returns_untyped_fallthrough() {
    // lldb emits an error string when the expression refers to an
    // undefined name. The parser falls through (no `(<type>) $N =`
    // pattern match) and returns the raw stdout as result text.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "expression --".to_string(),
        "error: <user expression 0>:1:1: use of undeclared identifier 'qqq'\n"
            .to_string(),
    )]));
    let request = req(
        4,
        "evaluate",
        serde_json::json!({"expression": "qqq"}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success); // error text surfaces via result, not DAP failure
    let body = response.body.unwrap();
    let result_str = body["result"].as_str().unwrap();
    assert!(result_str.contains("undeclared identifier"));
    assert!(body["type"].is_null() || !body.as_object().unwrap().contains_key("type"));
}

#[tokio::test]
async fn evaluate_in_nested_frame() {
    // frame_id selects which frame's locals are in scope.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "expression --".to_string(),
        "(int) $0 = 99\n".to_string(),
    )]));
    let request = req(
        5,
        "evaluate",
        serde_json::json!({
            "expression": "n",
            "frameId": 2,
            "context": "watch"
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    assert_eq!(body["result"], "99");
}

// =====================================================================
// §3.2 conditional bp — 4 tests
// =====================================================================

#[tokio::test]
async fn conditional_bp_set_with_condition_returns_verified() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        10,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{ "line": 7, "condition": "i > 10" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert_eq!(bps[0]["verified"], true);
    // Message carries the condition for editor hover tooltip.
    let msg = bps[0]["message"].as_str().unwrap();
    assert!(msg.contains("i > 10"));
}

#[tokio::test]
async fn unconditional_bp_set_without_condition_field_omits_message() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        11,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{ "line": 12 }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert_eq!(bps[0]["verified"], true);
    // Wave-2 unconditional bp does NOT carry a message field.
    let bp_obj = bps[0].as_object().unwrap();
    assert!(!bp_obj.contains_key("message") || bp_obj["message"].is_null());
}

#[tokio::test]
async fn conditional_bp_with_parse_error_condition_still_returns_bp() {
    // lldb accepts the bp set even if the condition has a parse
    // error — the condition fires at hit-time, not set-time. Wave-4
    // mirrors this: the bp is created with the condition string
    // attached; lldb will report the parse error when the bp hits.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        12,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{ "line": 7, "condition": "i ?? bad syntax" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps[0]["verified"], true);
    assert!(
        bps[0]["message"]
            .as_str()
            .unwrap()
            .contains("i ?? bad syntax")
    );
}

#[tokio::test]
async fn conditional_bp_threading_safety_two_bps_distinct_conditions() {
    // Two conditional bps in one setBreakpoints request — each carries
    // its own condition; both must be verified independently.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        13,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [
                { "line": 7, "condition": "i > 10" },
                { "line": 12, "condition": "j == 0" },
            ],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 2);
    assert!(bps[0]["message"].as_str().unwrap().contains("i > 10"));
    assert!(bps[1]["message"].as_str().unwrap().contains("j == 0"));
}

// =====================================================================
// §3.3 multi-thread — 4 tests
// =====================================================================

#[tokio::test]
async fn multi_thread_single_thread_program_still_works() {
    // Stub with no canned `thread list` response → list_threads
    // returns empty → handler falls back to single-thread shim.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(20, "threads", serde_json::json!({}));
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let threads = body["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["id"], 1);
    assert_eq!(threads[0]["name"], "main");
}

#[tokio::test]
async fn multi_thread_two_threads_surfaced_via_list_threads() {
    // Stub canned `thread list` stdout with 2 threads.
    let canned = "Process 12345 stopped\n  thread #1: tid = 0x1001, 0x100003ee4, name = 'main', queue = 'com.apple.main-thread'\n  thread #2: tid = 0x1002, 0x100003f44, name = 'worker-0', queue = 'com.cobrust.task-runtime'\n".to_string();
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread list".to_string(),
        canned,
    )]));
    let request = req(21, "threads", serde_json::json!({}));
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let threads = body["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0]["id"], 1);
    assert_eq!(threads[0]["name"], "main");
    assert_eq!(threads[1]["id"], 2);
    assert_eq!(threads[1]["name"], "worker-0");
}

#[tokio::test]
async fn multi_thread_per_thread_stack_trace_for_thread_2() {
    // Canned `thread backtrace` stdout — same parser as wave-2
    // stack_trace; the thread select prefix is consumed by send_command
    // stub fall-through.
    let bt = "* thread #2, queue = 'com.cobrust.task-runtime'\n  * frame #0: 0x100003ee4 fib`fib + 8 at fib.cb:8:5\n    frame #1: 0x100003f44 fib`worker_main + 12 at fib.cb:24:5\n".to_string();
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread backtrace".to_string(),
        bt,
    )]));
    let request = req(
        22,
        "stackTrace",
        serde_json::json!({"threadId": 2, "startFrame": 0, "levels": 20}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let frames = body["stackFrames"].as_array().unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["name"], "fib");
    assert_eq!(frames[1]["name"], "worker_main");
}

#[tokio::test]
async fn multi_thread_id_out_of_bounds_returns_empty_frames() {
    // No canned response for `thread select 999` / `thread backtrace`
    // → stub returns empty stdout → parse_stack_trace returns empty.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        23,
        "stackTrace",
        serde_json::json!({"threadId": 999}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    assert_eq!(body["totalFrames"], 0);
}

// =====================================================================
// §3.4 exception bp — 3 tests
// =====================================================================

#[tokio::test]
async fn exception_bp_panic_filter_set_verified() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        30,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["panic"]}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert_eq!(bps[0]["verified"], true);
    assert!(
        bps[0]["message"]
            .as_str()
            .unwrap()
            .contains("__cobrust_panic")
    );
}

#[tokio::test]
async fn exception_bp_result_err_filter_honest_scope_skip() {
    // The stub fast-path emits verified=true; in real lldb the
    // `cobrust_result_err_construct` symbol is currently unemitted
    // and lldb would report "no locations (pending)" → verified=false.
    // This stub-driver test exercises the wire shape; the honest-
    // scope-skip path is the real-lldb integration test (covered by
    // dap_e2e_smoke.rs when lldb-18 is on PATH).
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        31,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["result_err"]}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    // Message carries the symbol identifier so the user / agent can
    // diagnose missing-symbol cases.
    assert!(
        bps[0]["message"]
            .as_str()
            .unwrap()
            .contains("cobrust_result_err_construct")
    );
}

#[tokio::test]
async fn exception_bp_unreachable_filter_set_verified() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        32,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["unreachable"]}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert!(
        bps[0]["message"]
            .as_str()
            .unwrap()
            .contains("unreachable_internal")
    );
}

// =====================================================================
// §5 snapshots — 6 tests via insta
// =====================================================================

#[tokio::test]
async fn snapshot_evaluate_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "expression --".to_string(),
        "(cobrust::List) $0 = [1, 2, 3]\n".to_string(),
    )]));
    let request = req(
        100,
        "evaluate",
        serde_json::json!({"expression": "xs", "context": "watch"}),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_4_evaluate_response", response.body);
}

#[tokio::test]
async fn snapshot_conditional_bp_set_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        101,
        "setBreakpoints",
        serde_json::json!({
            "source": { "name": "fib.cb", "path": "/tmp/fib.cb" },
            "breakpoints": [{ "line": 7, "condition": "i > 10" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_4_conditional_bp_response", response.body);
}

#[tokio::test]
async fn snapshot_threads_list_response() {
    let canned = "Process 12345 stopped\n  thread #1: tid = 0x1001, 0x100003ee4, name = 'main'\n  thread #2: tid = 0x1002, 0x100003f44, name = 'worker-0'\n".to_string();
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread list".to_string(),
        canned,
    )]));
    let request = req(102, "threads", serde_json::json!({}));
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_4_threads_list_response", response.body);
}

#[tokio::test]
async fn snapshot_per_thread_stack_trace_response() {
    let bt = "* thread #2, queue = 'com.cobrust.task-runtime'\n  * frame #0: 0x100003ee4 fib`fib + 8 at fib.cb:8:5\n    frame #1: 0x100003f44 fib`worker_main + 12 at fib.cb:24:5\n".to_string();
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread backtrace".to_string(),
        bt,
    )]));
    let request = req(
        103,
        "stackTrace",
        serde_json::json!({"threadId": 2, "startFrame": 0, "levels": 20}),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_4_per_thread_stack_trace_response", response.body);
}

#[tokio::test]
async fn snapshot_exception_bp_set_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        104,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["panic", "result_err", "unreachable"]}),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_4_exception_bp_response", response.body);
}

#[tokio::test]
async fn snapshot_capabilities_v1_1_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(105, "initialize", serde_json::json!({"clientID": "vscode"}));
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_4_capabilities_v1_1_response", response.body);
}
