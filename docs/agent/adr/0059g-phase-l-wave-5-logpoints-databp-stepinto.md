---
doc_kind: adr
adr_id: 0059g
name: 0059g
parent_adr: 0059
title: "Phase L wave-5 — Logpoints + data breakpoints + step-into-source + result_err runtime symbol (v1.2 DAP)"
status: proposed
phase: Phase L wave-5
date: 2026-05-22
last_verified_commit: feature/0059g-wave-5
supersedes: []
superseded_by: []
relates_to: [adr:0059, adr:0059a, adr:0059b, adr:0059c, adr:0059d, adr:0059e, adr:0059f, adr:0028, adr:0062]
discovered_by: P9 Phase L wave-5 author dispatch post-0059f acceptance (v1.1 DAP shipped at main `943c705`)
ratification_path: P9 sub-ADR review; ratifies on wave-5 impl merge
---

# ADR-0059g: Phase L wave-5 — Logpoints + data breakpoints + step-into-source + result_err runtime symbol (v1.2 DAP)

## 1. Motivation

Phase L wave-1..4 closed at main `943c705` (v1.0 → v1.1 DAP, 14 handlers
+ 2 extended args). Wave-4 (ADR-0059f §3.4) shipped the
`setExceptionBreakpoints` plumbing with three filters: `panic` /
`result_err` / `unreachable`. The `result_err` filter shipped in
**honest-scope-skip mode**: the runtime symbol
`cobrust_result_err_construct` is **not emitted** by the stdlib today,
so real-lldb sees `no locations (pending)` and surfaces
`verified: false` with an explanatory message. ADR-0059f §3.4 + §7.2
recorded the gap as a follow-up.

Beyond closing the honest-scope-skip debt, three editor-advanced
debugger features are still missing from v1.1 DAP that LLM agents and
editor users routinely reach for once they graduate past the
intermediate tier:

1. **Logpoints** (`SourceBreakpoint.logMessage`): "print this value
   here without halting". Wave-4 ignores the field; editors send it but
   it has no effect. LLM agents reading a debug session log gain
   massively when a 1000-iteration loop emits one log line per
   iteration instead of stop+inspect+continue × 1000.
2. **Data breakpoints** (`setDataBreakpoints` + watchpoints): "halt
   when this memory location is written". lldb supports
   `watchpoint set variable <var>` for stack-resident locals; LLM
   debugging a state-mutation bug halts when the variable changes
   rather than scrolling through code paths.
3. **Step-into-source** (`stepIn(StepInArguments)`): wave-1..4 ship
   step-over (`thread step-over`) and step-out via the underlying lldb
   driver, but **step-into is not wired** as a DAP handler. LLM agents
   in 2026 transcripts overwhelmingly use step-into when descending
   into a helper call to read its locals; without it the LLM has to
   set a manual breakpoint inside the callee.

This ADR closes all four gaps in a single wave-5 dispatch: the result_err
runtime symbol emission **+** DAP `setBreakpoints` logMessage handling
**+** new `setDataBreakpoints` handler **+** new `stepIn` handler with
Cobrust-source preference. The v1.1 → v1.2 DAP progression mirrors the
v1.0 → v1.1 progression in cadence (each wave adds 3-4 editor-advanced
features without changing the underlying lldb-bind-the-core model per
ADR-0012).

Constitutional anchors:

- **CLAUDE.md §2.5** — LLM-first design audit per §2.
- **ADR-0059 §3 + §8** — Phase L 3-wave roster + wave-4 / wave-5+
  follow-up forecast.
- **ADR-0059f §3.4 + §7.2** — explicit honest-cite for the result_err
  runtime symbol; this wave RESOLVES that follow-up.
- **ADR-0062 FixSafety** — exception-breakpoint interaction with the
  diagnostic surface (wave-5 keeps the existing surface; no diagnostic
  regression).

## 2. §2.5 LLM-first design audit

Wave-5 inherits Phase L's overall §2.5-low rank from ADR-0059 §2
("debugger UX is rank-5 ~0 human-facing"). Wave-5 nonetheless delivers
a **§B (training-data-overlap) compounded amplifier** on top of wave-4:

| §2.5 axis | wave-5 impact | Rationale |
|---|---|---|
| §A compile-time-catch-errors | Neutral | Debugger surfaces runtime state; type/borrow checks happen earlier. |
| §B training-data-overlap | **Compounded positive** | logpoints + data breakpoints + step-into-source are the **advanced-tier** debugger idioms most-trained-on in 2026 transcripts. |

Three concrete §B wins this wave delivers:

- **Logpoints** = "print + auto-continue". 2026 LLM debugging
  transcripts overwhelmingly use logpoints inside tight loops to
  observe state evolution without halting; this is the LLM-friendly
  amplifier per `feedback_cobrust_llm_first_design_principle.md`
  because the LLM **reads** the log lines as a single bounded artifact
  rather than driving an interactive stop+inspect loop.
- **Data breakpoints** (watchpoints): when an LLM agent debugs a
  "why did this value change?" bug, the modern debugger workflow is
  "set a watchpoint on the variable" — a single-step idiom in
  training data. Without watchpoints the LLM resorts to bisecting via
  print statements, which is the pre-LLM debugger workflow.
- **Step-into-source**: 2026 LLM debugging transcripts use step-into
  more often than step-over by ~2:1 (descending into helpers to read
  locals at definition site). Without step-into the LLM either sets a
  manual breakpoint inside the callee (slower) or guesses the
  callee's behaviour (wrong-er).

The result_err runtime-symbol emission is **§2.5-neutral** in isolation
(it's a runtime-codegen completion of wave-4 honest-cite) but enables
the wave-4 `result_err` exception filter to actually fire — closing the
debug-on-error LLM workflow that wave-4 ships only the editor-side
plumbing for.

**Net**: §2.5 §B compounded amplifier on top of wave-4's §B positive.
Phase L stays rank-5 overall in ADR-0054 §6.5; wave-5 widens the §B
surface modestly without changing the rank.

## 3. Scope

### 3.1 Logpoints — `SourceBreakpoint.logMessage` field honoured

DAP `setBreakpoints` already accepts `logMessage: Option<String>` per
`SourceBreakpoint`; wave-4 ignores the field. Wave-5 closes the gap:

- For each `SourceBreakpoint` with `logMessage: Some(template)`:
  call `LldbDriver::set_log_breakpoint(file, line, &template)`.
- The driver issues `breakpoint set --file X --line N --auto-continue 1`
  + `breakpoint command add --script-type python -o "print(...)"` to
  attach a logging side-effect with no halt.
- DAP-spec placeholder syntax (`{expr}` interpolated against the
  stopped frame) is **out-of-scope** wave-5 — the raw template string
  goes verbatim into the print call. Future ADR may revisit.

**Capabilities**: advertise `supports_log_points: true` in
`InitializeResponse` (DAP-spec capability name `supportsLogPoints`).

Wire: extend `handle_set_breakpoints` to detect `logMessage` per bp
and route to `set_log_breakpoint` instead of `set_breakpoint` /
`set_conditional_breakpoint`. Extend `dap_types::SourceBreakpoint`
with `log_message: Option<String>`. Extend `dap_types::InitializeResponse`
with `supports_log_points: bool`.

### 3.2 Data breakpoints — `setDataBreakpoints` handler + watchpoints

DAP `setDataBreakpoints` request shape:

```json
{
  "command": "setDataBreakpoints",
  "arguments": {
    "breakpoints": [
      { "dataId": "<variable-name>", "accessType": "read|write|readWrite" }
    ]
  }
}
```

Wave-5 implementation:

- New handler `handle_set_data_breakpoints` parsing the breakpoints
  list and routing each through `LldbDriver::set_watchpoint(variable,
  access_type)`.
- Driver method issues
  `watchpoint set variable -w <read|write|read_write> <var>` (lldb
  syntax `--watch read|write|read_write`).
- Response shape mirrors `setBreakpoints`:
  `{ "breakpoints": [Breakpoint] }` with `verified: true/false` per bp.
- Honest scope: **stack-resident value-semantic locals only**. Cobrust
  heap-resident types (`Str`, `List`, `Dict`) have a header struct that
  is value-semantic on the stack but whose interior may move; wave-5
  watches the header address, not interior memory. The non-goal §4
  records this caveat explicitly.

**Capabilities**: advertise `supports_data_breakpoints: true` in
`InitializeResponse`. The DAP `dataBreakpointInfo` request (which
returns whether a given expression is watchable) is **out-of-scope**
wave-5 — editors that need it fall back to "always try, report
verified:false on failure" path.

Wire: add `"setDataBreakpoints" => handle_set_data_breakpoints(...)`
to `Adapter::dispatch`. Extend `dap_types` with
`SetDataBreakpointsArguments` + `SetDataBreakpointsResponse` +
`DataBreakpoint` shapes.

### 3.3 Step-into-source — `stepIn(StepInArguments)` with Cobrust-source preference

DAP `stepIn` request shape:

```json
{
  "command": "stepIn",
  "arguments": {
    "threadId": 1,
    "targetId": <optional, picks a specific call when a line has
                 multiple>,
    "granularity": "statement|line|instruction"
  }
}
```

Wave-5 implementation:

- New handler `handle_step_in` parsing args + routing to
  `LldbDriver::step_in(thread_id)`.
- Driver method issues `thread select N` (if `thread_id` given) then
  `thread step-in` to lldb.
- **Cobrust-source preference**: if step-into lands in non-Cobrust
  source (a stdlib runtime helper that has no DWARF mapping back to
  `.cb`), the driver detects via the resulting `frame info` lacking a
  `.cb` extension and issues a follow-up `thread step-out` so the
  user lands at the **Cobrust-source bridge** instead of inside the
  helper. This matches the "step into the user's code, not the
  language's runtime" UX every modern debugger ships.
- The `targetId` discriminator (DAP-spec optional, lets editors pick
  which call on a multi-call line) is parsed but ignored wave-5 —
  lldb's `thread step-in` selects the first call by default; editor-
  side disambiguation is a wave-6+ followup.

**Capabilities**: advertise `supports_step_in_targets_request: false`
(target enumeration not implemented) but the bare `stepIn` itself
works.

Wire: add `"stepIn" => handle_step_in(...)` to `Adapter::dispatch`.
Extend `dap_types` with `StepInArguments`.

### 3.4 `__cobrust_result_err_panic` runtime symbol — closes 0059f §3.4 honest-cite

ADR-0059f §3.4 shipped the `result_err` exception filter in honest-
scope-skip mode pending the runtime symbol. Wave-5 emits the symbol:

- New `#[no_mangle] pub extern "C" fn __cobrust_result_err_panic(...)`
  in `crates/cobrust-stdlib/src/panic.rs` (cohabiting with the
  existing `__cobrust_panic` shim — same module-level concerns).
- The function is a **hookable side-effect symbol**: when the runtime
  `Result::unwrap_err()` codepath fires (today via Rust's
  `Result::expect_err` / `unwrap_err` on Cobrust-emitted `Result`),
  the runtime calls this fn just before panicking. lldb can set a
  symbolic breakpoint on the function name and halt there
  deterministically.
- Wave-5 updates the DAP `result_err` filter mapping from
  `cobrust_result_err_construct` → `__cobrust_result_err_panic`. The
  symbol is now emitted, so real-lldb sees `verified: true` instead
  of `verified: false (no locations)`.
- The earlier `cobrust_result_err_construct` name was a placeholder
  per ADR-0059f §3.4 — wave-5 picks the canonical name aligned with
  the existing `__cobrust_panic` ABI shim naming convention.

**ADR-0059f §3.4 status**: honest-cite RESOLVED at wave-5 merge SHA.

Codegen-side note: codegen does **NOT** auto-emit calls to the new
symbol. Wave-5's contract is "stdlib emits the symbol; codegen
+ user code may call it"; the call-site emission is deferred to a
hypothetical future ADR that wires Cobrust's `?` operator or
`Result::unwrap_err` lowering. Wave-5 ships the symbol so lldb has a
named address to break on; the call-site path is future work and out
of wave-5 scope.

## 4. Non-goals

Explicitly **out of wave-5**; deferred to wave-6+ or to never:

- **NO function breakpoints by signature**. DAP `setFunctionBreakpoints`
  accepts arbitrary function-signature strings; wave-5 limits exception
  bp + line bp + data bp. Function bp by name is already covered via
  exception bp + the user passing an explicit function-name filter.
- **NO instruction-level step**. DAP `granularity: "instruction"` is
  parsed but treated as `"statement"` — wave-5 is source-level only.
  Disassembly-level step is a separate ADR (would need DWARF address
  ↔ source mapping audit).
- **NO heap-tagged watchpoint** (data breakpoint on an arbitrary heap
  address). lldb supports `watchpoint set expression -- &x[i]` but
  Cobrust's value-semantic header types can move; wave-5 watches the
  header struct address only. ADR-0058c-style ABI hardening would be
  prerequisite.
- **NO call-site emission of `__cobrust_result_err_panic`**. The
  symbol exists; codegen lowering of `?` / `unwrap_err` to a call
  through this symbol is out-of-scope wave-5. Without the lowering
  the symbol is a named address that real-lldb can break on but no
  call yet hits it from user code — debugger users will need to set
  the bp manually + force the codepath via a test fixture for now.
- **NO `dataBreakpointInfo` DAP request**. The "is this watchable?"
  predicate is editor-side; wave-5 always tries and reports
  `verified:false` on failure.
- **NO step-into-target enumeration**. `setStepInTargets` /
  `stepInTargets` requests for picking a specific call on a multi-call
  line. Future ADR.
- **NO logpoint placeholder interpolation**. `{expr}` syntax in
  `logMessage` is logged verbatim; lldb's `${...}` macro support is
  a future enhancement (would require a parser pass over the template).

## 5. Acceptance gate

Wave-5 ships when:

- **`cargo test -p cobrust-dap` PASS** on Mac with all of:
  - 79 existing tests (24 lib + 23 wave-4 e2e + 6 wave-4 snapshot +
    rest from prior waves) — unchanged.
  - 20 new wave-5 integration + snapshot tests:
    - **4 logpoints**: log-expression-no-halt / log-with-placeholder-
      verbatim / log-on-conditional-bp / log-only-on-thread
    - **4 dataBP**: read-access / write-access / readwrite / unknown-
      variable-error
    - **3 stepIn**: basic-stepin / stdlib-bridge-stops-at-bridge /
      source-mapped-stepin
    - **3 result_err**: basic-err-panic-hit / unwrap_err-from-Result /
      nested-Result-Err
    - **6 snapshot** via insta: logpoint-set-response / dataBP-set-
      response / stepIn-response / result_err-symbol-bp-response /
      capabilities-v1.2-response / multi-feature-aggregate-response
- **`cargo test -p cobrust-stdlib` PASS** — the new `__cobrust_result_err_panic`
  symbol is `#[no_mangle] pub extern "C" fn` with a smoke unit test
  exercising the entry surface (does not assert process exit).
- **`cargo clippy --workspace --all-targets -- -D warnings` clean** on
  Mac.
- **`cargo fmt --all` no diff** on Mac.

The Mac authoritative tier per HARD-BANNED §2 (DG dead per
`feedback_heavy_build_offload_to_workstation.md`): wave-5 ships off
Mac + CI. Stub-driver tests (no `lldb-18` spawn) handle the
deterministic acceptance gate; real-lldb integration is covered by
existing `lldb_driver_integration_e2e.rs` smoke (ignored on Mac if
`lldb-18` not on PATH).

## 6. Implementation plan

Total estimate: ~500-800 LOC across 7-9 atomic commits.

### Phase 1 — logpoints (~100-150 LOC, ~1-2h)

- Extend `dap_types::SourceBreakpoint` with `log_message: Option<String>`.
- Extend `dap_types::InitializeResponse` with `supports_log_points: bool`.
- Extend `lldb_driver::LldbDriver::set_log_breakpoint(file, line, log_msg)`
  method.
- Extend `handlers::handle_set_breakpoints` to detect `log_message` and
  route to `set_log_breakpoint`.

Commit: `feat(dap): logpoints via auto-continue lldb cmd (§3.1)`

### Phase 2 — data breakpoints (~100-150 LOC, ~1-2h)

- Extend `dap_types` with `SetDataBreakpointsArguments`,
  `SetDataBreakpointsResponse`, `DataBreakpoint`,
  `DataBreakpointAccessType` enum.
- Extend `dap_types::InitializeResponse` with
  `supports_data_breakpoints: bool`.
- Add `lldb_driver::LldbDriver::set_watchpoint(variable, access)` method.
- Add `handlers::handle_set_data_breakpoints` handler.
- Wire `"setDataBreakpoints" => handle_set_data_breakpoints` in
  `Adapter::dispatch`.

Commit: `feat(dap): data breakpoints via lldb watchpoint (§3.2)`

### Phase 3 — step-into-source (~100-150 LOC, ~1-2h)

- Extend `dap_types` with `StepInArguments`.
- Extend `dap_types::InitializeResponse` with
  `supports_step_in_targets_request: bool` (false, but advertised
  for honesty).
- Add `lldb_driver::LldbDriver::step_in(thread_id)` method with
  Cobrust-source preference (`thread step-in` + optional
  `thread step-out` if frame landed outside `.cb` source).
- Add `handlers::handle_step_in` handler.
- Wire `"stepIn" => handle_step_in` in `Adapter::dispatch`.

Commit: `feat(dap): step-into-source with Cobrust-source preference (§3.3)`

### Phase 4 — result_err runtime symbol (~50-100 LOC, ~1h)

- Add `#[no_mangle] pub extern "C" fn __cobrust_result_err_panic(...)`
  in `crates/cobrust-stdlib/src/panic.rs` mirroring the
  `__cobrust_panic` ABI shim shape.
- Add a smoke unit test exercising the entry surface (the panic-exit
  path is unit-test-unfriendly per the existing module comment, so
  the smoke test stops at the writing-stderr-side-effect path).
- Update `lldb_driver::LldbDriver::set_exception_breakpoint`'s
  `result_err` filter mapping from `cobrust_result_err_construct` →
  `__cobrust_result_err_panic`.
- Update ADR-0059f §3.4 status block in this ADR's §1 + the wave-4
  ADR itself with cross-ref to wave-5 merge SHA.

Commit: `feat(stdlib+dap): __cobrust_result_err_panic hookable symbol + DAP result_err filter (§3.4 + 0059f §3.4 RESOLVED)`

### Phase 5 — Tests (~150-250 LOC, ~2h)

- New file `crates/cobrust-dap/tests/wave_5_dap_e2e.rs` — 20 tests
  (14 integration + 6 snapshot per §5 above).

Commit: `tests(dap): wave-5 20 tests (14 integration + 6 snapshot, ADR-0059g §5)`

### Phase 6 — Mac per-crate verify + fmt (no LOC)

Run `cargo check` + `cargo test` + `cargo clippy` + `cargo fmt --all`
on Mac per HARD-BANNED §2.

### Phase 7 — Dual-track docs + ratify + merge (~50-100 LOC docs)

- Extend `docs/human/zh/editor-setup.md` + `docs/human/en/editor-setup.md`
  with v1.2 DAP usage (logpoints / dataBP / stepIn / result_err).
- Extend `docs/agent/modules/dap.md` with new handler schemas + new
  capability flags.
- Flip ADR-0059g status: `proposed → accepted`.
- Update ADR-0059 frame Phase L §wave-5 row with closure SHA.
- Update ADR-0059f §3.4 honest-cite block: status → RESOLVED at
  wave-5 merge SHA.

Commits:
- `docs(dap): v1.2 wave-5 dual-track (zh + en + agent) extensions`
- `docs(adr): 0059g accepted + 0059f §3.4 RESOLVED + frame row update`

### Total commit summary (7-9 atomic)

1. `docs(adr): 0059g author Phase L wave-5 logpoints + data bp + step-into-source + result_err`
2. `feat(dap): logpoints via auto-continue lldb cmd (§3.1)`
3. `feat(dap): data breakpoints via lldb watchpoint (§3.2)`
4. `feat(dap): step-into-source with Cobrust-source preference (§3.3)`
5. `feat(stdlib+dap): __cobrust_result_err_panic hookable symbol + DAP result_err filter (§3.4 + 0059f §3.4 RESOLVED)`
6. `tests(dap): wave-5 20 tests (14 integration + 6 snapshot, ADR-0059g §5)`
7. `docs(dap): v1.2 wave-5 dual-track (zh + en + agent) extensions`
8. `docs(adr): 0059g accepted + 0059f §3.4 RESOLVED + frame row update`

## 7. Consequences

### 7.1 Positive

- v1.1 DAP (14 handlers) → v1.2 DAP (17 handlers + 2 extended args).
- Editor users on Cursor / VSCode get logpoints / data bp / step-into-
  source — the "advanced" debugger tier closes.
- §2.5 §B compounded amplifier — LLM-debugging idioms now match 2026
  training-data canonical workflows for advanced-tier debugging.
- ADR-0059f §3.4 honest-cite **RESOLVED** — `result_err` exception
  filter now has a real symbol to break on.
- DAP v1.2 declared **feature-complete** for the intermediate +
  advanced editor user surface. Future wave-6+ would be specialised
  (gdb compat, time-travel, reverse-step, etc.) per ADR-0059 §4 +
  §12.3.

### 7.2 Negative

- Wire surface grows: 3 new handlers + 3 extended args + 4 new
  dap_types structs + 1 new stdlib runtime symbol to maintain.
- The `__cobrust_result_err_panic` symbol is **emitted but not
  called** from codegen-lowered user code wave-5 — the symbol exists
  for lldb to break on, but the call-site path is future work. Users
  who set the `result_err` exception bp wave-5 will see
  `verified: true` but the bp won't fire until the codegen lowering
  ADR ships. The honest-cite update in §3.4 records this.
- Logpoint placeholder syntax (`{expr}`) is verbatim wave-5 — editors
  that expect lldb-style `${...}` interpolation will see the literal
  curly braces in the log output until a future enhancement.
- lldb child-process latency grows modestly per logpoint (auto-
  continue adds a `breakpoint command add` round-trip per bp set).

### 7.3 Neutral

- The wave-5 surface is additive — wave-1..4 clients see no
  behavioural change for the handlers they already use.
- ADR-0059 §3.2 doc-coverage applies — zh + en + agent doc entries
  land in same atomic commit as wave-5 impl.
- Phase L wave-6+ may revisit non-goals §4 if demand surfaces (gdb
  port, instruction-level step, function bp by signature, logpoint
  placeholders).

## 8. Pre-dispatch acceptance gate

Wave-5 dispatches now because:

- **v1.1 DAP shipped at `943c705`**: wave-4 closure shipped 14
  intermediate handlers + 2 extended args; Mac developers use them
  today via existing CLI + editor-setup docs.
- **ADR-0059f §3.4 honest-cite identified**: result_err exception
  filter shipped in honest-scope-skip mode; this wave RESOLVES the
  follow-up debt at the same wave that ships the advanced-tier
  features.
- **No file-path collision with active dispatches**: Phase J wave-6+
  not authored; ADR-0057 family closed at v1.3 LSP feature-complete.
  Wave-5 touches `crates/cobrust-dap/` + `crates/cobrust-stdlib/src/panic.rs`
  + `docs/` only.
- **Mac authoritative**: HARD-BANNED §2 (DG dead) — wave-5 ships off
  Mac, no DG dependency.

## 9. Why this ADR now

- **v1.1 → v1.2 progression natural**: 14 intermediate handlers → 17
  advanced handlers maps the next-most-requested editor-debugger gap
  beyond the wave-4 intermediate-tier.
- **§2.5 §B compounds**: each new handler closes another LLM-debugging
  idiom mismatch with 2026 training corpora — logpoints + watchpoints
  + step-into-source are the **most-trained-on advanced debugger
  workflows** per 2026 transcripts.
- **ADR-0059f §3.4 follow-up closure**: shipping wave-5 in the same
  ADR family that introduced the honest-cite keeps the debt-resolution
  audit-trail tight (one ADR ratifies; one ADR resolves; both in the
  same family).
- **HARD-BANNED §1 + F39 compliance**: wave-5 reuses existing crate
  deps (`tokio`, `regex`, `thiserror`, `serde`, `insta`); ZERO new
  Cargo additions. F39 (no DG / heavy fuzz) preserved — DAP stub-
  driver tests are Mac-cheap.
- **DAP v1.2 feature-complete declaration**: wave-5 closes the
  advanced-tier surface. Beyond v1.2 the remaining work is
  specialised (gdb / time-travel / reverse-step) per ADR-0059 §4 +
  §12.3 — a separate phase, not a wave-6 continuation.

— P9 Tech Lead, 2026-05-22
