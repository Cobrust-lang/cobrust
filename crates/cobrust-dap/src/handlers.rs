//! DAP request handlers (ADR-0059b §3.2).
//!
//! 9 handlers covering the wave-2 surface: Initialize / Launch /
//! SetBreakpoints / Continue / Next / Pause / StackTrace / Variables /
//! Disconnect. Plus a stub `threads` handler that returns a hardcoded
//! single-thread response per ADR-0059b §5 non-goal.
//!
//! Each handler returns `Result<serde_json::Value, DapHandlerError>`;
//! the `Adapter::dispatch` wrapper turns the result into a DAP
//! `Response` with `success: true/false` accordingly.

use serde_json::Value;
use thiserror::Error;

use crate::Adapter;
use crate::dap_types::{
    Breakpoint, ContinueArguments, ContinueResponse, DisconnectArguments, InitializeResponse,
    LaunchArguments, NextArguments, PauseArguments, Request, SetBreakpointsArguments,
    SetBreakpointsResponse, StackTraceArguments, StackTraceResponse, VariablesArguments,
    VariablesResponse,
};
use crate::lldb_driver::DapError;

#[derive(Debug, Error)]
pub enum DapHandlerError {
    #[error("malformed DAP arguments: {0}")]
    MalformedArgs(#[from] serde_json::Error),
    #[error("missing required DAP arguments for command '{0}'")]
    MissingArgs(String),
    #[error("lldb driver error: {0}")]
    LldbDriver(#[from] DapError),
}

/// Marker trait — handlers are grouped under `DapHandlers` for
/// organisational clarity; the module-level functions are the
/// canonical surface.
pub struct DapHandlers;

/// Parse the `arguments` field of a DAP request into a typed struct.
fn parse_args<T: serde::de::DeserializeOwned>(request: &Request) -> Result<T, DapHandlerError> {
    let args = request
        .arguments
        .as_ref()
        .ok_or_else(|| DapHandlerError::MissingArgs(request.command.clone()))?;
    Ok(serde_json::from_value(args.clone())?)
}

// =====================================================================
// Initialize
// =====================================================================

/// Handle the `initialize` DAP request.
///
/// Returns the wave-2 capabilities advertisement. Per ADR-0059b §3.2,
/// the bare-minimum capabilities; most "supports..." flags are false.
pub async fn handle_initialize(
    _adapter: &Adapter,
    _request: &Request,
) -> Result<Value, DapHandlerError> {
    let capabilities = InitializeResponse {
        supports_configuration_done_request: false,
        supports_function_breakpoints: false,
        supports_conditional_breakpoints: false,
        supports_hit_conditional_breakpoints: false,
        supports_evaluate_for_hovers: false,
        supports_step_back: false,
        supports_set_variable: false,
        supports_restart_frame: false,
        supports_terminate_request: true,
    };
    Ok(serde_json::to_value(capabilities)?)
}

// =====================================================================
// Launch
// =====================================================================

/// Handle the `launch` DAP request.
///
/// Spawns lldb-18 + auto-loads wave-1 pretty-printers + targets the
/// user-supplied binary. Per ADR-0059b §5 non-goal, only `launch` is
/// supported (not `attach`).
pub async fn handle_launch(adapter: &Adapter, request: &Request) -> Result<Value, DapHandlerError> {
    let args: LaunchArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    driver.launch(&args.program, args.stop_on_entry).await?;
    Ok(serde_json::json!({}))
}

// =====================================================================
// SetBreakpoints
// =====================================================================

/// Handle the `setBreakpoints` DAP request.
///
/// Sets line breakpoints in `source.path`. Per ADR-0059b §5, the
/// `condition` field on each `SourceBreakpoint` is read but NOT
/// honoured (wave-3+ wires it through to lldb).
pub async fn handle_set_breakpoints(
    adapter: &Adapter,
    request: &Request,
) -> Result<Value, DapHandlerError> {
    let args: SetBreakpointsArguments = parse_args(request)?;
    let file = args
        .source
        .path
        .as_deref()
        .or(args.source.name.as_deref())
        .unwrap_or("<unknown>");

    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let mut breakpoints: Vec<Breakpoint> = Vec::with_capacity(args.breakpoints.len());
    for src_bp in args.breakpoints {
        let bp = driver.set_breakpoint(file, src_bp.line).await?;
        breakpoints.push(bp);
    }
    let response = SetBreakpointsResponse { breakpoints };
    Ok(serde_json::to_value(response)?)
}

// =====================================================================
// Continue
// =====================================================================

/// Handle the `continue` DAP request.
///
/// Resumes the inferior. Per ADR-0059b §5 single-thread non-goal,
/// `allThreadsContinued: true` is hardcoded.
pub async fn handle_continue(
    adapter: &Adapter,
    request: &Request,
) -> Result<Value, DapHandlerError> {
    let _args: ContinueArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let _stop = driver.continue_exec().await?;
    let response = ContinueResponse {
        all_threads_continued: true,
    };
    Ok(serde_json::to_value(response)?)
}

// =====================================================================
// Next (step-over)
// =====================================================================

/// Handle the `next` DAP request (step-over).
pub async fn handle_next(adapter: &Adapter, request: &Request) -> Result<Value, DapHandlerError> {
    let _args: NextArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let _stop = driver.next_step().await?;
    Ok(serde_json::json!({}))
}

// =====================================================================
// Pause
// =====================================================================

/// Handle the `pause` DAP request.
pub async fn handle_pause(adapter: &Adapter, request: &Request) -> Result<Value, DapHandlerError> {
    let _args: PauseArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let _stop = driver.pause().await?;
    Ok(serde_json::json!({}))
}

// =====================================================================
// StackTrace
// =====================================================================

/// Handle the `stackTrace` DAP request.
///
/// Returns the current call stack. Per ADR-0059b §5 single-thread
/// non-goal, the `threadId` argument is ignored.
pub async fn handle_stack_trace(
    adapter: &Adapter,
    request: &Request,
) -> Result<Value, DapHandlerError> {
    let _args: StackTraceArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let frames = driver.stack_trace().await?;
    let total = frames.len() as u32;
    let response = StackTraceResponse {
        stack_frames: frames,
        total_frames: Some(total),
    };
    Ok(serde_json::to_value(response)?)
}

// =====================================================================
// Variables
// =====================================================================

/// Handle the `variables` DAP request.
///
/// Returns the locals at the requested frame. Per ADR-0059b §3.2 +
/// §6.1 #4, `Variable::value` carries the wave-1 pretty-printer
/// summary verbatim.
pub async fn handle_variables(
    adapter: &Adapter,
    request: &Request,
) -> Result<Value, DapHandlerError> {
    let args: VariablesArguments = parse_args(request)?;
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    let variables = driver.variables(args.variables_reference).await?;
    let response = VariablesResponse { variables };
    Ok(serde_json::to_value(response)?)
}

// =====================================================================
// Disconnect
// =====================================================================

/// Handle the `disconnect` DAP request. Graceful lldb shutdown.
pub async fn handle_disconnect(
    adapter: &Adapter,
    request: &Request,
) -> Result<Value, DapHandlerError> {
    // disconnect's arguments are all optional, so missing is fine.
    let _args: DisconnectArguments = request
        .arguments
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let driver_arc = adapter.driver();
    let mut driver = driver_arc.lock().await;
    driver.disconnect().await?;
    Ok(serde_json::json!({}))
}

// =====================================================================
// Threads (stub — single-thread per ADR-0059b §5)
// =====================================================================

/// Handle the `threads` DAP request. Wave-2 single-thread non-goal:
/// returns one hardcoded `{ id: 1, name: "main" }`.
pub async fn handle_threads(
    _adapter: &Adapter,
    _request: &Request,
) -> Result<Value, DapHandlerError> {
    Ok(serde_json::json!({
        "threads": [{
            "id": 1,
            "name": "main",
        }],
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::missing_panics_doc)]
mod tests {
    use super::*;
    use crate::lldb_driver::LldbDriver;

    fn req(seq: i64, command: &str, args: Option<Value>) -> Request {
        Request {
            seq,
            type_field: "request".to_string(),
            command: command.to_string(),
            arguments: args,
        }
    }

    #[tokio::test]
    async fn initialize_returns_wave_2_capabilities() {
        let adapter = Adapter::new();
        let request = req(
            1,
            "initialize",
            Some(serde_json::json!({"clientID": "vscode"})),
        );
        let result = handle_initialize(&adapter, &request).await.unwrap();
        assert_eq!(result["supportsConfigurationDoneRequest"], false);
        assert_eq!(result["supportsTerminateRequest"], true);
        assert_eq!(result["supportsConditionalBreakpoints"], false);
    }

    #[tokio::test]
    async fn threads_returns_single_main_thread() {
        let adapter = Adapter::new();
        let request = req(2, "threads", None);
        let result = handle_threads(&adapter, &request).await.unwrap();
        assert_eq!(result["threads"][0]["id"], 1);
        assert_eq!(result["threads"][0]["name"], "main");
    }

    #[tokio::test]
    async fn set_breakpoints_uses_driver() {
        let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
        let request = req(
            3,
            "setBreakpoints",
            Some(serde_json::json!({
                "source": { "path": "fib.cb" },
                "breakpoints": [{ "line": 7 }, { "line": 12 }],
            })),
        );
        let result = handle_set_breakpoints(&adapter, &request).await.unwrap();
        let bps = result["breakpoints"].as_array().unwrap();
        assert_eq!(bps.len(), 2);
        assert_eq!(bps[0]["verified"], true);
        assert_eq!(bps[0]["line"], 7);
        assert_eq!(bps[1]["line"], 12);
    }

    #[tokio::test]
    async fn continue_returns_all_threads_continued() {
        let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![(
            "process continue".to_string(),
            "Process 12345 stopped\n  stop reason = breakpoint 1.1".to_string(),
        )]));
        let request = req(4, "continue", Some(serde_json::json!({"threadId": 1})));
        let result = handle_continue(&adapter, &request).await.unwrap();
        assert_eq!(result["allThreadsContinued"], true);
    }

    #[tokio::test]
    async fn stack_trace_with_stub_returns_empty_when_no_canned_response() {
        let adapter = Adapter::with_driver(LldbDriver::test_stub(vec![]));
        let request = req(5, "stackTrace", Some(serde_json::json!({"threadId": 1})));
        let result = handle_stack_trace(&adapter, &request).await.unwrap();
        assert_eq!(result["totalFrames"], 0);
    }
}
