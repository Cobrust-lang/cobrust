# python-dateutil corpus (M5)

This directory vendors a representative subset of the upstream
[`python-dateutil`](https://github.com/dateutil/dateutil) library
(version recorded in `UPSTREAM_VERSION`) for the Cobrust M5 translator
pipeline. M5 is the second translated library after `tomli` (M4).

## Scope window (M5)

The vendored source is **a deliberate subset** of the upstream library:
two sub-modules (`parser`, `relativedelta`) trimmed to a manageable
slice for synthetic-LLM canned responses. M6+ widens.

**In scope** (functions translated and gated):

`parser` sub-module (`upstream/parser_core.py`):

- `parse_iso(s: str) -> datetime` — ISO-8601 date / datetime parser
  for the strict subset `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SS`,
  `YYYY-MM-DDTHH:MM:SSZ`, and the same with offset `±HH:MM`.

`relativedelta` sub-module (`upstream/relativedelta_core.py`):

- `relativedelta_add(base: datetime, years, months, weeks, days,
  hours, minutes, seconds) -> datetime` — pure-arithmetic
  implementation of `dateutil.relativedelta.relativedelta` `__add__`.

**Out of scope (deferred to M6 widening)**:

- `parser.parse` — full free-form parser (handles 30+ formats).
- `tz` — timezone resolution.
- `rrule` — recurring rule expansion.
- `easter` — easter date computation.

The CPython oracle is `dateutil` itself for inputs in the scope
window. Where `dateutil`'s public API exposes the same call surface
(it does for `relativedelta_add` via `relativedelta(...)+ base`),
the differential gate compares directly. Where the API name is
M5-specific (`parse_iso`), the oracle is the corresponding
`datetime.fromisoformat` strict call (Python 3.11+).

## Files

- `UPSTREAM_VERSION` — pinned upstream release (`2.9.0.post0`).
- `UPSTREAM_LICENSE` — Apache-2.0 + BSD-3-Clause dual-license note.
- `upstream/parser_core.py` — vendored Python source subset for
  `parse_iso`.
- `upstream/relativedelta_core.py` — vendored Python source subset
  for `relativedelta_add`.
- `upstream_tests/test_parser_core.py` — pytest-format positive +
  negative cases for `parse_iso`.
- `upstream_tests/test_relativedelta_core.py` — pytest-format cases
  for `relativedelta_add`.
- `spec.toml` — L0 spec (machine-readable behavior contract).
- `harness/h_parse_iso.py` — L0 differential-test harness.
- `harness/h_relativedelta_add.py` — L0 differential-test harness.
- `canned_llm_responses.toml` — pre-recorded LLM responses keyed by
  `(task, function, attempt)`. Includes attempt-1 (deliberately
  broken) and attempt-2 (correct) responses for `parse_iso` to
  exercise the M5 repair loop end-to-end.
- `perf.toml` — per-library performance threshold override.
- `dependents/croniter/` — vendored test subset for L3 dependent.
- `dependents/freezegun/` — vendored test subset for L3 dependent.

## ADR linkage

- `adr:0007` — pipeline shape inherited from M4.
- `adr:0008` — repair loop, L2.perf gate.
- `adr:0009` — L3 downstream dependents (which 2 of 5).
