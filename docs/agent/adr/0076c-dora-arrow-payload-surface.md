---
doc_kind: adr
adr_id: 0076c
title: dora Arrow payload surface — what `.cb`-visible type a dora Event::Input / send_output carries
status: proposed
date: 2026-06-01
last_verified_commit: 936f13c
decision_owner: cto
supersedes: []
superseded_by: []
relates_to: [adr:0072, adr:0073, adr:0076, adr:0077, adr:0078, "strategy:dora-real-integration-plan", "strategy:numpy-translation-architecture", "claude.md:§2.2", "claude.md:§2.5", "claude.md:§5.1", "feedback:elegant_ecosystem_surface_no_legacy_debt"]
---

# ADR-0076c: dora Arrow payload surface

> **This is a DESIGN PROPOSAL for the CTO / user, NOT a final spec.** It scopes
> THE most consequential surface choice in the dora-cb pillar (`dora-real-integration-plan`
> §4.3 / §6 RISK R4) so the future Phase-B impl session can decide + execute
> without re-discovery. It writes ONLY this doc — no code is touched. The
> ratifying decision is the impl session's (or a CTO sign-off on the
> recommendation below). Empirical API facts are cited; anything not directly
> verified is marked **[UNVERIFIED]**. Status stays `proposed` until ratified.

---

## 1. Context

### 1.1 The question

`dora-real-integration-plan` §1.2 / §5 split the dora-REAL work into phases.
**Phase A** (shipped, commit `b32e965`; real `DoraNode` behind `--features
dora-real`) deliberately carries payloads as **strings only** — `event.data_str()
-> str` decodes the wire via `String::try_from(&ArrowData)`, and
`event.send_output(id, payload)` publishes a length-1 Arrow `StringArray` via
`payload.to_string().into_arrow()` (verified: `crates/cobrust-dora/src/cabi.rs`
`real::decode_arrow_payload` ~L955 + `real::send_output` ~L926).

**Phase B** must answer the question Phase A deferred:

> When a dora node's wire payload is a real `arrow::array::ArrayRef` (what flows
> through `Event::Input { data }` and `node.send_output(..)`), **what
> `.cb`-VISIBLE type should the `.cb` source see** for an input payload and for
> a `send_output` argument?

This is `dora-real-integration-plan` §4.3 / §6 R4, flagged there as *"the most
consequential surface choice in the whole plan"* and explicitly left as an OPEN
design question for a sub-ADR (candidate `ADR-0076c` — this document).

### 1.2 What flows on the dora wire (verified)

dora-rs nodes communicate **only** in the Apache Arrow in-memory columnar
format — zero-copy shared-memory on one host, TCP across hosts (dora-rs.ai;
dora README). The payload type the node API exposes:

- `Event::Input { id: DataId, metadata, data: ArrowData }` — the `data` field is
  an `ArrowData`, a newtype `pub struct ArrowData(pub arrow::array::ArrayRef)`
  that derefs to `arrow::array::ArrayRef = Arc<dyn Array>` (verified: GitHub
  `dora-rs/dora` `libraries/arrow-convert/src/lib.rs`).
- `arrow` 54.x's `ArrayRef` is a **dynamically-typed columnar 1-D array**; its
  concrete element type is one of `arrow::datatypes::DataType` — the primitive
  set is `Boolean`, `Int8/16/32/64`, `UInt8/16/32/64`, `Float16/32/64`, `Utf8`,
  `Binary`, plus nested `List`/`FixedSizeList`/`Struct`/`Map` (verified:
  docs.rs `arrow::datatypes::DataType`). Arrow is **flat + columnar**: an array
  has a `DataType` + a length + a null bitmap — it is NOT inherently
  n-dimensional.
- `dora-arrow-convert` exposes the marshalling surface dora re-exports
  (`pub use dora_arrow_convert::*`): `IntoArrow` (`42u64.into_arrow()` → a
  length-1 array; the author-facing output contract), and `into_vec<T: Copy +
  NumCast>()` registered for the numeric primitives `Float32/Float64`,
  `Int8/16/32/64`, `UInt8/16/32` — i.e. **"convert an Arrow array into a `Vec`
  of integers or floats"** — plus `String::try_from(&ArrowData)` for the Utf8
  case (verified: docs.rs `dora_arrow_convert`; the `into_vec` macro registry in
  `arrow-convert/src/lib.rs`). The per-scalar / per-`Vec` `TryFrom<&ArrowData>`
  blocks live in `from_impls.rs` / `into_impls.rs` — **[UNVERIFIED]** the exact
  per-type list (`bool`, `u8`, `i64`, `f64`, `Vec<u8>`, `Vec<i64>`, `Vec<f64>`
  are the expected members; read those two modules on dispatch).

### 1.3 What a robotics dataflow actually sends (verified, qualitative)

The dora literature is consistent: a node "publishes image data at a fixed
frequency; the height, width, and channel count remain constant" — i.e. a
**camera frame is a flat `UInt8Array`** whose `(H, W, C)` shape lives in the
node/dataflow **metadata**, not in the Arrow array's own rank (the array is 1-D
of length `H*W*C`). Numeric sensor/state/tensor payloads are **`Float64Array` /
`Float32Array`**; control/command payloads are small numeric or string arrays
(dora-rs.ai; the camera→detection→LLM→action canonical chain). The load-bearing
facts for this ADR:

1. **The dominant real payload is a flat NUMERIC array** (`UInt8` images,
   `Float64`/`Float32` tensors) — Phase A's string-only surface does NOT carry
   real numeric dataflow (the F36 honesty point: a "real dataflow" demo that
   only moves strings is not real robotics data).
2. **n-D shape is carried in metadata, not the array** — so a `.cb` payload
   surface does NOT strictly need n-dimensional arrays to be *correct*; a flat
   1-D typed array + a shape side-channel is the dora-native shape. (This
   matters a lot for option (A) below: coil's n-D power is not the gating
   requirement.)
3. **The dtype set is multi-type** (`UInt8`, `Int*`, `Float*`, `Bool`, `Utf8`),
   not f64-only.

### 1.4 The candidate reuse surface — `coil.Buffer` (verified, with a correction)

`dora-real-integration-plan` §1.1 / §4.3 repeatedly calls `coil.Buffer`
*"f64-only"*. **That is true of the current `.cb` SURFACE, but NOT of the
underlying Rust type** — a distinction this ADR must get right:

- `coil.Buffer` is the `.cb`-side alias for a boxed `coil::Array` (verified:
  `cobrust-coil/src/cabi.rs` ABI header L17-22 + `COIL_BUFFER_ADT` wraps
  `coil::Array`, `ecosystem.rs` L289-293).
- `coil::Array` is a **5-variant tagged union over `ndarray::ArrayD<T>`**:
  `Int32(ArrayD<i32>)`, `Int64(ArrayD<i64>)`, `Float32(ArrayD<f32>)`,
  `Float64(ArrayD<f64>)`, `Bool(ArrayD<bool>)` (verified: `cobrust-coil/src/array.rs`
  L42-49). So it is **multi-dtype AND n-dimensional** at the type level.
- BUT every constructor + op currently EXPOSED to `.cb` builds **`Float64`
  only** (`coil.zeros/ones/eye/mgrid/...` all pass `Dtype::Float64`; verified:
  `cabi.rs` L168/L182 + the `ecosystem.rs` rows). The `.cb` user today has **no
  way to make a non-f64 Buffer** — that is what "f64-only" really means.
- `coil`'s dtype enum is `Int32/Int64/Float32/Float64/Bool` + `Complex64/128`
  (verified: `cobrust-coil/src/dtype.rs` L26-49). It has **no `UInt8`** — the
  single most common dora payload (image bytes) is absent.

### 1.5 The dtype impedance, made precise

| dora Arrow `DataType` | coil `Dtype` / `Array` variant | Robotics use |
|---|---|---|
| `UInt8` | **ABSENT in coil** | **camera images** (dominant!) |
| `Int8/16` | ABSENT (coil has Int32/Int64) | rare |
| `Int32` | `Int32` ✓ | counts, ids |
| `Int64` | `Int64` ✓ | timestamps, counts |
| `UInt16/32/64` | ABSENT | sensor counts |
| `Float16` | ABSENT | half-precision tensors |
| `Float32` | `Float32` ✓ | tensors (ML) |
| `Float64` | `Float64` ✓ | state vectors |
| `Boolean` | `Bool` ✓ | masks/flags |
| `Utf8` | N/A (coil is numeric) | commands, labels |
| `List`/`FixedSizeList` | N/A | nested/struct sensor msgs |

Five of coil's variants map 1:1 to Arrow primitives (`Int32/Int64/Float32/Float64/Bool`),
which is a genuinely good overlap. But the **two highest-value robotics dtypes —
`UInt8` (images) and `Utf8` (commands) — have no coil home**, and the n-D-shape
metadata side-channel has no representation on either side yet.

### 1.6 Constraints this decision is bound by

- **CLAUDE.md §2.2 / §5.1 + the "elegant ecosystem, no legacy debt" design law**
  (`feedback_elegant_ecosystem_surface_no_legacy_debt`): one-way-to-do-each-thing;
  do not import other languages' footguns; the `.cb` surface is a clean
  re-design, not a mechanical clone of pyarrow/numpy.
- **CLAUDE.md §2.5 (LLM-writes-it-right)**: prefer the surface an LLM agent
  emits correctly first try — maximize overlap with Python/Rust training data;
  prefer compile-time catches over runtime surprises.
- **The proven ecosystem chain (ADR-0072 / 0073)**: any new handle/type slots
  into the same L1→L5 wiring (`ecosystem.rs` manifest row → MIR retarget →
  codegen extern → `cabi.rs` shim → static link). The `COIL_BUFFER_ADT` handle +
  `lookup_handle_method`/`lookup_handle_attr`/`lookup_buffer_binop` rows are the
  precedent (verified: `ecosystem.rs` L1013-1690).
- **The Phase-A reality** (`cabi.rs` `mod real`): the ambient-node mechanism
  (§4.4 of the plan, thread-local `AMBIENT_NODE`) + the string round-trip
  already work; whatever Phase B picks must extend, not rewrite, that.

---

## 2. Options considered

Four options. Each scored against §2.2/§5.1 (one-way / no-debt), §2.5
(LLM-right + compile-time-catch), the marshalling cost, and how much it leans on
the proven chain.

### (A) REUSE `coil.Buffer` as the dora payload surface

`event.data_buffer() -> coil.Buffer`; `event.send_output(id, some_buffer)`. One
array type across the numerical pillar AND the robotics pillar.

- **Pro (one-way-to-do-it, §5.1):** the strongest elegant-law story — a `.cb`
  robot program does `import coil` for math AND the same `Buffer` flows on the
  dora wire. No second array type to learn. Matches the plan §4.3's own
  preferred direction and `numpy-translation-architecture`'s "coil is the
  numeric substrate" framing. The 5 overlapping dtypes (`Int32/Int64/Float32/Float64/Bool`)
  marshal cleanly via a hand-written `ndarray ↔ arrow` bridge (the plan §4.3
  option-1 bridge; mirrors how coil already hand-marshals to faer).
- **Pro (§2.5 training-data overlap):** `coil` is numpy-shaped; LLMs write
  `buf.mean()`, `a.dot(b)` correctly (these rows already exist — `ecosystem.rs`
  L1338, L1647). A robot policy `inference.cb` doing `coil` matmul on a received
  Buffer is exactly the plan §6 Phase-C CartPole demo shape.
- **Con (real impedance — the §1.5 table):** coil has **no `UInt8`** (camera
  images, the dominant payload) and **no `Utf8`** (commands). A received image
  would have to up-cast `UInt8 → Int32`/`Float64` (4-8× memory blow-up + a copy
  on every frame — unacceptable for a camera at 30fps) or be rejected. coil is
  also **f64-only at the `.cb` surface today** (§1.4): there is no `.cb` syntax
  to construct or even observe a non-f64 Buffer, so "reuse coil" silently
  implies *also* widening coil's whole `.cb` dtype surface (constructors,
  `repr`, ops) — a large blast radius into the numerical pillar for a robotics
  consumer (the plan §4.3 explicitly rejected the inverse — making coil
  arrow-backed — for the same blast-radius reason).
- **Con (n-D mismatch):** coil's signature power is n-D `ndarray`; dora arrays
  are flat-1-D-with-shape-in-metadata (§1.3). Reusing coil drags an n-D model
  onto a 1-D wire and leaves the shape-metadata side-channel unmodelled — an
  impedance in the *opposite* direction from the dtype one.
- **Con (coupling two pillars):** binds the dora pillar's release cadence to
  coil's. A coil dtype refactor (e.g. M7.7 adding `Int8`/`UInt32`, foreshadowed
  in `array.rs` L13-14) would ripple into dora's wire contract.

### (B) A NEW minimal `.cb` arrow type — a `pa`-shim / `dora.Frame`

A thin first-class type that mirrors Arrow's real shape: a typed-1-D-array
handle carrying `(dtype, data, optional shape-metadata)`. Surface idiom either
`pa.array_f64([...])` (pyarrow-shaped, the plan §4.3 + ADR-0076 §3/§5 sketch) or
a robotics-named `dora.Frame`.

- **Pro (matches the wire, §2.5 honest-shape):** a `dora.Frame`/`pa.array`
  *is* an Arrow array — `UInt8` images, `Utf8` commands, the shape-metadata
  side-channel all have a natural home. Zero impedance; the marshalling is an
  identity-ish wrap of `ArrowData`, not a dtype-narrowing conversion. No copy on
  the hot image path.
- **Pro (§2.5 training-data overlap, pyarrow flavour):** `pa.array(...)` /
  `pa.array_f64(...)` occurs in dora's own Python node corpus + pyarrow training
  data; an LLM writing a dora node has seen it. (Counterpoint: it has *also*
  seen numpy more than pyarrow — see (D).)
- **Pro (clean re-design, no-debt law):** a from-scratch `dora.Frame` can DROP
  pyarrow's footguns (the chunked-array / builder / schema sprawl) and expose
  only the dataflow-relevant slice — exactly the "elegant ecosystem, no legacy
  debt" mandate. It can also make dtype a **compile-time-checked** type
  parameter (`dora.Frame[f64]`), a §2.5 compile-time-catch win the string
  surface can't give.
- **Con (a SECOND array type — the §5.1 sprawl risk):** now `.cb` has BOTH
  `coil.Buffer` (numeric math) AND `dora.Frame`/`pa.array` (wire payloads). A
  robot policy receives a `Frame`, must convert to a `Buffer` to do matmul, then
  convert back to emit — two array types + a conversion ceremony, the precise
  one-way-to-do-it violation the elegant law warns against. Unless the two are
  unified (→ option D), this is the biggest cost.
- **Con (build cost):** a genuinely new ecosystem handle (new ADT id block, new
  `cabi.rs` shims, new manifest rows, new dtype/constructor surface, zh/en/agent
  docs) — the heaviest of the four, and it competes with coil for "the `.cb`
  array type" mindshare.

### (C) STRING / BYTES-only for Phase B — what Phase A already ships

Keep `event.data_str() -> str` + `event.send_output(id, str)`; add a
`bytes`/`data_bytes()` accessor (Arrow `Binary`/`UInt8`) so binary blobs round-trip,
but DEFER all typed numeric arrays.

- **Pro (ships now, bounded):** zero new type design; extends the working
  Phase-A `mod real`. A `bytes` accessor + `send_output_bytes` (the dora API
  already has `send_output_bytes(id, params, len, &[u8])`) covers
  serialize-it-yourself nodes and is genuinely useful (image bytes CAN flow as
  `bytes`, the user just hand-decodes).
- **Pro (no premature commitment):** does not lock the array-surface decision
  before the coil-dtype + n-D-metadata questions are resolved — the most
  reversible option.
- **Con (NOT real numeric dataflow — the F36 honesty point):** a robot program
  doing `coil` math on a sensor tensor cannot receive that tensor as a typed
  array — it gets a `str`/`bytes` and must hand-parse. This reasserts exactly
  the gap the plan §1.1 flags ("not real numeric dataflow"). For the §6
  Phase-C CartPole demo (coil-policy on a received state vector) this is a
  blocker, not a deferral.
- **Con (§2.5 violation):** an LLM writing a dora node will reach for a typed
  array (`np`/`pa`); forcing `bytes`-hand-parse is the *opposite* of
  write-it-right — it's the legacy "stringly-typed payload" footgun the
  elegant-law mandate names explicitly.

### (D) HYBRID — scalars/strings/bytes now via the existing surface; a typed-array surface as a later phase, UNIFIED with coil

Phase B-1: keep + extend the Phase-A string surface (add `bytes`); ship the
**numeric round-trip via `coil.Buffer`** for the 5 overlapping dtypes ONLY,
behind an explicit `event.data_buffer()` / `send_output(id, buffer)` that
documents the `UInt8→` and `Utf8` gaps as known divergences. Phase B-2 (or
C): if/when the gaps bite, introduce the `UInt8`/`Utf8`/shape-metadata coverage
as a coil dtype-tier widening (one array type stays the answer) — NOT a second
type.

- **Pro (one-way-to-do-it preserved, §5.1):** there is still exactly ONE `.cb`
  array type (`coil.Buffer`); the wire just doesn't carry *every* Arrow dtype in
  B-1. The robot policy stays "receive Buffer → coil math → emit Buffer", no
  conversion ceremony, no second type.
- **Pro (real numeric dataflow NOW):** `Float64`/`Float32` tensors + `Int*`
  state + `Bool` masks round-trip as typed arrays in Phase B — the CartPole
  demo's state vector works. The dominant *numeric* robotics payload is covered.
- **Pro (honest, bounded, §2.5-compile-time-catch path):** the `UInt8`/`Utf8`
  gaps are documented divergences (a `@py_compat`-style manifest note), not
  silent — and the future fix is a *coil* widening (add `Dtype::UInt8`), which
  also benefits the numerical pillar (numpy has `uint8`). Images that must flow
  before that ship as `bytes` (option C's accessor) — a clear, named fallback,
  not a footgun.
- **Pro (smallest correct increment, §8 op-instructions):** B-1 is the plan
  §4.3 option-1 `ndarray ↔ arrow` bridge for 5 dtypes + the dropped
  `inputs/outputs` metadata threading (the one real compiler increment, plan
  §4.5) — no new array TYPE, just a new accessor + the bridge.
- **Con (deferred dtype coverage):** camera-image `UInt8` typed arrays + `Utf8`
  typed arrays are NOT first-class in B-1 (str + bytes only). A pure image
  pipeline is partially served (bytes work; typed `UInt8` doesn't) until the
  coil widening lands.
- **Con (commits coil as the answer):** if the user later wants a pyarrow-idiom
  `pa.array` surface for dora-Python-corpus overlap (the (B) §2.5 pro), this
  forecloses it. That trade — coil-unity over pyarrow-familiarity — is the core
  decision the CTO/user should confirm.

---

## 3. Recommendation

**Adopt (D) — the HYBRID, unified on `coil.Buffer`.** Concretely, in priority
order:

1. **Phase B-1 (the v0.7.0 numeric-dataflow increment):** add a typed-numeric
   round-trip via the EXISTING `coil.Buffer` handle for the **5 overlapping
   dtypes** (`Float64/Float32/Int64/Int32/Bool`), plus a `bytes` accessor for
   raw blobs. Keep `event.data_str()` for the Utf8 case. **One** `.cb` array
   type; **no** new `pa`/`Frame` type in v0.7.0.
2. **Document the `UInt8`/`Utf8`/n-D-shape-metadata gaps as named divergences**
   (a provenance/manifest note), with `bytes` + `data_str` as the explicit,
   non-footgun fallbacks for them.
3. **Defer** the decision between "widen coil with `UInt8`/`Utf8`/shape" (the
   unity path) and "introduce a `pa`-shim" (the pyarrow-familiarity path) to a
   later phase — but RECOMMEND the coil-widening (unity) path, and ask the CTO
   to confirm that trade now so B-1 doesn't get re-litigated.

### Why (D) over the others — the §2.5 + elegant-law rationale

- **vs (A) pure-coil:** (D) IS (A) for the dtypes where coil fits, but refuses
  to up-cast `UInt8` images 4-8× or pretend coil's f64-only `.cb` surface
  already covers the wire. It gets (A)'s one-way-to-do-it win WITHOUT silently
  committing to a full coil-dtype-surface widening inside a robotics sprint.
- **vs (B) new `pa`/`Frame` type:** (D) refuses the second array type in
  v0.7.0. The elegant-law / §5.1 "one way to do each thing" is the binding
  constraint; a robot program juggling `Buffer` AND `Frame` + conversions is the
  exact sprawl the law forbids. (B)'s honest-Arrow-shape pro is real but does
  not outweigh two-array-types-for-one-job; if `UInt8`/`Utf8` coverage becomes
  load-bearing, widening the ONE type (coil) is more elegant than minting a
  second.
- **vs (C) string/bytes-only:** (D) adds the `bytes` accessor (C's whole value)
  but ALSO ships real typed numeric arrays, so it clears the F36 honesty bar +
  unblocks the CartPole demo. (C) alone reasserts the stringly-typed footgun
  §2.5 + the elegant-law name explicitly.
- **§2.5 net:** the dominant LLM prior for "numeric array in Python" is **numpy**,
  and `coil` IS the numpy rebrand — so a `coil.Buffer` payload maximizes
  training-data overlap better than a pyarrow `pa.array` for the *math* a robot
  node does, while `bytes`/`data_str` cover the non-numeric tail honestly.
  Making the Buffer dtype a future compile-time-checked parameter
  (`Buffer[f64]`) is the §2.5 compile-time-catch upside, reachable from (D),
  foreclosed by (C).

> **This recommendation is reversible at the (D)-B-2 boundary** — nothing in B-1
> prevents a later `pa`-shim if the user prefers pyarrow-familiarity over
> coil-unity. That is exactly why (D) is the smallest correct increment (§8).

---

## 4. The concrete Phase-B increment (what to build)

This grounds (D)-B-1 in the proven L1→L5 chain. **All paths verified against
HEAD `936f13c`.**

### 4.1 Runtime / cabi (`cobrust-dora/src/cabi.rs`, `mod real`)

- **Input typed read.** Add `__cobrust_dora_event_data_buffer(event) -> *mut u8`
  returning a boxed `coil::Array` (a `coil.Buffer` handle). Body: match the
  `Event::Input` `ArrowData`'s `data_type()`; for each of the 5 supported
  primitives, build the matching `ndarray::ArrayD<T>` from the Arrow array's
  values (1-D; the bridge below), wrap in the matching `coil::Array` variant,
  `Box::into_raw`. For `UInt8`/`Utf8`/unsupported → either up-cast-to-`Float64`
  with a logged divergence OR return a null sentinel (decide on dispatch; prefer
  the explicit error path per §2.5). The decoded payload must be stored so a
  later call is idempotent OR the accessor must own the decode — mirror the
  existing `decode_arrow_payload` placement.
- **The `ndarray ↔ arrow` bridge** (the plan §4.3 option-1, hand-marshalled —
  the crux new code): for output, walk a `coil::Array` arm's contiguous slice
  (`ArrayD::as_slice()`), build an `arrow::buffer::Buffer`, construct the
  matching `arrow::array::{Float64Array, Float32Array, Int64Array, Int32Array,
  BooleanArray}`, `.into_arrow()` / pass to `send_output`. For input, the
  reverse: `arr.values()` slice → `ndarray::ArrayD::from_shape_vec`. **1-D
  first**; n-D via the shape-metadata side-channel is a later increment.
  **[UNVERIFIED]** the exact arrow 54.x constructors + zero-copy vs copy
  (`PrimitiveArray::from` / `ScalarBuffer`); read the `arrow::array` rustdoc on
  dispatch.
- **Output typed send.** Extend the `send_output` path (today
  `real::send_output(id, &str)` ~L926) with a Buffer overload that takes the
  boxed `coil::Array`, runs the bridge, and publishes the typed Arrow array via
  the ambient `DoraNode` (the `AMBIENT_NODE` thread-local already in place,
  ~L804). The string + bytes paths stay.
- **`bytes` accessor.** Add `__cobrust_dora_event_data_bytes` (Arrow
  `Binary`/`UInt8` → a `.cb` `bytes`/list-of-u8) + a `send_output_bytes`
  delegate to the dora `send_output_bytes(id, params, len, &[u8])` API.
- **Drop discipline:** the returned Buffer is `.cb`-owned → freed once via the
  existing `__cobrust_coil_buffer_drop` (the manifest `handle_drop_symbol(COIL_BUFFER_ADT)`
  already resolves — `ecosystem.rs` L364). No new drop symbol for the reused
  Buffer. A 1000-event run must show balanced Buffer drops (plan §5 Phase-B
  done-means 5).

### 4.2 Manifest / typecheck (`cobrust-types/src/ecosystem.rs`)

- **New `DORA_EVENT_ADT` rows** in `lookup_handle_method`: `("data_buffer")
  -> coil_buffer_ty()`, and a `send_output` overload/variant taking
  `vec![Ty::Str, coil_buffer_ty()]` (the output-id + a Buffer). Note: the
  current `send_output` row is `vec![Ty::Str, Ty::Str]` (L1316) — Phase B needs
  either a second method name (`send_output_buffer`) or arg-type-polymorphic
  dispatch; **prefer a distinct method name** for the compile-time clarity §2.5
  wants (an LLM picks `send_output_buffer` vs `send_output` unambiguously).
  This REUSES `coil_buffer_ty()` (`ecosystem.rs` L292) verbatim — no new ADT,
  no new `Ty` constructor.
- **The ONE real compiler increment (plan §4.5 / §6 R9):** thread the F68-dropped
  `@dora.node(inputs/outputs)` metadata into the manifest so the real loop
  dispatches on `id.as_str()` per declared port AND a mistyped output id rejects
  at `cobrust check` (`TypeError::DoraUnknownOutputId { id, declared, suggestion }`,
  ADR-0076 §6 Phase-2 done-means 2). Small + additive; mirrors existing
  ecosystem-id checks. This is orthogonal to the payload-surface choice but is
  the Phase-B compiler work the surface increment rides alongside.

### 4.3 No MIR / codegen change expected

Per the plan §4.5 + `cabi.rs` L20-27: the `Buffer` handle ABI (opaque `*mut u8`,
`Box` into/from raw) is identical to coil's existing handle, and the
`Constant::FnRef` callback is unchanged. Phase B is a `cabi.rs` body extension +
manifest rows + the one type-checker metadata pass — **not** a MIR/codegen
change. (Confirm by grep that `lower.rs` has no dora-specific code — the plan
§2.1 says it's empty.)

### 4.4 The e2e (F36-honest — must be REAL, not named-real)

- A `≥2`-node all-`.cb` dataflow where a sender emits a `Float64` `coil.Buffer`
  (e.g. `coil.zeros(4)` filled) and a receiver reads `event.data_buffer()`,
  runs a `coil` op (`buf.mean()` / `a.dot(b)` — rows exist), asserts the value.
  Driven by a real `dora start` (or the hermetic `DORA_TEST_WITH_INPUTS`
  integration-testing path the Phase-A `dora_real_node_e2e.rs` already uses).
- **Differential bit-faithfulness gate** (plan §5 Phase-B done-means 4): a
  `[f64]` array round-trips through Arrow IPC bit-identically vs a Rust/Python
  sibling node (catches the bridge's endianness / null-bitmap / layout bugs).
- The test name must not over-promise (F36 discipline): a `dora_buffer_*` test
  that only checks compilation is a reassertion of F36 — it MUST drive a real
  round-trip with a real typed Arrow array on the wire.

---

## 5. Consequences

- **Positive**
  - One `.cb` array type (`coil.Buffer`) spans numeric math + the dora wire —
    the elegant-law / §5.1 one-way-to-do-it win, no `Buffer↔Frame` ceremony.
  - Real typed numeric dataflow ships in v0.7.0 (clears the F36 honesty bar;
    unblocks the §6 Phase-C CartPole coil-policy demo).
  - Reuses `COIL_BUFFER_ADT` + `coil_buffer_ty()` + the existing drop symbol +
    the ambient-node mechanism verbatim — minimal new surface, maximal chain
    reuse (ADR-0072/0073 pattern).
  - The `UInt8`/`Utf8` gaps are named divergences with honest `bytes`/`str`
    fallbacks — not silent footguns; the future fix (coil dtype widening)
    benefits the numerical pillar too.
- **Negative**
  - Camera-image `UInt8` typed arrays + `Utf8` typed arrays are deferred (bytes
    + str only in B-1); a pure image pipeline is partially served until a coil
    `UInt8` widening lands.
  - Commits coil as "the `.cb` array type" — forecloses a pyarrow-idiom `pa`
    surface unless the user later revisits (the trade the CTO should confirm).
  - The `ndarray ↔ arrow` bridge is real, correctness-sensitive new code
    (endianness, null bitmap, n-D layout, zero-copy-vs-copy) — the plan §5
    Phase-B "heaviest risk".
  - coil's n-D model vs dora's flat-1-D-with-shape-in-metadata is left
    unmodelled in B-1 (1-D only); the shape side-channel is a later increment.
- **Neutral / unknown**
  - Whether `UInt8` should up-cast-to-`Float64`-with-divergence OR hard-reject
    (a §2.5 compile-time-catch vs ergonomics call — decide on dispatch; this ADR
    leans reject-explicitly).
  - Whether the dtype becomes a compile-time `Buffer[f64]` parameter (a §2.5
    upside reachable from (D)) is itself a follow-up (coil's `.cb` surface is
    dtype-monomorphic-f64 today).

---

## 6. Risks / unknowns (carry forward for the impl session)

| # | Item | Severity | Notes |
|---|---|---|---|
| U1 | **`UInt8` (camera images) has NO coil home** — the dominant robotics payload | HIGH | The core §1.5 impedance. B-1 serves it via `bytes` only; typed `UInt8` needs a coil `Dtype::UInt8` widening (the unity path) — confirm the user accepts deferring typed images. |
| U2 | **The `ndarray ↔ arrow` bridge correctness** — endianness, null bitmap, n-D layout, zero-copy vs copy | HIGH | Plan §5 Phase-B heaviest risk. Needs the differential bit-faithfulness gate (§4.4). **[UNVERIFIED]** exact arrow 54.x constructors. |
| U3 | **Exact `dora-arrow-convert` `TryFrom<&ArrowData>` / `IntoArrow` per-type list** | MEDIUM | The `from_impls.rs`/`into_impls.rs` modules — **[UNVERIFIED]**; `into_vec<T: Copy+NumCast>` + `String::try_from` confirmed, the per-`Vec`/per-scalar set is not. Read on dispatch. |
| U4 | **`send_output` overload vs distinct method name** for the Buffer arg | MEDIUM | Current row is `(Str, Str)` (L1316). §2.5 favors a distinct `send_output_buffer` for compile-time clarity; confirm the manifest supports two rows cleanly (one return type per row — same constraint ADR-0077 §7 hit for coil `dot`). |
| U5 | **n-D shape side-channel** — dora carries `(H,W,C)` in metadata, coil carries rank in the array | MEDIUM | B-1 is 1-D only. How the `.cb` source sees/sets the shape metadata is unscoped (a `Metadata` accessor surface is a later increment). |
| U6 | **Does "reuse coil" silently require widening coil's whole `.cb` dtype surface?** | MEDIUM | coil is f64-only at the `.cb` surface today (§1.4). B-1 dodges this by decoding INTO a Buffer (the variant is set by the wire dtype, not a `.cb` constructor) — but `Buffer.repr()`/ops on a non-f64 received Buffer must be exercised (coil's ops ARE multi-dtype internally; confirm no f64 assumption leaks). |
| U7 | **Up-cast vs reject policy for unsupported dtypes** | LOW | §5 neutral/unknown; leans reject-explicitly per §2.5. |
| U8 | **The one compiler increment (F68 metadata threading + `DoraUnknownOutputId`)** rides alongside but is orthogonal | LOW | Plan §4.5/§6 R9 — small + additive; the first dora-specific type-checker code. Not gated by the payload-surface choice. |
| U9 | **pyarrow-familiarity vs coil-unity** — the reversible (D)-B-2 fork | LOW (reversible) | The strategic trade. (D) recommends unity but B-1 forecloses nothing; surface to the CTO/user for an explicit call before B-2. |

### Explicitly NOT verified (and why deferral is OK)

- The exact `arrow` 54.2.1 array constructors / `ScalarBuffer` zero-copy API and
  the `dora-arrow-convert` per-type `TryFrom`/`IntoArrow` blocks — read on
  dispatch; the *shape* of the bridge (flat 1-D, 5 dtypes) is the load-bearing
  decision and is grounded.
- Whether dora 0.5.0 → next minor changes `ArrowData`/`IntoArrow` — re-survey on
  dispatch eve (plan §3.0 F35-sibling discipline; pin `=0.5.0`).
- The n-D shape-metadata surface — deferred past B-1 by design (§1.3: dora
  payloads are flat-1-D, shape lives in metadata).

---

## 7. Evidence / sources

**Internal (read 2026-06-01, HEAD `936f13c`):**
- `crates/cobrust-dora/src/cabi.rs` — the Phase-A real path (`mod real`:
  `decode_arrow_payload` ~L955 string-only decode; `send_output` ~L926 length-1
  `StringArray` via `.into_arrow()`; `AMBIENT_NODE` thread-local ~L804).
- `crates/cobrust-coil/src/array.rs` — `coil::Array` 5-variant tagged union over
  `ndarray::ArrayD<T>` (L42-49) — the multi-dtype + n-D correction to the plan's
  "f64-only".
- `crates/cobrust-coil/src/dtype.rs` — coil `Dtype` set (`Int32/Int64/Float32/Float64/Bool`
  + `Complex64/128`; **no `UInt8`**, L26-49).
- `crates/cobrust-coil/src/cabi.rs` — the `Buffer` handle ABI (boxed `coil::Array`,
  L17-22) + the f64-only `.cb` constructor reality (L168/L182).
- `crates/cobrust-types/src/ecosystem.rs` — `COIL_BUFFER_ADT` (L154) +
  `coil_buffer_ty()` (L292) + `lookup_handle_method` dora/coil rows (L1267-1343)
  + `handle_drop_symbol` (L364) + `lookup_handle_attr` shape/ndim/size (L1647-1660)
  — the manifest reuse surface.
- `docs/agent/strategy/dora-real-integration-plan.md` — §4.3 / §6 R4 (this ADR's
  charter), §9 the resolved staticlib spike, §4.4 the ambient-node mechanism.
- `docs/agent/adr/0076-dora-cb-stream-y.md` — the ratified architecture (§3/§5
  the `pa.array_i64` sketch this ADR re-decides).

**External (dora-rs + arrow, verified 2026-06-01):**
- `ArrowData(pub arrow::array::ArrayRef)` + `into_vec<T: Copy+NumCast>` numeric
  registry (`Float32/Float64`, `Int8/16/32/64`, `UInt8/16/32`) + `IntoArrow` —
  GitHub `dora-rs/dora` `libraries/arrow-convert/src/lib.rs` + docs.rs
  `dora_arrow_convert` 0.5.0.
- `Event::Input { id, metadata, data: ArrowData }`, `DoraNode::send_output` /
  `send_output_bytes`, `IntoArrow` — docs.rs `dora_node_api` 0.5.0
  (<https://docs.rs/dora-node-api/latest/dora_node_api/>).
- `arrow::datatypes::DataType` primitive + nested variant set
  (`Boolean/Int*/UInt*/Float16/32/64/Utf8/Binary/List/FixedSizeList/Struct/Map`)
  — docs.rs `arrow` 54.x (<https://docs.rs/arrow/latest/arrow/array/trait.Array.html>).
- Arrow as dora's only wire format (zero-copy columnar; shared-mem on-host, TCP
  cross-host) + camera frames as fixed-size flat image arrays with shape stable
  across frames — dora-rs.ai + `dora-rs/dora` README.

---

## 8. Done-means for ratifying THIS ADR

This is a proposal; it is "done" when:
1. The CTO/user confirms the **coil-unity vs pyarrow-familiarity** trade (U9) —
   i.e. that (D) on `coil.Buffer` is the v0.7.0 direction, OR redirects to (B).
2. The `UInt8`/`Utf8`/n-D-shape deferral (U1/U5) is accepted as named divergence
   for v0.7.0, with `bytes`/`str` as the sanctioned fallbacks.
3. The impl session lifts §4's increment into Phase-B sprints, re-surveying the
   **[UNVERIFIED]** arrow/dora APIs (U2/U3) on dispatch eve and pinning
   `dora-node-api = "=0.5.0"` (plan §3.0 F35-sibling).

Until then: `status: proposed`.
