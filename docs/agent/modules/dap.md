---
module_id: dap
last_verified_commit: feature/0059g-wave-5
milestone: L.wave5
dependencies:
  - tools/lldb-cobrust/printers.py             # wave-1 pretty-printers
  - tools/lldb-cobrust/.lldbinit               # auto-load snippet
  - crates/cobrust-frontend/src/lib.rs         # parse_str (held for future source-line mapping)
  - crates/cobrust-types/src/check.rs          # TypeCheckCtx (held for future Cobrust-source evaluator)
  - crates/cobrust-stdlib/src/panic.rs         # wave-5: __cobrust_result_err_panic hookable symbol (ADR-0059g §3.4)
adr:
  - 0059   # Phase L frame
  - 0059b  # Wave-2 DAP server (this module's wave-2 foundation)
  - 0059a  # Wave-1 pretty-printers (consumed via lldb auto-load)
  - 0059e  # Phase L §6.1 wave-4 str-runtime + frame-variable + closure (post-wave-3 v1.0 closure)
  - 0059f  # Phase L wave-4 watch + conditional bp + multi-thread + exception bp (v1.1)
  - 0059g  # Phase L wave-5 logpoints + dataBP + stepIn + result_err RESOLVED (v1.2)
  - 0058c  # DWARF v5 emission (prerequisite gate)
  - 0028   # Structured-concurrency runtime (multi-thread debugger consumer)
  - 0012   # Bind-the-core (lldb is externally maintained)
---

# cobrust-dap

## Purpose

Cobrust Debug Adapter Protocol (DAP) server.

Wave-2 (ADR-0059b) ships a stdio DAP server with 9 request handlers
covering single-thread step-debug. The crate spawns `lldb-18` as a
child process + auto-loads the wave-1 pretty-printers (per
ADR-0059a) so editor-side `Variables` views surface Cobrust
source-form values verbatim. Wave-3+ extends to conditional
breakpoints, attach mode, and source-level `evaluate` per ADR-0059b
§5 non-goals.

## Public surface

| Item | Anchor | Kind |
|---|---|---|
| `Adapter` | `crates/cobrust-dap/src/lib.rs::Adapter` | struct (DAP request dispatcher) |
| `Adapter::new() -> Self` | `crates/cobrust-dap/src/lib.rs::Adapter::new` | constructor (lazy lldb spawn) |
| `Adapter::with_driver(LldbDriver) -> Self` | `crates/cobrust-dap/src/lib.rs::Adapter::with_driver` | constructor (test stub) |
| `Adapter::dispatch(&Request) -> Response` | `crates/cobrust-dap/src/lib.rs::Adapter::dispatch` | request routing |
| `run_stdio_loop(Adapter, R, W) -> io::Result<()>` | `crates/cobrust-dap/src/lib.rs::run_stdio_loop` | Content-Length-framed stdio loop |
| `LldbDriver` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver` | struct (lldb-18 child-process driver) |
| `LldbDriver::new_stub()` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::new_stub` | not-yet-spawned constructor |
| `LldbDriver::test_stub(Vec<(String, String)>)` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::test_stub` | snapshot-test stub |
| `LldbDriver::launch(&str, bool) -> Result<(), DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::launch` | spawn + target binary |
| `LldbDriver::set_breakpoint(&str, u32) -> Result<Breakpoint, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::set_breakpoint` | `breakpoint set --file --line` |
| `LldbDriver::continue_exec() -> Result<StopReason, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::continue_exec` | `process continue` |
| `LldbDriver::next_step() -> Result<StopReason, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::next_step` | `thread step-over` |
| `LldbDriver::stack_trace() -> Result<Vec<StackFrame>, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::stack_trace` | `thread backtrace` |
| `LldbDriver::variables(i64) -> Result<Vec<Variable>, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::variables` | `frame variable --no-args` |
| `LldbDriver::evaluate(&str, Option<i64>) -> Result<(String, Option<String>), DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::evaluate` | wave-4 §3.1: `expression --` REPL routing |
| `LldbDriver::set_conditional_breakpoint(&str, u32, &str) -> Result<Breakpoint, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::set_conditional_breakpoint` | wave-4 §3.2: `--condition '<expr>'` wiring |
| `LldbDriver::list_threads() -> Result<Vec<ThreadInfo>, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::list_threads` | wave-4 §3.3: `thread list` parser |
| `LldbDriver::stack_trace_for_thread(i64) -> Result<Vec<StackFrame>, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::stack_trace_for_thread` | wave-4 §3.3: per-thread backtrace |
| `LldbDriver::set_exception_breakpoint(&str) -> Result<Breakpoint, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::set_exception_breakpoint` | wave-4 §3.4: per-filter symbol map; wave-5 result_err → `__cobrust_result_err_panic` |
| `LldbDriver::set_log_breakpoint(&str, u32, &str) -> Result<Breakpoint, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::set_log_breakpoint` | wave-5 §3.1: logpoints via `--auto-continue 1` + `breakpoint command add` |
| `LldbDriver::set_watchpoint(&str, Option<&str>) -> Result<Breakpoint, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::set_watchpoint` | wave-5 §3.2: data breakpoints via `watchpoint set variable -w <access> <var>` |
| `LldbDriver::step_in(i64) -> Result<StopReason, DapError>` | `crates/cobrust-dap/src/lldb_driver.rs::LldbDriver::step_in` | wave-5 §3.3: `thread step-in` + Cobrust-source preference (step-out if landing in non-`.cb`) |
| `handle_initialize`, `handle_launch`, … (13 handlers + evaluate sibling) | `crates/cobrust-dap/src/{handlers.rs,evaluate.rs}` | per-command async dispatcher; wave-5 adds `handle_set_data_breakpoints` + `handle_step_in` |
| `Request`, `Response`, `InitializeResponse`, `EvaluateArguments`, `EvaluateResponse`, `ThreadInfo`, `ThreadsResponse`, `SetExceptionBreakpoints{Arguments,Response}`, `ExceptionBreakpointsFilter`, `SetDataBreakpoints{Arguments,Response}`, `DataBreakpoint`, `StepInArguments`, … | `crates/cobrust-dap/src/dap_types.rs` | hand-rolled DAP type structs; wave-5 adds dataBP + stepIn shapes + 3 capability flags (`supports_log_points` / `supports_data_breakpoints` / `supports_step_in_targets_request`) |

## DAP request → lldb command mapping (per ADR-0059b §3.2)

| DAP request | lldb command | Notes |
|---|---|---|
| `initialize` | (none; capability advertisement) | Returns `Capabilities` JSON with wave-2 supports_* flags. |
| `launch` | `target create '<binary>'; process launch --stop-at-entry` | Wave-2 lazily spawns lldb on the first launch. Pretty-printers auto-loaded via `command script import` before this step. |
| `setBreakpoints` | `breakpoint set --file '<file>' --line <line>` | Per-line wrapper; `condition` field on `SourceBreakpoint` is read but NOT honoured (per ADR-0059b §5). |
| `continue` | `process continue` | Returns `allThreadsContinued: true` (single-thread per §5). |
| `next` | `thread step-over` | Returns empty body; client polls `StoppedEvent`. |
| `pause` | `process interrupt` | Returns empty body. |
| `stackTrace` | `thread backtrace` | Regex-parsed; returns `stackFrames: [{ id, name, source, line, column }, ...]`. |
| `variables` | `frame variable --no-args` | Wave-1 pretty-printer summaries pass through verbatim in `Variable::value`. |
| `disconnect` | `process kill; quit` | Best-effort lldb cleanup. |
| `threads` | `thread list` | wave-4: per-thread `{id, name}` from regex-parsed lldb stdout; empty-result falls back to single-thread shim. |
| `evaluate` | `frame select N; expression -- <expr>` | wave-4 ADR-0059f §3.1; `expression` routed verbatim to lldb. |
| `setExceptionBreakpoints` | `breakpoint set --name <symbol>` per filter | wave-4 ADR-0059f §3.4; 3 filters: panic / result_err / unreachable. **wave-5 ADR-0059g §3.4**: result_err symbol updated to `__cobrust_result_err_panic` (RESOLVED). |
| `setBreakpoints` (logpoint variant) | `breakpoint set --file '<f>' --line N --auto-continue 1; breakpoint command add --script-type python -o 'print(...)'` | wave-5 ADR-0059g §3.1; routed when `SourceBreakpoint.logMessage` is `Some(...)`. |
| `setDataBreakpoints` | `watchpoint set variable -w <access> <var>` per entry | wave-5 ADR-0059g §3.2; access types: read / write / read_write. Honest scope: stack-resident value-semantic locals only. |
| `stepIn` | `thread select N; thread step-in` + optional `thread step-out` if landing outside `.cb` source | wave-5 ADR-0059g §3.3; Cobrust-source preference, targetId parsed but ignored. |

Wave-4 extends `setBreakpoints` to honour each `SourceBreakpoint`'s
`condition` field via `LldbDriver::set_conditional_breakpoint`:
`breakpoint set --file '<file>' --line <line> --condition '<expr>'`.
Wave-4 extends `stackTrace` to call `LldbDriver::stack_trace_for_thread(thread_id)`
which prefixes the `thread backtrace` with `thread select <thread_id>`.

## lldb-18 stdout parser surface (per ADR-0059b §3.3)

Regex-based parsers in `crates/cobrust-dap/src/lldb_driver.rs`:

| Parser | Input shape | Output |
|---|---|---|
| `parse_breakpoint` | `Breakpoint 1: where = fib.cb`fib + 8 at fib.cb:7, ...` | `Breakpoint { id: Some(1), verified: true, line: Some(7), ... }` |
| `parse_stop_reason` | `stop reason = breakpoint 1.1` / `stop reason = step over` / `exited with status = 0` | `StopReason::{Breakpoint, Step, Exit, Pause, Unknown}` |
| `parse_stack_trace` | `frame #0: 0x... fib`fib + 8 at fib.cb:8:5` | `Vec<StackFrame>` |
| `parse_variables` | `(cobrust::List) xs = [1, 2, 3]` | `Vec<Variable>` with `value = "[1, 2, 3]"` + `type_name = Some("cobrust::List")` |
| `parse_evaluate` | `(int) $0 = 42` or unrecognised fall-through | `(value_text, Option<type_name>)` |
| `parse_threads` | `thread #N: tid = ..., name = '<name>'` | `Vec<ThreadInfo { id, name }>`; missing name → `thread-N` |

Each parser has a unit-test in the same file covering both the happy
path + a degraded path (unparseable input → unverified breakpoint or
`StopReason::Unknown`). Per ADR-0059b §7.1 mitigation: parse-fail
emits `tracing::warn!` + returns degraded-but-functional output; the
DAP loop does NOT crash.

## Pipeline dispatch

```text
Editor (Cursor/VSCode) -> Content-Length framed JSON -> cobrust-dap stdin
                                                              │
                                                              ▼
                                                       Adapter::dispatch
                                                              │
                                                              ▼
                                                   handle_<command>(adapter, request)
                                                              │
                                                              ▼
                                                     LldbDriver::<method>
                                                              │
                                                              ▼
                                              lldb-18 child stdin (one line per command)
                                                              │
                                                              ▼
                                              lldb-18 child stdout (until "(lldb)" prompt)
                                                              │
                                                              ▼
                                                     Regex parse → DAP type
                                                              │
                                                              ▼
                                            Content-Length framed JSON -> cobrust-dap stdout
                                                              │
                                                              ▼
                                                      Editor (Variables pane)
```

## Done means

- `cargo check -p cobrust-dap` exits 0 on Mac single-crate scope.
- `cargo test -p cobrust-dap` PASS for 99 tests (27 lib + 8 + 5 + 12 +
  5 + 22 wave-4 + 20 wave-5) + 2 ignored (lldb-18-spawn-gated e2e).
- 5 wave-2 snapshot tests + 6 wave-4 snapshot tests + 6 wave-5
  snapshot tests in
  `tests/{dap_handler_snapshots.rs,wave_4_dap_e2e.rs,wave_5_dap_e2e.rs}`
  lock the wire shape across the v1.2 surface.
- 2 e2e smokes (`#[ignore]`-gated) in `tests/dap_e2e_smoke.rs` +
  `tests/lldb_driver_integration_e2e.rs` cover the stdio handshake
  + real-lldb integration; run on a host with `lldb-18` on PATH via
  `cargo test -p cobrust-dap -- --ignored`.
- ADR-0059g status flips `proposed → accepted` with
  `last_verified_commit` after merge.

## Non-goals (post-wave-5)

- No `setVariable` request (read-only inspection; Cobrust ownership
  makes mid-step rewrite semantically fraught per ADR-0059 §4).
- No `attach` mode (only `launch`).
- No reverse step / time-travel.
- No `loadedSources` enumeration.
- No generic Adt pretty-print for user-defined `class` structs
  (deferred Phase L+ wave; renders as raw lldb struct dumps today).
- No source-level Cobrust expression evaluator (`evaluate` routes
  expressions verbatim to lldb's C-like parser; `match` /
  comprehensions / generic-function calls in watch are out-of-scope
  per ADR-0059 §4).
- No logpoints (log-only bp) or data breakpoints (memory watchpoints);
  deferred wave-5+ per ADR-0059f §4.

## See also

- ADR-0059 — Phase L frame.
- ADR-0059a — wave-1 pretty-printers (consumed via lldb auto-load).
- ADR-0059b — this wave-2 spec.
- ADR-0058c — DWARF v5 emission (prerequisite gate).
- ADR-0012 — bind-the-core (lldb is externally maintained).
- `docs/human/{zh,en}/editor-setup.md` — user-facing setup guide.
- `docs/agent/modules/lsp.md` — sibling `cobrust-lsp` module (mirror shape).
