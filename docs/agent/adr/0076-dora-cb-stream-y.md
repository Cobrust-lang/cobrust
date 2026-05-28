---
doc_kind: adr
adr_id: 0076
title: dora-cb Stream Y architecture — `cobrust-dora` crate, FFI wrapper, dataflow → callback bridge
status: draft
date: 2026-05-28
last_verified_commit: bf2974e
decision_owner: cto
relates_to: [adr:0028, adr:0070, adr:0072, adr:0073, adr:0074, adr:0075, "strategy:dora-cb-architecture", "strategy:v0.7.0-dora-cb-integration-roadmap", "strategy:numpy-translation-architecture", "claude.md:§2.5", "claude.md:§4.2"]
---

# ADR-0076: dora-cb Stream Y architecture

## 1. Context

v0.7.0 Stream Y (per ADR-0070 §2.2) is the dora-cb robotics-readiness program.
The user mandate (2026-05-25, verbatim in ADR-0070 §1.1):

> "为之后能做 dora-cb 正式参与机器人项目做准备 [...] 务必都在 0.7.0 前弄好"

Translation: dora-cb must be production-bar — formally participating in
robotics projects — before v0.7.0 release.

ADR-0070 §6 Q4 deferred the architectural decision (FFI vs translation; crate
layout; manifest shape; user-facing surface; phasing). The 2026-05-25 dora-rs
API survey landed as `docs/agent/strategy/v0.7.0-dora-cb-integration-roadmap.md`
and recommended **Option A (FFI to dora-rs Rust runtime)** at §5. This ADR
ratifies that recommendation as the binding decision for Stream Y and
provides the per-layer wiring, the per-phase plan, and the Done-means gates
each phase must clear before the next dispatches.

### Pre-state at HEAD `bf2974e`

- ADR-0072 (.cb ecosystem-import chain) is RATIFIED with **6 modules CI-verified**
  (den / nest / strike / scale / molt + pit + hood). The L1→L5 chain (typecheck
  manifest → MIR intrinsic-rewrite → codegen externs → C-ABI shims → static
  link) is the proven pattern.
- ADR-0073 (.cb↔Rust callback marshalling) is RATIFIED. pit's
  `__cobrust_pit_app_route` trampoline is the load-bearing reference
  implementation; hood's `__cobrust_hood_command_handler` is the second proof
  (committed 2026-05-28, HEAD `bf2974e`).
- ADR-0074 (decorator desugar) is RATIFIED. `@app.route("/x")` over a fn def
  desugars to the explicit `_ = app.route("GET", "/x", fn_name)` call form.
- ADR-0075 (RV+WASM target enablement) is PROPOSED; Phase 1 (riscv64) Sprint A
  has landed (HEAD `c37ac6e`). dora-cb Phase 1 ship target is the host triple
  initially; riscv64 cross-build is a stretch in Phase 3.
- ECO_ADT_BASE block table (per `crates/cobrust-types/src/ecosystem.rs:41-94`):
  den `0x000-0x0FF`, strike `0x100-0x1FF`, scale `0x200-0x2FF`, molt
  `0x300-0x3FF`, pit `0x400-0x4FF`, hood `0x500-0x5FF`. **The next free
  256-slot block is `0x600-0x6FF`** — this ADR claims it for `dora`.

### What this ADR answers vs. what it defers

| | This ADR | Deferred |
|---|---|---|
| Crate layout (Q1) | YES | — |
| Exposure mechanism (Q2) | YES | — |
| Manifest shape (Q3, AdtId slot) | YES (`0x600` block) | — |
| Dataflow vs function model (Q4) | YES (decorator-form) | yaml-driven path (Phase 2 sub-ADR if needed) |
| Phase plan (Q5) | YES (3 phases + gates) | — |
| ROS2 bridge | NO | sub-ADR 0076a after Phase 2 |
| `wasm32-wasip1` for dora nodes | NO | dora-rs uses sockets via Zenoh; WASI p1 has no sockets → out of scope per ADR-0075 §2 |
| `riscv64gc-unknown-linux-gnu` cross-build | partial (Phase 3 stretch) | full cross-CI: ADR-0075 Sprint B+C dependency |

## 2. Q1 — Crate layout

**Decision: ONE parent `cobrust-dora` crate. No sub-crates per dataflow primitive.**

Rejected alternatives:

- **Sub-crates per primitive (`cobrust-dora-node`, `cobrust-dora-operator`,
  `cobrust-dora-source`, `cobrust-dora-sink`)** — Rejected. dora-rs itself
  splits `dora-node-api`, `dora-operator-api`, `dora-node-api-c`,
  `dora-node-api-python` because each is a different language binding to the
  same Rust core. Cobrust nodes ARE dora-node-api consumers; there is no
  "Cobrust operator API" distinct from Cobrust node API. Splitting would
  invent a distinction that the underlying runtime doesn't make. Violates
  CLAUDE.md §5.1 ("one way to do each thing in the core language").
- **Fold into `cobrust-stdlib`** — Rejected. dora pulls in zenoh + arrow +
  bincode + tokio + opentelemetry. Adding ~50 transitive deps to the stdlib
  link path for every `.cb` program that doesn't import dora violates the
  "link only imported modules' archives" rule from ADR-0072 §5 R3 (link
  bloat). The standalone crate isolates this weight.

Rationale: ADR-0072's 6 prior modules all live as standalone crates
(`cobrust-{den,nest,strike,scale,molt,pit,hood}`). Following the precedent
keeps the dispatch templates and the audit checklists unchanged. The crate
layout mirrors how dora-rs itself shapes its public Rust API: ONE
`dora-node-api` crate that node-author packages link against. The Cobrust
node author analogously links against ONE `cobrust-dora` crate.

The crate provides:

- `crates/cobrust-dora/src/lib.rs` — re-exports + module roster.
- `crates/cobrust-dora/src/cabi.rs` — the `#[no_mangle] extern "C"` surface
  the .cb codegen retargets onto (mirrors `cobrust-pit/src/cabi.rs`).
- `crates/cobrust-dora/src/node.rs` — thin Rust wrapper over `DoraNode`
  + `EventStream` from `dora-node-api`.
- `crates/cobrust-dora/src/event.rs` — Cobrust-side `Event` Adt mapping for
  `dora_node_api::Event::{Input, Stop, ...}`.
- `crates/cobrust-dora/src/arrow_view.rs` — minimal Arrow `RecordBatch`
  helpers exposed to the .cb side as scalar accessors (Phase 1: `i64` + `str`;
  Phase 2: `list`, `dict`).
- `crates/cobrust-dora/build.rs` — same `-undefined dynamic_lookup` cdylib
  pattern as `cobrust-pit/build.rs` (ADR-0072 Q5; no Rust-level dep on
  cobrust-stdlib).
- `crates/cobrust-dora/Cargo.toml` — `crate-type = ["rlib", "staticlib"]`
  (no `cdylib` until a PyO3 reverse-binding surface is needed; ADR-0011
  precedent says we don't add it speculatively).

## 3. Q2 — Exposure mechanism

**Decision: C-ABI shims (the `cobrust-pit` axum + `cobrust-hood` clap pattern),
NOT PyO3 reverse-binding.**

Both `cobrust-den` (rusqlite) and `cobrust-pit` (axum) ship the same C-ABI
shim shape — the difference between them is the upstream library, NOT the
chain. The Q2 framing in the dispatch brief contrasts "PyO3" vs "raw .cb
C-ABI"; the existing 6 ecosystem modules all use the raw C-ABI shim. dora-cb
joins that consensus. PyO3 is reserved for the BACKWARDS direction (Python
imports a Cobrust crate as a CPython extension); dora-cb's binding direction
is .cb → Rust, which is the C-ABI shim path.

Rejected alternatives:

- **PyO3 wrapper around `dora-node-api`** — Rejected. PyO3 is for the
  Python-extension surface (the path that lets a CPython runtime `import
  cobrust_strike` and call Cobrust-emitted code). dora-cb's binding direction
  is REVERSED: a .cb program (AOT-compiled to ELF) needs to CALL INTO
  dora-rs, not be called by a Python interpreter. The pit/hood C-ABI shim
  pattern is the existing, proven path for this direction. ADR-0073 §6
  done-means gates 3 + 4 already verify the link + run discipline.
- **Re-implement dora-rs node API in pure Cobrust** — Rejected. Zenoh +
  Arrow + tokio integration is dora-rs's core competency; re-implementing
  any of it in Cobrust would violate §2.5 schedule risk (the doc-only sprint
  this ADR rides on) AND wastes the dora-rs investment. We are a
  CONSUMER of dora-rs, not a competitor.
- **Synthetic "dora-emulator" for testing** — Rejected. The CI smoke gate
  must invoke real `dora start` against a real dataflow.yml; emulating dora
  produces a fixture-name vs behavior drift (memory:F36) the project has
  already paid for once on the 0058a wave.

Rationale (and the §2.5 robotics-latency-budget question):

The robotics latency budget is **sub-millisecond per dataflow tick** for
control loops. PyO3 wrapping would force every event through PyO3's GIL
acquire / release + Python-object marshalling — adds ~10-100 μs per call,
unacceptable for control. The C-ABI shim is a **direct function call** from
the `.cb`-emitted text segment into `dora-node-api`'s Rust functions; the
trampoline ADR-0073 §2 D4 adds ~50 ns (one indirect call + one
`Box::into_raw/from_raw` pair) per event — negligible.

The §2.5 LLM-first overlap rule is preserved because the C-ABI shim is
INVISIBLE at the .cb source level. The user writes:

```python
import dora
import pyarrow as pa

@dora.node(inputs=["tick"], outputs=["reading"])
fn on_event(event: dora.Event) -> Result[None, dora.Error]:
    if event.kind == "input" and event.id == "tick":
        let v: i64 = compute(event.data.as_i64())
        node.send_output("reading", pa.array_i64([v]))
    return Ok(None)
```

— which is bit-identical to the dora-rs Python binding ergonomic. The
LLM sees the surface it trained on; the C-ABI happens under it.

## 4. Q3 — Manifest shape

**Decision: `DORA_NODE_ADT = AdtId(ECO_ADT_BASE + 0x600)` block claimed for
dora. Initial 4 ADT slots; `EcoParam::Callback(dora_event_handler_fn_ty())`
for the on-event registration.**

The Node concept is **both** a handle (lifetime-managed channel handle to
dora coordinator) AND a callback receiver (the on_message → on_data
progression). ADR-0073 already proves this dual role works: pit's
`PIT_APP_ADT` is a handle, and `pit.App.route` is its callback-bearing
method. dora-cb mirrors the shape.

### AdtId slot allocation

```rust
// in cobrust-types/src/ecosystem.rs (NEW SLOT — claim 0x600 per ADR-0076 §4)
pub const DORA_NODE_ADT:        AdtId = AdtId(ECO_ADT_BASE + 0x600);
pub const DORA_EVENT_ADT:       AdtId = AdtId(ECO_ADT_BASE + 0x601);
pub const DORA_ARROW_ARRAY_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x602);
pub const DORA_METADATA_ADT:    AdtId = AdtId(ECO_ADT_BASE + 0x603);
// Phase 1 caps at 4 slots; remaining 0x604..0x6FF reserved for follow-ups
// (Ros2Context / Ros2Subscription / Operator handles per Phase 3).
```

Rejected alternatives:

- **Use the existing `ECO_ADT_BASE + 0x300` molt block + retire molt** —
  Rejected. molt (datetime, dateutil rebrand) is a live, CI-verified
  ecosystem module per ADR-0072 Q1. No retirement on schedule. Claiming the
  next free block (`0x600`) is the additive precedent.
- **Generic `Ty::Opaque(drop_symbol)`** — Rejected per ADR-0072 Q3 design
  precedent. Nominal Adt handles give compile-time method dispatch (§2.5
  compile-time-catch); generic opaque pointers defer all errors to runtime.

### EcoParam shape for the on-event registration

```rust
pub fn dora_event_handler_fn_ty() -> FnTy {
    FnTy {
        params: vec![dora_event_ty()],            // takes Event
        ret: Box::new(Ty::Result(                 // returns Result[None, Error]
            Box::new(Ty::None),
            Box::new(Ty::Str),                    // error rendered as str in Phase 1
        )),
        kind: FnKind::Free,
    }
}

// Manifest entry (Phase 1 surface)
(DORA_NODE_ADT, "run") => Some(EcoSig {
    runtime_symbol: "__cobrust_dora_node_run",
    params: vec![EcoParam::Callback(dora_event_handler_fn_ty())],
    ret: Ty::Int,                                 // exit code (0 = clean)
    tier: PyCompatTier::Semantic,
}),
(DORA_NODE_ADT, "send_output") => Some(EcoSig::from_values(
    "__cobrust_dora_node_send_output",
    vec![Ty::Str, dora_arrow_array_ty()],         // output_id + Arrow payload
    Ty::None,
    PyCompatTier::Semantic,
)),
```

Rejected alternatives:

- **Polling iterator API (`for event in node: ...`)** — Rejected for the
  Phase 1 callback site. Iterators in Cobrust require a working iterator
  protocol over an Adt handle; dora-rs's Rust API uses a callback or an
  iterator (`events.into_iter()`). The CALLBACK form is the §2.5 overlap
  target with the Python `@dora.node` decorator pattern + lets the trampoline
  fire ADR-0073 §2 D4 verbatim. Polling iterator is a Phase 2 add-on (the
  `for event in node:` form via a Cobrust iterator-protocol bridge).
- **`async fn on_event` (Python `async def`)** — Rejected for v0.7.0. CLAUDE.md
  §2.2 mandates single structured-concurrency; ADR-0028's tokio runtime is
  the only path. dora-rs uses tokio underneath but the Rust API is sync from
  the node-author's POV; matches our model.

### `@py_compat` tier

`PyCompatTier::Semantic` for every dora-cb manifest entry. dora-rs is not
CPython-bit-parity (it's a distinct runtime); behavioral equivalence to the
dora-rs Python binding IS the parity target. The L2 verifier (ADR-0037 +
ADR-0052c when ratified) consumes this tier to choose differential-test
tolerance.

## 5. Q4 — Dataflow vs function model

**Decision: decorator-based, with explicit-form fallback. The yaml-driven
form lives at the dora-coordinator layer (existing); the .cb side gates on
`@dora.node` for static input/output declaration.**

The canonical .cb shape (Phase 1+2):

```python
import dora
import pyarrow as pa

@dora.node(inputs=["tick"], outputs=["reading"])
fn on_event(event: dora.Event) -> Result[None, dora.Error]:
    if event.is_input("tick"):
        let v: i64 = compute(event.data.as_i64())
        node.send_output("reading", pa.array_i64([v]))
    return Ok(None)

fn main() -> i64:
    let node = dora.Node()
    return node.run(on_event)
```

ADR-0074's decorator desugar converts `@dora.node(inputs=..., outputs=...)`
over `on_event` into a SYNTHETIC sibling call that REGISTERS the input /
output declarations into the manifest at HIR-lower time:

```python
# Desugared (HIR-internal):
let __dora_decl_on_event = dora._node_declare(
    "on_event", ["tick"], ["reading"], on_event
)
```

The `dora._node_declare` shim is a thin static record (no runtime effect at
Phase 1) that the `cobrust check` typechecker reads to verify (a)
`event.is_input("tick")` matches a declared input, (b) `node.send_output(
"reading", ...)` matches a declared output. Mistyped IDs are
`TypeError::DoraUnknownInputId { id, declared }` etc. at compile time —
the §2.5 compile-time-catch rule.

Rejected alternatives:

- **Builder-based (`dora.Node().with_input("tick").with_output("reading")
  .start(on_event)`)** — Rejected. Builder pattern is verbose, has no
  Python-overlap (dora's Python API is `@dora.node` or implicit-via-yaml,
  not builder). LLM-first §2.5 overlap loss vs decorator form is large.
- **Yaml-only (`dora.run("dataflow.yml")` reads input/output from the yaml)**
  — Rejected for primary surface. The CLI compiles a single `.cb` source to a
  `.elf` binary; the binary doesn't know its dataflow context until dora
  coordinator spawns it with env vars. But: declaring inputs/outputs in
  the binary's source (via `@dora.node`) lets `cobrust check` catch
  ID-typos at compile time — strict §2.5 win. The yaml STILL lives as the
  cluster-wide topology declaration; .cb just adds a per-node declaration
  for static verification.
- **No decorator at all — pure runtime `for event in node:`** — Rejected
  for static-verification reasons above. Allowed as the "fallback" form for
  programs that don't want the decorator (the `for event in node:` polling
  form lands in Phase 2 alongside ADR-0073's existing handle-method
  manifest entries, gated on iterator-protocol bridge).

§2.5 compliance scoreboard:

| Surface | Training-data overlap | Compile-time-catch | LLM-first verdict |
|---|---|---|---|
| `@dora.node(inputs=[...], outputs=[...])` | STRONG (dora-rs Python pattern) | STRONG (compile-time ID verify) | **PASS** |
| Builder | WEAK (no precedent) | MEDIUM | reject |
| Pure yaml | MEDIUM (config-as-truth, but spread across two files) | WEAK (runtime-only) | reject |

## 6. Q5 — 3-phase roadmap matched to "周→天" pace

Each phase has Done-means + verification gates; Phase N+1 sprint does not
dispatch until Phase N's gates clear an independent paired audit (ADSD
mandatory per project SOP).

### Phase 1 — `cobrust-dora` scaffold + minimal Node hello-world

**Wall budget: ~2 day-units (~1 work-week at agent velocity).**

Scope:

- NEW crate `crates/cobrust-dora/` per §2 layout, deps: `dora-node-api =
  "0.2.x"` (pinned exact-patch per ADR-0075 §10 F35-sibling discipline);
  `arrow = "58"`; `bincode = "1.3"`; tokio is transitively pulled from
  dora-node-api; no opentelemetry yet.
- `cobrust-types/src/ecosystem.rs` adds the `0x600` block + 4 ADT consts +
  `dora_event_handler_fn_ty()` + 2 manifest rows (`(DORA_NODE_ADT, "run")`
  + `(DORA_NODE_ADT, "send_output")`) + the free-fn `("dora", "Node")` row.
- `cobrust-dora/src/cabi.rs` ports the pit trampoline pattern: 4 shims
  (`__cobrust_dora_node_new` / `__cobrust_dora_node_run` /
  `__cobrust_dora_node_send_output` / `__cobrust_dora_node_drop`).
- `cobrust-cli/src/build.rs` recognises `dora` in `locate_ecosystem_archive`
  + per-import link path (no compiler internals changes — ADR-0072 path).
- E2E test: `crates/cobrust-cli/tests/dora_hello_e2e.rs` + new
  `examples/dora_hello/` directory with `sender.cb`, `receiver.rs` (the
  receiver stays Rust until Phase 2 has the receiver-side Cobrust shape),
  `dataflow.yml`. Test invokes `dora start dataflow.yml`, asserts the
  Rust receiver gets the Cobrust-sent message.

Compiler-changes audit: **ZERO new compiler primitives.** Every layer reuses
ADR-0072 + ADR-0073's shipped chain.

Done-means (5 gates, all required):

1. `cobrust check examples/dora_hello/sender.cb` — 0 errors.
2. `cobrust build examples/dora_hello/sender.cb -o sender` — emits ELF;
   `nm sender | grep __cobrust_dora_` shows the 4 shim symbols resolved.
3. `dora start examples/dora_hello/dataflow.yml` runs the dataflow; the
   Rust receiver logs the Cobrust-emitted message body (`"hello from
   cobrust"` or analogous canonical literal).
4. `cobrust-dora::cabi::DROP_COUNT` shows exactly-once handle drops at
   shutdown — Node + Event drops.
5. CI green: fmt / clippy / build / test / doc-coverage / cargo test
   --workspace --locked all pass on the host triple matrix (macOS +
   Linux per ADR-0046).

### Phase 2 — Multi-IO, yaml-load, panic-safety, decorator sugar

**Wall budget: ~3 day-units (~1.5 work-weeks at agent velocity).**

Scope:

- Multi-input / multi-output: extend `@dora.node(inputs=[...], outputs=
  [...])` decorator to declare ≥2 inputs and ≥2 outputs. Add manifest
  variants if needed (e.g. `(DORA_NODE_ADT, "set_input_handler")`).
- `for event in node:` polling iterator form (alternative to callback —
  for nodes that need explicit control-flow over event ordering). Requires
  the iterator-protocol bridge in cobrust-types — small extension to
  `DORA_NODE_ADT`'s manifest.
- Yaml-driven discovery: `dora.dataflow_descriptor()` returns the running
  dataflow's yaml as a parsed dict. Manifest row + shim.
- Panic safety on cross-boundary callback per ADR-0073 §5.3 — already
  inherited from the pit trampoline pattern; verify here.
- Drop discipline per ADR-0072 §6 — Node + Event + ArrowArray handles all
  schedule exactly once via the ADR-0072 §5 R1 drop schedule. Verify via
  the DROP_COUNT instrument extended to dora.
- Receiver-side .cb sample: rewrite `examples/dora_hello/receiver.rs` as
  `receiver.cb` (proves both sender + receiver Cobrust path).

Done-means (5 gates):

1. A 4-node Cobrust dataflow (sender → filter → aggregator → sink, all .cb)
   runs `dora start` round-trip.
2. Mistyped output ID (`@dora.node(outputs=["reading"])` + `node
   .send_output("redaing", ...)`) rejects at `cobrust check` with
   `TypeError::DoraUnknownOutputId { id: "redaing", declared:
   ["reading"], suggestion: "did you mean reading?" }`.
3. A handler that panics on event aborts the process with the
   `__cobrust_panic`-routed message (ADR-0073 §3 Q5 abort-on-panic) —
   the dora coordinator restarts or fails the dataflow per its own policy.
4. ArrowArray drop discipline verified — 1000-event run shows
   `(drop_count_after - drop_count_before) == events_processed * (drops_per_event)`.
5. CI green; doc-coverage updated for the new manifest rows.

### Phase 3 — Real-robotics demo

**Wall budget: ~3 day-units.**

Scope: physical-ish demo proving the e2e robotics dataflow works under
Cobrust. Pick a simulated CartPole control loop (no physical robot needed —
the demo is reproducible in CI):

- Node 1: `sim_env.cb` — drives a CartPole simulator (uses `coil` for the
  state-vector math), emits `state` (4-element Arrow array).
- Node 2: `inference.cb` — tiny CNN / MLP policy (uses `coil` for matmul),
  consumes `state`, emits `action`.
- Node 3: `actuator.cb` — applies action to sim_env, closes the loop.

Done-means:

1. `dora start examples/dora_cartpole/dataflow.yml` runs 100 ticks without
   crashing; final stand-time exceeds a baseline random-policy.
2. CI smoke test ships a quick (10-tick) variant gated on the dora binary
   being on PATH; gated jobs skip if dora not present.
3. zh + en + agent docs all cite the demo as the v0.7.0 robotics readiness
   evidence (Y.7 Done-means in ADR-0070 §2.2 grid).
4. Optional stretch: riscv64 cross-build of the CartPole demo (consumes
   ADR-0075 Phase 1) — verifies the dora-cb chain is cross-target clean.
5. Token consumption + perf numbers go in the v0.7.0 release notes per
   §1.2 of CLAUDE.md.

## 7. How this relates to ADR-0072 / ADR-0073 / ADR-0074

| ADR | Role for dora-cb |
|---|---|
| **ADR-0028 (M13 concurrency runtime)** | The tokio singleton — dora-rs has its own; the cabi.rs trampoline MUST NOT double-init. Implementation detail: enter dora's runtime as a guest within `__cobrust_dora_node_new` if a Cobrust tokio runtime already exists; else init dora's. Mirrors strike's pattern. |
| **ADR-0070 (v0.7.0 master design)** | Parent ADR; §6 Q4 is now CLOSED by this ADR's §3 Decision (FFI via C-ABI shims, no PyO3 wrapper). Follow-up commit can amend ADR-0070 to reflect the resolution. |
| **ADR-0072 (.cb ecosystem-import chain)** | The L1→L5 chain dora-cb plugs into. Manifest entry (L1), MIR retarget (L2), codegen extern (L3), C-ABI shim (L4), static link (L5) — all reused as-is. AdtId block table extended (§4 above). |
| **ADR-0073 (.cb↔Rust callback marshalling)** | The trampoline pattern that makes `node.run(on_event)` work. dora's `EventStream::into_iter()` callback is wrapped EXACTLY like pit's axum handler closure. fn-pointer materialisation, Box::into_raw/from_raw, abort-on-panic — all inherited verbatim. |
| **ADR-0074 (decorator desugar)** | `@dora.node(inputs=..., outputs=...)` is desugared at HIR level into the synthetic `dora._node_declare(...)` static record. Reuses ADR-0074's machinery; no new desugar primitives. |
| **ADR-0075 (RV + WASM)** | Phase 3 stretch: riscv64 cross-build of dora-cb nodes (dora-rs cross-compiles for riscv64-linux-gnu cleanly per upstream CI). WASM is OUT per ADR-0075 §5 + §2 phase-2 ecosystem exclusion (network APIs unavailable in WASI p1; dora needs sockets). |

## 8. Risks (top)

1. **dora-rs API drift (workspace `0.2.1`, pre-SemVer)** — F35-sibling risk
   (memory:F35). Mitigation: pin `dora-node-api` exact patch + the upstream
   commit SHA in `crates/cobrust-dora/Cargo.toml` + a commit-message
   "as-of" qualifier per ADR-0075 §10. If dora bumps a breaking patch
   between this ADR's ratification and Phase 1 dispatch, re-survey first.
2. **tokio runtime double-init** — dora-rs initialises its own multi-thread
   tokio in `DoraNode::init_from_env`. Cobrust's stdlib `std.task` also
   initialises (ADR-0028). Naively, both try to install a runtime → panic.
   Mitigation: dora's runtime initialisation is via
   `tokio::runtime::Builder` which takes a current-thread guard; the cabi
   shim enters dora's runtime as a guest if a Cobrust runtime is already
   set, else creates its own. See `cobrust-strike` for the proven pattern.
3. **Arrow type system leakage into Cobrust user code** — `pyarrow.Array`
   is Python-ergonomic; Cobrust must mirror via `dora.ArrowArray` Adt + a
   few accessors. Phase 1 ships scalar `i64` + `str` only (sufficient for
   the hello-world); list / dict payloads are Phase 2. Larger Arrow
   surface is a tracked follow-up sub-ADR (potential ADR-0076c).
4. **Zenoh transport build complexity** — Zenoh links to native libraries
   (some C-level); macOS + Linux x86_64 + Linux aarch64 CI must all
   build cleanly. Mitigation: spike Phase 1 Sprint A is a `cargo build -p
   cobrust-dora --release` smoke on all 3 targets BEFORE the .cb-side
   wiring lands. If Zenoh fails on a target, fall back to dora's
   `tcp` transport for that target (Zenoh-supported feature flag).
5. **dora coordinator binary on CI** — the dora coordinator binary must be
   on PATH for the Phase 1 E2E test. CI installs via `cargo install
   dora-cli` or downloads a prebuilt release. Gating: the E2E test skips
   (not fails) if `dora` is not on PATH, with a clear `SKIP` log.
   `cargo test --workspace --locked` stays green either way.
6. **Drop-once discipline cross-boundary** — pit + hood's `DROP_COUNT`
   pattern catches double-free in unit tests. dora-cb adds Node + Event +
   ArrowArray handles; verify each schedules drop exactly once.

## 9. §2.5 LLM-first compliance audit

Per CLAUDE.md §2.5 mandatory rubric:

| Design choice | Compile-time-catch? | Training-data overlap? | Verdict |
|---|---|---|---|
| `import dora; node = dora.Node()` | Type-check at L1 (manifest) | STRONG — matches `dora-rs` Python import | PASS |
| `@dora.node(inputs=..., outputs=...)` decorator | Compile-time ID verification (DoraUnknownInputId) | STRONG — Flask `@app.route` adjacent + dora's own Python | PASS |
| `node.send_output("id", pa.array(...))` | Compile-time output-ID verify (DoraUnknownOutputId) | STRONG — pyarrow + dora pattern | PASS |
| `event.is_input("tick")` | Compile-time input-ID verify | MEDIUM — dora-specific helper, but follows Python idiom | PASS |
| Result-typed callback (`Result[None, Error]`) | Static-check return shape | STRONG — Rust idiom + Cobrust §2.2 mandate | PASS |
| C-ABI shim hidden from .cb source | INVISIBLE (correct by design — implementation detail) | n/a | PASS |
| 4 ADTs (Node, Event, ArrowArray, Metadata) | Per-method dispatch at compile time | STRONG | PASS |

No §2.5 violations identified at design time. Phase 1 audit will re-score
the actual emitted surface.

## 10. Done-means (this ADR — doc-only sprint)

- This ADR file lands at `docs/agent/adr/0076-dora-cb-stream-y.md` with the
  frontmatter contract above.
- Strategy companion `docs/agent/strategy/dora-cb-architecture.md` lands
  with the mermaid dataflow diagram + the wrapper-FFI pattern matching the
  numpy-translation-architecture insight sister doc.
- ADR README index `docs/agent/adr/README.md` gains a single ADR-0076 row.
- `bash scripts/doc-coverage.sh` exits 0.
- `cargo fmt --check` clean (no src touched).
- Commit lands on `main` with identity `wbj010101`, Co-Authored-By trailer,
  single commit message `docs(adr): ADR-0076 dora-cb Stream Y architecture
  (draft, doc-only)`.

## 11. Open questions for future sub-ADRs

- **ADR-0076a — ROS2 bridge from Cobrust dora nodes**: dora-rs ships
  `ros2-bridge-node` as a separate binary. Cobrust nodes can consume ROS2
  messages by setting an `input` of type `ros2://topic`. Cobrust DOES NOT
  need a ROS2 API itself; dora handles the bridge. But: when a Cobrust user
  wants to PUBLISH a ROS2 topic from .cb, the surface needs design.
  Schedule: after Phase 2.
- **ADR-0076b — `cobrust.gpu` x dora**: Phase 3 inference node uses CNN via
  `coil`. If `coil` gains GPU dispatch (per
  `numerical-compute-hardware-tiering.md` strategy doc), dora-cb inference
  nodes inherit GPU automatically. Cross-cutting; tracked separately.
- **ADR-0076c — Arrow payload surface widening**: list[i64] + dict[str, X]
  + nested struct support beyond Phase 1's i64 + str scalars. Likely
  Phase 2 or post-v0.7.0 depending on Phase 1 calibration.
- **ADR-0076d — operator-API (in-process node runtime) path**: dora-rs's
  `dora-operator-api` is a distinct surface for in-process nodes. Phase 3
  stretch only; not on the v0.7.0 critical path.
- **ADR-0076e — opentelemetry exposure from .cb dora nodes**: dora-rs
  embeds opentelemetry tracing as an optional feature. Cobrust nodes could
  emit spans automatically; design coordination with `cobrust-stdlib`'s
  `tracing` integration (CLAUDE.md §9 logs rule). Phase 3 stretch.
- **Re-survey of dora-rs as-of Phase 1 dispatch eve**: the 2026-05-25
  survey is 3 days old at this ADR's ratification. F35-sibling: re-survey
  IF dora-rs HEAD shifted between 2026-05-25 and dispatch eve; otherwise
  the survey stands.

## 12. Evidence

- `docs/agent/strategy/v0.7.0-dora-cb-integration-roadmap.md` (2026-05-25
  empirical survey)
- `docs/agent/strategy/dora-cb-architecture.md` (this ADR's companion)
- `docs/agent/strategy/numpy-translation-architecture.md` (sister
  wrapper-FFI shape)
- `crates/cobrust-pit/src/cabi.rs` (the reference trampoline impl ADR-0073
  ratified; line-anchored at the comments throughout)
- `crates/cobrust-pit/build.rs` (the `-undefined dynamic_lookup` cdylib
  pattern this crate clones)
- `crates/cobrust-types/src/ecosystem.rs:41-94` (ECO_ADT_BASE block
  table; line `41` declares `0xE000_0000`, lines `83-94` show pit + hood
  blocks the dora `0x600` claim extends)
- `docs/agent/adr/0070-v0.7.0-master-design.md` §2.2 Stream Y grid + §6 Q4
  (the resolved question)
- `docs/agent/adr/0072-cb-ecosystem-import-wiring.md` (the L1→L5 chain)
- `docs/agent/adr/0073-cb-callback-marshalling.md` (the trampoline pattern)
- `docs/agent/adr/0074-cb-ecosystem-decorator-desugar.md` (the
  `@dora.node` desugar precedent)
- upstream: <https://github.com/dora-rs/dora> workspace `0.2.1`
  (2026-05-25 survey snapshot in v0.7.0-dora-cb-integration-roadmap.md §2)
