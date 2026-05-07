---
doc_kind: module
module_id: mod:msgpack
crate: cobrust-msgpack
last_verified_commit: 908f67c
dependencies: [mod:translator]
---

# Module: msgpack

## Purpose

Cobrust translation of `msgpack-python` 1.0.8 — the M6 native-extension
milestone (constitution §7). Demonstrates that the translator
subsystem handles libraries with both pure-Python (`fallback.py`) and
Cython (`_packer.pyx` / `_unpacker.pyx`) sources end-to-end, with
byte-identical output across both source forms.

## Status

- **M6 — delivered.** All 19 functions translated via the synthetic-LLM
  pipeline (17 pure-Python + 2 Cython-typed). The Cython lexical shim
  (`mod:translator::cython`) routes `_packer.pyx` / `_unpacker.pyx`
  entries through `task = "translate_cython"`. The L2.behavior +
  L2.perf gates fire end-to-end with the perf-repair-loop demo
  exercising ADR-0010 §4. L3.pyo3-wrapper passes via subprocess
  CPython oracle; `--features pyo3` build path is wired per
  ADR-0011.

## Public surface (M6)

```rust
pub fn pack(value: &MsgValue, out: &mut Vec<u8>) -> Result<(), MsgError>;
pub fn pack_to_vec(value: &MsgValue) -> Result<Vec<u8>, MsgError>;
pub fn unpack(data: &[u8]) -> Result<MsgValue, MsgError>;

pub enum MsgValue {
    Nil,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Str(String),
    Bin(Vec<u8>),
    Array(Vec<MsgValue>),
    Map(Vec<(String, MsgValue)>),
}

impl MsgValue {
    pub fn to_json(&self) -> serde_json::Value;
}

pub struct MsgError {
    pub kind: MsgErrorKind,
    pub message: String,
}

pub enum MsgErrorKind { Pack, Unpack }

// Helper translations (also pub).
pub fn pack_array(items: &[MsgValue], out: &mut Vec<u8>) -> Result<(), MsgError>;
pub fn pack_bin(value: &[u8], out: &mut Vec<u8>);
pub fn pack_float(value: f64, out: &mut Vec<u8>);
pub fn pack_int(value: i64, out: &mut Vec<u8>);
pub fn pack_map(items: &[(String, MsgValue)], out: &mut Vec<u8>) -> Result<(), MsgError>;
pub fn pack_str(value: &str, out: &mut Vec<u8>);
pub fn pack_uint(value: u64, out: &mut Vec<u8>);
pub fn pack_uint_cython(value: u64, out: &mut Vec<u8>);    // Cython entrypoint
pub fn unpack_array(data: &[u8], pos: usize, length: usize) -> Result<(Vec<MsgValue>, usize), MsgError>;
pub fn unpack_bin(data: &[u8], pos: usize, length: usize) -> Result<(Vec<u8>, usize), MsgError>;
pub fn unpack_float(data: &[u8], pos: usize, n_bytes: usize) -> Result<(f64, usize), MsgError>;
pub fn unpack_int(data: &[u8], pos: usize, n_bytes: usize) -> Result<(i64, usize), MsgError>;
pub fn unpack_map(data: &[u8], pos: usize, length: usize) -> Result<(Vec<(String, MsgValue)>, usize), MsgError>;
pub fn unpack_one(data: &[u8], pos: usize) -> Result<(MsgValue, usize), MsgError>;
pub fn unpack_str(data: &[u8], pos: usize, length: usize) -> Result<(String, usize), MsgError>;
pub fn unpack_uint(data: &[u8], pos: usize, n_bytes: usize) -> Result<(u64, usize), MsgError>;
pub fn unpack_uint_cython(data: &[u8], pos: usize, n_bytes: i32) -> Result<u64, MsgError>;  // Cython entrypoint
```

## Scope window (M6)

In scope:

- nil, bool, signed integer (i64-clamped), float (f32 + f64), str
  (utf-8), binary (bytes), fixed-size array, fixed-size map (str keys).
- Pure-Python `fallback.py` form + Cython `_packer.pyx` /
  `_unpacker.pyx` typed entrypoints.

Out of scope (M7+):

- ext types (any kind); timestamp ext.
- Streaming `Unpacker.feed()`.
- `default=` / `object_hook=` callbacks.
- raw=False legacy mode.

## Invariants

- **Bytes-identical output.** For every M6-scope input, the
  `cobrust-msgpack::pack` output equals
  `corpus/msgpack/upstream/msgpack_core.pack` output byte-for-byte.
  Verified at `crates/cobrust-msgpack/tests/msgpack_fuzz.rs::pack_unpack_round_trips_panic_free`
  (≥ 1000 random inputs across 3 seeds).
- **Round-trip identity.** `unpack(pack_to_vec(x))` equals
  `canonicalise(x)` for every M6-scope value, where
  `canonicalise()` collapses `Int(n>=0)` to `UInt(n)` (matching the
  pack→unpack lossy fixint path).
- **Determinism.** Identical
  `(source, toolchain, router decisions)` ⇒ byte-identical generated
  crate. Verified at
  `crates/cobrust-translator/tests/msgpack_pipeline.rs::msgpack_pipeline_is_deterministic_across_runs`.
- **Sorted-key map encoding.** `pack_map(...)` always iterates in
  sorted-key order so emission is deterministic across runs.

## Gates (M6 — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L0 | spec produced | `corpus/msgpack/spec.toml` + harness/ committed | ✅ |
| L1 | code emitted | every file has provenance header + per-fn task tag | ✅ |
| L2.build | `cargo build --release` | zero warnings | ✅ |
| L2.behavior | bytes-identical fuzz | ≥ 1000 panic-free + diff against `msgpack_core.pack` | ✅ |
| L2.perf | benchmark | ≥ 0.7× CPython on `pack`+`unpack` (native-ext tier per ADR-0010 §3); pass_ratio = 1.0 | ✅ |
| L3.pyo3 | PyO3-shaped wrapper | subprocess CPython oracle + `--features pyo3` compiles | ✅ |
| L3.dependents | redis-py + msgpack-numpy | `gates.dependents.covered = ["redis-py", "msgpack-numpy"]`; `deferred = ["pyspark"]` | ✅ 2/3 + 1 deferred per ADR-0010 |

Failure at L2.perf routes through the `PerfVerifier` callback per
ADR-0010 §4 — the canned table ships a deliberately perf-broken
attempt-1 of `pack_uint` and a corrected attempt-2 to exercise the
repair loop without real LLM keys.

## Repair-loop demo

`crates/cobrust-translator/tests/msgpack_pipeline.rs::msgpack_pipeline_perf_repair_loop_recovers_on_attempt_2`
asserts: the pipeline lands at attempt-2 (`repair_attempts == 1`),
the corrected emission contains no `PERF-BROKEN` marker, and the
diagnostic blob is persisted to
`out/msgpack/diagnostics/pack_uint__2.toml`. Companion test
`msgpack_pipeline_escalates_when_perf_always_fails` verifies the
escalation path raises `EscalationExceeded { failed_gate: "l2_perf" }`
and writes `failure_report.md`.

## Provenance manifest

Written to `crates/cobrust-msgpack/PROVENANCE.toml`. Schema per
ADR-0007 §3 + ADR-0010 §"M6 manifest fields":

```toml
[source]
library = "msgpack"
version = "1.0.8"
sha256 = "44dab293fdcff9850974ba37388acdc1a4be42fba7c6d76ae512067a63777bb8"
file_count = 1

[gates]
l2_perf = "pass (native-ext tier ≥ 0.70× per ADR-0010 §3; ...)"
l3_downstream_dependents = "pass 2/3 (redis-py, msgpack-numpy); deferred 1/3 (pyspark) to M7 per ADR-0010"

[gates.dependents]
covered = ["redis-py", "msgpack-numpy"]
deferred = ["pyspark"]
deferred_reason = "M6 budget; pyspark needs JVM; M7+ widens"
skipped = []
skipped_reason = ""
```

## Done means (M6 — DONE)

- [x] All 19 spec functions translated (17 pure-Python + 2 Cython-typed).
- [x] L0 spec + canned table + harness committed at `corpus/msgpack/`.
- [x] L2.behavior fuzz: ≥ 1000 inputs × 3 seeds; bytes-identical
      with `msgpack_core.pack` for all panic-free cases.
- [x] L2.perf gate: native-ext tier (0.7×); perf-repair loop tested
      end-to-end via `PerfVerifier` injection.
- [x] L3.pyo3 wrapper + `--features pyo3` build path wired per
      ADR-0011 §3.
- [x] L3.dependents 2/3 (redis-py + msgpack-numpy); pyspark
      explicitly deferred to M7 per ADR-0010.
- [x] Determinism asserted across runs.
- [x] Cython lexical shim (`mod:translator::cython`) parses
      `_packer.pyx` / `_unpacker.pyx` constructs.
- [x] PyO3 binding module (`crates/cobrust-msgpack/src/pyo3_bindings.rs`)
      exposes `pack` / `unpack` with a clean Python value-tree
      conversion.

## Non-goals

- Not a complete msgpack implementation: ext types, timestamps,
  streaming reads are M7+.
- Not a Python-side wheel publication: `setup.py` is a placeholder
  per ADR-0011 §6 (M7+ adds maturin).
- Not a benchmark target for cross-language perf comparison: the
  M6 numbers are recorded for the gate, not for marketing.

## Cross-references

- `mod:translator` — pipeline entrypoint that produced this crate.
- `mod:dateutil` — sister M5 crate; M6 widened its L3 to 5/5
  per ADR-0010 §5.
- [adr:0010](../adr/0010-native-ext-translation.md) — M6 methodology.
- [adr:0011](../adr/0011-pyo3-build-path.md) — PyO3 build path.
- [adr:0007](../adr/0007-translator-pipeline.md) — pipeline base.
- [adr:0008](../adr/0008-l2-perf-and-repair-loop.md) — perf-repair
  loop infrastructure.
- Constitution `CLAUDE.md` §7 — M6 milestone definition.
