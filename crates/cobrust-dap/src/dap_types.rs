//! Hand-rolled DAP protocol type structs (ADR-0059b §4.1).
//!
//! Per ADR-0059b §4.1, wave-2 ships hand-rolled DAP types rather than
//! adopting the `dap` crate from crates.io (sparse maintenance + API
//! churn vs lldb-18 pinning). The shapes below cover the 9 wave-2
//! handlers; future waves extend the surface.
//!
//! All shapes mirror the DAP §"Specification" v1.55 JSON wire format.
//! Optional fields use `#[serde(skip_serializing_if = "Option::is_none")]`
//! so editor-side parsers see clean JSON without missing-field noise.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// DAP base `ProtocolMessage` request. The `command` field discriminates
/// the request variant; `arguments` is parsed per-command from a
/// `serde_json::Value` to keep this enum-free.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Request {
    pub seq: i64,
    #[serde(rename = "type")]
    pub type_field: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// DAP base `ProtocolMessage` response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Response {
    pub seq: i64,
    #[serde(rename = "type")]
    pub type_field: String,
    pub request_seq: i64,
    pub success: bool,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

// ====================================================================
// Initialize
// ====================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeArguments {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_start_at1: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns_start_at1: Option<bool>,
}

/// DAP `Capabilities` response body for `initialize`. Wave-2 only
/// advertises the load-bearing capabilities (per ADR-0059b §3.2).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    /// The debug adapter supports the `configurationDone` request.
    /// Wave-2: false (no pre-launch config phase).
    pub supports_configuration_done_request: bool,
    /// The debug adapter supports function-level breakpoints. Wave-2:
    /// false (line breakpoints only).
    pub supports_function_breakpoints: bool,
    /// The debug adapter supports conditional breakpoints. Wave-2:
    /// false (per ADR-0059b §5 non-goal).
    pub supports_conditional_breakpoints: bool,
    /// The debug adapter supports hit-count breakpoints. Wave-2: false.
    pub supports_hit_conditional_breakpoints: bool,
    /// The debug adapter supports the `evaluate` request. Wave-2:
    /// false (per ADR-0059b §5 non-goal).
    pub supports_evaluate_for_hovers: bool,
    /// The debug adapter supports stepping back. Wave-2: false.
    pub supports_step_back: bool,
    /// The debug adapter supports `setVariable`. Wave-2: false.
    pub supports_set_variable: bool,
    /// The debug adapter supports restarting a frame. Wave-2: false.
    pub supports_restart_frame: bool,
    /// The debug adapter supports the `terminate` request. Wave-2:
    /// true (graceful disconnect ends the lldb child).
    pub supports_terminate_request: bool,
    /// Per ADR-0059f §3.4, the exception-breakpoint filters the
    /// adapter advertises. Wave-4 emits three: panic / result_err /
    /// unreachable. Wave-2..3 emit an empty list (skip-serializing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception_breakpoint_filters: Vec<ExceptionBreakpointsFilter>,
}

// ====================================================================
// Launch
// ====================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchArguments {
    /// Path to the Cobrust-compiled binary (the `cobrust build --debug`
    /// output). Required.
    pub program: String,
    /// Working directory for the launched binary. Defaults to the
    /// directory containing `program`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Command-line args passed to the launched binary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Stop on entry (break at `main` before any user code runs).
    /// Wave-2: honoured.
    #[serde(default)]
    pub stop_on_entry: bool,
}

// ====================================================================
// SetBreakpoints
// ====================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsArguments {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<SourceBreakpoint>,
    #[serde(default)]
    pub source_modified: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    /// Conditional expression. Wave-2 reads but does NOT honour this
    /// field (per ADR-0059b §5 non-goal). Phase L+ wave-3 wires it
    /// through to lldb's `--condition` arg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsResponse {
    pub breakpoints: Vec<Breakpoint>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

// ====================================================================
// Continue / Next / Pause
// ====================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueArguments {
    pub thread_id: i64,
    #[serde(default)]
    pub single_thread: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueResponse {
    pub all_threads_continued: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextArguments {
    pub thread_id: i64,
    #[serde(default)]
    pub single_thread: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granularity: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseArguments {
    pub thread_id: i64,
}

// ====================================================================
// StackTrace
// ====================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceArguments {
    pub thread_id: i64,
    #[serde(default)]
    pub start_frame: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceResponse {
    pub stack_frames: Vec<StackFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_frames: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    pub line: u32,
    pub column: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

// ====================================================================
// Variables
// ====================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesArguments {
    /// Reference to the variable container (frame's scope, struct,
    /// array, …). Wave-2 only handles frame-scope references.
    pub variables_reference: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesResponse {
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    /// Display value — for Cobrust types this is the wave-1
    /// pretty-printer summary verbatim (e.g. `"[1, 2, 3]"`,
    /// `"\"hello\""`, `"{1: \"a\"}"`).
    pub value: String,
    /// DWARF type name (e.g. `"cobrust::List"`, `"cobrust::Str"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    /// Reference for further drill-in (0 = leaf, no children).
    pub variables_reference: i64,
}

// ====================================================================
// Disconnect
// ====================================================================

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectArguments {
    #[serde(default)]
    pub restart: bool,
    #[serde(default)]
    pub terminate_debuggee: bool,
    #[serde(default)]
    pub suspend_debuggee: bool,
}

// ====================================================================
// Evaluate (wave-4 ADR-0059f §3.1)
// ====================================================================

/// DAP `evaluate` request arguments.
///
/// Per ADR-0059f §3.1, wave-4 routes `expression` verbatim to lldb's
/// REPL via the `expression` command. `context` selects the caller
/// surface ("watch" / "repl" / "hover" / "clipboard") but wave-4
/// treats all four identically.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateArguments {
    /// The expression source string to evaluate. Routed verbatim to
    /// lldb's REPL — Cobrust syntax that coincides with C
    /// (arithmetic, comparisons, field access via `.`, array indexing
    /// via `[]`) works passthrough.
    pub expression: String,
    /// Optional frame id selecting which stack frame's locals are in
    /// scope. `None` means the currently selected frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<i64>,
    /// The caller context per DAP spec: `"watch" | "repl" | "hover" |
    /// "clipboard"`. Wave-4 ignores this discriminant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// DAP `evaluate` response body.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResponse {
    /// The evaluation result — lldb's stdout summary verbatim (already
    /// wave-1 pretty-printer-formatted where applicable).
    pub result: String,
    /// DWARF type name parsed from lldb's `(<type>) $N = …` prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    /// Reference for drill-in (0 = leaf, no children). Wave-4 always
    /// emits 0 — drill-in via `variables` request remains scoped to
    /// frame-locals per wave-2 surface.
    pub variables_reference: i64,
}

// ====================================================================
// Threads (wave-4 ADR-0059f §3.3 — replaces wave-2 hardcoded stub)
// ====================================================================

/// DAP `threads` response body — the list of OS threads currently
/// stopped under lldb's control.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsResponse {
    pub threads: Vec<ThreadInfo>,
}

/// DAP `Thread` per the spec.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    pub id: i64,
    pub name: String,
}

// ====================================================================
// SetExceptionBreakpoints (wave-4 ADR-0059f §3.4)
// ====================================================================

/// DAP `setExceptionBreakpoints` arguments.
///
/// Per ADR-0059f §3.4, wave-4 advertises three filters in
/// `InitializeResponse.exception_breakpoint_filters`: `"panic"`,
/// `"result_err"`, `"unreachable"`. Editors send back the subset the
/// user enabled.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetExceptionBreakpointsArguments {
    /// The enabled filter identifiers. Wave-4: `"panic"` /
    /// `"result_err"` / `"unreachable"` are recognised; others are
    /// silently ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<String>,
}

/// DAP `setExceptionBreakpoints` response body.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetExceptionBreakpointsResponse {
    /// Per-filter `Breakpoint` records mirroring the input order.
    /// `verified: false` when the underlying lldb symbol is
    /// unavailable (e.g. `result_err` filter in builds where the
    /// runtime symbol is not emitted — honest-scope-skip per
    /// ADR-0059f §3.4).
    pub breakpoints: Vec<Breakpoint>,
}

/// Capability advertisement entry for an exception breakpoint filter.
/// Wave-4 advertises three.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionBreakpointsFilter {
    /// Stable identifier (e.g. `"panic"`).
    pub filter: String,
    /// Human-readable label for the editor UI.
    pub label: String,
    /// Default-on (per `InitializeResponse` advertisement).
    #[serde(default)]
    pub default: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::missing_panics_doc)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_json() {
        let req = Request {
            seq: 1,
            type_field: "request".to_string(),
            command: "initialize".to_string(),
            arguments: Some(serde_json::json!({"clientID": "vscode"})),
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back.command, "initialize");
        assert_eq!(back.seq, 1);
    }

    #[test]
    fn response_round_trips_json() {
        let resp = Response {
            seq: 2,
            type_field: "response".to_string(),
            request_seq: 1,
            success: true,
            command: "initialize".to_string(),
            message: None,
            body: Some(serde_json::json!({"supportsConfigurationDoneRequest": false})),
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&s).unwrap();
        assert!(back.success);
        assert_eq!(back.command, "initialize");
    }

    #[test]
    fn initialize_response_serializes_camel_case() {
        let resp = InitializeResponse {
            supports_configuration_done_request: false,
            supports_terminate_request: true,
            ..InitializeResponse::default()
        };
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"supportsConfigurationDoneRequest\":false"));
        assert!(s.contains("\"supportsTerminateRequest\":true"));
    }

    #[test]
    fn stack_frame_serializes_camel_case() {
        let f = StackFrame {
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
        };
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("\"line\":8"));
        assert!(s.contains("\"column\":5"));
    }

    #[test]
    fn variable_serializes_cobrust_form() {
        let v = Variable {
            name: "xs".to_string(),
            value: "[1, 2, 3]".to_string(),
            type_name: Some("cobrust::List".to_string()),
            variables_reference: 0,
        };
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("\"value\":\"[1, 2, 3]\""));
        assert!(s.contains("\"type\":\"cobrust::List\""));
    }

    #[test]
    fn source_breakpoint_optional_condition_reads_without_failure() {
        let json = r#"{"line": 7}"#;
        let bp: SourceBreakpoint = serde_json::from_str(json).unwrap();
        assert_eq!(bp.line, 7);
        assert!(bp.condition.is_none());
    }
}
