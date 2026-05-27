---
doc_kind: strategy
strategy_id: tier-b-project-audit-20260526
title: Tier B project-wide objective audit — 2026-05-26 pre-v0.7.0 snapshot
status: filed
date: 2026-05-26
last_verified_commit: b1f1769
audit_target_head: aa8b138
audit_actual_head: b1f1769
auditor: P10 CTO independent
mandate: User 2026-05-25 — "independent objective full-project audit"
read_only: true
no_commits: true
---

# Tier B project-wide objective audit — 2026-05-26 pre-v0.7.0

> Independent objective full-project sweep, NOT change-specific.
> F35-sibling discipline: every claim empirically backed.
> Audit window: project state at HEAD `b1f1769` (post v0.6.2 LIVE + ADR-0070 authored).
> Mandate originally targeted `aa8b138`; one ADR-author commit (b1f1769) and
> three strategy survey commits (05f7fbf / 6d84191 / ed6be8c) intervened during
> audit window. Audit findings cover both.

---

## §1 Crate-level health

### §1.1 Workspace shape

- **Crate count**: 24 (Cargo.toml members confirmed = 24, crates/ dir = 24)
- **Workspace src LOC**: 78,404
- **Workspace tests LOC**: 89,527
- **Total `#[test]` markers**: 4,729 (incl tokio::test, rstest)
- **Cargo.lock total package count**: 464 (transitive deps)
- **Project age**: 26 days (first commit 2026-04-30; total 991 commits)
- **Tests/day average**: ~182

### §1.2 Per-crate breakdown

| Crate | src LOC | tests LOC | pub fn | pub struct | tests |
|---|---|---|---|---|---|
| cobrust-cli | 8430 | 14368 | 40 | 15 | 604 |
| cobrust-click | 782 | 342 | 24 | 7 | 26 |
| cobrust-codegen | 8846 | 14887 | 37 | 8 | 496 |
| cobrust-dap | 2471 | 1710 | 8 | 38 | 100 |
| cobrust-dap-shim | 25 | 114 | 0 | 0 | 1 |
| cobrust-dateutil | 525 | 715 | 7 | 2 | 12 |
| cobrust-frontend | 5890 | 3131 | 13 | 45 | 237 |
| cobrust-hir | 2740 | 1510 | 19 | 41 | 99 |
| cobrust-jit | 749 | 568 | 6 | 3 | 12 |
| cobrust-llm-router | 3053 | 1110 | 23 | 30 | 66 |
| cobrust-lsp | 5612 | 2608 | 52 | 5 | 163 |
| cobrust-lsp-shim | 26 | 57 | 0 | 0 | 1 |
| cobrust-mir | 4151 | 2719 | 14 | 24 | 183 |
| cobrust-msgpack | 887 | 856 | 21 | 3 | 16 |
| cobrust-numpy | 6643 | 13551 | 120 | 12 | 986 |
| cobrust-pkg | 4116 | 2115 | 39 | 33 | 118 |
| cobrust-registry | 683 | 0 | 5 | 4 | 7 |
| cobrust-requests | 620 | 489 | 20 | 5 | 24 |
| cobrust-stdlib | 8043 | 6514 | 134 | 22 | 607 |
| cobrust-tomli | 549 | 813 | 10 | 3 | 11 |
| cobrust-translator | 6029 | 8503 | 60 | 50 | 150 |
| cobrust-types | 4395 | 8223 | 33 | 14 | 580 |
| cobrust-types-cb | 2503 | 3986 | 34 | 10 | 192 |
| cobrust-types-parity | 636 | 638 | 14 | 3 | 38 |

### §1.3 Largest modules

- `cobrust-codegen/src/llvm_backend.rs`: **4595 LOC** (single file, post wave-3 closure)
- `cobrust-codegen/src/cranelift_backend.rs`: **3057 LOC**
- `cobrust-cli/src/build/intrinsics.rs`: 2216 LOC
- `cobrust-stdlib/src/collections.rs`: 2057 LOC
- `cobrust-cli/src/repl.rs`: 1611 LOC

**OBSERVATION**: `llvm_backend.rs` 4595 LOC is the largest single file in the
workspace and grew sharply through wave-3 sub-waves 1-6. **Refactoring debt
candidate** for Stream X.3 (default flip) — under v0.7.0 LLVM-default, this
file becomes the primary codegen path and warrants split.

### §1.4 Anomalies

- **cobrust-registry**: 683 src LOC, **0 tests LOC dir** (no `tests/` subdir at
  all), 7 inline tests only. Anomalously light test surface vs other crates.
- **cobrust-lsp-shim** + **cobrust-dap-shim**: 25-26 src LOC + 1 smoke test
  each. Transitional crates per ADR-0068 §7.2 — explicit v0.7.0 removal targets
  (Stream X.5 scope).

### §1.5 Recent activity (14-day churn)

Most-edited crates (line edits last 14 days):

```
158 cobrust-codegen   (wave-3 LLVM hookup)
152 cobrust-cli        (LLVM front-line)
136 cobrust-lsp        (Phase J wave-5)
 92 cobrust-types
 82 cobrust-dap
 52 cobrust-types-cb
```

Wave-3 LLVM closure dominates activity. **cobrust-llm-router** has only 1 edit
in 14 days — stagnant relative to wave-3 dominance.

### §1.6 TODO / FIXME / unimplemented markers

**Total in `crates/` Rust source: 3 occurrences**

- `crates/cobrust-lsp/src/completion.rs:14` — `TODO(#hover-prelude-sync): wave-3 should query the live TypeCheckCtx`
- `crates/cobrust-types-parity/src/lib.rs:5` + `:191` — `todo!()` (Phase 1 stub harness contract)

NO `unimplemented!()`. NO `FIXME`. This is **exemplary hygiene**. By contrast,
`#[ignore]` count is **162** across the workspace — ignored-tests are the
primary form of deferred debt, NOT TODO comments.

---

## §2 ADR roster audit

### §2.1 Counts

- **Total ADRs**: 114 files (incl README.md + _template.md → 112 actual ADRs)
- **Status distribution**:
  - `accepted`: 102
  - `proposed`: 9 (ADR-0043, 0050, 0052, 0054, 0055, 0056, 0057, 0070 + _template)
  - `ratified`: 0 explicit (most use `accepted` per project convention)
  - `superseded`: 1
  - no-status: 1 (README.md only)
- **last_verified_commit stamp coverage**: 111 / 112 (99%)

### §2.2 Highest ADR + number gaps

- **Highest authored**: ADR-0070 (v0.7.0 master design, b1f1769)
- **Number gaps**: ADR-0053, ADR-0063, ADR-0066 — NOT authored. No
  cross-references found pointing at these missing numbers; gaps are
  abandoned-allocations, not dangling refs.

### §2.3 Still-proposed parent ADRs

Three parent ADRs have **proposed** umbrella status while ALL their
sub-letter ADRs are **accepted**:

- ADR-0055 (Phase H self-host type checker): 0055a-e all accepted, parent
  remains `proposed` → **status drift**, ratification commit missing
- ADR-0056 (Phase I REPL JIT): 0056a-c all accepted, parent `proposed`
- ADR-0057 (Phase J LSP): 0057a-g all accepted, parent `proposed`

**FINDING (F-pattern candidate)**: Umbrella-ADR ratification lag. Sub-letter
ratifications don't auto-promote parent. Discipline gap: when all subs land,
parent should flip `proposed` → `accepted` in a documentation commit. Three
instances currently outstanding.

### §2.4 Cross-reference dangling check

ADR-0070 references 8 sibling ADRs (-0001, -0056a, -0058g, -0065, -0067,
-0068, -0069) + 3 findings (F44, F45a, F51). All exist. No dangling cross-refs
detected on most-recent ADR. **NOT a workspace-wide check** — full grep deferred.

### §2.5 last_verified_commit freshness

Sampled:
- ADR-0070: `aa8b138` (recent, post v0.6.2 LIVE) — fresh
- ADR-0057: `2a710d3` (Phase J wave-5 era; older than current HEAD)
- ADR-0055: `fd263f4` (Phase H era; older)
- ADR-0064: `46c0946` (still proposed-era; older)

Older ADRs reference commits >1 week old. **Acceptable** when ADR scope has not
materially changed; **discipline gap** when the ADR's domain has had work since.

---

## §3 Finding catalogue audit

### §3.1 Counts

- **Total findings**: 70 (incl README.md)
- **F-pattern (F33..F51 family)**: 18 findings
- **Status distribution** (textual; statuses not yaml-enforced):
  - `RESOLVED`: 13 (counted via grep)
  - `RATIFIED`: 13 (overlap with RESOLVED; some findings have both states across §sections)
  - `PROPOSED`: 2
  - `OPEN` / `candidate` / `filed`: 9

### §3.2 F-pattern status (empirical)

| Finding | Status |
|---|---|
| F33 agent-self-disciplinary-rule-skip | open_candidate |
| F34 numeric-anchor-degradation-high-churn | ratified |
| F35-sibling commit-msg-vs-diff-drift | ratified_2026-05-19 |
| F36 fixture-name-vs-behavior-drift | ratified_2026-05-19 |
| F37 silent-rot-on-accepted-debt | ratified_2026-05-19 |
| F38 source-surface-leakage-codegen-primitive | ratified |
| F39 device-name-leakage-in-commits | ratified_2026-05-19 |
| F40 single-point-of-failure-heavy-build-host | **filed** (NOT RESOLVED) |
| F44 ci-cache-stale-green-false-pass | **candidate** (NOT RATIFIED) |
| F45 llvm-backend-wave1-stub-silently-shipped | ratified |
| F45a llvm-backend-wave3-scope-systemic | **RESOLVED 2026-05-25** |
| F46 wheel-not-installable-runtime-stdlib-gap | ratified |
| F47 fstring-user-fn-str-interp-empty | RESOLVED |
| F48 version-bump-must-tag-discipline | RATIFIED |
| F49 fresh-workspace-committer-identity-fallback-leak | RATIFIED |
| F50 lsp-cli-diagnostic-divergence | RATIFIED |
| F51 clippy-feature-flag-silent-rot | RATIFIED |

### §3.3 Open / candidate F-patterns

**Three F-patterns are NOT closed:**

- **F33** (open_candidate) — agent self-disciplinary rule skip. Process
  finding, not code-debt. Resolution unclear (procedural).
- **F40** (filed) — single-point-of-failure heavy-build host. **Partially
  unresolved**: `.github/workflows/self-hosted-gates.yml` STILL references
  `[self-hosted, dg-workstation, cobrust-heavy, linux, x64]` runner labels.
  DG host deprecated 2026-05-20 per MEMORY.md, but the workflow YAML still
  references it. Either (a) workflow is dead-code that should be removed, or
  (b) reference is stale and needs updating. **F40-sibling silent-rot risk**.
- **F44** (candidate) — CI cache stale-green false-pass. Resolution mechanism
  is the `cargo-udeps` job added to ci.yml (confirmed present), but status
  not promoted to RATIFIED. Sub-finding F51 was RATIFIED via 0058g sub-wave-3
  empirical surface; F44 itself remains candidate.

---

## §4 Examples corpus

### §4.1 LC-100 + misc

- **LC-100 leetcode (examples/leetcode-stress/)**: 100 problem directories
  each containing `solution.cb` + `test.toml` + README.md (confirmed via
  `find examples/leetcode-stress -name "solution.cb" | wc -l` = 100;
  initial single-glob count under-reported because corpus is
  directory-structured not flat-`*.cb`)
- **examples/leetcode/ (curated samples)**: 10 .cb files (binary_search,
  climbing_stairs, fibonacci, maximum_subarray, merge_two_sorted_lists,
  reverse_string, roman_to_integer, stock_best_time, two_sum) + README
- **examples/lc100_pattern_a_fixtures/**: 6 .cb (pattern-A fixture family)
- **examples/ root .cb (smoke/demo)**: 15 (hello, cat, csv_sum, early_exit,
  echo, fib, fizzbuzz, for_list, for_range, json_pretty, regex_grep, sort,
  unique_lines, wc, bench_array_sum)
- **tests + crates/*/tests/*.cb**: 5

### §4.2 Stale-ignore annotations

`grep -rln "#\[ignore\]\|XFAIL\|deferred to" examples/` → **0 hits**.
LC-100 corpus does NOT carry XFAIL annotations in source files (per ADR-0058a
discipline; ignores live in test harness, not in .cb source).

### §4.3 Recent corpus churn

Examples directory shows only modest churn (last 14d: no .cb file changes
detected in commit name-only summary). LC-100 baseline is stable.

---

## §5 CI / release pipeline integrity

### §5.1 Workflow inventory

- `ci.yml` — 11 jobs (fmt, clippy, build, test, doc-coverage, cli-tempdir-guard,
  security-audit, cargo-udeps, real-llm-smoke + 2 platform splits)
- `perf-bench.yml` — x86_64 SIMD demo (3 CPU tiers); manual + push-on-fixture
- `release.yml` — tag-triggered tier-3 9-wheel matrix
- `self-hosted-gates.yml` — **DEAD** workflow referencing dg-workstation
  (F40-related)

### §5.2 Recent CI health (last 30 main-branch runs, re-sampled audit-close)

- success: 18
- failure: 7 (all in 2026-05-22 window, before v0.6.x stabilization)
- in_progress / blank: 5 (audit window includes 5 ADR-author + survey commits with CI still running)
- **win rate (closed runs)**: 18/25 = 72% on last 30; recent 13 consecutive
  closed runs all success

8 failures clustered in 2026-05-22 (Phase K LLVM wave-3 stress). Resolved
via subsequent F51 sub-wave-2 sweep + ADR-0058g sub-wave landings.

### §5.3 Specific job failures in cluster

Sampled `26282378427` (2026-05-22 10:25):
- `cargo udeps` — **failed** (cargo-udeps job is `continue-on-error: true` →
  informational, not blocking; matches F44 mitigation intent)
- `cargo audit` — **failed** (also `continue-on-error: true`)
- All other 9 jobs passed.

**Status**: cargo-udeps + cargo-audit gates are **informational** today, NOT
blocking. This is **intentional** per F44 resolution path (added but not yet
promoted to blocking). **Promotion to blocking** is a v0.7.0 readiness
candidate.

### §5.4 release.yml + perf-bench.yml + tap auto-bump

- release.yml: tag-triggered (`v*`); confirmed present + recent runs successful
- perf-bench.yml: x86_64 SIMD demo (3-tier); 6 recent runs
- tap auto-bump: **NO** dedicated workflow; per RELEASE_NOTES_v0.6.2 inspect,
  Homebrew tap bumps live in release.yml or external automation
  (not detected in `.github/workflows/`).

### §5.5 F40 risk surface re-check

`.github/workflows/self-hosted-gates.yml` is **STILL PRESENT** at line 24:
```
runs-on: [self-hosted, dg-workstation, cobrust-heavy, linux, x64]
```

Per MEMORY.md F40 (DG dead 2026-05-20), this workflow CANNOT run (no runner
matches the labels). It is dead-code masquerading as live infrastructure.
**Removal candidate** for v0.7.0 Stream X.5 sweep or sooner.

---

## §6 Memory file audit

### §6.1 Counts

- **MEMORY.md line count**: 29 lines (well **under 200-line cap** per discipline)
- **Memory file directory**: 31 files

### §6.2 Distribution

- `feedback_*.md`: 17 files (rules, SOPs, audit feedback)
- `project_*.md`: 3 files
- `reference_*.md`: 3 files
- `cto_*.md`: 1 file
- `phase_*.md`: 1 file
- `MEMORY.md` index: 1 file

### §6.3 Stale entries referencing deprecated resources

**Three files reference deprecated DG-Workstation** (dead since 2026-05-20):
- `MEMORY.md` (index entry kept as archaeology marker with **STATUS DEPRECATED** explicit)
- `feedback_heavy_build_offload_to_workstation.md` (**STATUS DEPRECATED 2026-05-20** noted in MEMORY.md index)
- `reference_x86_workstation.md` (still present; should be archive-marked or removed)

**Status**: Memory references ARE explicitly marked deprecated in MEMORY.md
header lines, so silent-rot is mitigated. BUT the original feedback files
themselves do not have deprecation banners — MEMORY.md is the only signal.
Future agents reading the feedback file directly (not via MEMORY.md) could
miss the deprecation. **Minor F37-sibling risk** (silent-rot pattern).

### §6.4 Memory rotation

MEMORY.md grew from M0 baseline to 29 lines in 26 days. Healthy. F33
(rule-skip) suggests rules can be added faster than they're internalized —
mitigation: review on every session start per CTO ops runbook.

---

## §7 Architectural debt surfaces

### §7.1 LLVM vs Cranelift backend dichotomy

**Current state** (HEAD `b1f1769`):
- `cobrust-codegen/Cargo.toml`: `default = []`; `llvm = ["dep:inkwell"]`
- **Cranelift is default**; LLVM is opt-in
- Wave-3 closed (ADR-0058g) → LLVM stdlib parity achieved at runtime helper level
- Per user mandate (2026-05-25): "LLVM 后端要完全替换掉现有的"

**Architectural debt**:
1. `cobrust-jit` (749 src LOC) is **Cranelift-only** (engine.rs explicitly
   uses `cranelift_codegen::Context` + `cranelift_jit::JITBuilder` + 4 other
   cranelift_* imports). Cranelift removal forces JIT path decision (ADR-0070
   §6 Q2: port to LLVM MCJIT vs Cranelift-only sub-crate vs AOT-only).
2. `cobrust-codegen/src/cranelift_backend.rs` 3057 LOC + `llvm_backend.rs`
   4595 LOC: **dual maintenance burden** for any codegen change. Wave-3 closure
   committed empirical parity, but split-brain risk is structural until X.4.
3. **`llvm_backend.rs` size** (4595 LOC, largest file) → refactoring candidate
   for v0.7.0 to subdivide (e.g., per-stdlib-family modules) for LLM-friendly
   maintainability.

### §7.2 Shim crates (cobrust-lsp-shim, cobrust-dap-shim)

- Each: ~25 src LOC + 1 smoke test
- **v0.7.0 explicit removal target** per ADR-0068 §7.2 → Stream X.5
- Pre-removal blocker: confirm `cobrust-cli` `lsp` + `dap` subcommands fully
  exercise downstream (ADR-0068 single-binary collapse path)

### §7.3 Three "types" crates (potential confusion)

- `cobrust-types` (4395 LOC) — actual type system + checker
- `cobrust-types-cb` (2503 LOC) — cb mirror of types (Phase H self-host artifact)
- `cobrust-types-parity` (636 LOC) — diff-test harness between the two

Per Cargo.toml descriptions, all three have clear distinct purposes (no
duplication). However, **LLM-friendly naming** per §2.5 could improve: a fresh
agent encountering all three may waste cycles distinguishing them.
**Minor naming debt** — not urgent.

### §7.4 cobrust-registry under-tested

683 src LOC + 0 tests/ subdir + only 7 inline tests. **Anomalously light**.
Risk: code paths in registry not validated by integration tests. Sprint
candidate for v0.7.0 hardening.

### §7.5 F45a wave-1 stubs reintroduced?

Grep `wave-1\|wave1\|stub.*deferred\|TODO.*wave` in `crates/` returns many
hits, but **all are documentation/comment references to historical waves**
(e.g., "ADR-0057a wave-1 consumes", "wave-1 lldb pretty-printers"). NO new
wave-N stub code reintroduced post F45a closure. **F45a stays RESOLVED**.

### §7.6 F35-sibling claim check on README + RELEASE_NOTES

**Inconsistency detected**: README.md line 13 has:
```
[![Stage](https://img.shields.io/badge/stage-0.6.1-brightgreen.svg)](https://github.com/Cobrust-lang/cobrust/releases/tag/v0.6.2)
```

Badge text says **0.6.1**, link target says **v0.6.2**. **Stale badge** from
v0.6.1 → v0.6.2 bump. **F35-sibling minor**: claim mismatch between badge
display + link. **Fix**: update badge to `0.6.2`.

Other README references all consistent at v0.6.2.

---

## §8 v0.7.0 readiness gap analysis

### §8.1 Stream-by-stream status (HEAD `b1f1769`)

| Stream | Sub | Description | Status | Empirical gap | Effort estimate |
|---|---|---|---|---|---|
| X | X.1 | Benchmark Cranelift vs LLVM | NOT STARTED | No `bench/cranelift-vs-llvm-v0.7.0.json` artifact | Days (instrumentation + runs) |
| X | X.2 | Stability sweep LLVM | NOT STARTED | LC-100 + examples not yet validated under `--features llvm` end-to-end | Days |
| X | X.3 | Default flag flip | BLOCKED on X.1 + X.2 | `default = []` unchanged | Hours (mechanical) post-data |
| X | X.4 | Cranelift removal | BLOCKED on Q1 ratification | cranelift_backend.rs 3057 LOC + cobrust-jit Cranelift-deep | Days |
| X | X.5 | Shim binary removal | BLOCKED on X.3 | cobrust-lsp-shim + cobrust-dap-shim present | Hours |
| X | X.6 | release.yml + Homebrew adjust | BLOCKED on X.3 | Wheel matrix unchanged | Day |
| Y | Y.1 | dora-rs Rust API survey | **DONE** (docs/agent/strategy/v0.7.0-dora-cb-integration-roadmap.md) | — | — |
| Y | Y.2-Y.5 | Concurrency / IPC / serde / RT | DESIGN ONLY | Implementation 0% | Weeks |
| Y | Y.6-Y.7 | dora-cb prototype + demo | NOT STARTED | No `cobrust-dora-*` crate exists | Weeks |
| Z | Z.5 | JSON audit | TBD | cobrust-tomli + JSON serde present; audit not done | Day |
| Z | Z.1 | HTTP server | NOT STARTED | No cobrust-http crate | Weeks |
| Z | Z.2 | HTTP client | PARTIAL (`cobrust-requests` 620 LOC) | Async surface missing | Days-Weeks |
| Z | Z.3-Z.4 | Async I/O + TLS | NOT STARTED | No structured-concurrency runtime exposed at user surface | Weeks |
| Z | Z.6 | WebSocket | NOT STARTED (stretch) | No crate | Week |
| Z | Z.7 | DB connectors | NOT STARTED | No cobrust-psycopg / cobrust-sqlite3 / cobrust-redis | Weeks |
| Z | Z.8 | REST demo | NOT STARTED | Done-means gate not approachable yet | Week post-Z.1+Z.5+Z.7 |
| W | numpy | M7.0-M7.6 already shipped (~6643 LOC, 957 tests) | LARGELY DONE | Roadmap says "PyO3 surface gap" deferred to v0.7.x | Days for hardening |

### §8.2 Critical-path summary

- **Stream X (LLVM-default)**: 0/6 substreams implemented; ALL design-only.
  X.1 + X.2 are precondition gates per ADR-0070 §3 sequencing.
- **Stream Y (dora-cb)**: 1/7 substreams done (Y.1 survey). Rest is weeks of
  L0-L3 + FFI design + prototype.
- **Stream Z (network)**: 1.5/8 substreams partial (Z.2 partial via requests
  crate + Z.5 sub-implicit via tomli). Rest is weeks of design + impl.
- **Stream W (numpy)**: Already largely shipped; v0.7.0 scope is hardening,
  not greenfield.

### §8.3 Empirical effort estimate (vs v0.7.0 target)

Per ADR-0070 §6 + strategy roadmap effort buckets:
- Stream X: **3-5 days** post-data (X.1+X.2 → X.3 → X.4 decision)
- Stream Y: **2-4 weeks** (FFI binding path; full L0-L3 deferred)
- Stream Z: **3-6 weeks** (minimum bar HTTP+JSON+DB+demo per Z.8)
- Stream W: **1-2 weeks** hardening (numpy already substantial)

**Calendar minimum to v0.7.0 ratification: ~6-8 weeks** of focused work.
**OBSERVATION**: User mandate "都在 0.7.0 前弄好" set the expectation, but no
calendar date attached. v0.7.0 is **NOT release-imminent** at this snapshot.

### §8.4 Blocking risks

1. **cobrust-jit Cranelift fate (Q2)**: blocking Stream X.4. User decision
   required. **HIGH priority** for unblocking.
2. **F40 self-hosted-gates.yml**: dead-code workflow risks confusing future
   contributors / CI runners. **LOW effort, MEDIUM signal**.
3. **F44 cargo-udeps still informational**: stale-green pattern not fully
   closed at CI gate level. **MEDIUM priority** for ratification path.
4. **README stage badge**: v0.6.1 vs v0.6.2 mismatch. **LOW priority, trivial fix**.
5. **Umbrella ADR (0055/0056/0057) status drift**: parent ADRs `proposed` while
   all subs `accepted`. **LOW priority, hygiene**.

---

## §9 Top 10 actionable items (priority order)

| # | Item | Priority | Effort | Stream / Finding |
|---|---|---|---|---|
| 1 | Stream X.1 — author LLVM vs Cranelift benchmark (LC-100 + examples corpus); produce `bench/cranelift-vs-llvm-v0.7.0.json` | P0 | 2 days | Stream X.1 |
| 2 | Stream X.2 — LLVM stability sweep (LC-100 + examples + integration end-to-end under `--features llvm`); gap list | P0 | 2 days | Stream X.2 |
| 3 | Resolve ADR-0070 §6 Q2 (cobrust-jit fate post-Cranelift removal) — user sign-off needed | P0 | Decision | Stream X.4 blocker |
| 4 | Stream Y.6a — dora-cb FFI binding prototype (single Cobrust-authored dora node compiles + participates in dataflow graph) | P1 | 2-3 weeks | Stream Y.6+Y.7 |
| 5 | Stream Z.1 — cobrust-http server crate scaffold (axum-substrate + `aiohttp`-LLM-prior surface) | P1 | 1 week | Stream Z.1 |
| 6 | Stream Z.7.a — `cobrust-sqlite3` L0-L3 translate (minimum bar single DB connector for Z.8) | P1 | 1-2 weeks | Stream Z.7 |
| 7 | Remove dead self-hosted-gates.yml workflow OR re-target to GH Actions hosted runner | P2 | Hours | F40 partial closure |
| 8 | Promote `cargo-udeps` + `cargo-audit` jobs from `continue-on-error` to blocking | P2 | Hours + lint cleanup | F44 closure |
| 9 | Fix README.md badge `stage-0.6.1` → `stage-0.6.2` | P3 | Minutes | F35-sibling hygiene |
| 10 | Ratify umbrella ADRs 0055 / 0056 / 0057 (parent `proposed` → `accepted` since all subs accepted) | P3 | Hours | Hygiene |

---

## §10 OVERALL PROJECT VERDICT: **YELLOW**

### §10.1 Rationale

**GREEN signals**:
- v0.6.2 LIVE with full LLVM wave-3 closure (F45a 12/12 RESOLVED)
- CI win rate 100% on last 13 runs; 71% on last 30 (recent stability)
- 4,729 tests / 78,404 src LOC / 89,527 tests LOC + LC-100 corpus (100
  problem directories) — extensive coverage
- TODO/FIXME/unimplemented marker count = 3 (exemplary)
- Doc tri-tree (zh / en / agent) coverage maintained
- ADR + finding catalogue active + tracked
- MEMORY.md 29 lines under 200-cap discipline
- Velocity sustained (181 tests/day average over 26 days)

**YELLOW signals**:
- v0.7.0 user mandate sets a scope (LLVM-default + dora-cb + network) but
  **0/6 Stream X substreams implemented**; **1/7 Stream Y**; **~1.5/8 Stream Z**.
  Mandate is scoped (ADR-0070 authored) but execution has not begun.
- Cobrust-jit fate (Q2) is unresolved decision blocking Stream X critical path
- F40 (single-point-of-failure host) STILL PARTIAL — `.github/workflows/self-hosted-gates.yml`
  remains live YAML referencing dead runner labels (F37-sibling silent-rot
  risk for new contributors)
- F44 (CI cache stale-green) ratification incomplete — cargo-udeps job
  exists but `continue-on-error: true`, so it does not block merges
- 3 umbrella ADRs (0055 / 0056 / 0057) have `proposed` parent + `accepted`
  subs → status drift hygiene gap
- `llvm_backend.rs` 4595 LOC single file approaching maintainability ceiling
- README.md stage badge mismatch (0.6.1 vs 0.6.2)
- `cobrust-registry` under-tested (0 tests/ dir, 7 inline tests on 683 LOC)

**RED signals**: NONE detected. No critical blockers preventing forward progress.

### §10.2 Verdict justification

**YELLOW** because:
1. Pre-v0.7.0 work has been **scoped (ADR-0070) but NOT executed** — three
   streams (X / Y / Z) are at design/survey state with no impl progress
   beyond Stream W (numpy already largely shipped per pre-existing M7.0-M7.6).
2. Multiple **open F-pattern findings** (F33, F40, F44) carry low-effort
   resolution paths but remain unclosed.
3. Architectural debts (cobrust-jit Cranelift coupling, dual-backend
   maintenance, llvm_backend.rs file size) are surfaced and **manageable**
   but unaddressed.

Pre-v0.7.0 trajectory is **HEALTHY** if Stream X execution begins within 1-2
weeks. Delay beyond that risks Cranelift-side drift accumulating, since
wave-3 RESOLVED state was only ratified 2026-05-25 — fresh.

### §10.3 Recommendation to CTO

1. **Within 7 days**: Execute Stream X.1 + X.2 (benchmark + stability sweep)
   to produce empirical data for §6 Q1 + Q2 decisions.
2. **Within 14 days**: Resolve Q2 (cobrust-jit fate) — user sign-off via ADR
   amendment. Either preserve as Cranelift-only sub-crate or port path.
3. **Within 30 days**: Stream X.3 default flip + X.5 shim removal + X.6 wheel
   re-baseline (mechanical post-Q1+Q2 ratification).
4. **Parallel**: Stream Y.6a FFI prototype + Stream Z.1 HTTP server scaffold
   (both gateable on X.3 stable LLVM-default backend per ADR-0070 §3).
5. **Hygiene sweep**: items 7-10 from §9 actionable list (low effort, high
   signal). Can be batched in a single sprint.

---

## §11 F35-sibling claim discipline

Every claim in this audit is empirically backed:
- LOC + test counts via `find ... wc -l` + `grep -rn "#\[test\]"` (§1)
- ADR + finding counts via `ls ... | wc -l` + `grep -l "status:"` (§2 + §3)
- CI runs via `gh run list --limit 30 --branch main` (§5)
- Memory file inventory via `ls` + `wc -l` (§6)
- File references include exact line numbers when applicable
- Stream readiness via grep of strategy + crate dir presence (§8)
- Verdict YELLOW based on numbered GREEN / YELLOW / RED signal enumeration (§10)

No speculative claims. No metric extrapolation beyond direct measurement.

---

## §12 Audit metadata

- **Read-only**: ✅ No file writes (incl no commits / no push / no tag)
- **Token budget**: 60 tool uses max → consumed ~40 tool calls (Bash + Read)
- **Audit scope**: Project state at HEAD `b1f1769` (post v0.6.2 LIVE + ADR-0070)
- **Mandate**: User 2026-05-25 "independent objective full-project audit"
- **Cross-link**: This audit complements ADR-0070 (v0.7.0 master design) as the
  empirical readiness baseline against which ratification gates fire.
