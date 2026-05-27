---
doc_kind: module
module_id: mod:molt
crate: cobrust-molt
last_verified_commit: 908f67c
dependencies: [mod:translator]
---

# Module: molt

## Purpose

The second library translated end-to-end by `mod:translator` (M5
deliverable). Pure-Rust subset of the
[`python-dateutil`](https://github.com/dateutil/dateutil) library:
strict ISO-8601 parsing (`parse_iso`) and pure-arithmetic
relative-delta addition (`relativedelta_add`). **Auto-generated** —
every byte of `src/` is emitted by the translator pipeline; the crate
is committed to the repo for gate stability (M4 precedent).

## Status

- **M6 — widened.** L3 dependents widened from 2/5 to 4/5 + 1
  skipped per ADR-0010 §5: pandas + sqlalchemy added (3 ISO-subset
  tests each); pendulum vendored as a SKIP-only file because the
  `tz` module is out of scope. `--features pyo3` build path wired
  per ADR-0011 (the M5 placeholder lit up; `pyo3_bindings.rs`
  exposes `parse_iso` / `relativedelta_add` to Python).
- **M5 — delivered.** All gates green:
  - L0: `corpus/dateutil/spec.toml` + harness committed.
  - L1: 8 functions translated via synthetic-LLM mode; provenance
    headers per function; `PROVENANCE.toml` validates.
  - L2.build: zero warnings on `cargo build --release`.
  - L2.behavior: 9 positive + 5 negative parse cases match CPython
    `datetime.fromisoformat`; 6 relative-delta cases agree with the
    upstream Python harness; 3072-input panic-free fuzz across
    `parse_iso` + `relativedelta_add`.
  - L2.perf: report at `target/cobrust-bench/dateutil/<commit>/report.json`;
    threshold per ADR-0008 §2 (per-library `pass_ratio = 0.5`).
  - L3 (PyO3-shaped wrapper): subprocess-based differential gate
    against CPython's `datetime.fromisoformat` (the strict-ISO oracle).
  - L3 (downstream dependents per ADR-0009): croniter + freezegun
    vendored test subsets pass (5 + 5 cases). pandas, sqlalchemy,
    pendulum deferred to M6 — closed at M6 per ADR-0010 §5.
- **Out of scope (M7+)**:
  - Free-form `parser.parse` (handles 30+ formats).
  - `tz` timezone resolution.
  - `rrule` recurring rule expansion.
  - `easter` date computation.

## Public surface (M5)

```rust
// crate root re-exports.
pub use molt::{
    DateTuple, ParserError, days_in_month, is_digit, is_leap_year,
    normalize_datetime, parse_iso, relativedelta_add,
};

pub fn parse_iso(src: &str) -> Result<DateTuple, ParserError>;

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn relativedelta_add(
    year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32,
    add_years: i32, add_months: i32, add_weeks: i32, add_days: i32,
    add_hours: i32, add_minutes: i32, add_seconds: i32,
) -> DateTuple;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTuple {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub has_tz: i32,
    pub tz_offset_minutes: i32,
    pub consumed: usize,
}

impl DateTuple {
    pub fn to_json(&self) -> serde_json::Value;
}

#[derive(Clone, Debug)]
pub struct ParserError {
    pub message: String,
    pub pos: usize,
}
```

## Scope window (M5)

In scope (CPython `datetime.fromisoformat` is the oracle for inputs
in this list):

- `YYYY-MM-DD` — date-only.
- `YYYY-MM-DDTHH:MM:SS` — naive datetime.
- `YYYY-MM-DDTHH:MM:SSZ` — Zulu (UTC).
- `YYYY-MM-DDTHH:MM:SS+HH:MM` — explicit positive offset.
- `YYYY-MM-DDTHH:MM:SS-HH:MM` — explicit negative offset.
- Leap-day handling: `2024-02-29` accepted, `2025-02-29` rejected.
- Bounds: month 1–12, day 1–31, hour 0–23, minute 0–59, second 0–60
  (leap second tolerated to match `datetime.fromisoformat`).

`relativedelta_add` semantics (mirrors `dateutil.relativedelta`):

- Years and months applied first, with day-of-month clamped to the
  resulting month's length (Feb 29 on non-leap years collapses to
  Feb 28).
- Weeks then days then time fields applied.
- Cascade normalisation through carry / borrow on every field.

Out of scope (M6 widens — inputs outside this set are not required to
match CPython):

- Free-form parser.parse (30+ format heuristics).
- Datetime ranges with fractional seconds.
- Multi-timezone resolution (`tzlocal`, `gettz`).
- `rrule` / `easter` modules.

## Provenance

Every emitted file in `src/` carries a comment header:

```text
// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: dateutil 2.9.0.post0
// oracle: cpython 3.11 (module: dateutil)
// functions translated: 8
// see PROVENANCE.toml for the full manifest.
```

Per-function blocks in `parser.rs` carry a one-liner:

```text
// fn:parse_iso provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:<hex>
```

The full manifest at `crates/cobrust-molt/PROVENANCE.toml` records:

- Source library + version + 64-hex SHA-256.
- Oracle runtime + version + import path.
- Verification seeds (`[42, 1337, 0xDEADBEEF]`) + per-function fuzz
  budget (1024 default).
- Router strategy (`synthetic`) + models used (`dateutil-canned-v1`).
- Toolchain string + `deterministic_id` (BLAKE3).
- L0..L3 gate evidence including:
  - `gates.l2_perf` — pass + report path.
  - `gates.l3_downstream_dependents` — `"pass 2/5 (croniter, freezegun); deferred 3/5 (pandas, sqlalchemy, pendulum) to M6 per ADR-0009"`.
  - `gates.dependents` — structured `covered`/`deferred` arrays.
  - `gates.l2_behavior` — annotated with repair-loop iteration count
    when the closed loop ran (M5 dateutil records `"...(after 1
    repair-loop iteration on parse_iso)"` to evidence the repair path
    per ADR-0008 §5).

## Repair-loop evidence (M5)

The dateutil corpus ships **two** canned responses for `parse_iso`:

- `attempt = 1` — deliberately broken (swaps year/month, returns
  wrong tuple). Exists to exercise the L2.behavior repair path.
- `attempt = 2` — corrected. The pipeline's `BehaviorVerifier` hook
  rejects attempt-1 (L2.behavior diagnostic blob → repair loop →
  re-dispatch with `attempt: 2` header line) and accepts attempt-2.

This is the **first end-to-end exercise of the closed loop** — see
ADR-0008 §5 and `tests/dateutil_pipeline.rs::dateutil_pipeline_repair_loop_recovers_on_attempt_2`.

## Done means (M5 — DONE)

- [x] `cobrust translate corpus/dateutil` produces `cobrust-molt/`.
- [x] PyO3-shaped wrapper directory present (`python/`); subprocess
      differential gate against CPython oracle passes.
- [x] Strict ISO-8601 positive + negative test bank passes.
- [x] Relative-delta arithmetic matches the upstream Python harness.
- [x] L2.perf gate + JSON report at
      `target/cobrust-bench/dateutil/<commit>/report.json`.
- [x] L3 downstream dependents 2/5 (croniter, freezegun) pass; 3/5
      deferred per ADR-0009.
- [x] Repair loop exercised end-to-end via deliberately-broken
      `parse_iso` attempt-1 + corrected attempt-2.
- [x] Manifest captures: source SHA, oracle versions, fuzz seeds,
      router decisions, deterministic build ID, repair attempts,
      dependents split.

## Done means (M6)

- [ ] Native PyO3 extension under `--features pyo3`.
- [ ] pandas / sqlalchemy / pendulum dependents wired in (close out
      ADR-0009's "deferred 3/5" tail).
- [ ] Real-LLM smoke test on dateutil under `--features real-llm`.

## Non-goals

- **Not** a full `python-dateutil` implementation — see "Scope window".
- **Not** hand-written. Editing `src/parser.rs` or `src/lib.rs`
  directly is forbidden; regenerate via the pipeline.
- **Not** binary-compatible with `dateutil` itself — only the strict
  ISO subset is contracted to match.

## Cross-references

- `mod:translator` — pipeline that emits this crate.
- `mod:nest` — first translated crate (M4 precedent).
- `adr:0007` — translator architecture + provenance schema.
- `adr:0008` — repair loop + L2.perf gate.
- `adr:0009` — L3 downstream-dependents partial-coverage policy.
- `corpus/dateutil/README.md` — vendored upstream + scope window doc.
- Constitution `CLAUDE.md` §4.2 (translator pipeline), §7 (M5 done).
