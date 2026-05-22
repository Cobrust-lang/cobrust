//! ADR-0059g Phase L wave-5 — 20 integration + snapshot tests.
//!
//! Coverage matrix per ADR-0059g §5:
//! - 4 logpoints (§3.1)
//! - 4 dataBP (§3.2)
//! - 3 stepIn (§3.3)
//! - 3 result_err (§3.4 + 0059f §3.4 RESOLVED)
//! - 6 snapshot via insta
//!
//! All tests use `LldbDriver::test_stub(...)` — no lldb-18 spawn. The
//! existing `lldb_driver_integration_e2e.rs` covers the real-lldb path
//! when `lldb-18` is on PATH (ignored on Mac by default).

#![allow(clippy::unwrap_used, clippy::missing_panics_doc)]

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
// §3.1 logpoints — 4 tests
// =====================================================================

#[tokio::test]
async fn logpoint_log_expression_no_halt() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        1,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{ "line": 7, "logMessage": "loop iter: i=42" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert_eq!(bps[0]["verified"], true);
    let msg = bps[0]["message"].as_str().unwrap();
    assert!(msg.contains("logpoint"));
    assert!(msg.contains("loop iter"));
}

#[tokio::test]
async fn logpoint_log_with_placeholder_verbatim() {
    // DAP-spec `{expr}` placeholder is wave-5 verbatim — the literal
    // curly-brace text shows in the bp message until the placeholder-
    // interpolation enhancement ships (out-of-scope wave-5 §4).
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        2,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{ "line": 12, "logMessage": "value of i is {i}" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let msg = body["breakpoints"][0]["message"].as_str().unwrap();
    assert!(msg.contains("{i}"));
}

#[tokio::test]
async fn logpoint_takes_precedence_over_condition() {
    // If both condition and logMessage are present, logMessage wins
    // (DAP spec — the breakpoint becomes a logpoint regardless).
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        3,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{
                "line": 7,
                "condition": "i > 10",
                "logMessage": "hit at i > 10"
            }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let msg = body["breakpoints"][0]["message"].as_str().unwrap();
    // Logpoint message wins over condition message.
    assert!(msg.contains("logpoint"));
    assert!(msg.contains("hit at"));
}

#[tokio::test]
async fn logpoint_multiple_logpoints_in_one_request() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        4,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [
                { "line": 7, "logMessage": "first log" },
                { "line": 12, "logMessage": "second log" },
            ],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 2);
    assert!(bps[0]["message"].as_str().unwrap().contains("first log"));
    assert!(bps[1]["message"].as_str().unwrap().contains("second log"));
}

// =====================================================================
// §3.2 dataBP — 4 tests
// =====================================================================

#[tokio::test]
async fn data_bp_read_access() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        10,
        "setDataBreakpoints",
        serde_json::json!({
            "breakpoints": [{ "dataId": "counter", "accessType": "read" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert_eq!(bps[0]["verified"], true);
    let msg = bps[0]["message"].as_str().unwrap();
    assert!(msg.contains("watchpoint"));
    assert!(msg.contains("counter"));
    assert!(msg.contains("read"));
}

#[tokio::test]
async fn data_bp_write_access_default_when_unset() {
    // DAP-spec semantics: accessType is optional; default is "write".
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        11,
        "setDataBreakpoints",
        serde_json::json!({
            "breakpoints": [{ "dataId": "state" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let msg = body["breakpoints"][0]["message"].as_str().unwrap();
    assert!(msg.contains("write"));
}

#[tokio::test]
async fn data_bp_readwrite_access() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        12,
        "setDataBreakpoints",
        serde_json::json!({
            "breakpoints": [{ "dataId": "shared_buf", "accessType": "readWrite" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let msg = body["breakpoints"][0]["message"].as_str().unwrap();
    assert!(msg.contains("read_write"));
}

#[tokio::test]
async fn data_bp_unknown_access_type_emits_verified_false() {
    // accessType outside the {read,write,readWrite} set surfaces a
    // verified-false bp with a diagnostic message instead of erroring.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        13,
        "setDataBreakpoints",
        serde_json::json!({
            "breakpoints": [{ "dataId": "foo", "accessType": "bogus" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bp = &body["breakpoints"][0];
    assert_eq!(bp["verified"], false);
    let msg = bp["message"].as_str().unwrap();
    assert!(msg.contains("unknown watchpoint access"));
    assert!(msg.contains("bogus"));
}

// =====================================================================
// §3.3 stepIn — 3 tests
// =====================================================================

#[tokio::test]
async fn step_in_basic_stepin() {
    // Stub fast-path returns Step; the handler emits `{}` per DAP spec.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread step-in".to_string(),
        "  thread #1, stop reason = step in\n".to_string(),
    )]));
    let request = req(
        20,
        "stepIn",
        serde_json::json!({"threadId": 1, "granularity": "statement"}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    // stepIn returns an empty body per DAP spec.
    let body = response.body.unwrap();
    assert!(body.is_object());
    assert!(body.as_object().unwrap().is_empty());
}

#[tokio::test]
async fn step_in_with_target_id_parsed_but_ignored() {
    // target_id is parsed (no parse error) but wave-5 ignores it.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        21,
        "stepIn",
        serde_json::json!({
            "threadId": 1,
            "targetId": 42,
            "granularity": "line"
        }),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
}

#[tokio::test]
async fn step_in_instruction_granularity_coerced_to_statement() {
    // Wave-5 honest scope: source-level only. `instruction`
    // granularity is parsed but treated as statement.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread step-in".to_string(),
        "  thread #1, stop reason = step in\n".to_string(),
    )]));
    let request = req(
        22,
        "stepIn",
        serde_json::json!({"threadId": 1, "granularity": "instruction"}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
}

// =====================================================================
// §3.4 result_err (0059f §3.4 RESOLVED) — 3 tests
// =====================================================================

#[tokio::test]
async fn result_err_filter_uses_new_runtime_symbol() {
    // The DAP filter `result_err` now maps to `__cobrust_result_err_panic`
    // — the symbol shipped in `crates/cobrust-stdlib/src/panic.rs` per
    // ADR-0059g §3.4. The wave-4 placeholder name
    // `cobrust_result_err_construct` is superseded.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        30,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["result_err"]}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let msg = body["breakpoints"][0]["message"].as_str().unwrap();
    assert!(msg.contains("__cobrust_result_err_panic"));
    assert!(!msg.contains("cobrust_result_err_construct"));
}

#[tokio::test]
async fn result_err_multi_filter_includes_panic_and_unreachable() {
    // setExceptionBreakpoints with three filters; result_err is the
    // wave-5 RESOLVED one. All three should be verified on stub.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        31,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["panic", "result_err", "unreachable"]}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 3);
    for bp in bps {
        assert_eq!(bp["verified"], true);
    }
    // result_err bp carries the new symbol name in its message.
    let result_err_msg = bps[1]["message"].as_str().unwrap();
    assert!(result_err_msg.contains("__cobrust_result_err_panic"));
}

#[tokio::test]
async fn result_err_filter_only_returns_single_bp() {
    // Just the result_err filter — confirm wave-5 doesn't accidentally
    // emit multiple bps from a single filter.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        32,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["result_err"]}),
    );
    let response = adapter.dispatch(&request).await;
    assert!(response.success);
    let body = response.body.unwrap();
    let bps = body["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert_eq!(bps[0]["verified"], true);
}

// =====================================================================
// §5 snapshots — 6 tests via insta
// =====================================================================

#[tokio::test]
async fn snapshot_logpoint_set_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        100,
        "setBreakpoints",
        serde_json::json!({
            "source": { "name": "fib.cb", "path": "/tmp/fib.cb" },
            "breakpoints": [{ "line": 7, "logMessage": "iter: i" }],
        }),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_5_logpoint_set_response", response.body);
}

#[tokio::test]
async fn snapshot_data_bp_set_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        101,
        "setDataBreakpoints",
        serde_json::json!({
            "breakpoints": [
                { "dataId": "counter", "accessType": "write" },
                { "dataId": "buffer", "accessType": "readWrite" },
            ],
        }),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_5_data_bp_set_response", response.body);
}

#[tokio::test]
async fn snapshot_step_in_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread step-in".to_string(),
        "  thread #1, stop reason = step in\n".to_string(),
    )]));
    let request = req(
        102,
        "stepIn",
        serde_json::json!({"threadId": 1, "granularity": "statement"}),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_5_step_in_response", response.body);
}

#[tokio::test]
async fn snapshot_result_err_symbol_bp_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        103,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["result_err"]}),
    );
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_5_result_err_symbol_bp_response", response.body);
}

#[tokio::test]
async fn snapshot_capabilities_v1_2_response() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(104, "initialize", serde_json::json!({"clientID": "vscode"}));
    let response = adapter.dispatch(&request).await;
    insta::assert_json_snapshot!("wave_5_capabilities_v1_2_response", response.body);
}

#[tokio::test]
async fn snapshot_multi_feature_aggregate_response() {
    // Confirms the v1.2 surface — logpoint + dataBP + stepIn + result_err
    // all in one debug session sequence. Exercises the aggregate shape
    // an editor sees during a real session.
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "thread step-in".to_string(),
        "  thread #1, stop reason = step in\n".to_string(),
    )]));
    let mut results = serde_json::Map::new();
    let logpoint_req = req(
        105,
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": "fib.cb" },
            "breakpoints": [{ "line": 7, "logMessage": "iter" }],
        }),
    );
    results.insert(
        "logpoint".to_string(),
        adapter.dispatch(&logpoint_req).await.body.unwrap(),
    );
    let data_bp_req = req(
        106,
        "setDataBreakpoints",
        serde_json::json!({
            "breakpoints": [{ "dataId": "x", "accessType": "write" }],
        }),
    );
    results.insert(
        "data_bp".to_string(),
        adapter.dispatch(&data_bp_req).await.body.unwrap(),
    );
    let step_in_req = req(107, "stepIn", serde_json::json!({"threadId": 1}));
    results.insert(
        "step_in".to_string(),
        adapter.dispatch(&step_in_req).await.body.unwrap(),
    );
    let result_err_req = req(
        108,
        "setExceptionBreakpoints",
        serde_json::json!({"filters": ["result_err"]}),
    );
    results.insert(
        "result_err".to_string(),
        adapter.dispatch(&result_err_req).await.body.unwrap(),
    );
    insta::assert_json_snapshot!("wave_5_multi_feature_aggregate_response", results);
}
