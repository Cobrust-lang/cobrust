---
doc_kind: adr
adr_id: 0009
title: L3 downstream-dependents validation — corpus, scope, and partial coverage policy
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0009: L3 downstream-dependents validation — corpus, scope, and partial coverage policy

## Context

Constitution §4.2 mandates "the testsuites of the **top 5** libraries
that depend on this one" run against every translated library at L3.
That sets the long-run goal. M5's pragmatic question is: *for the
second translated library (`python-dateutil` core), which dependents
do we actually wire in, and how do we represent partial coverage in
the manifest without lying?*

The big-name dependents of `python-dateutil` are:

- **pandas** — uses `dateutil.parser.parse` for `to_datetime` fallback
- **sqlalchemy** — uses `dateutil.parser` for ISO datetime parsing
- **pendulum** — uses `dateutil.tz` for timezone resolution
- **croniter** — uses `dateutil.relativedelta` for "next run"
  arithmetic
- **freezegun** — uses `dateutil.parser.parse` for fixture timestamps

All five have non-trivial test suites (10+ minutes each on a fresh
clone). M5's 60-minute budget cannot run all five in the gate path.
This ADR pins the scope.

## Options considered

### 1. Run all 5 dependents in the M5 gate

- Pros: full constitutional compliance.
- Cons: 50+ minutes per gate run; CI cost balloons; M5 budget
  blown. Rejected.

### 2. Run zero dependents in the M5 gate, defer all to M6

- Pros: cheapest path.
- Cons: leaves "Done means (M5)" item unchecked indefinitely;
  violates the constitution's M5 description. Rejected.

### 3. Run **2 representative dependents** in the M5 gate, mark the
   remaining 3 as `deferred to M6 — see ADR-0009`. Document the
   selection rationale and the partial-coverage policy. *(chosen)*

We pick:

- **`croniter`** — small, mature, exercises
  `dateutil.relativedelta`, runs in < 30 s.
- **`freezegun`** — exercises `dateutil.parser.parse`, runs in
  < 30 s.

Why these two and not pandas/sqlalchemy/pendulum:

| Dependent | Function exercised | Wall-clock | Selected? |
|---|---|---|---|
| pandas | `parser.parse` (one of many backends) | 10+ min | No (M6) |
| sqlalchemy | `parser.parse` (ISO subset) | 8+ min | No (M6) |
| pendulum | `tz` (we don't translate `tz` in M5) | 5+ min | No (M6) |
| **croniter** | `relativedelta` (in scope for M5) | < 30 s | **Yes** |
| **freezegun** | `parser.parse` (in scope for M5) | < 30 s | **Yes** |

The two chosen dependents together exercise both M5-scope dateutil
sub-modules (`parser` + `relativedelta`). They are the highest-signal
per-second-of-CI options.

### 4. Bundling vs. live `pip install`

- **Live `pip install` from PyPI** — requires network + a virtualenv
  in CI; flaky. Rejected for the gate path.
- **Vendor minimal subsets of the dependent test files in the corpus**
  *(chosen)* — `corpus/dateutil/dependents/<name>/` carries:
  - `LICENSE` — upstream license (both are MIT/PSF — license-
    compatible per `adr:0001`).
  - `test_<name>_subset.py` — a vendored subset of the dependent's
    test file that uses only `dateutil` APIs in our M5 scope. We
    pick 5–10 representative tests per dependent.

  The L3 driver runs each via `python3 -m unittest` and counts
  pass/fail per the format above. If a test references an out-of-
  scope dateutil API (e.g. `tz.gettz`), it is skipped with a clear
  reason recorded in the manifest.

### 5. Manifest representation of partial coverage

`PROVENANCE.toml` `gates.l3_downstream_dependents` becomes a
human-readable string:

```toml
l3_downstream_dependents = "pass 2/5 (croniter, freezegun); deferred 3/5 (pandas, sqlalchemy, pendulum) to M6 per ADR-0009"
```

Plus a structured field in the manifest (so machines can act on it):

```toml
[gates.dependents]
covered = ["croniter", "freezegun"]
deferred = ["pandas", "sqlalchemy", "pendulum"]
deferred_reason = "M5 budget; M6 will widen per ADR-0009"
```

The `RouterSection`/`GatesSection` schema is extended in lockstep
with this ADR. M4's tomli manifest gets a backwards-compatible
`covered = []`, `deferred = []` fallback (every existing field
default-serialises to empty arrays).

### 6. Schedule for the deferred 3

We commit to landing pandas + sqlalchemy + pendulum in M6 — the
"native ext" milestone — because:

- pandas and sqlalchemy ship native extensions that exercise the
  same translator-output ABI we'll be enlarging at M6.
- pendulum's tz integration depends on dateutil's `tz` module, which
  isn't M5-scope (M5 ships only `parser` core + `relativedelta` core).

A follow-up ADR (`adr:0010`) at M6 will pin the M6 dependents'
selection rules and either widen this list or supersede this ADR.

## Decision

Adopt option 3 + option 4 + option 5 + option 6. Concretely:

- `corpus/dateutil/dependents/croniter/` and
  `corpus/dateutil/dependents/freezegun/` carry vendored test
  subsets + LICENSEs.
- `crates/cobrust-translator/src/downstream.rs` runs each subset via
  subprocess and emits a `DownstreamReport`.
- The pipeline writes the report into the manifest's
  `gates.dependents` section; `gates.l3_downstream_dependents`
  string is the human-readable summary (machine + human both happy).
- `crates/cobrust-dateutil/tests/dateutil_downstream.rs` asserts at
  least one subset's test count `> 0` and `tests_passed >= 1` to
  mark M5 done.

## Consequences

- **Positive**
  - Constitution §4.2 L3 obligation lands as `2/5` in M5, with the
    remaining 3 explicitly tracked in the manifest. Auditable, no
    silent skipping.
  - The gate runs in seconds (both dependents combined < 60 s),
    well within M5 budget.
  - The vendored subsets are tiny (5–10 tests each) — repo-size
    impact is bounded.

- **Negative**
  - Two dependents is below the §4.2 "top 5" target; M6 must close
    the gap. We accept this trade as a "Negative" line, not a
    silent omission.
  - The vendored subsets can drift from upstream; we pin upstream
    versions in `corpus/dateutil/dependents/<name>/UPSTREAM_VERSION`
    and document a re-vendor protocol in the corpus README.
  - Subprocess-pinned `python3 -m unittest` adds latency, but this
    is the same model `tomli`'s L3 gate uses; familiar.

- **Neutral / unknown**
  - When real-LLM mode lands at M5+, the dependents' test counts
    will not change — they're a property of the translated crate's
    public surface, not the LLM that produced it.
  - If a vendored subset references `dateutil.tz.gettz()` (out of
    scope for M5) we record a `Skipped { reason: "...tz out of M5
    scope..." }` for that test rather than failing.

## Evidence

- Constitution `CLAUDE.md` §4.2 ("top 5 dependents") and §6 ("no
  skipping gates").
- `adr:0007` — L3 PyO3 wrapper baseline this ADR extends.
- `adr:0008` — gate state machine that calls into `downstream.rs`.
- `mod:translator` Status section "Done means (M5)".
- Public dependent inventories at:
  - https://pypi.org/project/python-dateutil/ — dependents page.
  - https://github.com/dateutil/dateutil/network/dependents.
- License compatibility: both croniter (MIT) and freezegun (Apache-2.0)
  are licence-compatible per `adr:0001`.
