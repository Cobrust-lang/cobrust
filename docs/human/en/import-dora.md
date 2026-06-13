# `import dora` — robotics dataflow nodes from Cobrust (callback marshalling third proof)

> Status: ADR-0076 Phase 2 (synthetic runtime, multi-IO subset). The
> NINTH ecosystem module — and the THIRD to cross a callback through the
> C ABI (after pit's `fn(Request) -> Response` and hood's `fn() -> i64`).
> The shape here is `fn(dora.Event) -> i64`, mixing pit's Event-receiver
> borrow pattern with hood's i64 exit-code intent.
>
> The DEFAULT runtime is intentionally synthetic — `node.run()` injects
> canned events without depending on the real dora-rs daemon. Phase 1 proved
> the single-input chain; Phase 2 adds **multi-input dispatch** +
> **`event.send_output(...)`**. **#146 Phase A** then makes it REAL behind an
> opt-in `dora-real` feature — a genuine `dora-node-api` `DoraNode` +
> `events.recv()` loop (see "Going real" below). Real Arrow array payloads,
> yaml-loaded dataflows, and the ROS2 bridge remain later phases.

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
  The payload surface is `str`-only for now; Arrow `RecordBatch`
  accessors for typed multi-element payloads are deferred (ADR-0076c).

## Multi-IO: many inputs, one output (Phase 2)

Declare the node's input + output ports with the
`@dora.node(inputs=[...], outputs=[...])` decorator. The handler then
fires **once per declared input** — dispatch on `event.id()` — and emits
results with **`event.send_output(output_id, payload)`**:

```python
import dora

@dora.node(inputs=["tick", "camera"], outputs=["reading"])
fn on_event(event: dora.Event) -> i64:
    if str_eq_lit(event.id(), "camera") == 1:
        let payload: str = event.data_str()
        let _ = event.send_output("reading", payload)
    print_no_nl("saw input: ")
    print(event.id())
    return 0

fn main() -> i64:
    let node = dora.Node("sensor")
    let _ = node.run()
    return 0
```

```bash
cobrust build prog.cb -o prog
./prog
# saw input: tick
# output[reading]=frame_001
# saw input: camera
```

- **Multi-input dispatch** — declaring two inputs makes the synthetic
  runtime inject one canned event per input id (in declaration order), so
  the handler runs twice. `event.id()` tells the two apart. (The canned
  payload is `frame_001` for `camera`, `frame_<id>` for other inputs —
  a real broker supplies the actual data.)
- **`event.send_output(output_id, payload) -> i64`** — emit a `str`
  payload on a **declared** output port. The output id is validated
  against the `outputs=[...]` you declared. When the id is a **string
  literal** (`send_output("pose", ...)`), an undeclared id is now caught
  at **compile time** (ADR-0092): `cobrust check` / `cobrust build` fails
  with `unknown dora output id …; declared outputs: [...]` and, on a near
  typo, a `did you mean …?` suggestion — so you fix it before you ever
  run. A **computed** id (a variable) cannot be checked statically, so it
  keeps the runtime guard (a clear stderr message + a `-1` return, never a
  silent drop). The synthetic runtime captures a successful emission to
  stdout as `output[<id>]=<payload>`. Returns 0 on a successful emit.
  `send_output` hangs off the **Event** (not the Node) because the Event
  is the one handle in the handler's scope.

> Why `str_eq_lit(event.id(), "camera") == 1` and not `event.id() ==
> "camera"`? `str`-vs-`str` `==` is a separate language feature; the
> `str_eq_lit(...)` helper is the proven dispatch form today.

## Typed numeric payloads — `coil.Buffer` in, `coil.Buffer` out

A `str` payload is fine for commands and labels, but a robot's real data
is **numbers**: a state vector, a sensor tensor, a control command. dora
moves these as typed [Apache Arrow](https://arrow.apache.org/) arrays — and
Cobrust hands them to you as a **`coil.Buffer`** (the same array type
`import coil` gives you for math). One array type spans the numeric pillar
*and* the dora wire — no second type to learn, no conversion ceremony:

```python
import dora
import coil

@dora.node(inputs=["state"], outputs=["action"])
fn policy(event: dora.Event) -> i64:
    let obs: coil.Buffer = event.data_buffer()   # a typed numeric input
    let m: f64 = coil.mean(obs)                   # do real numpy-style math
    let action: coil.Buffer = coil.full(3, m)     # build a typed output
    let _ = event.send_output_buffer("action", action)  # emit it
    return 0

fn main() -> i64:
    let node = dora.Node("policy_node")
    let _ = node.run()
    return 0
```

- **`event.data_buffer() -> coil.Buffer`** — read a typed-numeric input
  payload as a `coil.Buffer`. The supported element types are
  **`float64`, `float32`, `int64`, `int32`, and `bool`** — the dtypes that
  overlap between Arrow and `coil`. An `int64` array stays `int64` (it is
  **not** silently turned into a float); a `float64` array stays `float64`.
  The Buffer is **yours**: it is freed automatically when it goes out of
  scope (exactly once — no leak, no double-free). On the synthetic default
  build (no broker) you get a canned `float64 [1.0, 2.0, 3.0]` so the chain
  runs in tests; under `--features dora-real` you get the real decoded
  array.
- **`event.send_output_buffer(output_id, buffer) -> i64`** — emit a
  `coil.Buffer` as a typed Arrow array on a **declared** output port. It is
  a **distinct method** from `send_output` (not an overload) so the compiler
  — and an LLM writing your node — always know which one you mean. The same
  compile-time output-id check applies: a string-literal typo
  (`send_output_buffer("acton", ...)` when you declared `action`) is caught
  at `cobrust check`. The `buffer` you pass is **borrowed**, not consumed —
  your scope still owns it and drops it once.

### Raw byte payloads — `data_bytes()` / `send_output_bytes()`

For a **raw byte** payload (an image blob, a serialized message, anything
that is not one of the 5 numeric dtypes), use the `bytes` accessor — the
raw-bytes sibling of the Buffer pair:

```python
import dora

@dora.node(inputs=["camera"], outputs=["reply"])
fn handler(event: dora.Event) -> i64:
    let raw: bytes = event.data_bytes()       # an Arrow Binary/UInt8 blob
    print(raw.hex())                           # bytes are first-class
    let _ = event.send_output_bytes("reply", raw)  # emit it back
    return 0

fn main() -> i64:
    let node = dora.Node("bytes_node")
    let _ = node.run()
    return 0
```

- **`event.data_bytes() -> bytes`** — read a raw byte payload (an Arrow
  `Binary` blob or a flat `UInt8` list) as a first-class `bytes` value. This
  is the **complement** of `data_buffer()`: `data_buffer()` handles the 5
  numeric dtypes, `data_bytes()` handles raw bytes — the two never overlap.
  Every byte is preserved **exactly**: a `0xFF` round-trips unchanged (the
  raw-bytes path is never UTF-8-lossy, unlike a string). The `bytes` is
  **yours** — freed once at scope exit. On the synthetic build you get a
  canned non-UTF-8 `b"\x00\xff\x01"`; under `--features dora-real` you get
  the real decoded blob. A null-bearing / non-bytes payload returns an
  **empty** `bytes` (and logs why), never a silent garbage read.
- **`event.send_output_bytes(output_id, b) -> i64`** — emit a `bytes` value
  as an Arrow `Binary` blob on a **declared** output port. Same distinct-name
  + compile-time output-id-typo-catch discipline as `send_output_buffer`. The
  `b` you pass is **borrowed**, not consumed.

> **Why `coil.Buffer` and not a new `pa.array` type?** One array type for
> both math and the wire is the elegant, one-way-to-do-it choice (ADR-0076c).
> A robot policy receives a `Buffer`, runs `coil` math on it, and emits a
> `Buffer` — no `Frame ↔ Buffer` juggling. (This is reversible: a pyarrow-style
> surface could be added later if it is ever wanted.)

> **Images and text each have their own accessor.** Camera frames are
> `uint8`/`Binary` and commands are `utf8` — neither is one of the 5 numeric
> dtypes above, so `data_buffer()` is not the right tool. Use
> `event.data_bytes()` for a raw byte / image blob (see above) and
> `event.data_str()` for a text payload. A non-numeric payload to
> `data_buffer()` returns an empty Buffer (and logs why on the real build) —
> a **named, honest gap**, not a silent failure.

> **Arrays with missing values (nulls) are also a named gap.** Arrow arrays
> can mark some slots as "null" (missing); a `coil.Buffer` is a dense array
> with no concept of "missing". So a **null-bearing** input array does **not**
> round-trip — `data_buffer()` returns an empty Buffer and logs why, rather
> than silently turning a null into `0` / `false` (which would corrupt your
> data without telling you). Send a null-free array (or use `data_str` for a
> non-numeric payload).

## Going real — the `dora-real` feature (#146 Phase A)

The default `cobrust-dora` build is **synthetic** (the `node.run()` loop
injects canned events, so the chain works with no dora daemon — ideal for
fast tests + the wasm target). Building with the **`dora-real`** feature
swaps that loop for a **genuine `dora-node-api` node**:

```bash
# build the REAL dora runtime archive (heavy: pulls the dora + arrow +
# tokio stack — the first build is ~11 minutes)
cargo build -p cobrust-dora --features dora-real
```

With the feature on, the SAME `.cb` source above becomes a real dora node:

- `dora.Node(name)` calls the real `DoraNode::init_from_env()` (the node
  joins a real `dora start` dataflow — the dora daemon spawns it and injects
  its config),
- `node.run()` drains the **real** `EventStream`, firing your handler once
  per real `Event::Input` and stopping on `Event::Stop`,
- `event.data_str()` decodes the **real** Apache Arrow payload that arrived
  on the wire (Utf8 strings); `event.data_buffer()` decodes a **real** typed
  numeric Arrow array (`Float64Array`/`Int64Array`/…) into a `coil.Buffer`,
- `event.send_output(id, payload)` publishes a **real** Arrow string array,
  and `event.send_output_buffer(id, buffer)` publishes a **real** typed Arrow
  numeric array on the node's output port (other nodes receive it).

**The source you write does not change** — the same `import dora` program is
synthetic by default and real under the feature. The C-ABI surface, the
manifest, and codegen are identical; only the runtime body swaps (a
`cabi.rs`-local change, not a compiler change). The one compiler-side
addition is a macOS `-framework CoreFoundation` link flag, emitted
automatically only when a `dora`-importing program is linked on macOS.

Notes + limits:

- **Native-only.** A real dora node uses `tokio` networking, which does not
  exist on wasm32 — so `--features dora-real` is native-only. The wasm dora
  story stays synthetic (the default build cross-compiles to
  `wasm32-wasip1`).
- **Heavy.** The real archive pulls ~100 extra crates; binaries are large
  (~85 MB stripped). This is why the feature is opt-in, not the default
  (mirrors how `coil` gates `faer` behind `coil-faer`).
- **Typed numeric arrays work** (`coil.Buffer ↔ Arrow`, ADR-0076c) for
  `float64/float32/int64/int32/bool`, on both the synthetic + real builds —
  see "Typed numeric payloads" above.

## What you don't get (deferred — honest)

- ~~Typed numeric array payloads~~ — **shipped (ADR-0076c)** for the 5
  dtypes `float64/float32/int64/int32/bool` via `event.data_buffer()` /
  `event.send_output_buffer(...)`. Still deferred: **`uint8`** (camera
  images) + **`utf8`** typed arrays + **n-dimensional shape metadata** — use
  `data_str()` for text and (soon) a `bytes` accessor for raw image blobs.
- Yaml-loaded dataflows (`dora.run("dataflow.yml")`).
- ~~Compile-time rejection of an undeclared output id~~ — **shipped
  (ADR-0092).** A **string-literal** undeclared `send_output` id is now a
  `cobrust check` / `cobrust build` error (`DoraUnknownOutputId`) with a
  declared-list + nearest-match FIX. Only a **computed** (non-literal) id
  still relies on the runtime `-1` guard.
- `for event in node:` polling iterator form.
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
