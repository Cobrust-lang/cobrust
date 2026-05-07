# tomli corpus (M4)

This directory vendors a representative subset of the upstream
[`tomli`](https://github.com/hukkin/tomli) library (version recorded in
`UPSTREAM_VERSION`) for the Cobrust M4 translator pipeline.

## Scope window (M4)

The vendored source is **a deliberate subset** of the upstream parser:
enough surface to drive a non-trivial L0..L3 closed-loop run while
keeping the function count tractable for synthetic-LLM canned
responses.

**In scope** (functions translated and gated):

- `loads(s: str) -> dict` — entrypoint
- `_skip_whitespace(state)` — lexical helper
- `_parse_basic_string(state)` — `"..."` strings
- `_parse_literal_string(state)` — `'...'` strings
- `_parse_int(state)` — decimal ints (no underscores in M4)
- `_parse_bool(state)` — `true` / `false`
- `_parse_array(state)` — `[...]` arrays of homogeneous primitives
- `_parse_inline_table(state)` — `{ ... }` inline tables
- `_parse_key(state)` — bare keys (alpha + underscore)
- `_parse_table_header(state)` — `[section]` headers
- `_parse_kv(state, dest)` — `key = value` pairs

**Out of scope (M5 widens)**:

- Multi-line strings, raw triple-quoted strings (`"""..."""`).
- Numeric exotica: hex/oct/bin ints, underscores, infinities, NaN,
  floats with exponents.
- Datetime / time / date types.
- Array-of-tables (`[[...]]`).
- Inline-table key-paths (`a.b.c = 1`).

The CPython oracle is the full Python 3.11 `tomllib`; for inputs
inside the M4 scope window the two must agree exactly. Inputs outside
the scope window are not run through the differential gate.

## Files

- `UPSTREAM_VERSION` — pinned upstream release.
- `UPSTREAM_LICENSE` — MIT license text from upstream.
- `upstream/tomli_loads.py` — the vendored Python source (subset).
- `upstream_tests/test_loads.py` — pytest-format tests; the M4 gate
  runs these against both the translated crate and CPython
  `tomllib` for differential equivalence.
- `spec.toml` — L0 spec (machine-readable behavior contract).
- `harness/h_loads.py` — L0 differential-test harness driver.
- `canned_llm_responses.toml` — pre-recorded LLM responses keyed by
  prompt hash; used by the synthetic provider during M4 gate runs.
