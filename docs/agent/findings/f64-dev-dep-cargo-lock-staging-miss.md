---
finding_id: F64
title: Dev-dep Cargo.lock staging miss → CI --locked fail-cascade
status: candidate
date: 2026-05-28
discovered_during: "#151 RAII tempdir refactor (1b05ae3) post-push CI on a6ee367"
related: [F44, F49]
---

# F64 — Dev-dep `Cargo.lock` staging miss → CI `--locked` fail-cascade

## What happened

Run [`26578585966`](https://github.com/Cobrust-lang/cobrust/actions/runs/26578585966)
on `a6ee367` failed across **all 6 substantive jobs** within seconds:

```
cargo clippy -D warnings (macos-latest): failure
cargo clippy -D warnings (ubuntu-latest): failure
cargo build (macos-latest): failure
cargo build (ubuntu-latest): failure
cargo test (macos-latest): failure
cargo test (ubuntu-latest): failure
cargo fmt --check: success
scripts/doc-coverage.sh: success
scripts/cli-tempdir-guard.sh: success
cargo udeps (unused deps): success
cargo audit (advisory): success
```

The 6-failure cluster looked alarming — "what huge regression did the 5-commit
stack land?" — but the ubuntu-build log's tail was the giveaway:

```
help: to generate the lock file without accessing the network,
      remove the --locked flag and use --offline instead.
##[error]Process completed with exit code 101.
```

Lockfile mismatch. Same error mode → 6 lookalike failures because every
`cargo {build,clippy,test} --locked` precheck aborts at lock-resolve before any
compilation begins. **Six failures, one root cause.**

## Root cause

Commit `1b05ae3` ("refactor(tests): RAII tempdir for codegen/cli integration
tests (F63, closes #151)") added `tempfile` as a `[dev-dependencies]` entry in
`crates/cobrust-codegen/Cargo.toml`:

```toml
# crates/cobrust-codegen/Cargo.toml
[dev-dependencies]
tempfile = { workspace = true }
```

Locally `cargo test` and `cargo build` silently regenerated `Cargo.lock` to
add the matching entry:

```
[[package]]
name = "cobrust-codegen"
dependencies = [..., "target-lexicon", "tempfile", "thiserror 1.0.69"]
```

That regenerated `Cargo.lock` line was **never `git add`-ed** in `1b05ae3`. The
commit shipped with `Cargo.toml` and the 27 refactored test files, but not the
1-line `Cargo.lock` companion. CI's `--locked` flag correctly refused to
auto-regenerate and aborted with exit 101.

## Why local-PASS / CI-FAIL diverged

| layer | local invocation | CI invocation |
|-------|------------------|---------------|
| build | `cargo build --workspace` (no `--locked`) | `cargo build --workspace --all-targets --locked` |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` (no `--locked`) | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| test | `cargo test -p cobrust-codegen --test ...` (no `--locked`) | `cargo test --workspace --locked` |

Local cargo silently mutated `Cargo.lock` on first run; the commit author
inspected the post-build tree but did not `git status` or `git diff Cargo.lock`
before committing. The "all tests pass" signal at the dev console is genuine —
but useless as a CI-readiness proxy when the lockfile drift is hidden.

## Sibling findings

- **F49** — Pre-flight identity-check pattern. Both F49 and F64 are
  "verifiable-via-one-command checks the dispatcher forgot to ask the agent
  to run before commit." F49 was `git config user.email` + sanity-grep on
  commit msg; F64 is `git status -- Cargo.lock` after `cargo build`.

- **F44** — CI cache stale-green false-pass. Inverse polarity from F64: F44
  was "CI cache hit hid a real lint regression that local cold-build would
  have caught"; F64 is "CI cold `--locked` caught a real lockfile drift that
  local warm-cache hid." Both share the moral: **the dev box and CI run
  different commands; align them or be prepared to be surprised.**

- **F35-sibling** — DEV-agent commit-msg vs diff drift. F64's sibling on
  staging discipline: the agent did a job (#151 RAII refactor) and produced
  a commit, but the commit's **file set** was incomplete relative to the
  job's **dependency footprint**. Just as F35-sibling demanded "commit
  message mirrors the actual diff," F64 demands "commit set mirrors the
  actual dependency manifest mutation."

## Remediation (this PR, commit `73aa3bb`)

One-line fix to the staging miss:

```diff
 # Cargo.lock (line 749)
   "cobrust-hir",
   "cobrust-mir",
   "target-lexicon",
+  "tempfile",
   "thiserror 1.0.69",
```

That's it. No source changes, no version-resolution churn, no transitive
drift.

## Long-term fix — agent dispatch template addition

Every Agent dispatch prompt that *might* touch dependencies (`Cargo.toml`,
`workspace.dependencies`, or any crate's `[dependencies]` / `[dev-dependencies]`
/ `[build-dependencies]`) MUST include this pre-commit check:

```
## Cargo.lock staging discipline
Before EVERY `git commit`:
  git status -- Cargo.lock                  # must show clean OR staged
  git diff --cached -- Cargo.lock | head    # if lock changed, must be staged
If `Cargo.lock` shows in `git status` as unstaged ("M  Cargo.lock" left side
blank, right side M), STOP — `git add Cargo.lock` before commit.

This is non-negotiable. CI runs `cargo {build,clippy,test} --locked` which
will reject ANY lockfile drift, including the silent ones cargo auto-applies
on the dev box.
```

Add to:
- `~/.claude/projects/.../memory/cto_operations_runbook.md` — "Pre-commit
  pre-flight" section.
- Future "small-task" agent dispatch templates (post-F49 family).
- Project-wide `docs/agent/agent-dispatch-pre-commit-checklist.md` (NEW,
  if F64 ratifies).

## Cost analysis

- Failed CI minutes: ~3 min of substantive jobs aborting at lock-resolve, plus
  the cache POSTs ran briefly before the failed-job cancellation propagated.
  Cheap compared to the ADR-0023 §A3 size-bench fixtures that would have
  burned ~25 min of compute, since the abort happened pre-build.
- Author time-cost to diagnose: ~3 min (CI log read → root cause → 1-line
  remediation).
- Bystander cost: 2 in-flight Agent dispatches (RV Sprint A + ADR-0074
  impl) on a slightly-stale `Cargo.lock` baseline — but since their workflow
  is local `cargo build` (no `--locked`), they'll auto-regenerate when they
  reach build-and-test, including this fix.

Total cost: low. Lesson is the value.

## Promotion path

`candidate` → `ratified` after:
1. The dispatch-template addition lands (CTO ops runbook update +
   project-wide checklist if appropriate).
2. One follow-up agent dispatch that adds a dep ships with `Cargo.lock`
   correctly staged, proving the discipline is reproducible from the
   updated template.
3. Mention in `docs/agent/findings/INDEX.md` once F64 ratified.

## Lineage hook

F64 belongs to the post-F49 family of "easily verified pre-flight checks the
dispatcher must demand explicitly." Pattern: agents reliably do the
substantive work, but skip the boring pre-commit verification step that
catches the boring class of failures. The fix is always "make the verification
step a non-skippable line in the dispatch template" — never "tell the agent to
be more careful."
