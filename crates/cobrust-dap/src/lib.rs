//! `cobrust-dap` — Cobrust Debug Adapter Protocol (DAP) implementation.
//!
//! Phase L wave-2 (ADR-0059b) — bridges Phase L wave-1 lldb pretty-printers
//! (`tools/lldb-cobrust/printers.py`) to editor-side step-debug. Wave-2
//! ships a stdio DAP server with 9 handlers (Initialize / Launch /
//! SetBreakpoints / Continue / Next / Pause / StackTrace / Variables /
//! Disconnect) backed by a `tokio::process::Command::new("lldb-18")` child
//! process. The driver auto-loads the wave-1 pretty-printers on init so
//! `Variables` responses surface Cobrust source-form values verbatim
//! (`xs: List<Int> = [1, 2, 3]`, not raw struct bytes).
//!
//! Per ADR-0012 (bind-the-core): lldb-18 is externally maintained; this
//! crate marshals DAP <-> lldb without re-implementing either.
//!
//! Public surface (wave-2):
//! - [`Adapter`] — the top-level DAP request dispatcher.
//! - [`LldbDriver`] — lldb-18 child-process driver.
//! - [`run_stdio_loop`] — DAP stdio framing loop (Content-Length framed).
//! - DAP type structs in [`dap_types`].
//! - Handlers in [`handlers`].
//!
//! Wave-3+ extends this surface to conditional breakpoints, attach mode,
//! and source-level expression `evaluate` per ADR-0059b §5 non-goals.

#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::unused_async)] // Some DAP handlers are simple capability advertisements; trait-shape consistency wins.
#![allow(clippy::unnecessary_wraps)] // Parsers return Result for symmetry with driver Result chain.
#![allow(clippy::cast_possible_truncation)] // DAP wire shapes use u32/i64; controlled inputs.
#![allow(clippy::cast_possible_wrap)] // Same reasoning as truncation.
#![allow(clippy::struct_excessive_bools)] // DAP InitializeResponse is wire-defined with N supports_* bools.
#![allow(clippy::large_enum_variant)] // DriverKind variants differ in size by design (Real has child handles).

pub mod dap_types;
pub mod evaluate;
pub mod handlers;
pub mod lldb_driver;

use std::io;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

pub use dap_types::{
    Breakpoint, ContinueArguments, ContinueResponse, DataBreakpoint, DisconnectArguments,
    EvaluateArguments, EvaluateResponse, ExceptionBreakpointsFilter, InitializeArguments,
    InitializeResponse, LaunchArguments, NextArguments, PauseArguments, Request, Response,
    SetBreakpointsArguments, SetBreakpointsResponse, SetDataBreakpointsArguments,
    SetDataBreakpointsResponse, SetExceptionBreakpointsArguments, SetExceptionBreakpointsResponse,
    Source, SourceBreakpoint, StackFrame, StackTraceArguments, StackTraceResponse, StepInArguments,
    ThreadInfo, ThreadsResponse, Variable, VariablesArguments, VariablesResponse,
};
pub use handlers::DapHandlers;
pub use lldb_driver::{LldbDriver, StopReason};

/// Run the Cobrust DAP server over stdio.
///
/// ADR-0068 §4.1: unified entry point for the `cobrust dap` subcommand
/// (`crates/cobrust-cli/src/dap.rs`). (ADR-0070 X.5 deleted the
/// transitional `cobrust-dap` shim binary at v0.7.0; the subcommand is now
/// the sole caller.)
/// Builds a tokio runtime, starts a tracing subscriber that writes to
/// stderr (DAP stdout is reserved for Content-Length framed JSON), and
/// runs [`run_stdio_loop`] with a fresh [`Adapter`] until the client
/// sends a `disconnect` request or closes stdin.
///
/// # Errors
///
/// Returns the underlying tokio runtime build error if the multi-thread
/// runtime cannot be created, or any I/O error surfaced by the stdio
/// loop (typically `BrokenPipe` on abnormal client exit).
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_writer(std::io::stderr)
            .init();

        let adapter = Adapter::new();
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        run_stdio_loop(adapter, stdin, stdout).await
    })?;
    Ok(())
}

/// The Cobrust DAP `Adapter`.
///
/// Routes incoming DAP requests through the 9 wave-2 handlers (per
/// ADR-0059b §3.2) and dispatches lldb commands via [`LldbDriver`].
/// The adapter holds the driver behind a `Mutex` so concurrent
/// requests serialise on lldb's sequential REPL (lldb's stdin/stdout
/// is not a request/response demux; one command at a time).
pub struct Adapter {
    /// The lldb-18 child-process driver. Wrapped in `Arc<Mutex<...>>`
    /// so the stdio reader/writer tasks can share access.
    driver: Arc<Mutex<LldbDriver>>,
    /// Monotonically increasing sequence number for outbound DAP
    /// responses and events (per DAP `ProtocolMessage.seq`).
    seq: Arc<Mutex<i64>>,
}

impl Adapter {
    /// Construct a new `Adapter` with a stub driver (lldb is spawned
    /// lazily on the first `Launch` request).
    pub fn new() -> Self {
        Self {
            driver: Arc::new(Mutex::new(LldbDriver::new_stub())),
            seq: Arc::new(Mutex::new(1)),
        }
    }

    /// Construct an `Adapter` wrapping a pre-built `LldbDriver` (used
    /// by snapshot tests that pass a stub driver returning canned
    /// stdout).
    #[must_use]
    pub fn with_driver(driver: LldbDriver) -> Self {
        Self {
            driver: Arc::new(Mutex::new(driver)),
            seq: Arc::new(Mutex::new(1)),
        }
    }

    /// Allocate the next outbound `seq` value.
    pub async fn next_seq(&self) -> i64 {
        let mut s = self.seq.lock().await;
        let v = *s;
        *s += 1;
        v
    }

    /// Borrow the inner driver (mutex-guarded; serialised access).
    pub fn driver(&self) -> Arc<Mutex<LldbDriver>> {
        Arc::clone(&self.driver)
    }
}

impl Default for Adapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the DAP stdio loop: read framed JSON requests from `stdin`,
/// dispatch through `adapter`, write framed JSON responses to `stdout`.
///
/// Per DAP §"Base Protocol", every message is framed as
/// `Content-Length: N\r\n\r\n<JSON body of length N>`. This loop reads
/// the header lines until blank, then reads `N` bytes of body.
///
/// Returns `Ok(())` on graceful client disconnect (EOF on stdin) or
/// on a `Disconnect` DAP request.
pub async fn run_stdio_loop<R, W>(adapter: Adapter, stdin: R, mut stdout: W) -> io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(stdin);
    loop {
        // Parse the Content-Length: N header(s).
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                // EOF: client disconnected. Graceful exit.
                return Ok(());
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break; // End of headers
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                let n: usize = rest.trim().parse().unwrap_or(0);
                content_length = Some(n);
            }
            // Other headers (Content-Type, …) are ignored.
        }
        let Some(len) = content_length else {
            // Malformed frame: no Content-Length header. Bail out.
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "missing Content-Length header",
            ));
        };
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).await?;

        let request: Request = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("malformed DAP request JSON: {e}");
                continue;
            }
        };

        // Dispatch.
        let is_disconnect = request.command == "disconnect";
        let response = adapter.dispatch(&request).await;
        write_response(&mut stdout, &response).await?;
        if is_disconnect {
            return Ok(());
        }
    }
}

/// Write a DAP `Response` to `stdout` with Content-Length framing.
pub async fn write_response<W>(stdout: &mut W, response: &Response) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(response)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdout.write_all(header.as_bytes()).await?;
    stdout.write_all(&body).await?;
    stdout.flush().await?;
    Ok(())
}

impl Adapter {
    /// Dispatch a DAP request through the appropriate handler.
    ///
    /// Per ADR-0059b §3.2, wave-2 supports 9 commands. Unknown
    /// commands receive a `success: false` response with an error
    /// message; the protocol continues.
    pub async fn dispatch(&self, request: &Request) -> Response {
        let seq = self.next_seq().await;
        let result = match request.command.as_str() {
            "initialize" => handlers::handle_initialize(self, request).await,
            "launch" => handlers::handle_launch(self, request).await,
            "setBreakpoints" => handlers::handle_set_breakpoints(self, request).await,
            "continue" => handlers::handle_continue(self, request).await,
            "next" => handlers::handle_next(self, request).await,
            "pause" => handlers::handle_pause(self, request).await,
            "stackTrace" => handlers::handle_stack_trace(self, request).await,
            "variables" => handlers::handle_variables(self, request).await,
            "disconnect" => handlers::handle_disconnect(self, request).await,
            "threads" => handlers::handle_threads(self, request).await,
            "evaluate" => evaluate::handle_evaluate(self, request).await,
            "setExceptionBreakpoints" => {
                handlers::handle_set_exception_breakpoints(self, request).await
            }
            "setDataBreakpoints" => handlers::handle_set_data_breakpoints(self, request).await,
            "stepIn" => handlers::handle_step_in(self, request).await,
            other => {
                tracing::info!("unsupported DAP command (wave-2 scope): {other}");
                Ok(serde_json::json!({
                    "message": format!("command '{other}' not implemented in wave-2"),
                }))
            }
        };
        match result {
            Ok(body) => Response {
                seq,
                type_field: "response".to_string(),
                request_seq: request.seq,
                success: true,
                command: request.command.clone(),
                message: None,
                body: Some(body),
            },
            Err(err) => Response {
                seq,
                type_field: "response".to_string(),
                request_seq: request.seq,
                success: false,
                command: request.command.clone(),
                message: Some(err.to_string()),
                body: None,
            },
        }
    }
}
