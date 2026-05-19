---
doc_kind: adr
adr_id: 0059b
parent_adr: 0059
title: "Phase L wave-2 — DAP server crate (`cobrust-dap`)"
status: proposed
date: 2026-05-19
last_verified_commit: 1629550
supersedes: []
superseded_by: []
relates_to: [adr:0059, adr:0059a, adr:0057, adr:0057a, adr:0058c, adr:0012]
discovered_by: ADR-0059 §3.2 wave-2 row; sub-ADR roster dispatch
ratification_path: P9 sub-ADR review under ADR-0059 frame; ratifies on impl merge
---

# ADR-0059b: Phase L wave-2 — DAP server crate (`cobrust-dap`)

## 1. Motivation

ADR-0059 §3.2 frames Phase L wave-2 as "VSCode / Cursor users live in
DAP, not raw lldb. Without a DAP server they cannot debug Cobrust
programs in their editor." Wave-1 (ADR-0059a, merged at HEAD `1629550`)
shipped `tools/lldb-cobrust/printers.py` — six pretty-printers translating
runtime layouts back to Cobrust source form. The printers attach to
`lldb-18` when a user runs `lldb-18 <binary>` interactively; what they
DON'T do is power the **editor-side step-debugger UX** Cursor / VSCode
users expect from "Run > Start Debugging".

The Debug Adapter Protocol (DAP) is the wire format VSCode + every
DAP-conforming editor (Cursor, Neovim DAP, Emacs DAP-mode, JetBrains
products, …) speak between IDE and debugger backend. Without a Cobrust
DAP server, the wave-1 pretty-printers are confined to terminal lldb;
editor users see no step-through, no breakpoint UI, no Variables pane.

Wave-2 closes this gap. The crate ships:

- A new `cobrust-dap` workspace crate (binary + library), stdio
  transport, mirroring the `cobrust-lsp` (ADR-0057a) shape.
- A child-process driver that spawns `lldb-18`, auto-loads the wave-1
  pretty-printers via `command script import`, and marshals DAP
  requests to lldb commands + parses lldb stdout into DAP responses.
- 9 DAP request handlers (Initialize / Launch / SetBreakpoints / Continue
  / Next / Pause / StackTrace / Variables / Disconnect) covering the
  load-bearing "single-thread step-debug" workflow.

Constitutional anchors:

- **CLAUDE.md §1** — "Python ergonomics, Rust safety"; editor-side
  step-debug is THE visible Python-ergonomics surface for any static
  language. Without it, Cursor users feel like they're debugging C.
- **CLAUDE.md §2.5** — wave-2 §2.5 audit per §2 below.
- **ADR-0012** — bind-the-core. lldb is externally maintained; this
  crate marshals DAP <-> lldb without re-implementing either.
- **ADR-0059 §3.2** — wave-2 scope binding.
- **ADR-0057a** — `cobrust-lsp` stdio shape mirrored byte-for-byte:
  `tokio::main` + stdin/stdout + a tower-style server impl.

## 2. §2.5 LLM-first design audit

Wave-2 inherits ADR-0059's "rank-5, §2.5-low" framing but pulls in
two non-trivial positives the bare pretty-printers don't:

| §2.5 axis | Wave-2 impact | Rationale |
|---|---|---|
| §A compile-time-catch-errors | Neutral | DAP exposes runtime state; type/borrow checks happen earlier. |
| §B training-data-overlap | **Positive** | DAP `Variables` responses arrive at the agent as canonical JSON shapes (`{ name, value, type, variablesReference }`). The agent reading a Cursor "Run > Start Debugging" session sees the SAME shape it reads in every Python `debugpy` / Node `vscode-js-debug` / Rust `rust-analyzer` debug session — a heavily-trained-on protocol. Cobrust matches this byte-for-byte through the lldb pretty-printer reflection. |
| §B locals-view = LLM-consumable | **Positive** | Wave-1 pretty-printers render `xs: List<Int> = [1, 2, 3]`; the DAP `Variables` response surfaces this string in `value` — verbatim Python `repr` shape the LLM recognises without prose-stripping. |

**Net**: wave-2 amplifies wave-1's §2.5 §B win by an order of
magnitude — pretty-printer output is now consumable from agent-LLM
running INSIDE Cursor, not just from a human reading a terminal lldb
log. Phase L stays rank-5 in ADR-0054's ROI ordering, but the
LLM-amplifier surface is materially larger than wave-1 alone.

The §A compile-time-catch axis is properly Phase J's territory (LSP
diagnostics). Wave-2 makes no claim there.

## 3. Scope — new crate, 9 handlers, lldb-18 child-process driver

### 3.1 New crate `cobrust-dap`

Workspace member at `crates/cobrust-dap/`:

```
crates/cobrust-dap/
├── Cargo.toml
├── src/
│   ├── main.rs           # stdio binary entrypoint (mirrors cobrust-lsp main.rs)
│   ├── lib.rs            # Adapter + handlers + driver re-exports
│   ├── handlers.rs       # 9 DAP request handlers
│   └── lldb_driver.rs    # lldb-18 child-process driver
└── tests/
    ├── dap_handler_snapshots.rs   # 5 snapshot tests (insta)
    └── dap_e2e_smoke.rs           # 1 end-to-end smoke test
```

Workspace root `Cargo.toml` `members = [...]` += `crates/cobrust-dap`.

### 3.2 9 DAP request handlers

| Handler | DAP request | Wave-2 surface |
|---|---|---|
| `handle_initialize` | `initialize` | Returns capabilities `{ supportsConfigurationDoneRequest: false, supportsFunctionBreakpoints: false, supportsConditionalBreakpoints: false }`. Wave-2 is the bare minimum. |
| `handle_launch` | `launch` | Spawns lldb-18 + auto-loads pretty-printers + targets the user-supplied binary path. NO `attach` (out of scope, wave-3+). |
| `handle_set_breakpoints` | `setBreakpoints` | Per-file breakpoint list; emits `BreakpointEvent::verified` per line that lldb resolved. |
| `handle_continue` | `continue` | `process continue` in lldb; emits `StoppedEvent::reason: "breakpoint"` on next hit. |
| `handle_next` | `next` | `thread step-over`; emits `StoppedEvent::reason: "step"`. |
| `handle_pause` | `pause` | `process interrupt`; emits `StoppedEvent::reason: "pause"`. |
| `handle_stack_trace` | `stackTrace` | `thread backtrace`; parses lldb stack frames + emits DAP `StackFrame[]` with `id`, `name`, `source`, `line`, `column`. |
| `handle_variables` | `variables` | `frame variable --no-args` at frame N; uses wave-1 pretty-printer summaries verbatim for `Variable::value`. |
| `handle_disconnect` | `disconnect` | `process kill` + `quit` in lldb, then graceful child-process exit. |

### 3.3 Child-process driver (NOT lldb-rs binding)

ADR-0012 (bind-the-core): lldb is externally maintained. Wave-2
spawns lldb-18 as a child process via `tokio::process::Command` and
marshals DAP requests to lldb's command-line REPL. This avoids the
lldb-rs Rust binding (`lldb-sys` crate; `lldb-rs` higher-level wrapper)
because:

- The lldb-rs API churns across LLVM major versions (ADR-0059 §5.1).
- The lldb-rs build chain requires linking against system lldb,
  introducing platform-specific build fragility (macOS Homebrew lldb
  vs Linux apt lldb-18 vs <self-hosted-runner> custom-built llvm-18).
- The child-process boundary gives clean error isolation: a lldb
  crash terminates the child, not the cobrust-dap server.

Trade-off: parsing lldb stdout is fragile vs structured FFI. We
mitigate via:

- Pin lldb-18 specifically (per ADR-0058 §4 LLVM-version pin).
- Regex-based parser with named-capture groups + per-pattern
  unit tests.
- Debug-log on parse-failure: emit `tracing::warn!` with the
  unparseable line; fall back to "raw lldb stdout in `value` field"
  so the user gets degraded-but-functional output.

## 4. Implementation

### 4.1 `Cargo.toml`

```toml
[package]
name = "cobrust-dap"
version = "0.3.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Cobrust DAP (Debug Adapter Protocol) server (wave-2 per ADR-0059b)."
keywords = ["cobrust", "dap", "debug-adapter", "lldb"]
categories = ["development-tools::debugging"]

[[bin]]
name = "cobrust-dap"
path = "src/main.rs"

[lib]
path = "src/lib.rs"

[lints]
workspace = true

[dependencies]
cobrust-frontend = { path = "../cobrust-frontend", version = "=0.3.0" }
cobrust-types = { path = "../cobrust-types", version = "=0.3.0" }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { version = "1.40", features = ["macros", "rt-multi-thread", "fs", "io-util", "io-std", "sync", "time", "process"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
regex = "1"
thiserror = { workspace = true }
```

NOTE on the `dap` crate from crates.io: an initial review of available
DAP types crates (`dap = 0.4`) showed API churn + sparse maintenance.
Wave-2 ships **hand-rolled** DAP type structs (Serde-derived) in
`src/dap_types.rs`. The DAP protocol's request/response shapes are
stable JSON; a hand-rolled subset for the 9 wave-2 handlers (~150 LOC)
is lower-risk than adopting an unstable upstream crate that may not
pin lldb-18-compatible variants.

### 4.2 Protocol framing

LSP and DAP share the "Content-Length: N\r\n\r\n<JSON body>" framing.
Wave-2 reuses the same `tokio::io::AsyncBufReadExt` line-by-line
header parser pattern `cobrust-lsp` already validates (see
`crates/cobrust-lsp/src/main.rs::main`). Stdio binary entrypoint
shape:

```rust
#[tokio::main]
async fn main() {
    // tracing-subscriber init (stderr only)
    // Adapter::new()
    // run_stdio_loop(adapter, stdin, stdout).await
}
```

### 4.3 Command routing

```text
DAP Request (stdin JSON)
    │
    ▼
Adapter::handle(request) -> Response
    │
    ▼
LldbDriver::<request_method>(args) -> Result<DriverResponse, DapError>
    │
    ▼
write to lldb child stdin: e.g. "breakpoint set --file fib.cb --line 7\n"
    │
    ▼
read lldb child stdout: e.g. "Breakpoint 1: where = fib.cb`fib + 8 at fib.cb:7"
    │
    ▼
regex parse -> BreakpointId(1) -> DAP SetBreakpointsResponse with Breakpoint { id: 1, verified: true, line: 7 }
    │
    ▼
Response (stdout JSON, written by tokio AsyncWriteExt)
```

### 4.4 Synchronization & lifecycle

- `LldbDriver` owns the `tokio::process::Child` + a `tokio::sync::Mutex<()>`
  that serialises every command (lldb's stdin/stdout is a sequential
  REPL, not a request/response demux).
- Each command writes one line + reads stdout lines until lldb's
  `(lldb)` prompt re-appears (sentinel parse).
- Timeout per command: 5s. Past 5s, surface a `DapError::LldbTimeout`
  and continue serving; the user's next DAP request gets a clean
  response.

## 5. Non-goals (wave-2; later waves may revisit)

Wave-2 explicitly DOES NOT ship:

- **Conditional breakpoints**: `setBreakpoints[].condition` field is
  read from the DAP request but NOT honoured. Wave-3+ task to wire
  through to lldb's `--condition` arg. (ADR-0059 §3.2 noted this.)
- **Source-level expression watch**: `evaluate` DAP request is NOT
  implemented. Wave-2 returns `{ result: "<not supported in wave-2>",
  variablesReference: 0 }`. Phase L+ wave when a Cobrust source-level
  evaluator inside lldb is feasible.
- **Multi-thread debug**: single-thread Cobrust programs only.
  `threads` request returns a single hardcoded `{ id: 1, name: "main" }`.
  Cobrust programs today are single-threaded (ADR-0019 §"deferred to
  Phase F"); multi-thread debug is post-Phase L.
- **Attach mode**: `attach` request returns `{ success: false }`. Only
  `launch` (spawn a fresh binary) is supported. Attach to existing
  process is a Phase L+ extension.
- **Source loading**: `loadedSources` request returns `[]`. The wave-2
  driver doesn't index user source files at startup; lldb does it
  per-breakpoint via its `breakpoint set --file` resolution.
- **Reverse step / time-travel**: out of scope per ADR-0059 §4 wave-1
  non-goals; same applies here.
- **Generic Adt printing**: pretty-printers from wave-1 cover Str /
  List / Dict / Set / Tuple / Option only. User-defined `class`
  structs render as raw lldb struct dumps; wave-2 surfaces this as-is.

## 6. Acceptance gate

Wave-2 acceptance bundles 5 snapshot tests + 1 e2e smoke:

### 6.1 Snapshot tests (5; via `insta`)

In `crates/cobrust-dap/tests/dap_handler_snapshots.rs`:

1. **`snapshot_initialize_response`** — Initialize request →
   InitializeResponse JSON shape with capabilities `{
   supportsConfigurationDoneRequest: false, ... }`.
2. **`snapshot_set_breakpoints_response`** — SetBreakpoints request
   with 2 lines → SetBreakpointsResponse with `breakpoints: [
   { verified: true, line: 7 }, { verified: true, line: 12 } ]`.
   (Synthetic: driver returns fixed `BreakpointId`s without actually
   talking to lldb; the request → response shape is tested.)
3. **`snapshot_stack_trace_shape`** — StackTrace request at a paused
   frame → StackTraceResponse with `stackFrames: [ { id: 0, name:
   "fib", source: { path: "fib.cb" }, line: 8, column: 5 } ]`.
4. **`snapshot_variables_response`** — Variables request at frame 0
   with synthetic `xs: List<Int> = [1, 2, 3]` local → VariablesResponse
   with `variables: [ { name: "xs", value: "[1, 2, 3]", type:
   "List<Int>" } ]`. Confirms wave-1 pretty-printer output passes
   through unchanged.
5. **`snapshot_continue_paused_event`** — Continue request → Continue
   response + a follow-up StoppedEvent JSON shape.

Snapshot tests run WITHOUT spawning lldb (use `LldbDriver::test_stub()`
that returns canned stdout for each command pattern; the parser sees
the same bytes as a real lldb session).

### 6.2 End-to-end smoke (1)

In `crates/cobrust-dap/tests/dap_e2e_smoke.rs`:

- **`e2e_dap_full_handshake`** — guarded by `#[ignore]` + `cfg!(target_os
  = "linux")` (<self-hosted-runner> only; lldb-18 reliably available there
  per ADR-0058 §4):
  - `cargo build --release -p cobrust-cli`
  - `cobrust build --debug examples/fib.cb -o /tmp/fib-dap-smoke`
  - Spawn `cobrust-dap` child.
  - Stream DAP frames: Initialize → Launch → SetBreakpoints line N →
    Continue → wait for StoppedEvent (breakpoint hit) → StackTrace →
    Variables → Disconnect.
  - Assert: full handshake completes; StackTrace returns >= 1 frame;
    Variables returns a non-empty list.

E2E test runs only on DG (Linux/lldb-18 host); Mac excludes via cfg
gate. This honours the heavy-build-offload policy.

### 6.3 Quantitative success criteria

- `cargo test -p cobrust-dap` exits 0 on DG.
- 6 tests PASS (5 snapshot + 1 e2e smoke marked `#[ignore]` on Mac,
  runnable via `cargo test -p cobrust-dap -- --ignored` on DG).
- POSTFLIGHT clean: PRE/POST `/tmp/cobrust-*` count delta == 0.
- Zero regressions on Phase H/I/J/K/L wave-1/M baselines.

## 7. Risk register

### 7.1 lldb-18 stdout parser fragility

- **Risk**: lldb's stdout format evolves between LLVM major versions.
  Wave-2 regex parsers may break on LLVM-19 or LLVM-20.
- **Mitigation**: per-pattern unit tests in `lldb_driver.rs` covering
  the exact stdout shapes wave-2 depends on. On parse-failure, emit
  `tracing::warn!` + return a degraded-but-functional response (raw
  stdout string in DAP `value` field). The user gets feedback; the
  agent loop doesn't crash. Phase L+ may pin lldb-18 stdout format
  via SchemaTest or move to lldb-rs FFI if churn becomes prohibitive.

### 7.2 Pretty-printer output format stability

- **Risk**: wave-1 pretty-printers emit `xs: List<Int> = [1, 2, 3]`
  via lldb's `type summary` system. If wave-1 refactors the summary
  format (e.g. adds `(N elems)` prefix), wave-2's `value` field
  changes shape downstream; snapshot tests catch but require
  re-snapshotting.
- **Mitigation**: snapshot tests in §6.1 #4 lock the wave-1 output
  shape into wave-2's contract. Any wave-1 follow-up that changes
  the format must update wave-2 snapshots in the same PR.

### 7.3 Protocol version skew with VSCode

- **Risk**: VSCode + Cursor pin DAP version 1.55+ as of 2026; wave-2
  hand-rolled types target 1.55. Older DAP-conforming editors (some
  Vim DAP plugins) may speak 1.50, missing fields.
- **Mitigation**: wave-2 sets `protocolVersion: 1` in
  InitializeResponse; missing-field handling is via Serde's
  `#[serde(default)]` on all optional inputs. If skew bites a real
  user, follow-up sub-ADR adds the missing version branch.

### 7.4 Mac CI surface: lldb-18 unreliable via Homebrew

- **Risk**: `brew install llvm@18` lldb works for interactive use but
  has known issues spawning under `cargo test` (codesigning quirks,
  `lldb-server` not auto-launched).
- **Mitigation**: e2e smoke (§6.2) is `#[ignore]` + Linux-only. Mac
  developers run snapshot tests only. DG runs the full e2e gate. This
  matches ADR-0059a wave-1's lldb-on-DG-only test posture.

## 8. Phase plan

| Phase | Work | Wall |
|---|---|---|
| Author this ADR | §1-§7 spec | 1 commit |
| Phase 1: crate scaffold | Cargo.toml + main.rs + lib.rs skeleton | ~30 min |
| Phase 2: lldb-18 driver | lldb_driver.rs + 4 lldb wrapper methods | ~3-4h |
| Phase 3: 9 handlers | handlers.rs + DAP type structs | ~2-3h |
| Phase 4: snapshot tests | 5 insta snapshots + 1 e2e smoke | ~1-2h |
| Phase 5: DG verify | full test run on <self-hosted-runner> | ~30 min |
| Phase 6: dual-track docs | zh + en + agent module doc | ~30 min |
| Phase 7: ratify | ADR flip + cascade note | 1 commit |

Total wall: ~8-10h sub-agent velocity (vs ADR-0059 §13 forecast of
~1.5w wall; sub-agent compression ~3-4x for new-crate work).

## 9. Sub-ADR roster

Single ADR; no further sub-sub-sprints. Sibling sub-ADRs under
parent ADR-0059:

- **ADR-0059a** — wave-1 lldb pretty-printers (merged at `1629550`).
- **ADR-0059b** — this ADR; wave-2 DAP server.
- **ADR-0059c** — wave-3 `cobrust debug` CLI (queued).

Wave-2 ratifies on impl merge per ADR-0059 §10 frame-ADR ratification
path.

## 10. Pre-dispatch acceptance gate

Wave-2 dispatch may proceed only when:

- [x] ADR-0059a (wave-1) merged: pretty-printers at HEAD `1629550`.
- [x] ADR-0058c (DWARF emission) ratified at `a46fe85`.
- [x] Parent ADR-0059 frame in `proposed` status (ratifies on this
      wave's impl merge per ADR-0059 §10 frame-ADR rule).
- [x] lldb-18 available on <self-hosted-runner> per ADR-0058 §4.
- [x] No regressions on existing Phase J `cobrust-lsp` snapshot
      corpus — wave-2 touches a new crate only.

## 11. Consequences

### 11.1 Positive

- Cursor / VSCode "Run > Start Debugging" works end-to-end on Cobrust
  programs after wave-2 ratifies (subject to user-side launch.json
  wiring documented in §6 + dual-track docs).
- ADR-0059 §3.2 wave-2 row CLOSED.
- §2.5 §B LLM-amplifier surface extends from terminal lldb to
  in-editor Cursor agent.
- New crate boundary preserves crate-isolation: a DAP regression
  doesn't bleed into LSP / CLI / type-checker surfaces.
- Bind-the-core (ADR-0012): lldb-18 + DAP protocol + pretty-printers
  are externally maintained. Cobrust contributes ~1000-1500 LOC of
  marshalling.

### 11.2 Negative

- Adds ~1000-1500 LOC + 4 new deps (regex, tokio-process feature +
  thiserror reuse).
- Child-process spawn adds ~200-500ms startup latency per debug
  session (lldb-18 cold start). Acceptable per §2.5-low rank.
- Hand-rolled DAP types are wave-2's call but a maintenance burden:
  every new DAP feature requires adding the type struct + Serde
  derive. Phase L+ may revisit `dap` crate if mature variant lands.
- e2e smoke is DG-only; Mac CI gets snapshot-only coverage. Same
  posture as ADR-0059a wave-1.

### 11.3 Neutral

- New `crates/cobrust-dap/` subtree; CLAUDE.md §3 doc-coverage applies
  (zh/en/agent doc entries land in same atomic commit as Phase 6).
- Wave-3 (CLI) unblocked by wave-2's existence: `cobrust debug` will
  spawn cobrust-dap directly for editor-side workflows + lldb-18
  directly for terminal workflows.
- Future Phase L+: conditional breakpoints, source-level evaluate,
  multi-thread debug, attach mode, generic Adt pretty-print.

## 12. Dispatch readiness

| Phase | TEST hrs | DEV hrs | Wall |
|---|---|---|---|
| ADR author | 0 | 1 | 0.25 |
| Phase 1 scaffold | 0 | 0.5 | 0.25 |
| Phase 2 lldb driver | 1 | 3 | 1 |
| Phase 3 handlers | 0.5 | 2.5 | 1 |
| Phase 4 tests | 2 | 1 | 0.5 |
| Phase 5 DG verify | 0.5 | 0.5 | 0.5 |
| Phase 6 dual-track docs | 0 | 1 | 0.25 |
| Phase 7 ratify | 0 | 0.5 | 0.25 |
| **Total** | **4** | **10** | **~4 days at human velocity / ~1-2 days at sub-agent velocity** |

Mode: P9 direct (this dispatch). Host: <self-hosted-runner> for Phase 5
gate per heavy-build-offload policy.

## 13. Why this ADR now

- **ADR-0059a merged at `1629550`**: wave-1 pretty-printers shipped;
  wave-2 unblocked per ADR-0059 §8 wave plan.
- **User directive 2026-05-19**: P9 Phase L wave-2 dispatch.
- **Smallest-correct-increment**: wave-2 ships the load-bearing
  in-editor debugger surface in ~1-2 days sub-agent velocity. Wave-3
  CLI builds on the same lldb-18 host requirement; dispatch
  sequentially or parallel-after-wave-2 ratifies.

— P9 Tech Lead, 2026-05-19
