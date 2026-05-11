---
doc_kind: adr
adr_id: 0042
title: snapshot-lint enforcement — close F1.1 for snapshot schema
status: accepted
date: 2026-05-11
last_verified_commit: 4186c8e
supersedes: []
superseded_by: []
---

# ADR-0042: snapshot-lint enforcement — close F1.1 for snapshot schema

## Context

The `project_state_snapshot.md` schema carries a declared `schema_invariant`
block (frontmatter lines 6-11) that was added 5+ turns ago:

```yaml
schema_invariant: |
  Every ADR mentioned in any section must appear in §"ADR roster" table.
  Every finding mentioned in any section must appear in §"Findings ledger".
  Binary verification list appears EXACTLY ONCE under §"main branch state".
  HEAD SHA in §"main branch state" must equal `git log -1 --format=%h main` at write time.
```

This invariant has been violated in every subsequent CTO turn — 5 confirmed
incidents matching the **F1.1** pattern (ADSD `failure-modes-catalogue.md`):
"declared invariant without verification." A declared invariant that is
never machine-enforced is security theater (review-claude Methodology §10
anti-pattern #4).

**F1.1 incident history (5 confirmed)**:

1. `project_state_snapshot.md` HEAD drifted to `18a4bbc` while real HEAD
   advanced 22 commits to `c40623e` (v0.1.0 stable).
2. ADR roster ended at 0039 while 0040 and 0041 had landed on main.
3. Phase F milestones listed Wave 1 only; Wave 2 + v0.1.0 stable not recorded.
4. Test count read 2,545 while actual workspace count was 2,611.
5. `m10-sha-pin-hallucination` finding listed in snapshot frontmatter but
   the file had not yet been written (sonnet sprint in flight).

The M10 SHA-pin hallucination finding (`docs/agent/findings/m10-sha-pin-hallucination.md`)
also demonstrates that **the same F1.1 pattern fires beyond the snapshot**:
any external version reference (commit SHA, tag, package version) without
a verification command in the prompt can silently produce a fabricated
artifact. That case (action SHA verification in CI) is distinct scope and
is not the primary target of this ADR; it is recorded as future work
in §"Consequences."

**Constraint**: `project_state_snapshot.md` lives in the CTO's user memory
directory (`~/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/`),
outside the git repository. GitHub Actions CI runners cannot reach this
path. A pure CI enforcement is therefore impossible without either:
(a) copying the snapshot into the repo (changes CTO workflow), or
(b) running the check from the CTO's local machine before merges land.

Option (b) is the only path that preserves the existing memory-file
workflow and genuinely prevents violations. The enforcement vehicle is a
pre-commit hook that the CTO opts into via `git config core.hooksPath`.

## Options considered

### Option A — status quo + manual review

Continue relying on CTO discipline to update the snapshot at each turn.
No automation added.

- Pro: zero friction.
- Con: F1.1 has already fired 5 times in <50 turns. Pattern is load-bearing.
  Manual review is evidentially insufficient.

### Option B — CI lint against a repo-copy snapshot

Copy `project_state_snapshot.md` into the repo (e.g. `docs/agent/snapshot.md`),
add a CI job `snapshot-lint` that enforces Invariants 2-4. Inv 1 (HEAD
freshness) is trivially satisfied by CI because the repo copy is updated
before merge.

- Pro: CI enforces without developer-side setup.
- Con: the snapshot serves as CTO memory between compaction events; moving
  it into the repo changes the write surface and adds a two-location sync
  problem. CTO must now update both the memory file and the repo copy.
  Complexity increases, new F1.1 surface introduced.

### Option C — CI lint + pre-commit hook on developer machine (selected)

Ship `scripts/snapshot-lint.sh` in the repo. The script runs locally
against the memory-dir snapshot. A `--ci-mode` flag skips Inv 1 (HEAD
freshness, which CI cannot check) so the script can run in CI against a
provided snapshot path without failing on a missing file.

Provide `.githooks/pre-commit-snapshot-lint` and document the install
incantation in this ADR. The CTO opts in once; violations thereafter
require an explicit `--no-verify` bypass (visible in git history).

- Pro: F1.1 has a machine-enforced path. Future violations require
  active bypass (audit trail).
- Con: only enforced on machines where `git config core.hooksPath .githooks`
  has been run. CI alone cannot enforce Inv 1. This is the fundamental
  limit imposed by the memory-dir location.

## Decision

Adopt **Option C**.

Ship:

1. `scripts/snapshot-lint.sh` — enforces 4 invariants; `--ci-mode` skips
   Inv 1 for CI use.
2. `.githooks/pre-commit-snapshot-lint` — thin wrapper that calls the
   script without `--ci-mode` (Inv 1 included) before any commit.

Install (CTO's local machine, one-time):

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit-snapshot-lint
```

Bypass (emergency only):

```bash
git commit --no-verify -m "..."
# --no-verify is visible in git history; use only with documented justification
```

**What the script enforces**:

- **Inv 1** (HEAD freshness): snapshot `**HEAD**: \`<sha>\`` field must equal
  `git log -1 --format=%h main`. Violation → F1.1 staleness detected.
- **Inv 2** (ADR roster completeness): every `docs/agent/adr/00*.md` file
  has a row in the snapshot's ADR roster table. Row match: `| 0042 |` or
  `| [0042](...) |`.
- **Inv 3** (findings ledger completeness): every `docs/agent/findings/*.md`
  (excluding README) is mentioned in the snapshot as `` `<basename>` ``
  (backtick-quoted, no `.md` extension).
- **Inv 4** (binary verification list): the line containing
  `cobrust build examples/hello.cb` appears exactly once in the snapshot.

**CI integration (optional, Inv 2-4 only)**:

In `.github/workflows/ci.yml`, add a job that passes the script a
snapshot file if one is present in the repo's `docs/agent/` tree:

```yaml
  snapshot-lint:
    name: snapshot schema invariants (Inv 2-4)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683  # v4.2.2
      - run: |
          SNAP="$HOME/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/project_state_snapshot.md"
          if [ -f "$SNAP" ]; then
            bash scripts/snapshot-lint.sh --ci-mode "$SNAP"
          else
            echo "snapshot-lint: snapshot not present in CI runner — skipping"
          fi
```

This job silently no-ops in the public CI runner (snapshot not present)
but enforces Inv 2-4 if the file happens to be accessible (local docker
run, self-hosted runner with mounted volume). The honest note is that
**only the pre-commit hook path enforces Inv 1**.

## Consequences

- **Positive**
  - F1.1 for snapshot schema has an explicit machine-enforced path.
  - Future violations of Inv 1-4 require active bypass; the bypass is
    visible in git history via `--no-verify`.
  - `scripts/snapshot-lint.sh` is runnable in `--ci-mode` without a
    snapshot present (exit 0 with informational output), so it never
    blocks CI on the public runner.
- **Negative**
  - Enforcement only active on machines where the CTO has run the
    one-time install. Sub-agents and remote CI cannot reach the memory
    dir; they cannot enforce Inv 1.
  - Adding a new hook path (`core.hooksPath`) may interfere with any
    existing local hooks. CTO must audit before installing.
- **Neutral**
  - `scripts/snapshot-lint.sh` is compatible with macOS bash 3.2 (no
    associative arrays, no `[[` extended syntax, no process substitution
    in portable-mode paths). Verified on macOS arm64 dev host.
  - The action-SHA verification gap (M10 hallucination root cause) is a
    distinct F1.1 surface. Future work: add a `scripts/verify-action-pins.sh`
    that queries `gh api repos/<owner>/<repo>/commits/<sha>` for every
    SHA-pinned action in `.github/workflows/*.yml`. This closes the M10
    category permanently; it is out of scope for this ADR.

## Evidence

- Incident history: `project_state_snapshot.md` schema_invariant block
  (frontmatter lines 6-11); 5 consecutive violations before this ADR.
- M10 hallucination root cause: `docs/agent/findings/m10-sha-pin-hallucination.md`
- Fix commit: `4186c8e` (revert hallucinated SHA pins to tag form)
- Hallucination commit: `e937037` (M10 Wave 2 F+G M-batch)
- Handoff doc: `review-claude-handoff/handoff-pack/dispatches/post-v0.1.0-final-handoff.md` §2
- ADR-0019 — Phase E roadmap; M10 was the CI milestone this hallucination corrupted
- ADR-0038 — Phase F roadmap; F1.1 is a named anti-pattern in the methodology layer
- ADSD failure-modes-catalogue: `review-claude-handoff` team's `reference/failure-modes-catalogue.md` F1.1 + F13
