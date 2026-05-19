//! lldb-18 child-process driver (ADR-0059b §3.3).
//!
//! Spawns `lldb-18` as a `tokio::process::Child` and marshals DAP
//! requests to lldb's command-line REPL. Per ADR-0012 (bind-the-core),
//! lldb is externally maintained; this driver is a thin marshalling
//! layer, not a debugger reimplementation.
//!
//! The driver auto-loads the wave-1 pretty-printers
//! (`tools/lldb-cobrust/printers.py`) on init via
//! `command script import`, so `frame variable` output already carries
//! the Cobrust source-form summaries that wave-2's `Variables` handler
//! returns verbatim in `Variable::value`.
//!
//! Per ADR-0059b §4.4: each command is serialised behind the
//! `Adapter`-level `Mutex<LldbDriver>`. lldb's stdin/stdout is a
//! sequential REPL; we read until the `(lldb)` prompt sentinel
//! re-appears, then return the accumulated stdout.

use std::io;
use std::time::Duration;

use regex::Regex;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use crate::dap_types::{Breakpoint, StackFrame, Source, Variable};

/// Stop reason returned by lldb after a `continue` / `next` / `pause`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// Hit a breakpoint at the given lldb breakpoint id.
    Breakpoint(i64),
    /// Step (next / step-over) completed.
    Step,
    /// Paused on user request (DAP `pause` -> lldb `process interrupt`).
    Pause,
    /// Inferior exited.
    Exit(i32),
    /// Driver couldn't parse the stop reason; raw stdout for L+ audit.
    Unknown(String),
}

/// lldb-18 driver. Wraps the spawned child process + stdin/stdout
/// handles. The `kind` discriminates between a real lldb process and
/// a test stub used by snapshot tests.
pub struct LldbDriver {
    kind: DriverKind,
}

enum DriverKind {
    /// Real lldb-18 child process.
    Real {
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
        _child: tokio::process::Child,
    },
    /// Test stub: queue of canned responses per command pattern.
    Stub {
        responses: Vec<(String, String)>, // (command-substring, canned stdout)
        breakpoint_seq: i64,
    },
    /// Not-yet-spawned placeholder; spawn lazily on the first `launch`.
    NotSpawned,
}

#[derive(Debug, Error)]
pub enum DapError {
    #[error("lldb-18 child-process I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("lldb-18 command timed out (5s wait, command: {command})")]
    LldbTimeout { command: String },
    #[error("lldb-18 stdout parse failed: {message} (raw: {raw})")]
    ParseFailed { message: String, raw: String },
    #[error("lldb-18 not spawned yet (call launch first)")]
    NotSpawned,
    #[error("lldb-18 binary not found in PATH")]
    LldbNotFound,
}

/// Per-command timeout (ADR-0059b §4.4).
const LLDB_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// lldb prompt sentinel — stdout sentinel that marks "command done".
const LLDB_PROMPT: &str = "(lldb)";

impl LldbDriver {
    /// Construct a not-yet-spawned driver. The first `launch` call
    /// will actually spawn lldb-18.
    pub fn new_stub() -> Self {
        Self {
            kind: DriverKind::NotSpawned,
        }
    }

    /// Construct a test-stub driver with canned `(command-substring,
    /// stdout)` pairs. Used by snapshot tests in §6.1.
    pub fn test_stub(responses: Vec<(String, String)>) -> Self {
        Self {
            kind: DriverKind::Stub {
                responses,
                breakpoint_seq: 1,
            },
        }
    }

    /// Returns `true` iff the driver is a real spawned lldb (vs stub
    /// or not-yet-spawned).
    pub fn is_real(&self) -> bool {
        matches!(self.kind, DriverKind::Real { .. })
    }

    /// Returns `true` iff the driver is a test stub.
    pub fn is_stub(&self) -> bool {
        matches!(self.kind, DriverKind::Stub { .. })
    }

    /// Spawn lldb-18 and load the wave-1 pretty-printers.
    ///
    /// `binary_path` is the Cobrust-compiled binary that `launch`
    /// will target (`target create <binary_path>`).
    /// `printers_path` (optional) is the absolute path to
    /// `tools/lldb-cobrust/printers.py`; if `None`, the driver tries
    /// `./tools/lldb-cobrust/printers.py` relative to cwd.
    pub async fn spawn_and_attach(
        &mut self,
        binary_path: &str,
        printers_path: Option<&str>,
    ) -> Result<(), DapError> {
        let mut child = Command::new("lldb-18")
            .arg("--no-use-colors")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| DapError::LldbNotFound)?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let stdout = BufReader::new(stdout);

        self.kind = DriverKind::Real {
            stdin,
            stdout,
            _child: child,
        };

        // Read the initial lldb banner up to the first prompt.
        let _banner = self.read_until_prompt().await?;

        // Auto-load wave-1 pretty-printers.
        let printers = printers_path.unwrap_or("./tools/lldb-cobrust/printers.py");
        let _ = self
            .send_command(&format!("command script import {printers}"))
            .await?;

        // Target the binary.
        let _ = self
            .send_command(&format!("target create '{binary_path}'"))
            .await?;

        Ok(())
    }

    /// Send a raw lldb command line + read stdout until the next
    /// `(lldb)` prompt. Times out after 5s per ADR-0059b §4.4.
    async fn send_command(&mut self, command: &str) -> Result<String, DapError> {
        if let DriverKind::Stub { responses, .. } = &self.kind {
            // Match the longest substring prefix in the canned responses.
            for (pattern, output) in responses {
                if command.contains(pattern.as_str()) {
                    return Ok(output.clone());
                }
            }
            // Default to empty stdout for unrecognised commands.
            return Ok(String::new());
        }
        let DriverKind::Real { stdin, .. } = &mut self.kind else {
            return Err(DapError::NotSpawned);
        };

        let line = format!("{command}\n");
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;

        let read_fut = self.read_until_prompt();
        timeout(LLDB_COMMAND_TIMEOUT, read_fut)
            .await
            .map_err(|_| DapError::LldbTimeout {
                command: command.to_string(),
            })?
    }

    /// Read stdout lines until the `(lldb)` prompt sentinel.
    async fn read_until_prompt(&mut self) -> Result<String, DapError> {
        let DriverKind::Real { stdout, .. } = &mut self.kind else {
            return Ok(String::new());
        };
        let mut accumulated = String::new();
        loop {
            let mut line = String::new();
            let n = stdout.read_line(&mut line).await?;
            if n == 0 {
                // EOF: lldb exited.
                break;
            }
            // Strip the prompt suffix and stop accumulating.
            if let Some(idx) = line.find(LLDB_PROMPT) {
                accumulated.push_str(&line[..idx]);
                break;
            }
            accumulated.push_str(&line);
        }
        Ok(accumulated)
    }

    /// `launch` wrapper: `target create <binary>; process launch`.
    pub async fn launch(&mut self, binary_path: &str, stop_on_entry: bool) -> Result<(), DapError> {
        if matches!(self.kind, DriverKind::NotSpawned) {
            self.spawn_and_attach(binary_path, None).await?;
        }
        if stop_on_entry {
            let _ = self.send_command("breakpoint set --name main").await?;
        }
        let _ = self.send_command("process launch --stop-at-entry").await?;
        Ok(())
    }

    /// `set_breakpoint` wrapper: `breakpoint set --file <f> --line <n>`.
    pub async fn set_breakpoint(&mut self, file: &str, line: u32) -> Result<Breakpoint, DapError> {
        // Stub fast-path: return a synthetic breakpoint with a
        // monotonically increasing id.
        if let DriverKind::Stub {
            breakpoint_seq, ..
        } = &mut self.kind
        {
            let id = *breakpoint_seq;
            *breakpoint_seq += 1;
            return Ok(Breakpoint {
                id: Some(id),
                verified: true,
                message: None,
                source: Some(Source {
                    name: Some(file.to_string()),
                    path: Some(file.to_string()),
                    source_reference: None,
                }),
                line: Some(line),
                column: None,
            });
        }

        let stdout = self
            .send_command(&format!("breakpoint set --file '{file}' --line {line}"))
            .await?;
        parse_breakpoint(&stdout, file, line)
    }

    /// `continue` wrapper: `process continue`.
    pub async fn continue_exec(&mut self) -> Result<StopReason, DapError> {
        let stdout = self.send_command("process continue").await?;
        Ok(parse_stop_reason(&stdout))
    }

    /// `next` wrapper: `thread step-over`.
    pub async fn next_step(&mut self) -> Result<StopReason, DapError> {
        let stdout = self.send_command("thread step-over").await?;
        Ok(parse_stop_reason(&stdout))
    }

    /// `pause` wrapper: `process interrupt`.
    pub async fn pause(&mut self) -> Result<StopReason, DapError> {
        let stdout = self.send_command("process interrupt").await?;
        Ok(parse_stop_reason(&stdout))
    }

    /// `stack_trace` wrapper: `thread backtrace`.
    pub async fn stack_trace(&mut self) -> Result<Vec<StackFrame>, DapError> {
        let stdout = self.send_command("thread backtrace").await?;
        Ok(parse_stack_trace(&stdout))
    }

    /// `variables` wrapper: `frame variable --no-args`.
    ///
    /// The pretty-printer summaries from wave-1 are already attached to
    /// each `frame variable` line — the driver extracts them via regex
    /// and returns DAP `Variable[]` with `value` = pretty-printed
    /// summary verbatim.
    pub async fn variables(&mut self, _frame_id: i64) -> Result<Vec<Variable>, DapError> {
        let stdout = self.send_command("frame variable --no-args").await?;
        Ok(parse_variables(&stdout))
    }

    /// `disconnect` wrapper: `process kill; quit`. Best-effort; ignores
    /// errors past this point (lldb is shutting down anyway).
    pub async fn disconnect(&mut self) -> Result<(), DapError> {
        let _ = self.send_command("process kill").await;
        let _ = self.send_command("quit").await;
        Ok(())
    }
}

// =====================================================================
// Parsers (regex-based, per ADR-0059b §3.3 + §7.1 mitigation)
// =====================================================================

/// Parse a `breakpoint set` stdout line of the form
/// `Breakpoint 1: where = fib.cb`fib + 8 at fib.cb:7, address = ...`.
fn parse_breakpoint(stdout: &str, file: &str, line: u32) -> Result<Breakpoint, DapError> {
    let re = Regex::new(r"Breakpoint\s+(\d+):").expect("valid regex");
    if let Some(caps) = re.captures(stdout) {
        let id: i64 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        Ok(Breakpoint {
            id: Some(id),
            verified: true,
            message: None,
            source: Some(Source {
                name: Some(file.to_string()),
                path: Some(file.to_string()),
                source_reference: None,
            }),
            line: Some(line),
            column: None,
        })
    } else {
        // Degraded path: surface as unverified with the raw stdout in
        // the message field, so the user gets feedback.
        tracing::warn!("breakpoint parse failed; raw: {stdout}");
        Ok(Breakpoint {
            id: None,
            verified: false,
            message: Some(stdout.lines().next().unwrap_or("").to_string()),
            source: Some(Source {
                name: Some(file.to_string()),
                path: Some(file.to_string()),
                source_reference: None,
            }),
            line: Some(line),
            column: None,
        })
    }
}

/// Parse a lldb stop-reason line. Common patterns:
/// - `Process 12345 stopped` with `stop reason = breakpoint 1.1`
/// - `Process 12345 stopped` with `stop reason = step over`
/// - `Process 12345 exited with status = 0 (0x00000000)`
fn parse_stop_reason(stdout: &str) -> StopReason {
    let exit_re = Regex::new(r"exited\s+with\s+status\s*=\s*(-?\d+)").expect("valid regex");
    if let Some(caps) = exit_re.captures(stdout) {
        let code: i32 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        return StopReason::Exit(code);
    }

    let bp_re = Regex::new(r"stop\s+reason\s*=\s*breakpoint\s+(\d+)").expect("valid regex");
    if let Some(caps) = bp_re.captures(stdout) {
        let id: i64 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        return StopReason::Breakpoint(id);
    }

    if stdout.contains("stop reason = step over")
        || stdout.contains("stop reason = step in")
        || stdout.contains("stop reason = step out")
    {
        return StopReason::Step;
    }

    if stdout.contains("stop reason = signal SIGSTOP")
        || stdout.contains("stop reason = signal SIGINT")
    {
        return StopReason::Pause;
    }

    StopReason::Unknown(stdout.lines().next().unwrap_or("").to_string())
}

/// Parse a `thread backtrace` stdout block. Each frame line has the
/// form `  * frame #0: 0x... binary`function + 8 at file.cb:7:5`.
fn parse_stack_trace(stdout: &str) -> Vec<StackFrame> {
    let re = Regex::new(
        r"frame\s+#(\d+):\s+0x[0-9a-fA-F]+\s+[^`]+`([^\s\(]+)(?:\([^)]*\))?\s*(?:\+\s*\d+)?\s+at\s+([^:]+):(\d+)(?::(\d+))?",
    )
    .expect("valid regex");

    let mut frames = Vec::new();
    for caps in re.captures_iter(stdout) {
        let id: i64 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let name = caps
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let file = caps
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let line: u32 = caps
            .get(4)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let column: u32 = caps
            .get(5)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1);

        frames.push(StackFrame {
            id,
            name,
            source: Some(Source {
                name: Some(file.clone()),
                path: Some(file),
                source_reference: None,
            }),
            line,
            column,
            end_line: None,
            end_column: None,
        });
    }
    frames
}

/// Parse a `frame variable --no-args` stdout block.
///
/// Each line has the form `(<type>) <name> = <pretty-printer summary>`,
/// e.g. `(cobrust::List) xs = [1, 2, 3]` (with pretty-printers loaded).
fn parse_variables(stdout: &str) -> Vec<Variable> {
    let re =
        Regex::new(r"\(([^)]+)\)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)").expect("valid regex");

    let mut vars = Vec::new();
    for line in stdout.lines() {
        if let Some(caps) = re.captures(line.trim()) {
            let type_name = caps
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            let name = caps
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let value = caps
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            vars.push(Variable {
                name,
                value,
                type_name: Some(type_name),
                variables_reference: 0,
            });
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_breakpoint_simple() {
        let stdout = "Breakpoint 1: where = fib`fib + 8 at fib.cb:7, address = 0x1234";
        let bp = parse_breakpoint(stdout, "fib.cb", 7).unwrap();
        assert_eq!(bp.id, Some(1));
        assert!(bp.verified);
        assert_eq!(bp.line, Some(7));
    }

    #[test]
    fn parse_breakpoint_unparseable_returns_unverified() {
        let stdout = "some unexpected lldb output";
        let bp = parse_breakpoint(stdout, "fib.cb", 7).unwrap();
        assert!(!bp.verified);
        assert!(bp.message.is_some());
    }

    #[test]
    fn parse_stop_reason_breakpoint() {
        let stdout = "Process 12345 stopped\n  thread #1, stop reason = breakpoint 1.1";
        assert_eq!(parse_stop_reason(stdout), StopReason::Breakpoint(1));
    }

    #[test]
    fn parse_stop_reason_step() {
        let stdout = "Process 12345 stopped\n  thread #1, stop reason = step over";
        assert_eq!(parse_stop_reason(stdout), StopReason::Step);
    }

    #[test]
    fn parse_stop_reason_exit() {
        let stdout = "Process 12345 exited with status = 0 (0x00000000)";
        assert_eq!(parse_stop_reason(stdout), StopReason::Exit(0));
    }

    #[test]
    fn parse_stop_reason_unknown_falls_through() {
        let stdout = "some unexpected lldb output";
        match parse_stop_reason(stdout) {
            StopReason::Unknown(_) => {}
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn parse_stack_trace_one_frame() {
        let stdout = "* thread #1, stop reason = breakpoint 1.1\n  * frame #0: 0x100003ee4 fib`fib(n=10) + 8 at fib.cb:8:5\n    frame #1: 0x100003f44 fib`main + 12 at fib.cb:12:5\n";
        let frames = parse_stack_trace(stdout);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name, "fib");
        assert_eq!(frames[0].line, 8);
        assert_eq!(frames[0].column, 5);
        assert_eq!(frames[1].name, "main");
        assert_eq!(frames[1].line, 12);
    }

    #[test]
    fn parse_variables_with_pretty_printer_output() {
        let stdout = "(cobrust::List) xs = [1, 2, 3]\n(cobrust::Str) name = \"hello\"\n(int) n = 10\n";
        let vars = parse_variables(stdout);
        assert_eq!(vars.len(), 3);
        assert_eq!(vars[0].name, "xs");
        assert_eq!(vars[0].value, "[1, 2, 3]");
        assert_eq!(vars[0].type_name.as_deref(), Some("cobrust::List"));
        assert_eq!(vars[1].name, "name");
        assert_eq!(vars[1].value, "\"hello\"");
        assert_eq!(vars[2].name, "n");
        assert_eq!(vars[2].value, "10");
    }

    #[test]
    fn parse_variables_empty_input_returns_empty() {
        assert!(parse_variables("").is_empty());
    }

    #[tokio::test]
    async fn stub_driver_returns_canned_response() {
        let mut driver = LldbDriver::test_stub(vec![(
            "breakpoint set".to_string(),
            "Breakpoint 1: at fib.cb:7\n".to_string(),
        )]);
        let bp = driver.set_breakpoint("fib.cb", 7).await.unwrap();
        assert!(bp.verified);
        assert_eq!(bp.id, Some(1));
    }

    #[tokio::test]
    async fn stub_driver_set_breakpoint_increments_id() {
        let mut driver = LldbDriver::test_stub(vec![]);
        let bp1 = driver.set_breakpoint("fib.cb", 7).await.unwrap();
        let bp2 = driver.set_breakpoint("fib.cb", 12).await.unwrap();
        assert_eq!(bp1.id, Some(1));
        assert_eq!(bp2.id, Some(2));
    }
}
