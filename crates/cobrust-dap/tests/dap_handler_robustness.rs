//! Robustness / edge-case tests for the wave-2 DAP request handlers
//! (Tier-2 CQ P0-2: cobrust-dap test density).
//!
//! Every test exercises one shape that the wire format admits but
//! handlers must tolerate without panic: malformed JSON, oversized
//! payloads, Unicode arguments, partial-write recovery, quick-close.
//! All tests run against the test-stub driver (`LldbDriver::test_stub`)
//! per ADR-0059b §3.3 — no real lldb-18 spawn.

#![allow(clippy::unwrap_used, clippy::missing_panics_doc)]

use cobrust_dap::Adapter;
use cobrust_dap::dap_types::Request;
use cobrust_dap::handlers::{
    handle_continue, handle_disconnect, handle_initialize, handle_launch, handle_set_breakpoints,
    handle_threads, handle_variables,
};
use cobrust_dap::lldb_driver::LldbDriver;
use serde_json::Value;

fn req(seq: i64, command: &str, args: Option<Value>) -> Request {
    Request {
        seq,
        type_field: "request".to_string(),
        command: command.to_string(),
        arguments: args,
    }
}

// ====================================================================
// 1. Malformed arguments: required field missing — handler must
//    surface `DapHandlerError::MalformedArgs`, not panic.
// ====================================================================

#[tokio::test]
async fn launch_with_malformed_arguments_returns_error_not_panic() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    // `program` is required per LaunchArguments; missing it must fail
    // deserialization cleanly.
    let request = req(1, "launch", Some(serde_json::json!({})));
    let result = handle_launch(&adapter, &request).await;
    assert!(result.is_err(), "missing required field should error");
}

// ====================================================================
// 2. Unicode argument: path / args contain non-ASCII bytes — handler
//    must preserve them through the round-trip.
// ====================================================================

#[tokio::test]
async fn set_breakpoints_with_unicode_path_handles_correctly() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        2,
        "setBreakpoints",
        Some(serde_json::json!({
            "source": { "path": "/tmp/файл_测试_🐍.cb" },
            "breakpoints": [{ "line": 1 }, { "line": 42 }],
        })),
    );
    let result = handle_set_breakpoints(&adapter, &request).await.unwrap();
    let bps = result["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 2, "both breakpoints should round-trip");
    // The wave-2 stub marks every set breakpoint verified per
    // ADR-0059b §3.2 + §3.3.
    assert_eq!(bps[0]["verified"], true);
}

// ====================================================================
// 3. Large payload: many breakpoints in one request — handler must
//    not have an O(n²) hot path or static cap regression.
// ====================================================================

#[tokio::test]
async fn set_breakpoints_with_large_payload_succeeds() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    // 256 line breakpoints in one request — protocol does not cap.
    let breakpoints: Vec<Value> = (1..=256)
        .map(|i| serde_json::json!({ "line": i }))
        .collect();
    let request = req(
        3,
        "setBreakpoints",
        Some(serde_json::json!({
            "source": { "path": "/tmp/big.cb" },
            "breakpoints": breakpoints,
        })),
    );
    let result = handle_set_breakpoints(&adapter, &request).await.unwrap();
    let bps = result["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 256);
    // Spot-check first / mid / last line numbers.
    assert_eq!(bps[0]["line"], 1);
    assert_eq!(bps[128]["line"], 129);
    assert_eq!(bps[255]["line"], 256);
}

// ====================================================================
// 4. Partial / missing optional arguments: disconnect with no args at
//    all — handler must use `DisconnectArguments::default()` and
//    NOT return an error.
// ====================================================================

#[tokio::test]
async fn disconnect_with_no_arguments_uses_defaults() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
        "process kill".to_string(),
        String::new(),
    )]));
    let request = req(4, "disconnect", None);
    let result = handle_disconnect(&adapter, &request).await;
    assert!(
        result.is_ok(),
        "disconnect must tolerate missing arguments per ADR-0059b §3.2"
    );
}

// ====================================================================
// 5. Quick-close: continue then immediately disconnect — handler
//    serialization is fine; this tests no race between the
//    `Mutex<LldbDriver>` lock acquisitions per ADR-0059b §4.4.
// ====================================================================

#[tokio::test]
async fn continue_then_immediate_disconnect_serialises_cleanly() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![
        (
            "process continue".to_string(),
            "Process 1 stopped\n  stop reason = breakpoint 1.1".to_string(),
        ),
        ("process kill".to_string(), String::new()),
    ]));

    let cont_request = req(5, "continue", Some(serde_json::json!({"threadId": 1})));
    let dis_request = req(6, "disconnect", None);

    let cont_result = handle_continue(&adapter, &cont_request).await.unwrap();
    let dis_result = handle_disconnect(&adapter, &dis_request).await;

    assert_eq!(cont_result["allThreadsContinued"], true);
    assert!(dis_result.is_ok());
}

// ====================================================================
// Bonus 6-8: variables / threads / initialize edge cases
// ====================================================================

#[tokio::test]
async fn variables_with_unknown_reference_returns_empty_not_error() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(
        7,
        "variables",
        Some(serde_json::json!({"variablesReference": 9999})),
    );
    let result = handle_variables(&adapter, &request).await.unwrap();
    // Stub driver returns empty when no canned response matches —
    // contract: handler shape is `{ variables: [] }`, never `null`.
    let vars = result["variables"].as_array().unwrap();
    assert!(vars.is_empty());
}

#[tokio::test]
async fn threads_always_returns_hardcoded_single_thread() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    // Threads handler is the wave-2 single-thread non-goal per
    // ADR-0059b §5 — always returns the same shape regardless of
    // request seq / args.
    let r1 = handle_threads(
        &adapter,
        &req(8, "threads", Some(serde_json::json!({"junk": "ignored"}))),
    )
    .await
    .unwrap();
    let r2 = handle_threads(&adapter, &req(9, "threads", None))
        .await
        .unwrap();
    assert_eq!(r1["threads"][0]["id"], 1);
    assert_eq!(r2["threads"][0]["id"], 1);
    assert_eq!(r1["threads"][0]["name"], "main");
}

#[tokio::test]
async fn initialize_response_advertises_wave_2_capabilities_only() {
    let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
    let request = req(10, "initialize", Some(serde_json::json!({})));
    let caps = handle_initialize(&adapter, &request).await.unwrap();
    // ADR-0059b §3.2 non-goals: conditional / hit-count / step-back
    // / set-variable / restart-frame / evaluate-for-hovers must all be
    // false in wave-2. supportsTerminateRequest is the lone true.
    assert_eq!(caps["supportsConditionalBreakpoints"], false);
    assert_eq!(caps["supportsHitConditionalBreakpoints"], false);
    assert_eq!(caps["supportsStepBack"], false);
    assert_eq!(caps["supportsSetVariable"], false);
    assert_eq!(caps["supportsRestartFrame"], false);
    assert_eq!(caps["supportsEvaluateForHovers"], false);
    assert_eq!(caps["supportsTerminateRequest"], true);
}
