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
- **Phase 3 — proposed.** Real-robotics CartPole simulation demo + real
  dora-rs orchestration.

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
- Typed compile-time `DoraUnknownOutputId` reject (Phase 2 is RUNTIME
  validation via the `-1` sentinel).
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
is proven without the heavy infra wired. A later phase replaces this
synthetic loop with the real `DoraNode::events().into_iter()` driven
dispatch over the zenoh broker.

## Gates (Phase 1 + Phase 2 — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L1 | typecheck manifest | `cobrust check` on the dora examples | passes |
| L2.build | `cargo build -p cobrust-dora` | zero warnings | passes |
| L2.behavior | in-crate cabi tests | 8/8 — drop-once + null tolerance + trampoline + shutdown + multi-input dispatch + send_output validate + declare idempotent | passes |
| L3.e2e.p1 | compile + link + run | `cargo test -p cobrust-cli --test dora_hello_e2e` 3/3 + `--test decorator_dora_e2e` 6/6 | passes |
| L3.e2e.p2 | compile + link + run | `cargo test -p cobrust-cli --test dora_multi_io_e2e` 3/3 (multi-input dispatch + send_output capture + single-input no-regression) | passes |

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

Open (real-infra Phase 2/3):

- [ ] Real `dora-node-api = "=0.2.x"` exact-pinned dep + tokio
      runtime guest-mode integration (mirrors strike's pattern) +
      real zenoh broker (replaces the synthetic trampoline).
- [ ] Arrow list/dict `RecordBatch` payloads beyond i64+str scalar
      (ADR-0076c) — `pa.array_i64(...)`.
- [ ] Yaml-loaded dataflows (`dora.run("dataflow.yml")`).
- [ ] Typed compile-time `DoraUnknownOutputId` reject (Phase 2 catches an
      undeclared output at RUNTIME via the `-1` sentinel; a compile-time
      check wants the static declared-output set on `TypedModule`).
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
