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

**Candidate.**  
Promote to **ratified** once:
- `cargo-udeps` CI gate lands (this commit), and
- next Phase closure runs clean-target clippy sweep with zero errors.
