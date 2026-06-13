---
doc_kind: adr
adr_id: 0076c
title: dora Arrow payload surface — what `.cb`-visible type a dora Event::Input / send_output carries
status: accepted
date: 2026-06-01
ratified_date: 2026-06-06
last_verified_commit: 18e9208
decision_owner: cto
supersedes: []
superseded_by: []
relates_to: [adr:0072, adr:0073, adr:0076, adr:0077, adr:0078, adr:0092, "strategy:dora-real-integration-plan", "strategy:numpy-translation-architecture", "claude.md:§2.2", "claude.md:§2.5", "claude.md:§5.1", "feedback:elegant_ecosystem_surface_no_legacy_debt"]
---

# ADR-0076c: dora Arrow payload surface

> **RATIFIED 2026-06-06 (status: accepted) for the (D)-B-1a numeric round-trip.**
> The original body below (§1–§8) was the design PROPOSAL; the CTO confirmed the
> **coil-unity over a `pa`-shim** trade (U9), and the (D)-B-1a increment (the 5
> overlapping dtypes `Float64/Float32/Int64/Int32/Bool` via the existing
> `coil.Buffer` handle) is IMPLEMENTED + gated. The `UInt8`/`Utf8`/n-D-shape
> residual stays explicitly deferred (named divergences, §1.5 + §5). See the new
> **§9 (Ratification + as-built)** for the verified arrow 54.3.1 API the proposal
> marked `[UNVERIFIED]` (U2/U3), the as-built surface, and the reversibility
> note. The proposal's `[UNVERIFIED]` tags in §1–§8 are superseded by §9's
> verified facts; they are left in place for provenance.
>
> **(D)-B-1b LANDED (the raw-`bytes` accessor — ADR-0093 Phase 2).** The
> `UInt8`/`Binary` divergence the B-1a numeric round-trip explicitly DEFERRED is
> now its COMPLEMENT accessor: `event.data_bytes() -> bytes` decodes Arrow
> `Binary` (a single-row blob) AND `UInt8` (a flat byte list) to a `.cb` `bytes`
> via `__cobrust_bytes_from_raw`; `event.send_output_bytes(id, b)` reads the
> borrowed `bytes` (`__cobrust_bytes_ptr` O(1) `&[u8]`) → a length-1 Arrow
> `BinaryArray` blob. SIMPLER than B-1a — `bytes` is a raw immutable `Vec<u8>`
> (NO 5-dtype dispatch, NO ndarray, NO coil dep); its drop is the EXISTING
> `__cobrust_bytes_drop` (no new registration). BYTE-FIDELITY: a `0xFF`/`0x00`
> non-UTF-8 byte round-trips EXACTLY. A NULL / null-bearing / non-bytes payload
> decodes to an EMPTY `bytes` + a recorded divergence (NEVER silent corruption,
> §2.2), mirroring B-1a's null handling. `check_dora_send_output_id` fires
> `DoraUnknownOutputId` for `send_output_bytes` too (the §2.5-A compile-time
> catch, arg0). See §4.1's `bytes accessor` bullet for the as-built shim map.

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
  §4.3 option-1 `ndarray ↔ arrow` bridge for 5 dtypes — no new array TYPE, just a
  new accessor + the bridge. (The `inputs/outputs` metadata threading is already
  DONE/load-bearing — F76; the one genuinely-remaining real-path compiler
  increment is the compile-time `DoraUnknownOutputId` output-id check, orthogonal
  to this payload surface — see plan §4.2 + §4.5.)
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
- **`bytes` accessor (LANDED — B-1b).** `__cobrust_dora_event_data_bytes`
  decodes Arrow `Binary` (`arr.value(0) -> &[u8]`) AND `UInt8`
  (`arr.values() -> &[u8]`) → a `.cb` `bytes` via `__cobrust_bytes_from_raw`
  (the recv loop pre-decodes into `DoraEventHandle.data_bytes:
  Option<Vec<u8>>`, mirroring `data_buffer`; null-bearing / non-bytes →
  `None` → empty bytes). `__cobrust_dora_event_send_output_bytes` reads the
  borrowed `bytes` (`__cobrust_bytes_ptr` + `_len` → `&[u8]`, NOT an O(n)
  `_get` loop) → a length-1 Arrow `BinaryArray` blob → the ambient
  `DoraNode::send_output`. Synthetic build: a canned non-UTF-8
  `b"\x00\xff\x01"` from `data_bytes()` + an `output[id]=bytes[len=n]`
  marker. Both shims reuse the Buffer pair's fn-type ABI (`(ptr)->ptr` /
  `(ptr,ptr,ptr)->i64`).
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
- **The ONE genuinely-remaining real-path compiler increment (plan §4.5 / §6 R9):**
  the **compile-time** `send_output` output-id check — a mistyped output id must
  reject at `cobrust check`
  (`TypeError::DoraUnknownOutputId { id, declared, suggestion }`, ADR-0076 §6
  Phase-2 done-means 2), instead of today's runtime-only `eprintln + -1`. Small +
  additive; mirrors existing ecosystem-id checks. This is orthogonal to the
  payload-surface choice but is the Phase-B compiler work the surface increment
  rides alongside. (NOTE — F76 correction: the `@dora.node(inputs/outputs)`
  metadata threading is **already done / load-bearing**; the desugar lowers each
  port id to a `dora.declare_input`/`declare_output` register-call, and the real
  loop already dispatches on `id.as_str()` — the dataflow YAML wires routing
  independent of the declared ports. Only the compile-time output-id check above
  remains.)

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
| U2 | **The `ndarray ↔ arrow` bridge correctness** — endianness, null bitmap, n-D layout, zero-copy vs copy | HIGH → **RESOLVED (B-1a + REPAIR)** | The 11 `arrow_bridge_tests` are the differential bit-faithfulness gate (§9.6): per-dtype round-trip + dtype-faithfulness + empty + 1000-event drop balance. **NULL BITMAP (REPAIR MAJOR):** `decode_arrow_buffer` GUARDS on `null_count() > 0` (logs the divergence + returns `None`) so a null is never silently materialised as `0`/`false` — covered by the 3 null-bearing tests. n-D layout stays deferred (flat 1-D by design, §1.3). arrow 54.3.1 constructors now VERIFIED (§9). |
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

All three are now satisfied — see §9. `status: accepted`.

---

## 9. Ratification + as-built (2026-06-06, HEAD 18e9208 → this change)

This section records the (D)-B-1a numeric round-trip AS BUILT and resolves the
proposal's `[UNVERIFIED]` arrow/dora API facts (U2/U3) by READING the
`dora-node-api 0.5.0` + `arrow 54.3.1` source extracted in the cargo registry.

### 9.1 CTO trade confirmation (U9)

**Confirmed: coil-unity over a `pa`-shim.** ONE `.cb` array type (`coil.Buffer`)
spans the numeric pillar AND the dora wire for the 5 overlapping dtypes. NO new
`pa`/`dora.Frame` type in v0.7.0. **This is reversible at the (D)-B-2 boundary**
— nothing in B-1a forecloses a later `pa`-shim if pyarrow-familiarity is later
preferred over coil-unity; the `data_buffer()` / `send_output_buffer` surface is
additive (it sits beside the unchanged `data_str` / `send_output` string path).

### 9.2 The verified arrow 54.3.1 / dora 0.5.0 API (resolves U2/U3)

- **arrow re-export:** `dora-node-api 0.5.0` `src/lib.rs` L89 `pub use arrow;` —
  so arrow is reached as `dora_node_api::arrow::*` with **NO new Cargo.toml dep**
  (F64-safe). `pub use dora_arrow_convert::*` (L90) re-exports `ArrowData` +
  `IntoArrow`.
- **`ArrowData`** = `pub struct ArrowData(pub arrow::array::ArrayRef)` (a newtype
  over `Arc<dyn Array>`), `Deref<Target = ArrayRef>` (dora-arrow-convert
  `src/lib.rs`). So `data.data_type()` + `data.as_any().downcast_ref::<T>()` +
  `data.len()` are the typed-read idiom (the SAME one dora's own `into_vec<T>`
  uses).
- **`send_output` 3rd arg:** `DoraNode::send_output(&mut self, output_id: DataId,
  parameters: MetadataParameters, data: impl Array)` (node/mod.rs L585) — the
  bound is `arrow::array::Array` (the TRAIT), and it calls `data.to_data()`. A
  concrete `Float64Array` / `BooleanArray` impls `arrow::array::Array`, so a
  `Arc::new(Float64Array::from(vec)) as ArrayRef` (which also impls `Array` via
  the blanket `Arc<dyn Array>` impl) is passed directly. (NOT `ArrowData` — that
  is the INPUT newtype; the output bound is the array trait.)
- **Constructors:** `Float64Array::from(Vec<f64>)` / `Int64Array` / `Int32Array`
  / `Float32Array` — all covered by `def_numeric_from_vec!` (primitive_array.rs
  L1457-1467). `BooleanArray::from(Vec<bool>)` (boolean_array.rs L355).
- **Accessors:** `PrimitiveArray::values() -> &ScalarBuffer<T::Native>`
  (primitive_array.rs L657), and `ScalarBuffer<T>: Deref<Target = [T]>`
  (arrow-buffer scalar.rs L104) — so `.values()` IS a `&[T]` slice (no copy;
  passed straight to coil's `array_*` constructor). `BooleanArray` is bit-packed
  (`.values() -> &BooleanBuffer`, NOT `&[bool]`), so the decode materialises it
  via `.value(i) -> bool` (boolean_array.rs L190).
- **`DataType`:** `dora_node_api::arrow::datatypes::DataType` (arrow re-exports
  `arrow_schema::{DataType, ...}`), matched on `Float64/Float32/Int64/Int32/Boolean`.
- **Live integration-testing wire (for the Part-C e2e):**
  `InputData::JsonObject { data: serde_json::Value, data_type: Option<Value> }`
  (dora-message 0.8.0 `integration_testing_format.rs`) converts a JSON array
  (`[0.5,1.5,2.5]` + `"Float64"`) to a real `Float64Array`; outputs are written
  to `DORA_TEST_WRITE_OUTPUTS_TO` as `{ id, data:[...], data_type:"Float64" }`
  (the `arrow_json::ArrayWriter` rendering). So a LIVE typed round-trip is
  hermetically testable with NO daemon.

### 9.3 As-built surface (the 5-layer wiring)

1. **cabi** (`cobrust-dora/src/cabi.rs`):
   `__cobrust_dora_event_data_buffer(event) -> *mut Buffer` (boxed `coil::Array`)
   + `__cobrust_dora_event_send_output_buffer(event, output_id, buf) -> i64`.
   Both DUAL-BUILD (mirror `__cobrust_dora_event_send_output`): the
   `#[unsafe(no_mangle)]` export validates the output id against
   `DECLARED_OUTPUTS` (fail-closed, both builds), then `#[cfg]`-dispatches to
   `real::*` (real arrow bridge) or the synthetic arm (canned Float64
   `[1,2,3]` for `data_buffer`; an `output[id]=buffer[len=n]` marker for
   `send_output_buffer` — NO arrow referenced). The event retains the decoded
   payload in a new `DoraEventHandle.data_buffer: Option<coil::Array>` field
   (decoded once at recv via `real::decode_arrow_buffer`; canned synthetic).
   The `ndarray↔arrow` bridge: IN = `real::decode_arrow_buffer` (dtype-dispatch
   → `coil::array_*`); OUT = `real::coil_to_arrow` (`coil::Array` arm
   `.iter().copied()` → `Float64Array::from` etc.), single-sourced so
   `send_output_buffer` AND the hermetic round-trip test share ONE bridge.
2. **manifest** (`cobrust-types/src/ecosystem.rs`): two `DORA_EVENT_ADT` rows —
   `("data_buffer") -> coil_buffer_ty()` (0 args) +
   `("send_output_buffer") -> [Ty::Str, coil_buffer_ty()] -> Ty::Int`. REUSE
   `coil_buffer_ty()` verbatim — NO new ADT / `Ty`. A DISTINCT method name (NOT
   a `send_output` overload) per §2.5 compile-time clarity (U4 resolved this way).
3. **MIR:** NO change (confirmed). The Buffer handle ABI (opaque `*mut u8`,
   `Box` into/from raw) == coil's existing handle; the handle-method call lowers
   through the existing generic eco path; the returned Buffer is a non-Copy
   `Ty::Adt` so `drop.rs` schedules its scope-exit drop automatically; the `buf`
   arg auto-borrows via `lower_eco_arg`'s Value-handle Move→Copy upgrade (so the
   `.cb` scope still owns + drops it once).
4. **codegen** (`cobrust-codegen/src/llvm_backend.rs`): two extern decls —
   `data_buffer` reuses the `(ptr)->ptr` event-id fn type; `send_output_buffer`
   reuses the `(ptr,ptr,ptr)->i64` send_output fn type.
5. **drop:** the `data_buffer()` return is `.cb`-owned, freed ONCE via the
   EXISTING `__cobrust_coil_buffer_drop` (`handle_drop_symbol(COIL_BUFFER_ADT)`)
   — NO new drop symbol. `send_output_buffer` BORROWS `buf` (reads, never frees).
   NOTE (REPAIR BLOCKER-A): the drop symbol resolving in the type-checker MANIFEST
   is NOT the same as it RESOLVING AT LINK TIME — the build's link-set scan also
   had to learn about drop-glue (see §9.5b) so `libcoil.a` lands on the link line.

### 9.4 The `DoraUnknownOutputId` extension (ADR-0092 interaction)

`check.rs::try_synth_ecosystem_call` now fires `check_dora_send_output_id` for
`name == "send_output" || name == "send_output_buffer"` — so a literal typo'd
output id in EITHER method is caught at `cobrust check` (the §2.5-A
compile-time-catch), not just at the runtime `-1` backstop. The id is arg0 for
both methods; the buffer is arg1 (unchecked, like the str payload).

### 9.5 The cross-crate link fix (new, load-bearing)

`cobrust-dora` is the FIRST workspace crate to depend on `cobrust-coil` as a
LIBRARY (for `coil::Array` + the `array_*` constructors — the payload type).
Because `cobrust-coil`'s `cabi` module emits `#[no_mangle]` `__cobrust_coil_*`
symbols, a naive dep would embed all of them in `libdora.a` — and a `.cb`
program importing BOTH `dora` and `coil` (the canonical robot-policy shape)
would then hit ~125 duplicate-symbol link errors (libcoil.a vs libdora.a).
**Fix:** gate `coil::cabi` behind a DEFAULT-ON `cabi` feature; `cobrust-dora`
deps coil with `default-features = false` → it pulls the data type + constructors
WITHOUT the shim symbols. `import coil` builds (which compile `libcoil.a` via
`cargo build -p cobrust-coil`, default features) keep the full shim surface
UNCHANGED. A `#![cfg_attr(not(feature = "cabi"), allow(dead_code))]` in coil's
lib.rs silences the otherwise-`-D warnings`-fatal dead_code the shim-less build
leaves (the `*_scalar` aggregates / element-wise helpers the cabi module was the
sole consumer of); coil's benches + the two `cabi`-touching integration tests
carry `required-features = ["cabi"]` so `--no-default-features --all-targets`
skips them. This is the elegant-law mechanism (mirrors coil's `pyo3`/`faer`
gates) — NOT a behavior change.

### 9.5b The link-set drop-glue scan fix (REPAIR BLOCKER-A)

The §9.5 gate stops the DUPLICATE-symbol clash, but a SECOND, distinct
link-set bug surfaced after the audit: a `.cb` node that obtains a
`coil.Buffer` via `event.data_buffer()` and uses it WITHOUT any explicit
`coil.<fn>()` call (the most natural shape — an ECHO node:
`data_buffer()` → `send_output_buffer()`) FAILED TO LINK with
`ld: ___cobrust_coil_buffer_drop not found`, even though `cobrust check`
PASSED.

**Root cause.** The build's `collect_ecosystem_modules`
(`cobrust-cli/src/build/intrinsics.rs`) decides which `lib<mod>.a` archives go
on the link line by scanning the lowered MIR — but it scanned ONLY
`Terminator::Call { func: Constant::Str(sym) }` callees. The `coil.Buffer`
owned by the echo node emits NO `__cobrust_coil_*` CALL; its only `coil`
symbol reference is the scope-exit DROP-GLUE (`__cobrust_coil_buffer_drop`,
which codegen emits from a `Terminator::Drop` on the `Ty::Adt(COIL_BUFFER_ADT)`
local via `handle_drop_symbol`). That symbol lives ONLY in `libcoil.a`
(`nm libcoil.a` has it as `T`; `nm libdora.a` has NO
`cobrust_coil_buffer_drop`). So the scan never added `coil`, `libcoil.a` was
never linked, and the build died. `data_buffer()` is the FIRST non-`coil`
module to hand out a `coil.Buffer`, so this drop-glue-blindness was newly
exercised by this change (every prior `coil.Buffer` came from an explicit
`coil.*` constructor call that the `Call` scan already saw).

The original build report conflated the two resolutions: the drop symbol
resolves in the type-checker MANIFEST (`handle_drop_symbol(COIL_BUFFER_ADT)`)
but that says NOTHING about whether the LINKER finds it — those are different
layers.

**Fix.** `collect_ecosystem_modules` now ALSO scans `Terminator::Drop`: it
resolves the dropped place's local type and, when it is an `Ty::Adt(id, _)`
whose `handle_drop_symbol(id)` maps to a recognized ecosystem prefix,
registers THAT module too. This MIRRORS codegen's own
`emit_drop_for_ty` (`Ty::Adt(id, _) => handle_drop_symbol(*id)`), so the
link set is exactly the symbol set the object file references — for ANY
ecosystem handle dropped at scope exit, not just `coil`. This is general (it
fixes the same latent blindness for any future non-owning module that returns
another module's handle), additive, and changes no behavior for programs that
already linked.

**Coverage.** The masking test gap (the audit's F36/F37-class finding) is
closed: `dora_buffer_io_e2e::test_e2e_dora_echo_buffer_no_explicit_coil_call_links`
is the synthetic-build echo node (`data_buffer()` → `send_output_buffer()`,
NO `coil.*` call) — it BUILDS only because the drop-glue scan now pulls
`libcoil.a`; and `dora_real_node_e2e` Part C-D is the same echo node under the
REAL archive (links `libcoil.a` from the drop alone + round-trips the real
decoded values). Both omit ALL explicit `coil.*` calls so they exercise the
drop-glue-ONLY link path the pre-fix tests incidentally masked.

### 9.6 Verification (this change)

- **Hermetic ndarray↔arrow round-trip** (the UNCONDITIONAL proof,
  `--features dora-real`): 11 tests in `cabi::arrow_bridge_tests` — bit-identical
  round-trip per dtype (Float64/Float32/Int64/Int32/Bool), dtype-faithful (Int64
  stays Int64, no float up-cast), empty-array-per-dtype, the Utf8→None
  divergence, a 1000-event balanced-drop loop, AND (REPAIR MAJOR) the three
  null-bitmap tests: a null-bearing `Float64Array` + a null-bearing
  `BooleanArray` each decode to `None` (the named null-bitmap divergence — NOT a
  silent `[1.0, 0.0, 3.0]` / `[true, false, false]` fabrication), plus an
  all-`Some` (null-free) control that still round-trips bit-faithfully (no
  false-positive null rejection). ALL PASS.
- **Synthetic cabi shim contract:** 4 new tests in `cabi::tests` (canned-buffer
  `data_buffer`, None/null empty-buffer fallback, `send_output_buffer`
  declared/undeclared validation + count, null-buffer tolerance). ALL PASS (12
  total synthetic lib tests).
- **`.cb` build e2e** (`dora_buffer_io_e2e.rs`, synthetic build, 5 tests):
  `data_buffer()` → `coil.print_buffer` / `coil.mean` / `coil.full` →
  `send_output_buffer`; the `DoraUnknownOutputId` negative + the non-literal
  skip; AND (REPAIR BLOCKER-A) the MINIMAL echo node
  (`data_buffer()` → `send_output_buffer()` with NO explicit `coil.<fn>()` call)
  that exercises the drop-glue-ONLY link path. ALL PASS, exit 0.
- **LIVE real round-trip** (`dora_real_node_e2e.rs` Part C + the new Part C-D,
  gated like Parts A/B): Part C — a real `Float64Array` delivered on the live
  `EventStream` → `data_buffer()` decodes it → `send_output_buffer` publishes it
  → the output file carries it back bit-faithfully as `Float64` `[0.5,1.5,2.5]`;
  Part C-D (REPAIR BLOCKER-A) — the drop-glue-ONLY echo node links `libcoil.a`
  from the `coil.Buffer` DROP alone (no explicit coil call) and round-trips the
  REAL decoded `[3.5, 4.5]` to the output file. PASS under
  `COBRUST_DORA_REAL_E2E=1`.
- **Both feature states** build + clippy clean (synthetic default + dora-real);
  the wired crates (types/codegen/cli) + coil (default + no-default) clippy
  clean; fmt clean; `--locked` consistent (Cargo.lock = +1 line
  `cobrust-coil` under `cobrust-dora`'s deps).

### 9.7 Residual (still deferred, named divergences)

- `UInt8` (camera images) + `Utf8` typed arrays + n-D shape metadata — unchanged
  from §5: `data_str` carries Utf8; `bytes` (the B-1b `data_bytes()` accessor,
  LANDED — `Binary`/`UInt8` decode to a raw `bytes`) carries raw blobs; the coil
  `Dtype::UInt8` widening (the unity path) is the eventual numeric-Buffer fix.
  `decode_arrow_buffer` logs a one-line divergence for any non-numeric /
  unsupported dtype (never a silent drop) and `data_buffer()` returns an empty
  Buffer in that case.
- **Null bitmap (NAMED divergence — REPAIR MAJOR, was the U2 silent-alteration
  risk):** `coil::Array` is a DENSE `ndarray` with NO null concept, so a
  null-BEARING arrow array (`null_count() > 0`) cannot round-trip faithfully —
  reading `.values()`/`.value(i)` would silently materialise a null slot as the
  raw underlying buffer value (a null `f64` → `0.0`, a null `bool` → `false`).
  `decode_arrow_buffer` now GUARDS on `null_count() > 0` BEFORE the dense decode:
  it LOGS the divergence (the input dtype + the null count) and returns `None`
  (→ `data_buffer()` hands the empty-buffer fallback), so a null is NEVER
  silently fabricated. A null-free array (the dora numeric-payload common case)
  is unaffected. The eventual unity fix (if null-bearing dora payloads appear) is
  a coil masked-array type; until then, send a null-free array (or `data_str` for
  a non-numeric payload).
- Up-cast-vs-reject (U7): the as-built decode REJECTS-to-`None` (→ empty Buffer +
  a logged divergence) rather than up-casting `UInt8 → Float64` — the §2.5
  explicit-error lean. The null-bitmap guard adopts the same reject-don't-fabricate
  posture.
