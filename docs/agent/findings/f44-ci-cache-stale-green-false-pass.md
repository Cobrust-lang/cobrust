---
name: f44
status: candidate
family: F37 silent-rot sibling
last_verified_commit: 6d89dd1
date: 2026-05-21
---

# F44 — CI Cache Stale Green / False-Pass

## §1 Pattern

CI green on a SHA does **NOT** guarantee the workspace is lint-clean.

A cache hit on `cargo target/` or an incomplete CI run can yield a false-green.
Stale lint errors silently lurk across many commits while every CI run reports PASS.

## §2 Empirical

19 `cobrust-lsp` clippy errors lurked since `a3a636c` (Phase J wave-2.2
hover+completion) through approximately 30 commits to `e38dfe4`
(v0.4.0-rc1 staged).

All CI runs in that window reported **PASS** while local
`cargo clippy --workspace --all-targets -- -D warnings` returned 19 errors.

## §3 Root Cause

**(a) Cache key does not include lint-tree hash.**  
`rust-cache` keys on commit SHA + `Cargo.lock` hash.  
A push that does not change `Cargo.lock` can hit a pre-existing cache entry;
the clippy step re-uses the cached `.fingerprint/` data and skips recomputation
even if source files changed.

**(b) Intermittent CI run interruption masked original failure.**  
The Ubuntu `test` job 132-minute hang from the `0064` fixture incident may have
left a CI run marked in-progress; a subsequent push triggered a new run whose
matrix started fresh — masking the original failure by orphaning it.

**(c) `--all-targets` not consistently enforced.**  
Some ad-hoc CI invocations omitted `--all-targets`, so `lib.rs` targets compiled
cleanly while `bin/` + integration-test targets accumulated warnings.

## §4 Detection Rule

Weekly (or per-close) CI gate that runs:

```bash
cargo clippy --workspace --all-targets --no-deps -- -D warnings
```

with cache **busted** (rust-cache key appended with a rotating nightly stamp
or `CACHE_BUST` secret).

## §5 Resolution Path

**(a)** `ci.yml` — ensure `--all-targets --no-deps` consistently used in the
`clippy` job (already present at HEAD; confirm `--no-deps` added).

**(b)** Add `cargo-udeps` CI job (Code Quality P1) to catch unused-dep regressions
that also lurk silently — companion gate rolled out in same commit as this finding.

**(c)** F37-style retroactive sweep: on every Phase closure, run
`cargo clippy --workspace --all-targets --no-deps -- -D warnings` from a clean
target dir (delete `.cobrust/target` before sweep) and verify zero errors before
tagging the milestone SHA.

## §6 F-Family

| Sibling | Shared root |
|---------|-------------|
| F37 silent-rot-on-accepted-debt | Errors accumulate invisibly; human/infra-caused respectively |
| F35-sibling commit-msg vs diff drift | Claim (CI PASS) diverges from actual landed state |
| F40 single-point-of-failure heavy-build | Both are infra-reliability failures; DG dead, CI cache stale |

## §7 Status

**Candidate (promotion blocked 2026-05-26).**  
Promote to **ratified** once:
- `cargo-udeps` CI gate lands (this commit), and
- next Phase closure runs clean-target clippy sweep with zero errors.

### §7.1 Blocking-promotion investigation 2026-05-26

Tier-B audit P2 batch fix-2 attempted to promote `cargo-audit` + `cargo-udeps` from
`continue-on-error: true` to blocking. Local baseline check (`cargo audit --deny warnings`
at HEAD `1e76a8f`) revealed:

```
Crate:     pyo3
Version:   0.22.6
Title:     Risk of buffer overflow in `PyString::from_object`
Date:      2025-04-01
ID:        RUSTSEC-2025-0020
Solution:  Upgrade to >=0.24.1
Dependency tree:
pyo3 0.22.6
├── cobrust-requests 0.6.2
├── cobrust-numpy 0.6.2
├── cobrust-msgpack 0.6.2
├── cobrust-dateutil 0.6.2
└── cobrust-click 0.6.2
```

Per F37 honest-debt rule + task-spec "Don't promote if existing issues silent",
promotion to blocking was aborted to avoid silent F37-sibling rot. The pyo3 0.22 →
0.24 upgrade spans 5 PyO3-wrapping crates and is a substantive sprint, not a
mechanical fix. Queued as separate ADR.

`cargo-udeps` baseline-check requires nightly toolchain (not installed locally);
deferred to a paired sprint with the audit upgrade since both follow the same
F37-debt-resolution pattern.

**Action items**:
- ADR-TBD: PyO3 0.22.6 → 0.24.1 upgrade across cobrust-{requests,numpy,msgpack,dateutil,click}
- After ADR-TBD lands clean: re-run baseline checks, promote both jobs blocking, ratify F44.
