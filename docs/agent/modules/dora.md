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

- **ADR-0076 Phase 1 — delivered.** Synthetic runtime: 6 trampolines
  + 2 drops + canned 1-event mock; proves the .cb→Rust→back-to-.cb
  callback chain end-to-end without depending on the real dora-rs
  coordinator. Same pattern as F65's synthetic-LLM provider — the
  chain is proven; the real infra is a follow-up sprint.
- **Phase 2 — proposed.** Multi-IO, `@dora.node(inputs=..., outputs=...)`
  decorator desugar (extends ADR-0074 for module-receiver decorators
  per finding F68), real `dora-node-api` dep, yaml-loaded dataflows.
- **Phase 3 — proposed.** Real-robotics CartPole simulation demo.

## Public surface (Phase 1)

C-ABI symbols (`#[no_mangle] extern "C"`) declared in
`crates/cobrust-dora/src/cabi.rs`:

```text
__cobrust_dora_node_new(name: *mut Str) -> *mut Node
__cobrust_dora_node_node(handler: *const c_void) -> i64
__cobrust_dora_node_run(node: *mut Node) -> i64
__cobrust_dora_node_shutdown(node: *mut Node) -> i64
__cobrust_dora_event_id(event: *mut Event) -> *mut Str
__cobrust_dora_event_data_str(event: *mut Event) -> *mut Str
__cobrust_dora_node_drop(node: *mut Node) -> void
__cobrust_dora_event_drop(event: *mut Event) -> void
```

Manifest entries (`crates/cobrust-types/src/ecosystem.rs`):

- `dora.Node(name: str) -> dora.Node` — construct synthetic Node.
- `dora.node(handler) -> i64` — register a `fn(dora.Event) -> i64`
  callback in the process-global slot (Phase 1 single-handler
  registration; Phase 2 widens to multi-handler per-node).
- `Node.run() -> i64` — invoke the registered handler once with a
  canned `("camera", "frame_001")` Event (synthetic Phase 1 mock).
- `Node.shutdown() -> i64` — soft-flag the Node shut down (Phase 1
  no-op toward a coordinator; Phase 2 sends a real signal).
- `Event.id() -> str` — input id (`"camera"` in Phase 1 mock).
- `Event.data_str() -> str` — payload as UTF-8 string (`"frame_001"`
  in Phase 1 mock).

ADT slot allocation (`DORA_NODE_ADT = AdtId(ECO_ADT_BASE + 0x600)`,
`DORA_EVENT_ADT = ECO_ADT_BASE + 0x601`) — seventh per-module 256-slot
block; `0x602..0x6FF` reserved for Phase 2 follow-ups (ArrowArray,
Metadata, Ros2Subscription handles).

## Scope window (Phase 1)

In scope:

- Phase 1 synthetic runtime: `dora.node(handler)` installs into a
  process-global slot; `node.run()` invokes once with a canned event.
- Single-input single-output single-callback shape — enough to prove
  the `.cb`→C-ABI→Rust→.cb callback chain end-to-end.
- The borrow shim pattern (`event.id() -> str` + `event.data_str() -> str`)
  mirrors pit.Request's `body()` + `path_param()` shape per ADR-0073 §2 D6.
- Drop discipline: Node owns its drop schedule (`DROP_COUNT`
  instrument); Event is Rust-owned (trampoline allocates + frees the
  `Box<Event>` per callback invocation — `handle_drop_symbol(DORA_EVENT_ADT)`
  returns `None`).

Out of scope (Phase 2 / Phase 3 follow-ups):

- Real `dora-node-api` dependency + real coordinator orchestration.
- Multi-input / multi-output per-node handler vector.
- `@dora.node(inputs=[...], outputs=[...])` decorator desugar (extends
  ADR-0074 for module-receiver decorators — finding F68).
- Yaml-loaded dataflows (`dora.run("dataflow.yml")`).
- Arrow `RecordBatch` payload accessors (Phase 1 ships `str` only).
- ROS2 bridge surface (sub-ADR 0076a).
- riscv64 cross-build (ADR-0075 Phase 1 dependency — Phase 3 stretch).
- Real-robotics CartPole demo (Phase 3 deliverable).

## Decorator-translation table (Phase 1 + Phase 2 plan)

| Python decorator | Phase 1 `.cb` form (explicit) | Phase 2 `.cb` form (decorator) |
|---|---|---|
| `@dora.node(inputs=["camera"], outputs=["det"])` | `let _ = dora.node(detect)` | `@dora.node(inputs=["camera"], outputs=["det"])` over `fn detect(event)` |
| `node = Node()` | `let node = dora.Node("detector")` | (unchanged) |
| `for event in node:` | (Phase 2 polling form) | (Phase 2 polling form) |

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

## Phase 1 SYNTHETIC runtime contract

Phase 1 ships intentionally without a `dora-node-api` dependency.
`__cobrust_dora_node_run` reads the process-global `REGISTERED_HANDLER`
slot, allocates a canned `DoraEventHandle { id: "camera", data_str:
"frame_001" }`, invokes the handler exactly once, frees the Event box,
and returns 0. This mirrors F65's synthetic-LLM provider precedent:
the chain is proven without the heavy infra wired. Phase 2 replaces
this synthetic loop with the real `DoraNode::events().into_iter()`
driven dispatch.

## Gates (Phase 1 — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L1 | typecheck manifest | `cobrust check examples/dora_hello/main.cb` | passes |
| L2.build | `cargo build -p cobrust-dora` | zero warnings | passes |
| L2.behavior | in-crate cabi tests | 5/5 — drop-once + null tolerance + trampoline + shutdown | passes |
| L3.e2e | compile + link + run | `cargo test -p cobrust-cli --test dora_hello_e2e` 3/3 | passes |

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

## Done means (Phase 2 — open)

- [ ] Real `dora-node-api = "=0.2.x"` exact-pinned dep + tokio
      runtime guest-mode integration (mirrors strike's pattern).
- [ ] `@dora.node(inputs=..., outputs=...)` decorator desugar (extends
      ADR-0074 for module-receiver decorators — finding F68).
- [ ] Multi-input / multi-output per-node handler vector.
- [ ] Yaml-loaded dataflows.
- [ ] Arrow `RecordBatch` payload accessors.

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
