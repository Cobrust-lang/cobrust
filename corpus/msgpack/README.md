# corpus/msgpack — M6 corpus subset

## Scope window

This corpus vendors a deliberately scoped subset of msgpack-python 1.0.8
for the M6 native-extension translation milestone. See ADR-0010 for the
full methodology and ADR-0011 for the PyO3 build path.

### In scope (M6)

- `pack(obj) -> bytes`, `unpack(bytes) -> obj`
- `Packer` / `Unpacker` skeleton classes
- Value types: nil, bool, signed integer (i64-clamped), float (f32 + f64),
  str (utf-8), binary (bytes), fixed-size array, fixed-size map.

### Out of scope (M7+)

- Ext types (any kind); timestamp ext.
- Streaming `Unpacker.feed()`.
- `default=` / `object_hook=` callbacks.
- raw=False legacy mode.

## Layout

- `upstream/fallback.py` — pure-Python encoder/decoder (the canonical
  byte-output reference).
- `upstream/_packer.pyx` / `upstream/_unpacker.pyx` — Cython sources;
  emit byte-identical output to `fallback.py`.
- `upstream/exceptions.py` — error types.
- `harness/h_pack.py`, `harness/h_unpack.py` — L0 differential harnesses.
- `canned_llm_responses.toml` — synthetic-LLM responses for both
  `task = translate` (pure-Py) and `task = translate_cython` (Cython).
- `perf.toml` — `threshold = 0.7, pass_ratio = 1.0` (native-ext tier).
- `dependents/` — vendored test subsets for redis-py + msgpack-numpy.

## Re-vendor protocol

Pin upstream by tag in `UPSTREAM_VERSION`. When pulling new upstream:

1. Diff the new fallback.py / _packer.pyx / _unpacker.pyx against the
   vendored copies.
2. Update `source_sha16` in canned_llm_responses.toml entries that
   reference the changed files.
3. Re-run the pipeline: `cargo test -p cobrust-translator --test
   msgpack_pipeline`.
4. Commit corpus + canned-table updates atomically.

The pipeline's staleness check (synthetic-stale error) ensures stale
responses cannot ship undetected.
