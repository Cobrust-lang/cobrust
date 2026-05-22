---
doc_kind: adr
adr_id: 0059f
name: 0059f
parent_adr: 0059
title: "Phase L wave-4 — Watch expressions + conditional bp + multi-thread + exception bp (v1.1 DAP)"
status: accepted
phase: Phase L wave-4
date: 2026-05-22
last_verified_commit: feature/0059f-wave-4
supersedes: []
superseded_by: []
relates_to: [adr:0059, adr:0059a, adr:0059b, adr:0059c, adr:0059d, adr:0059e, adr:0057f, adr:0028]
discovered_by: P9 Phase L wave-4 author dispatch post-§6.1-wave-4 ratification (v1.0 DAP shipped at `7fda081`)
ratification_path: P9 sub-ADR review; ratifies on wave-4 impl merge
---

# ADR-0059f: Phase L wave-4 — Watch expressions + conditional bp + multi-thread + exception bp (v1.1 DAP)

## 1. Motivation

Phase L wave-1 / wave-2 / wave-3 + §6.1 wave-4 closed at main `7fda081`.
The v1.0 DAP surface ships 10 basic handlers (Initialize / Launch /
SetBreakpoints / Continue / Next / Pause / StackTrace / Variables /
Disconnect / Threads-stub). Mac developers using Cursor / VSCode
"Run > Start Debugging" can now step through Cobrust source, set line
breakpoints, and inspect locals with wave-1 pretty-printer summaries.

That surface is the **basic** debugger tier. Editor users who graduate
from "set a breakpoint, step over, inspect" routinely reach for four
**intermediate** features the v1.0 DAP does not provide:

1. **Watch expressions** (DAP `evaluate`): "what is `arr[i] + 1` right
   now?" — typed in the editor's debug REPL or attached to a watch
   panel. v1.0 returns `command 'evaluate' not implemented in wave-2`.
2. **Conditional breakpoints** (`SourceBreakpoint.condition`): "only
   break when `i > 10`" — the bp triggers iff the condition evaluates
   truthy. v1.0 reads but ignores the field (per ADR-0059b §5 non-goal).
3. **Multi-thread visibility** (`threads` + per-thread `stackTrace`):
   ADR-0028 ships structured-concurrency primitives; programs with two
   or more threads stopped at a breakpoint currently show only "main".
   v1.0 `threads` returns hardcoded `[{id:1, name:"main"}]`.
4. **Exception breakpoints** (`setExceptionBreakpoints`): "break when a
   Cobrust panic / `Result::Err` is constructed / `unreachable!`
   fires" — debuggers ship this as a single-checkbox feature for "halt
   on uncaught error". v1.0 lacks the handler entirely.

ADR-0059 frame §3 + §8 (3-wave roster) explicitly forecasts Phase L+
followup ADRs for "the visible debugger surface". ADR-0059e
already closed the str-runtime + frame-variable + closure-capture gaps
(§6.1 wave-4). This ADR is the **second** Phase L+ followup, expanding
the DAP wire surface from v1.0 (10 handlers) to **v1.1** (14 handlers +
2 extended args).

## 2. §2.5 LLM-first design audit

Phase L wave-4 inherits Phase L's overall §2.5-low rank from
ADR-0059 §2 ("debugger UX is rank-5 ~0 human-facing"). The wave-4
expansion nevertheless delivers a modest §2.5 §B
(training-data-overlap) amplifier:

| §2.5 axis | wave-4 impact | Rationale |
|---|---|---|
| §A compile-time-catch-errors | Neutral | Debugger surfaces runtime state; type/borrow checks happen earlier. |
| §B training-data-overlap | **Modest positive** | "intermediate" debugger features (watch / conditional / multi-thread / exception) are the LLM-debugging idioms most-trained-on in 2026 transcripts. |

Three concrete §B wins this wave delivers:

- **Watch expressions**: LLM agent debugging at a breakpoint sees
  multiple variables simultaneously, not just one `frame variable`
  dump. This matches how LLM agents read GDB / lldb transcripts in
  training data — "let me check `arr[i]` and `len` at the same time".
  Without `evaluate`, the LLM is forced to set a breakpoint, hit it,
  inspect, restart — the LLM-friendliness deficit per
  `feedback_cobrust_llm_first_design_principle.md` is real.
- **Conditional breakpoints**: bp fires **only** when the condition is
  true → the LLM reading the debug session log gets a higher signal-
  to-noise ratio. Without this, an LLM debugging an issue inside a
  10K-iteration loop hits the breakpoint 10K times and has to scroll
  through 10K stop-events to find the one that matters.
- **Exception breakpoints**: "break on panic" is the **single most-
  trained-on** debugger workflow in modern transcripts (every Rust /
  Python tutorial hits "set a breakpoint on panic, step backwards to
  find the bug"). Wave-4 closes this gap for Cobrust panics +
  Result::Err construction sites + unreachable! intrinsic calls.

**Net**: §2.5 §B modest positive amplifier on top of wave-1's pretty-
printer §B. Phase L still rank-5 overall in ADR-0054 §6.5 ordering;
wave-4 expands the §B surface modestly without changing the rank.

## 3. Scope

### 3.1 `evaluate(EvaluateArguments)` — watch expression evaluation

DAP `evaluate` request handler:

- **Input**: `{ expression: String, frameId: Option<i64>, context: Option<String> }`.
  Per DAP spec, `context` is `"watch" | "repl" | "hover" | "clipboard"`;
  wave-4 routes all four through the same lldb path.
- **Implementation**: `LldbDriver::evaluate(expr, frame_id)` issues
  `frame select N` (if frame_id given) then `expression <expr>` to
  lldb's REPL; reads the resulting stdout until the prompt.
- **Output**: `{ result: String, type: Option<String>, variablesReference: i64 }`.
  `result` is lldb's stdout summary verbatim (already
  pretty-printer-formatted if wave-1 printers attached); `type` is
  parsed from the lldb response's `(<type>) $N = ...` prefix.

**Wire**: extend `Adapter::dispatch` to add `"evaluate" => handle_evaluate(...)`.
Extend `dap_types` with `EvaluateArguments` + `EvaluateResponse`.

### 3.2 `setBreakpoints` extended with per-bp `condition`

ADR-0059b §5 + `dap_types::SourceBreakpoint` already read
`condition: Option<String>` but explicitly ignore it. Wave-4 closes
that gap:

- For each `SourceBreakpoint` with `condition: Some(expr)`:
  call `LldbDriver::set_conditional_breakpoint(file, line, &expr)`
  which issues `breakpoint set --file X --line N --condition '<expr>'`
  to lldb.
- For each `SourceBreakpoint` with `condition: None`: existing
  unconditional path (no change).
- **Capabilities**: advertise `supportsConditionalBreakpoints: true`
  in `InitializeResponse`.

**Why condition-as-lldb-expression-string**: lldb's `--condition`
accepts a C-like expression. Cobrust integer comparisons happen to
coincide with C syntax (per ADR-0059 §4 non-goal note), so simple
predicates like `i > 10` work passthrough. Source-level Cobrust
condition parsing (e.g. allowing `i in xs` or `is_some(x)`) is **out
of scope** — ADR-0059 §4 deferred it explicitly.

### 3.3 `threads()` + per-thread `stackTrace`

ADR-0028 ships structured-concurrency primitives (tasks + channels).
Programs that use those primitives have N >= 2 OS threads at
breakpoint time. wave-4 surfaces them:

- **`LldbDriver::list_threads()`** issues `thread list` to lldb,
  parses each line of the form
  `  thread #N: tid = ..., 0x..., name = '<name>'`, returns
  `Vec<ThreadInfo { id, name }>`.
- **`handle_threads`** drops the hardcoded stub, calls
  `driver.list_threads()`, returns the actual thread list.
- **`LldbDriver::stack_trace_for_thread(thread_id)`** issues
  `thread select N` then `thread backtrace`, parses frames.
- **`handle_stack_trace`** reads `args.thread_id`, calls
  `stack_trace_for_thread(thread_id)` instead of the current
  selected-thread `stack_trace()`.

**Backward compat**: single-thread programs still surface
`[{id: 1, name: "main"}]` — no behaviour regression for v1.0 clients.

### 3.4 `setExceptionBreakpoints(filters)` — break on panic / Result::Err / unreachable

DAP `setExceptionBreakpoints` accepts `filters: Vec<String>` —
identifiers the adapter advertises in `InitializeResponse.exceptionBreakpointFilters`.
wave-4 advertises three filters:

- **`"panic"`** → `breakpoint set --name __cobrust_panic`. The
  symbol comes from the stdlib's panic runtime hook
  (`crates/cobrust-stdlib/src/panic.rs` if present; otherwise from
  `core::panicking::panic` for Rust-emitted panics).
- **`"result_err"`** → `breakpoint set --name cobrust_result_err_construct`.
  **Honest scope**: this symbol is **NOT** currently emitted by
  cobrust-codegen. If the symbol is unavailable at bp-set time, the
  handler **explicitly warns + skips** that filter; the response
  reports `verified: false` with `message: "symbol not emitted in
  current build"`. Future ADR (e.g. ADR-0059g) wires the runtime
  symbol; wave-4 ships the DAP plumbing.
- **`"unreachable"`** → `breakpoint set --name core::intrinsics::unreachable_internal`.
  Maps to LLVM's `unreachable` intrinsic via Rust core's symbol; if
  the symbol is stripped (release builds), reports `verified: false`.

**Wire**: add `handle_set_exception_breakpoints` handler, dispatch on
`"setExceptionBreakpoints"`. Extend `dap_types` with
`SetExceptionBreakpointsArguments` + `SetExceptionBreakpointsResponse`.
Extend `InitializeResponse` with `exception_breakpoint_filters: Vec<ExceptionBreakpointsFilter>`.

## 4. Non-goals

Explicitly **out of wave-4**; deferred to wave-5+ or to never:

- **NO step-into-source**. Wave-1 step-over (`thread step-over`) and
  step-out are sufficient for Phase L. Step-into adds a UI burden
  (which call to step into when a line has multiple calls) that
  outranks the §2.5 §B payoff. Deferred indefinitely.
- **NO logpoints** (log-only breakpoints that print a message without
  halting). Useful but additive; deferred wave-5 if demand surfaces.
- **NO data breakpoints** (watchpoints on memory addresses). lldb
  supports `watchpoint set variable X`, but the Cobrust ABI does not
  guarantee a stable address for value-semantic types (Str / List
  are heap-allocated headers whose addresses can move). Deferred
  until ADR-0058c-style ABI hardening.
- **NO source-level Cobrust expressions in `evaluate`**. Wave-4 routes
  expression strings verbatim to lldb's REPL — Cobrust syntax that
  coincides with C (arithmetic, comparisons, field access via `.`,
  array indexing via `[]`) works passthrough; everything else
  (`match`, comprehensions, generic-function calls) does not. Phase L
  §4 deferred this; wave-4 reaffirms the deferral.
- **NO `setVariable`** (mutating values mid-step). Same Cobrust-
  ownership reasoning as ADR-0059 §4: rewriting a borrowed value
  mid-step violates the borrow checker's invariants at runtime.
- **NO `stepBack` / time-travel**. ADR-0059 §4 deferred indefinitely.

## 5. Acceptance gate

Wave-4 ships when:

- **`cargo test -p cobrust-dap` PASS** on Mac with all of:
  - 24 existing unit tests (lib) — unchanged.
  - 22 new wave-4 integration + snapshot tests:
    - **5 evaluate**: simple-expr / field-access / bool-test /
      lookup-undefined-error / nested-frame-evaluation
    - **4 conditional bp**: bp-hits-when-true / bp-skips-when-false /
      parse-error-cond / threading-safety (two threads, one cond)
    - **4 multi-thread**: single-thread-still-works / 2-threads /
      per-thread-stack / thread-id-out-of-bounds
    - **3 exception bp**: panic-hit / result-err-honest-skip-if-no-symbol /
      unreachable-hit-if-symbol-present
    - **6 snapshot** via insta: evaluate-response / conditional-bp-set-
      response / threads-list-response / per-thread-stack-trace-
      response / exception-bp-set-response / capabilities-v1.1-response
- **`cargo clippy -p cobrust-dap --all-targets -- -D warnings` clean** on Mac.
- **`cargo fmt --all` no diff** on Mac.
- **`cargo check -p cobrust-dap`** PASS (compilation cleanly typed).

The Mac authoritative tier per HARD-BANNED §2 (DG dead per
`feedback_heavy_build_offload_to_workstation.md`): wave-4 ships off
Mac + CI. Stub-driver tests (no `lldb-18` spawn) handle the
deterministic acceptance gate; real-lldb integration is covered by
existing `lldb_driver_integration_e2e.rs` smoke (ignored on Mac if
`lldb-18` not on PATH).

## 6. Implementation plan

Total estimate: ~600-900 LOC across 7-9 atomic commits.

### Phase 1 — `evaluate` impl (~150-200 LOC, ~2h)

- New file `crates/cobrust-dap/src/evaluate.rs` (~80 LOC) — the
  `handle_evaluate` handler.
- Extend `dap_types` (~50 LOC) — `EvaluateArguments` + `EvaluateResponse`
  shape.
- Extend `lldb_driver` (~30 LOC) — `LldbDriver::evaluate` method.
- Wire `"evaluate"` in `Adapter::dispatch`.

Commit: `feat(dap): evaluate handler for watch expressions (§3.1)`

### Phase 2 — conditional breakpoints (~100-150 LOC, ~1-2h)

- Extend `LldbDriver::set_breakpoint` (or add
  `LldbDriver::set_conditional_breakpoint`) to take optional
  condition string and emit `--condition 'expr'`.
- Extend `handle_set_breakpoints` to read each `SourceBreakpoint`'s
  `condition` field and route accordingly.
- Update `InitializeResponse.supports_conditional_breakpoints` to
  `true`.

Commit: `feat(dap): conditional bp support in handle_set_breakpoints (§3.2)`

### Phase 3 — multi-thread support (~150-200 LOC, ~2h)

- Extend `dap_types` with `ThreadInfo`, `ThreadsResponse`.
- Add `LldbDriver::list_threads()` + `LldbDriver::stack_trace_for_thread(thread_id)`.
- Rewrite `handle_threads` to call `list_threads`.
- Extend `handle_stack_trace` to use per-thread trace.
- Add `parse_threads_output` regex parser for `thread list` stdout.

Commit: `feat(dap): multi-thread visibility via list_threads + per-thread stack_trace (§3.3)`

### Phase 4 — exception breakpoints (~100-150 LOC, ~1h)

- Extend `dap_types` with `SetExceptionBreakpointsArguments`,
  `SetExceptionBreakpointsResponse`, `ExceptionBreakpointsFilter`.
- Extend `InitializeResponse` with
  `exception_breakpoint_filters: Vec<ExceptionBreakpointsFilter>`.
- Add `handle_set_exception_breakpoints` handler.
- Add `LldbDriver::set_exception_breakpoint(filter)` method —
  per-filter symbol mapping per §3.4.

Commit: `feat(dap): exception breakpoints (panic / result_err / unreachable filters) (§3.4)`

### Phase 5 — Tests (~150-250 LOC, ~2h)

- New file `crates/cobrust-dap/tests/wave_4_dap_e2e.rs` —
  22 tests (16 integration + 6 snapshot per §5 above).

Commit: `tests(dap): wave-4 22 tests (16 integration + 6 snapshot, ADR-0059f §5)`

### Phase 6 — Mac per-crate verify + fmt (no LOC)

Run `cargo check` + `cargo test` + `cargo clippy` + `cargo fmt --all`
on Mac per HARD-BANNED §2.

### Phase 7 — Dual-track docs + ratify + merge (~50-100 LOC docs)

- Extend `docs/human/zh/editor-setup.md` + `docs/human/en/editor-setup.md`
  with v1.1 DAP usage (watch / conditional bp / multi-thread / exception).
- Extend `docs/agent/modules/dap.md` with new handler schemas.
- Flip ADR-0059f status: `proposed → accepted`.
- Update ADR-0059 frame Phase L §wave-4 row with closure SHA.

Commits:
- `docs(dap): v1.1 wave-4 dual-track (zh + en + agent) extensions`
- `docs(adr): 0059f accepted (Phase L wave-4 closure)`

### Total commit summary (7-9 atomic)

1. `docs(adr): 0059f author Phase L wave-4 advanced debugger UX`
2. `feat(dap): evaluate handler for watch expressions (§3.1)`
3. `feat(dap): conditional bp support in handle_set_breakpoints (§3.2)`
4. `feat(dap): multi-thread visibility via list_threads + per-thread stack_trace (§3.3)`
5. `feat(dap): exception breakpoints (panic / result_err / unreachable filters) (§3.4)`
6. `tests(dap): wave-4 22 tests (16 integration + 6 snapshot, ADR-0059f §5)`
7. `docs(dap): v1.1 wave-4 dual-track (zh + en + agent) extensions`
8. `docs(adr): 0059f accepted (Phase L wave-4 closure)`

## 7. Consequences

### 7.1 Positive

- v1.0 DAP (10 handlers) → v1.1 DAP (14 handlers + 2 extended args).
- Editor users on Cursor / VSCode get watch / conditional bp / multi-
  thread / exception bp — the "intermediate" debugger tier closes.
- §2.5 §B modest amplifier — LLM-debugging idioms aligned with 2026
  training data.
- ADR-0028 concurrency runtime debuggable for the first time
  (multi-thread visibility was missing in v1.0).

### 7.2 Negative

- Wire surface grows: 4 new handlers + 2 extended args + 6 new
  dap_types structs to maintain.
- `result_err` exception bp filter ships in **honest-scope-skip** mode
  — the runtime symbol it needs isn't emitted yet. Future ADR closes
  the gap; users see `verified: false` for that filter until then.
- lldb child-process latency budget grows modestly (more round-trips
  per debug session for watch updates).

### 7.3 Neutral

- `crates/cobrust-dap/src/evaluate.rs` is a new module (~80 LOC).
  ADR-0059 §3.2 doc-coverage applies — zh + en + agent doc entries
  land in same atomic commit as wave-4 impl.
- Phase L wave-5+ may revisit non-goals §4 if demand surfaces.

## 8. Pre-dispatch acceptance gate

Wave-4 dispatches now because:

- **v1.0 DAP shipped at `7fda081`**: §6.1 wave-4 closure shipped 10
  basic handlers; Mac developers use them today via existing CLI
  + editor-setup docs.
- **ADR-0059e ratified**: str-runtime + frame-variable + closure-
  capture gaps closed; wave-4 expands the surface on top of a known-
  good baseline.
- **No file-path collision with active dispatches**: Phase L wave-5+
  ADRs not authored; ADR-0057 family closed at Phase J wave-4. Wave-4
  touches `crates/cobrust-dap/` and `docs/` only.
- **Mac authoritative**: HARD-BANNED §2 (DG dead) — wave-4 ships off
  Mac, no DG dependency.

## 9. Why this ADR now

- **v1.0 → v1.1 progression natural**: 10 basic handlers → 14
  intermediate handlers maps the most-requested editor-debugger gap.
- **§2.5 §B compounds**: each new handler closes another LLM-
  debugging idiom mismatch with 2026 training corpora.
- **ADR-0028 concurrency runtime debuggability**: multi-thread
  visibility is a blocker for any structured-concurrency user wanting
  to debug across task boundaries; wave-4 unblocks.
- **HARD-BANNED §1 + F39 compliance**: wave-4 reuses existing crate
  deps (`tokio`, `regex`, `thiserror`, `serde`, `insta`); ZERO new
  Cargo additions. F39 (no DG / heavy fuzz) preserved — DAP stub-
  driver tests are Mac-cheap.

— P9 Tech Lead, 2026-05-22
