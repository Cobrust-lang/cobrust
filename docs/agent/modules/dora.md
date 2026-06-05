---
doc_kind: module
module_id: mod:dora
crate: cobrust-dora
last_verified_commit: HEAD
dependencies: [mod:types, mod:mir, mod:codegen, mod:stdlib]
---

# Module: dora

## Purpose

`cobrust-dora` bridges `.cb` source programs to the dora-rs robotics
dataflow runtime. Ninth ecosystem-module proof on the ratified `.cb`
ecosystem-import chain (ADR-0072) and the third module to exercise
the cross-boundary callback marshalling pattern (ADR-0073) — after
pit (Flask, sixth) and hood (click, seventh).

Per ADR-0076 ("dora-cb Stream Y architecture"), this is the Phase 1
deliverable: the C-ABI shim surface a compiled `.cb` program binds
onto when it does `import dora` and calls `dora.Node(name)` +
`dora.node(handler)` + `node.run()`. Phase 2 + Phase 3 wire the real
dora-rs daemon + the CartPole control-loop demo.

## Status

- **ADR-0076 Phase 1 — delivered.** Synthetic runtime: trampolines
  + drops + canned 1-event mock; proves the .cb→Rust→back-to-.cb
  callback chain end-to-end without depending on the real dora-rs
  coordinator. Same pattern as F65's synthetic-LLM provider — the
  chain is proven; the real infra is a follow-up sprint.
- **ADR-0076 Phase 2 — multi-IO subset delivered (synthetic trampoline).**
  MULTI-INPUT DISPATCH (`node.run()` injects one canned event per
  declared input id; the handler fires once per input) + SEND_OUTPUT
  capture (`event.send_output(output_id, payload)` validates against the
  declared outputs + captures to stdout as `output[<id>]=<payload>`). The
  `@dora.node(inputs=[...], outputs=[...])` decorator desugar now THREADS
  the IO metadata to the trampoline (it was validated-then-dropped in
  Phase 1 — finding F68) via `dora.declare_input` / `dora.declare_output`
  prologue register-calls. Still SYNTHETIC — no real zenoh broker.
  DEFERRED: Arrow list/dict payloads (ADR-0076c), the dora-yaml config
  path, the real `dora-node-api` dep + zenoh runtime, the typed
  compile-time `DoraUnknownOutputId` reject (Phase 2 catches an undeclared
  output at RUNTIME via a `-1` sentinel + stderr diagnostic).
- **#146 dora-cb Phase A — REAL `dora-node-api` runtime, behind an opt-in
  feature (delivered).** `cobrust-dora` now grows an OPTIONAL
  `dora-node-api = "=0.5.0"` dependency (exact-pinned, `default-features =
  false`) gated behind a `dora-real` feature (NOT in `default` — mirrors how
  `coil` gates `faer` behind `coil-faer`). With `--features dora-real`, the
  L4 runtime body swaps from the synthetic canned-event trampoline to a REAL
  `DoraNode::init_from_env()` + a blocking `events.recv()` loop firing the
  `.cb` callback once per real `Event::Input`; `event.data_str()` decodes the
  live `arrow::array::ArrayRef`; `event.send_output(id, payload)` publishes a
  real Arrow `StringArray` via the ambient live node. The DEFAULT build stays
  the SYNTHETIC trampoline (light, wasm-buildable, unchanged). **The C-ABI
  symbol surface + the ecosystem manifest + the codegen callback do NOT
  change** — only the `cabi.rs` bodies + two handle-struct shapes are
  `#[cfg]`-split (the dora-real-integration-plan §9 spike's load-bearing
  insight: a `cabi.rs`-local body swap, not a compiler change). The ONE
  compiler-side change is a target-gated macOS `-framework CoreFoundation`
  link flag in `cobrust-cli/src/build.rs` (emitted only when a `dora`-importing
  program is linked on a macOS target). Real-dora is NATIVE-ONLY (tokio-net
  hard-fails on wasm32 per §9, so the wasm dora story stays synthetic).
- **ADR-0092 — typed compile-time `DoraUnknownOutputId` reject
  (delivered).** Lifts the `event.send_output("<id>", _)` undeclared-id
  reject from RUNTIME (the `cabi.rs` `-1` sentinel + stderr) to COMPILE
  TIME (CLAUDE.md §2.5-A). A module PRE-PASS in `cobrust-types`
  (`check.rs::collect_dora_declared_outputs`) collects every
  `dora.declare_output("<lit>")` string-literal id (the
  `@dora.node(outputs=[...])` desugar) into
  `Ctx.dora_declared_outputs: Option<BTreeSet<String>>`; the
  `event.send_output` method-synth (`try_synth_ecosystem_call` Case 2)
  rejects a string-LITERAL id NOT in the set with
  `TypeError::DoraUnknownOutputId`. SKIP edges (no false-positive): a
  non-literal id (the runtime backstop stays) + a bare `@dora.node` (None
  set ⇒ inert). The §2.5-B FIX prints the declared-output list + a
  nearest-match (`did you mean \`twist\`?`, inline Levenshtein, NO new
  dep). NO new IR field, NO new manifest op — reuses the existing
  `lookup_module_fn("dora","declare_output")` recognition. The Arrow
  payload surface stays a SEPARATE next dispatch.
- **Phase B / Phase 3 — proposed.** `coil.Buffer ↔ Arrow` array payloads +
  multi-port routing (Phase B); the real-robotics CartPole simulation demo
  + cross-machine orchestration (Phase 3).

## Public surface (Phase 1 + Phase 2)

C-ABI symbols (`#[no_mangle] extern "C"`) declared in
`crates/cobrust-dora/src/cabi.rs`:

```text
# Phase 1
__cobrust_dora_node_new(name: *mut Str) -> *mut Node
__cobrust_dora_node_node(handler: *const c_void) -> i64
__cobrust_dora_node_run(node: *mut Node) -> i64
__cobrust_dora_node_shutdown(node: *mut Node) -> i64
__cobrust_dora_event_id(event: *mut Event) -> *mut Str
__cobrust_dora_event_data_str(event: *mut Event) -> *mut Str
__cobrust_dora_node_drop(node: *mut Node) -> void
__cobrust_dora_event_drop(event: *mut Event) -> void
# Phase 2 (multi-IO)
__cobrust_dora_declare_input(id: *mut Str) -> i64
__cobrust_dora_declare_output(id: *mut Str) -> i64
__cobrust_dora_event_send_output(
    event: *mut Event, output_id: *mut Str, payload: *mut Str
) -> i64   # 0 = emitted; -1 = undeclared output id (fail-closed)
```

Manifest entries (`crates/cobrust-types/src/ecosystem.rs`):

- `dora.Node(name: str) -> dora.Node` — construct synthetic Node.
- `dora.node(handler) -> i64` — register a `fn(dora.Event) -> i64`
  callback in the process-global slot (single-handler registration).
- `dora.declare_input(id: str) -> i64` (Phase 2) — push a declared input
  id onto the trampoline's `DECLARED_INPUTS` queue. The decorator desugar
  emits one per declared input.
- `dora.declare_output(id: str) -> i64` (Phase 2) — push a declared output
  id onto the trampoline's `DECLARED_OUTPUTS` set. Idempotent on a repeat.
- `Node.run() -> i64` — with NO declared inputs, invoke the handler once
  with a canned `("camera", "frame_001")` Event (Phase-1 path); with
  declared inputs, inject ONE canned event per declared input id (Phase-2
  multi-input dispatch).
- `Node.shutdown() -> i64` — soft-flag the Node shut down (Phase 1
  no-op toward a coordinator; a later phase sends a real signal).
- `Event.id() -> str` — input id the event arrived on.
- `Event.data_str() -> str` — payload as UTF-8 string (`"frame_001"` for
  `camera`; `"frame_<id>"` for other declared inputs).
- `Event.send_output(output_id: str, payload: str) -> i64` (Phase 2) —
  emit a Str payload on a declared output port. Validates `output_id`
  against `DECLARED_OUTPUTS` (undeclared → `-1` + stderr diagnostic, NOT a
  silent drop); captures the emission to stdout as `output[<id>]=<payload>`.
  Hangs off the Event handle (NOT `dora.Node`) because the Event is the
  ONLY handle in the handler's scope.

ADT slot allocation (`DORA_NODE_ADT = AdtId(ECO_ADT_BASE + 0x600)`,
`DORA_EVENT_ADT = ECO_ADT_BASE + 0x601`) — seventh per-module 256-slot
block; `0x602..0x6FF` reserved for Phase 2 follow-ups (ArrowArray,
Metadata, Ros2Subscription handles).

## Scope window (Phase 1 + Phase 2 synthetic)

In scope:

- Synthetic runtime: `dora.node(handler)` installs into a process-global
  slot; `node.run()` injects canned events (one per declared input, or a
  single `("camera", "frame_001")` fallback when none declared).
- MULTI-INPUT dispatch (Phase 2): the handler fires once per declared
  input id; `event.id()` discriminates.
- SEND_OUTPUT capture (Phase 2): `event.send_output(id, payload)` with
  declared-output validation + stdout capture.
- The borrow shim pattern (`event.id() -> str` + `event.data_str() -> str`
  + `event.send_output(...)`) mirrors pit.Request's `body()` shape per
  ADR-0073 §2 D6.
- Drop discipline: Node owns its drop schedule (`DROP_COUNT`
  instrument); Event is Rust-owned (trampoline allocates + frees the
  `Box<Event>` per callback invocation — `handle_drop_symbol(DORA_EVENT_ADT)`
  returns `None`).

Out of scope (real-infra Phase 2 / Phase 3 follow-ups — DEFERRED honestly):

- Real `dora-node-api` dependency + real coordinator orchestration + the
  real zenoh broker (the trampoline stays SYNTHETIC).
- Arrow list/dict `RecordBatch` payloads beyond i64+str scalar (ADR-0076c).
- Yaml-loaded dataflows (`dora.run("dataflow.yml")`).
- ~~Typed compile-time `DoraUnknownOutputId` reject~~ — **DONE (ADR-0092)**:
  a string-literal undeclared `send_output` id now rejects at
  `cobrust check` / `cobrust build`; a non-literal id keeps the runtime
  `-1` backstop.
- `for event in node:` polling iterator form.
- ROS2 bridge surface (sub-ADR 0076a).
- riscv64 cross-build (ADR-0075 Phase 1 dependency — Phase 3 stretch).
- Real-robotics CartPole demo (Phase 3 deliverable).

## Decorator-translation table

| Python decorator | `.cb` form | Desugars to (HIR prologue) |
|---|---|---|
| `@dora.node(inputs=["tick","camera"], outputs=["reading"])` over `fn on_event(event)` | (write verbatim) | `dora.declare_input("tick")` + `dora.declare_input("camera")` + `dora.declare_output("reading")` + `dora.node(on_event)` |
| `@dora.node` (bare) over `fn detect(event)` | (write verbatim) | `dora.node(detect)` (no declarations → single canned event) |
| `node = Node()` | `let node = dora.Node("detector")` | (unchanged) |
| `node.send_output("reading", v)` (Python: `node`-method) | `event.send_output("reading", payload)` | Event-handle method (Event is the only in-scope handle) |
| `for event in node:` | (deferred polling form) | (deferred polling form) |

The decorator threads each `inputs=`/`outputs=` port id through to the
synthetic trampoline as a `declare_input`/`declare_output` register-call
inserted at main's prologue BEFORE `dora.node(handler)` (so the runtime
sees the metadata before the handler installs). When no inputs/outputs are
declared (bare `@dora.node` or the explicit `dora.node(detect)` form), no
declaration calls are emitted and the trampoline falls back to the single
canned `("camera", "frame_001")` event — Phase-1 behavior preserved.

## Invariants

- **No silent translations.** Every shim has a per-function doc
  comment citing its ADR-0076 / ADR-0073 origin.
- **Drop-once discipline.** `__cobrust_dora_node_drop` asserts
  `DROP_COUNT` increments by exactly one per Node; the in-crate test
  suite verifies (`node_new_then_drop_increments_counter_once`).
- **Event Rust-owned.** The `.cb` side NEVER drops a `dora.Event`
  local (manifest `handle_drop_symbol` returns `None`); the trampoline
  owns the `Box<Event>` for the callback duration and frees it on return.
- **Panic safety.** Every callback invocation is wrapped in
  `std::panic::catch_unwind`; a panic across the C ABI aborts via
  `std::process::abort` (ADR-0073 §3 Q5).

## SYNTHETIC runtime contract (Phase 1 + Phase 2)

Phase 1 + Phase 2 ship intentionally without a `dora-node-api` dependency.
`__cobrust_dora_node_run` reads the process-global `REGISTERED_HANDLER`
slot + the `DECLARED_INPUTS` queue, then:

- **No declared inputs** (Phase-1 explicit `dora.node(detect)` form):
  allocate a canned `DoraEventHandle { id: "camera", data_str:
  "frame_001" }`, invoke the handler once, free the Event box, return 0.
- **N declared inputs** (Phase-2 `@dora.node(inputs=[...])` form): inject
  ONE canned event per declared input id (camera → `frame_001`, other →
  `frame_<id>`), invoking the handler N times in declaration order.

`event.send_output(id, payload)` validates `id` against `DECLARED_OUTPUTS`
and prints `output[<id>]=<payload>` (the synthetic capture the E2E
asserts). This mirrors F65's synthetic-LLM provider precedent: the chain
is proven without the heavy infra wired. The `dora-real` feature (below)
replaces this synthetic loop with the real `DoraNode` + `events.recv()`
driven dispatch.

## REAL runtime contract — `dora-real` feature (#146 Phase A)

Building `cobrust-dora --features dora-real` swaps the L4 bodies (in the
`#[cfg(feature = "dora-real")] mod real` submodule of `cabi.rs`) from the
synthetic trampoline to the real dora-node-api path. The exported
`#[unsafe(no_mangle)]` shim signatures are SINGLE-DEFINITION across both
builds (the ABI is identical); only the private bodies + the
`DoraNodeHandle` / `DoraEventHandle` fields are `#[cfg]`-split.

| Shim | synthetic (default) | real (`dora-real`) |
|---|---|---|
| `node_new` | name-only handle | `DoraNode::init_from_env()` → stash `(DoraNode, EventStream)` + a multi-thread tokio runtime in the handle (`None` on init failure → `run` returns `-1`) |
| `node_run` | canned-event loop (one per declared input, or `("camera","frame_001")`) | enter the tokio runtime; drain the REAL `EventStream` via blocking `recv()`; fire the `.cb` callback per `Event::Input`; `break` on `Event::Stop` / `None` |
| `event_id` / `event_data_str` | canned strings | `id.as_str()` + the DECODED Arrow `ArrayRef` payload (`String::try_from(&ArrowData)` for a Utf8 `StringArray`; debug fallback otherwise) |
| `event_send_output` | declared-output validate + `println!("output[id]=...")` capture | declared-output validate + publish a length-1 Arrow `StringArray` on the `id` port via the ambient live `DoraNode` (plan §4.4 option 1 — the run loop installs `&mut node` in a thread-local for the callback window) |
| `node_shutdown` / `node_drop` | soft flag / box-drop | additionally drop the live `DoraNode`/`EventStream` (leave the dora coordinator) |

The callback box / `catch_unwind` / abort-on-panic / free discipline is
SINGLE-SOURCED in `fire_callback` (shared by both loops), so panic safety +
drop-once hold identically. No panic crosses the C ABI on either build.

**Hermetic real testing (no daemon).** dora 0.5.0's `integration_testing`
mode — driven by the `DORA_TEST_WITH_INPUTS` env var pointing at a JSON
events file — makes `init_from_env()` construct a REAL node that feeds those
events through the REAL `EventStream`, with NO coordinator/daemon. The
F36-honest E2E (`crates/cobrust-cli/tests/dora_real_node_e2e.rs`) uses this
for a genuine live round-trip (see Gates).

**Weight / portability (plan §9 spike, accepted):** `libdora.a` 17 MB →
~450 MB; stripped `.cb` binary ~85 MB; lock ~559 → ~663 crates; +2
*unmaintained* (not CVE) audit ignores (`RUSTSEC-2025-0141` bincode +
`RUSTSEC-2025-0057` fxhash, both behind the optional feature). Real-dora is
NATIVE-ONLY — `tokio-net` hard-fails on wasm32, so the wasm dora story stays
SYNTHETIC-default (the default `cargo build -p cobrust-dora --target
wasm32-wasip1` is green; `--features dora-real` is not a wasm target).

## Gates (Phase 1 + Phase 2 — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L1 | typecheck manifest | `cobrust check` on the dora examples | passes |
| L2.build | `cargo build -p cobrust-dora` | zero warnings | passes |
| L2.behavior | in-crate cabi tests | 8/8 — drop-once + null tolerance + trampoline + shutdown + multi-input dispatch + send_output validate + declare idempotent | passes |
| L3.e2e.p1 | compile + link + run | `cargo test -p cobrust-cli --test dora_hello_e2e` 3/3 + `--test decorator_dora_e2e` 6/6 | passes |
| L3.e2e.p2 | compile + link + run | `cargo test -p cobrust-cli --test dora_multi_io_e2e` 3/3 (multi-input dispatch + send_output capture + single-input no-regression) | passes |
| L3.e2e.real | **F36-honest real proof** | `cargo test -p cobrust-cli --test dora_real_node_e2e` 2/2 — Part A: a `--features dora-real` `.cb` binary carries REAL `dora_node_api`+`arrow` symbols (`nm`), proving the real path LINKED (not the trampoline); Part B: a LIVE real `DoraNode`+`EventStream` round-trip via dora's hermetic `integration_testing` mode delivers a unique marker the handler prints (the synthetic trampoline would print canned `frame_tick`). Self-skips clean when the heavy real archive is unavailable; `COBRUST_DORA_REAL_E2E=1` makes a skip a hard failure (the CI real-gate lane). | passes (strict, macOS) |
| L2.behavior.real | `cargo build/clippy -p cobrust-dora --features dora-real` | zero warnings; the synthetic-contract cabi unit tests are `#[cfg]`-gated to `not(dora-real)` (the real path is E2E-proven) | passes |
| wasm | `cargo build -p cobrust-dora --target wasm32-wasip1` (DEFAULT) | synthetic-default cross-compiles to wasm32 (real-dora is native-only) | passes |

## Done means (Phase 1 — DONE)

- [x] Workspace member `crates/cobrust-dora/` with crate-type rlib +
      cdylib + staticlib.
- [x] 6 trampolines (`node_new` / `node_node` / `node_run` /
      `node_shutdown` / `event_id` / `event_data_str`) + 2 drops.
- [x] Manifest entries in `cobrust-types/src/ecosystem.rs` claiming
      AdtId 0x600 block (2 ADTs claimed; 0x602..0x6FF reserved).
- [x] codegen extern declarations in `cobrust-codegen/src/llvm_backend.rs`.
- [x] Intrinsic prefix recognizer in
      `cobrust-cli/src/build/intrinsics.rs::ecosystem_module_for_symbol`.
- [x] Demo `examples/dora_hello/main.cb` + E2E test
      `crates/cobrust-cli/tests/dora_hello_e2e.rs` (1 positive + 2
      negative typecheck).

## Done means (Phase 2 — multi-IO subset DONE; real-infra open)

Delivered (synthetic trampoline):

- [x] Multi-input dispatch — `node.run()` injects one canned event per
      declared input id (`__cobrust_dora_declare_input` + the run loop).
- [x] `event.send_output(id, payload)` — declared-output validation +
      stdout capture (`__cobrust_dora_event_send_output` +
      `__cobrust_dora_declare_output`).
- [x] `@dora.node(inputs=..., outputs=...)` decorator THREADS the IO
      metadata to the runtime (cobrust-hir `build_eco_module_register_calls`
      emits `declare_input`/`declare_output` prologue calls — finding F68's
      drop-then-validate gap closed).
- [x] Manifest rows + codegen externs + MIR retarget (auto via the
      manifest chain) + cabi shims for the 3 new symbols.
- [x] E2E `crates/cobrust-cli/tests/dora_multi_io_e2e.rs` 3/3 + cabi
      unit tests 8/8.

Delivered (#146 Phase A — REAL runtime behind the `dora-real` feature):

- [x] OPTIONAL `dora-node-api = "=0.5.0"` exact-pinned dep
      (`default-features = false`) + an optional `tokio` dep, both gated
      behind a `dora-real` feature NOT in `default` (mirrors `coil-faer`).
      Pin corrected from the stale ADR's `0.2.x` per
      dora-real-integration-plan §3.0 (F35-sibling: the crate is
      independently versioned at 0.5.0 as of 2026-06-01).
- [x] REAL `DoraNode::init_from_env()` + blocking `events.recv()` loop +
      a self-owned multi-thread tokio runtime (plan §3.2) — replaces the
      synthetic trampoline under the feature.
- [x] Scalar/str `event.send_output` publishes a real Arrow `StringArray`
      via the ambient live node (plan §4.4 option 1).
- [x] Target-gated macOS `-framework CoreFoundation` link flag in
      `cobrust-cli/src/build.rs` (the lean `default-features = false` config
      needs ONLY CoreFoundation — NOT IOKit/Security — per §9).
- [x] F36-honest E2E (`dora_real_node_e2e.rs`) — real-symbol link proof +
      a live `integration_testing` round-trip (mutation-survivable).
- [x] +2 audit ignores in `ci.yml` (`RUSTSEC-2025-0141` bincode +
      `RUSTSEC-2025-0057` fxhash — unmaintained, behind the optional feature).

Open (real-infra Phase B/3):

- [ ] `coil.Buffer ↔ Arrow` array payloads (`ndarray::ArrayD<f64> ↔
      arrow::Float64Array` bridge) beyond the Phase-A scalar/str
      (ADR-0076c) — the payload-surface design question (`coil.Buffer` vs a
      `pa`-shim) is the most consequential open choice (plan §4.3).
- [ ] Yaml-loaded dataflows (`dora.run("dataflow.yml")`).
- [x] Typed compile-time `DoraUnknownOutputId` reject (ADR-0092) — a
      module pre-pass collects the declared-output set on `Ctx`
      (`Option<BTreeSet<String>>`), NOT on `TypedModule`; the
      `event.send_output` synth rejects a string-literal undeclared id at
      type-check with a §2.5-B FIX (declared list + nearest-match). A
      non-literal id keeps the runtime `-1` backstop.
- [ ] `for event in node:` polling iterator form.

## Non-goals

- **Not** a re-implementation of dora-rs in Cobrust — the chain is
  C-ABI shim FFI per ADR-0076 §3 (Q2 decision).
- **Not** a translation of `dora-node-api-python` to Cobrust — the
  C-ABI binding direction is `.cb → Rust`, not the reverse.
- **Not** a ROS2 bridge — ROS2 is dora-rs's concern (`ros2://topic`
  inputs surface as plain Arrow Events to Cobrust nodes); sub-ADR
  0076a tracks the publish direction.

## Cross-references

- `mod:types` — ecosystem manifest at `crates/cobrust-types/src/ecosystem.rs`.
- `mod:mir` — `try_lower_ecosystem_call` chain (unchanged).
- `mod:codegen` — extern declarations + fn-pointer materialisation.
- `mod:stdlib` — `__cobrust_str_*` primitives the cabi shims bind to.
- `mod:pit` — sister sixth module (Flask, first callback proof).
- `mod:hood` — sister seventh module (click, second callback proof).
- [adr:0076](../adr/0076-dora-cb-stream-y.md) — Phase 1/2/3 plan.
- [adr:0072](../adr/0072-cb-ecosystem-import-wiring.md) — L1→L5 chain.
- [adr:0073](../adr/0073-cb-callback-marshalling.md) — trampoline pattern.
- [strategy:dora-cb-architecture](../strategy/dora-cb-architecture.md) — companion architecture doc.
- dora-rs upstream — https://github.com/dora-rs/dora.
