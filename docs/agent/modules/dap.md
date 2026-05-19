---
module_id: dap
last_verified_commit: feature/0059b-dev
milestone: L.wave2
dependencies:
  - tools/lldb-cobrust/printers.py             # wave-1 pretty-printers
  - tools/lldb-cobrust/.lldbinit               # auto-load snippet
  - crates/cobrust-frontend/src/lib.rs         # parse_str (held for future source-line mapping)
  - crates/cobrust-types/src/check.rs          # TypeCheckCtx (held for future evaluate handler)
adr:
  - 0059   # Phase L frame
  - 0059b  # Wave-2 DAP server (this module)
  - 0059a  # Wave-1 pretty-printers (consumed via lldb auto-load)
  - 0058c  # DWARF v5 emission (prerequisite gate)
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
| `handle_initialize`, `handle_launch`, … (9 handlers) | `crates/cobrust-dap/src/handlers.rs` | per-command async dispatcher |
| `Request`, `Response`, `InitializeResponse`, … | `crates/cobrust-dap/src/dap_types.rs` | hand-rolled DAP type structs |

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
| `threads` | (stub) | Single hardcoded `{ id: 1, name: "main" }` per §5. |

## lldb-18 stdout parser surface (per ADR-0059b §3.3)

Regex-based parsers in `crates/cobrust-dap/src/lldb_driver.rs`:

| Parser | Input shape | Output |
|---|---|---|
| `parse_breakpoint` | `Breakpoint 1: where = fib.cb`fib + 8 at fib.cb:7, ...` | `Breakpoint { id: Some(1), verified: true, line: Some(7), ... }` |
| `parse_stop_reason` | `stop reason = breakpoint 1.1` / `stop reason = step over` / `exited with status = 0` | `StopReason::{Breakpoint, Step, Exit, Pause, Unknown}` |
| `parse_stack_trace` | `frame #0: 0x... fib`fib + 8 at fib.cb:8:5` | `Vec<StackFrame>` |
| `parse_variables` | `(cobrust::List) xs = [1, 2, 3]` | `Vec<Variable>` with `value = "[1, 2, 3]"` + `type_name = Some("cobrust::List")` |

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
- `cargo test -p cobrust-dap` PASS for 28 tests (22 unit + 5 snapshot
  + 1 ignored e2e).
- 5 snapshot tests in `tests/dap_handler_snapshots.rs` lock the wire
  shape for: Initialize, SetBreakpoints, Continue, StackTrace,
  Variables (with wave-1 pretty-printer output).
- 1 e2e smoke (`#[ignore]`-gated) in `tests/dap_e2e_smoke.rs` covers
  the stdio handshake: Initialize → response shape check → Disconnect.
  Run on DG via `cargo test -p cobrust-dap -- --ignored`.
- ADR-0059b status flips `proposed → accepted` with
  `last_verified_commit` after merge.

## Non-goals (wave-2)

- No conditional breakpoints (`SourceBreakpoint::condition` ignored).
- No `evaluate` request (returns `"<not supported in wave-2>"`).
- No multi-thread debug (single-thread hardcoded).
- No `attach` mode (only `launch`).
- No `setVariable` request (read-only inspection).
- No reverse step / time-travel.
- No `loadedSources` enumeration.
- No generic Adt pretty-print (Phase L+ wave; user-defined `class`
  structs render as raw lldb struct dumps).

## See also

- ADR-0059 — Phase L frame.
- ADR-0059a — wave-1 pretty-printers (consumed via lldb auto-load).
- ADR-0059b — this wave-2 spec.
- ADR-0058c — DWARF v5 emission (prerequisite gate).
- ADR-0012 — bind-the-core (lldb is externally maintained).
- `docs/human/{zh,en}/editor-setup.md` — user-facing setup guide.
- `docs/agent/modules/lsp.md` — sibling `cobrust-lsp` module (mirror shape).
