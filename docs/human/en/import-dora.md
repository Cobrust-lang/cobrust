# `import dora` — robotics dataflow nodes from Cobrust (callback marshalling third proof)

> Status: ADR-0076 Phase 1 (synthetic runtime). The NINTH ecosystem
> module — and the THIRD to cross a callback through the C ABI (after
> pit's `fn(Request) -> Response` and hood's `fn() -> i64`). The shape
> here is `fn(dora.Event) -> i64`, mixing pit's Event-receiver borrow
> pattern with hood's i64 exit-code intent.
>
> Phase 1 is intentionally synthetic — `node.run()` mocks one canned
> `("camera", "frame_001")` event arrival without depending on the real
> dora-rs daemon. The chain is proven; Phase 2 wires the real dora-rs
> orchestration (multi-IO, yaml-loaded dataflows, ROS2 bridge access).

## Example first

```python
import dora

fn detect(event: dora.Event) -> i64:
    let frame: str = event.data_str()
    print_no_nl("got frame: ")
    print(frame)
    return 0

fn main() -> i64:
    let node = dora.Node("detector")
    let _ = dora.node(detect)
    let _ = node.run()
    return 0
```

Build and run:

```bash
cobrust build prog.cb -o prog
./prog
# got frame: frame_001
```

## What you get (Phase 1 surface)

- **`dora.Node(name) -> dora.Node`** — construct a synthetic dataflow
  node handle. `name` is the node identifier (e.g. `"detector"`,
  `"sensor"`). The handle drops once at scope exit via
  `__cobrust_dora_node_drop`.
- **`dora.node(handler) -> i64`** — register a top-level `fn(event:
  dora.Event) -> i64` callback as the node's event handler. Phase 1
  supports a single handler per process (multi-handler routing per
  input id is a Phase 2 feature alongside the
  `@dora.node(inputs=..., outputs=...)` decorator desugar). Returns 0
  (a sentinel — registration is a side-effect on the global slot);
  use `let _ = dora.node(detect)` to discard it.
- **`Node.run() -> i64`** — invoke the registered handler exactly once
  with the canned Phase 1 mock event (`id="camera"`,
  `data_str="frame_001"`). Returns 0 on success; -1 if no handler was
  registered. Phase 2 replaces this with the real
  `EventStream::into_iter()` loop driven by an actual dora coordinator.
- **`Node.shutdown() -> i64`** — flip a soft shutdown flag on the Node.
  Phase 1 no-op toward a real coordinator; Phase 2 sends the real
  signal. Returns 0.
- **`Event.id() -> str`** — the input id this event arrived on (e.g.
  `"camera"`). Borrow shim — allocates a fresh Cobrust `str` buffer.
- **`Event.data_str() -> str`** — the event payload as a UTF-8 string.
  Phase 1 surface is `str`-only; Phase 2 widens to Arrow `RecordBatch`
  accessors via `event.data_arrow()` for typed multi-element payloads.

## What you don't get (Phase 1 — deferred)

- Multi-input / multi-output orchestration (Phase 2 with
  `@dora.node(inputs=["a", "b"], outputs=["c"])` decorator).
- Real dora-rs daemon integration (Phase 2 with `dora-node-api` dep
  + `tokio` runtime guest-mode).
- Yaml-loaded dataflows (`dora.run("dataflow.yml")` — Phase 2).
- Arrow `RecordBatch` payload accessors (`event.data_arrow()` +
  primitive widening — Phase 2).
- ROS2 bridge publish surface (sub-ADR 0076a — Phase 3).
- riscv64 cross-build of `cobrust-dora` (ADR-0075 Phase 1 dependency
  — Phase 3 stretch).
- Real-robotics CartPole simulation demo (Phase 3 deliverable).

## Why FFI not translation?

dora-rs's hot path (Zenoh shared-memory transport + Arrow zero-copy +
tokio coordination) is the runtime's core competency. Re-implementing
any of that in Cobrust would chase a moving SemVer-0 target while
wasting the dora-rs investment. Cobrust nodes participate at the
`dora-node-api` Rust crate boundary; perf is identical to a
hand-written Rust dora node. See ADR-0076 §3 for the design rationale.

## Drop discipline

- `dora.Node` is a `.cb`-owned handle — scope-exit drops it once via
  `__cobrust_dora_node_drop`.
- `dora.Event` is Rust-owned — the trampoline owns the `Box<Event>`
  for the callback's duration and frees it on return. The `.cb` side
  must NOT drop a `dora.Event` local; the manifest enforces this by
  returning `None` from `handle_drop_symbol(DORA_EVENT_ADT)`.

## Cross-references

- [`import pit`](import-pit.md) — sister sixth module (first callback proof).
- [`import hood`](import-hood.md) — sister seventh module (second callback proof).
- ADR-0076 (`docs/agent/adr/0076-dora-cb-stream-y.md`) — Phase 1/2/3 architecture.
- ADR-0072 (`docs/agent/adr/0072-cb-ecosystem-import-wiring.md`) — the L1→L5 chain.
- ADR-0073 (`docs/agent/adr/0073-cb-callback-marshalling.md`) — the trampoline pattern.
- dora-rs upstream — <https://github.com/dora-rs/dora>.
