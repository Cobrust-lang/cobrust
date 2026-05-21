//! Serde round-trip tests for `cobrust-dap` DAP-protocol types
//! (Tier-2 CQ P0-2: cobrust-dap test density).
//!
//! Each test serializes a type to JSON, deserializes it back, and
//! verifies the round-trip equality (or, for camelCase fields, the
//! wire-format substring). The 10 covered types are the DAP wave-2
//! handler payload shapes per ADR-0059b §3.2.
//!
//! Per CLAUDE.md §5: `unwrap` is allowed in tests but the assertion
//! coverage stays explicit — every assertion verifies a single wire
//! invariant.

#![allow(clippy::unwrap_used, clippy::missing_panics_doc)]

use cobrust_dap::dap_types::{
    Breakpoint, ContinueArguments, ContinueResponse, DisconnectArguments, InitializeArguments,
    InitializeResponse, LaunchArguments, NextArguments, PauseArguments, Request, Response,
    SetBreakpointsArguments, SetBreakpointsResponse, Source, SourceBreakpoint, StackFrame,
    StackTraceArguments, StackTraceResponse, Variable, VariablesArguments, VariablesResponse,
};

// ====================================================================
// 1. Request / Response (base ProtocolMessage shapes)
// ====================================================================

#[test]
fn request_round_trip_preserves_arguments_value() {
    let req = Request {
        seq: 42,
        type_field: "request".to_string(),
        command: "setBreakpoints".to_string(),
        arguments: Some(serde_json::json!({"source": {"path": "/tmp/x.cb"}})),
    };
    let wire = serde_json::to_string(&req).unwrap();
    let back: Request = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.seq, 42);
    assert_eq!(back.command, "setBreakpoints");
    assert_eq!(back.type_field, "request");
    // arguments payload preserved verbatim
    assert_eq!(
        back.arguments.unwrap()["source"]["path"].as_str().unwrap(),
        "/tmp/x.cb"
    );
}

#[test]
fn response_round_trip_preserves_body_and_success_flag() {
    let resp = Response {
        seq: 100,
        type_field: "response".to_string(),
        request_seq: 42,
        success: false,
        command: "launch".to_string(),
        message: Some("file not found".to_string()),
        body: None,
    };
    let wire = serde_json::to_string(&resp).unwrap();
    let back: Response = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.seq, 100);
    assert_eq!(back.request_seq, 42);
    assert!(!back.success);
    assert_eq!(back.message.unwrap(), "file not found");
    assert!(back.body.is_none());
}

// ====================================================================
// 2. Initialize Arguments + Response (capabilities advertisement)
// ====================================================================

#[test]
fn initialize_arguments_round_trip_with_optional_fields() {
    // Per ADR-0059b §4.1 + dap_types.rs:48, the Cobrust DAP types use
    // serde `rename_all = "camelCase"` — `client_id` -> `clientId`
    // (Cobrust convention, not the DAP spec's `clientID` shape; the
    // wire-format mapper is the Cobrust derive macro).
    let json = r#"{
        "clientId": "vscode",
        "clientName": "Visual Studio Code",
        "adapterId": "cobrust",
        "pathFormat": "path",
        "linesStartAt1": true,
        "columnsStartAt1": true
    }"#;
    let args: InitializeArguments = serde_json::from_str(json).unwrap();
    assert_eq!(args.client_id.as_deref(), Some("vscode"));
    assert_eq!(args.adapter_id.as_deref(), Some("cobrust"));
    assert_eq!(args.lines_start_at1, Some(true));
}

#[test]
fn initialize_response_camel_case_emission_on_wire() {
    let resp = InitializeResponse {
        supports_configuration_done_request: true,
        supports_terminate_request: true,
        supports_set_variable: false,
        ..InitializeResponse::default()
    };
    let wire = serde_json::to_string(&resp).unwrap();
    // Wave-2 contract: DAP §"Capabilities" uses camelCase on the wire.
    assert!(wire.contains("\"supportsConfigurationDoneRequest\":true"));
    assert!(wire.contains("\"supportsTerminateRequest\":true"));
    assert!(wire.contains("\"supportsSetVariable\":false"));
    // Round-trip back through deserialization.
    let back: InitializeResponse = serde_json::from_str(&wire).unwrap();
    assert!(back.supports_configuration_done_request);
}

// ====================================================================
// 3. Launch Arguments (binary path + args + stop_on_entry)
// ====================================================================

#[test]
fn launch_arguments_round_trip_program_args_stop_on_entry() {
    let args = LaunchArguments {
        program: "/tmp/fib".to_string(),
        cwd: Some("/tmp".to_string()),
        args: vec!["--seed".to_string(), "42".to_string()],
        stop_on_entry: true,
    };
    let wire = serde_json::to_string(&args).unwrap();
    let back: LaunchArguments = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.program, "/tmp/fib");
    assert_eq!(back.cwd.unwrap(), "/tmp");
    assert_eq!(back.args, vec!["--seed".to_string(), "42".to_string()]);
    assert!(back.stop_on_entry);
}

// ====================================================================
// 4. SetBreakpoints Arguments + Response + Source + SourceBreakpoint
// ====================================================================

#[test]
fn set_breakpoints_arguments_round_trip_with_multiple_breakpoints() {
    let args = SetBreakpointsArguments {
        source: Source {
            name: Some("fib.cb".to_string()),
            path: Some("/tmp/fib.cb".to_string()),
            source_reference: None,
        },
        breakpoints: vec![
            SourceBreakpoint {
                line: 7,
                column: Some(1),
                condition: None,
            },
            SourceBreakpoint {
                line: 12,
                column: None,
                condition: Some("x > 10".to_string()),
            },
        ],
        source_modified: false,
    };
    let wire = serde_json::to_string(&args).unwrap();
    let back: SetBreakpointsArguments = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.breakpoints.len(), 2);
    assert_eq!(back.breakpoints[0].line, 7);
    assert_eq!(back.breakpoints[0].column, Some(1));
    // Conditional reads even though wave-2 doesn't honour it
    // (ADR-0059b §5 non-goal).
    assert_eq!(back.breakpoints[1].condition.as_deref(), Some("x > 10"));
}

#[test]
fn set_breakpoints_response_breakpoint_id_and_verified_round_trip() {
    let response = SetBreakpointsResponse {
        breakpoints: vec![Breakpoint {
            id: Some(1),
            verified: true,
            message: None,
            source: None,
            line: Some(7),
            column: None,
        }],
    };
    let wire = serde_json::to_string(&response).unwrap();
    let back: SetBreakpointsResponse = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.breakpoints.len(), 1);
    assert_eq!(back.breakpoints[0].id, Some(1));
    assert!(back.breakpoints[0].verified);
    assert_eq!(back.breakpoints[0].line, Some(7));
}

// ====================================================================
// 5. Continue / Next / Pause (execution-control args)
// ====================================================================

#[test]
fn continue_args_and_response_round_trip() {
    let args = ContinueArguments {
        thread_id: 1,
        single_thread: false,
    };
    let wire_args = serde_json::to_string(&args).unwrap();
    let back: ContinueArguments = serde_json::from_str(&wire_args).unwrap();
    assert_eq!(back.thread_id, 1);

    let resp = ContinueResponse {
        all_threads_continued: true,
    };
    let wire_resp = serde_json::to_string(&resp).unwrap();
    assert!(wire_resp.contains("\"allThreadsContinued\":true"));
}

#[test]
fn next_pause_args_round_trip_with_default_granularity() {
    let next = NextArguments {
        thread_id: 1,
        single_thread: false,
        granularity: Some("statement".to_string()),
    };
    let wire = serde_json::to_string(&next).unwrap();
    let back: NextArguments = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.thread_id, 1);
    assert_eq!(back.granularity.as_deref(), Some("statement"));

    let pause = PauseArguments { thread_id: 2 };
    let wire_pause = serde_json::to_string(&pause).unwrap();
    let back_pause: PauseArguments = serde_json::from_str(&wire_pause).unwrap();
    assert_eq!(back_pause.thread_id, 2);
}

// ====================================================================
// 6. StackTrace Arguments + Response + StackFrame
// ====================================================================

#[test]
fn stack_trace_args_and_response_round_trip_with_frames() {
    let args = StackTraceArguments {
        thread_id: 1,
        start_frame: 0,
        levels: Some(20),
    };
    let wire_args = serde_json::to_string(&args).unwrap();
    let back: StackTraceArguments = serde_json::from_str(&wire_args).unwrap();
    assert_eq!(back.thread_id, 1);
    assert_eq!(back.levels, Some(20));

    let resp = StackTraceResponse {
        stack_frames: vec![
            StackFrame {
                id: 0,
                name: "fib".to_string(),
                source: Some(Source {
                    name: Some("fib.cb".to_string()),
                    path: Some("/tmp/fib.cb".to_string()),
                    source_reference: None,
                }),
                line: 8,
                column: 5,
                end_line: None,
                end_column: None,
            },
            StackFrame {
                id: 1,
                name: "main".to_string(),
                source: None,
                line: 1,
                column: 1,
                end_line: None,
                end_column: None,
            },
        ],
        total_frames: Some(2),
    };
    let wire = serde_json::to_string(&resp).unwrap();
    let back_resp: StackTraceResponse = serde_json::from_str(&wire).unwrap();
    assert_eq!(back_resp.stack_frames.len(), 2);
    assert_eq!(back_resp.stack_frames[0].name, "fib");
    assert_eq!(back_resp.total_frames, Some(2));
    // camelCase wire emission for `stackFrames` per DAP §"StackFrame".
    assert!(wire.contains("\"stackFrames\""));
    assert!(wire.contains("\"totalFrames\":2"));
}

// ====================================================================
// 7. Variables Arguments + Response (pretty-printer payload carrier)
// ====================================================================

#[test]
fn variables_args_and_response_round_trip_with_cobrust_pretty_printed_values() {
    let args = VariablesArguments {
        variables_reference: 1000,
        filter: None,
        start: None,
        count: None,
    };
    let wire_args = serde_json::to_string(&args).unwrap();
    let back: VariablesArguments = serde_json::from_str(&wire_args).unwrap();
    assert_eq!(back.variables_reference, 1000);

    // Wave-2 contract: `Variable::value` is the wave-1 pretty-printer
    // summary verbatim. The serde shape must preserve it byte-for-byte.
    let resp = VariablesResponse {
        variables: vec![
            Variable {
                name: "xs".to_string(),
                value: "[1, 2, 3]".to_string(),
                type_name: Some("cobrust::List".to_string()),
                variables_reference: 0,
            },
            Variable {
                name: "name".to_string(),
                value: "\"hello\"".to_string(),
                type_name: Some("cobrust::Str".to_string()),
                variables_reference: 0,
            },
            Variable {
                name: "d".to_string(),
                value: "{1: \"a\"}".to_string(),
                type_name: Some("cobrust::Dict".to_string()),
                variables_reference: 2001,
            },
        ],
    };
    let wire = serde_json::to_string(&resp).unwrap();
    let back_resp: VariablesResponse = serde_json::from_str(&wire).unwrap();
    assert_eq!(back_resp.variables.len(), 3);
    assert_eq!(back_resp.variables[0].value, "[1, 2, 3]");
    assert_eq!(
        back_resp.variables[1].type_name.as_deref(),
        Some("cobrust::Str")
    );
    // DAP wire format uses `type` not `type_name`.
    assert!(wire.contains("\"type\":\"cobrust::Str\""));
    // variables_reference for drill-in must round-trip exact.
    assert_eq!(back_resp.variables[2].variables_reference, 2001);
}

// ====================================================================
// 8. Disconnect Arguments (optional-field defaults)
// ====================================================================

#[test]
fn disconnect_arguments_round_trip_with_all_defaults() {
    let args = DisconnectArguments {
        restart: false,
        terminate_debuggee: true,
        suspend_debuggee: false,
    };
    let wire = serde_json::to_string(&args).unwrap();
    let back: DisconnectArguments = serde_json::from_str(&wire).unwrap();
    assert!(!back.restart);
    assert!(back.terminate_debuggee);
    assert!(!back.suspend_debuggee);

    // Per ADR-0059b §3.2: disconnect's arguments may be entirely absent.
    let empty: DisconnectArguments = serde_json::from_str("{}").unwrap();
    assert!(!empty.restart);
    assert!(!empty.terminate_debuggee);
}
