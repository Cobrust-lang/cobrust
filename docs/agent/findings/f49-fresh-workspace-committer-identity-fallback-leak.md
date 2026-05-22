---
doc_kind: finding
finding_id: f49
title: Fresh-workspace committer identity fallback leaks OS user + device hostname
status: RATIFIED
family: F39 sibling (privacy class), F45a sibling (audit-scope class)
date: 2026-05-22
discovered_by: user (real-time)
last_verified_commit: 6491614
relates_to:
  - feedback_no_device_names_in_commits
  - f39-device-name-leakage-in-commits
  - f45a-llvm-backend-wave3-scope-systemic
  - cto_operations_runbook
---

## §1 Context

2026-05-22 16:55 local time, while creating new helper repo `Cobrust-lang/cobrust-tmlanguage` (TextMate grammar mirror for the github-linguist PR), CTO ran `git init` in a fresh `/tmp/` workspace WITHOUT setting local `user.name` / `user.email`. Global `~/.gitconfig` was also empty for these fields. Git's silent fallback:

1. `user.name` ← OS account display name (the user's real Chinese name)
2. `user.email` ← `${shell_user}@${hostname}` (here: device hostname containing the user's real name in plaintext)

Result: 3 commits pushed to **public** `Cobrust-lang/cobrust-tmlanguage` with author/committer fields exposing real name + device name. User caught immediately ("吴冰晶吴冰晶 ... 神经病啊,你又把我的名字放上去了").

This is a **double-leak** (name + device) and a **public-repo permanent** leak (any clone or fork preserves history).

## §2 Empirical proof

Before fix:
```
cbc1e0e | author=<real-name> <user@<device-hostname>.local> | committer=<real-name> <user@<device-hostname>.local>
575ef01 | author=<real-name> <user@<device-hostname>.local> | committer=<real-name> <user@<device-hostname>.local>
```

After Option-A-style rewrite (force-push to overwrite history; brand-new repo with no external dependents, so safe):
```
cd2fe04 | author=wbj010101 <wbj010101@gmail.com>
```

## §3 Root cause

The Cobrust workspace at `/Users/hakureirm/codespace/Study/Cobrust/` has **local** `git config user.name = wbj010101` (set during initial project setup), so all 34 commits to the main repo today are clean. But **global** git config is empty, so any *new* repo created elsewhere (including `/tmp/` ad-hoc workspaces, helper-repo bootstrapping, etc.) falls back to the OS-derived identity.

This is a class of leak the existing F39 ("device names in commits") rule did not explicitly anticipate — F39 was scoped to *commit-message and ADR-text content*, not to *commit metadata (author/committer fields)*.

## §4 Detection rules (mandatory; CTO + dispatch templates)

### §4.1 Per-dispatch pre-flight (machine-doable)

Before running `git commit` or `git push` in *any* workspace that is not the canonical Cobrust workspace:

```bash
# Verify local identity is set to a neutral handle that does not leak.
test "$(git config user.name)" = "wbj010101" \
  && test "$(git config user.email)" = "wbj010101@gmail.com" \
  || { echo "FAIL: local git identity not set; refusing to commit"; exit 1; }
```

### §4.2 Dispatch-template requirement

Every dispatch prompt that includes `git init` OR `git clone` of a non-Cobrust-canonical repo MUST include the line:

> "Before any commit, run: `git config user.name wbj010101 && git config user.email wbj010101@gmail.com`"

(Sub-agents inherit no global state; this must be explicit per dispatch.)

### §4.3 Post-author audit extension

The mandatory post-author audit (per `feedback_post_author_audit_mandatory`) MUST extend its scope to include any external repo created or mutated by the sprint. Audit must fetch `git log --pretty='%an <%ae>'` on each external repo and verify all commits use the neutral identity.

The 2026-05-22 retro audit (`aa85ca79a6c4dc469`) scanned only the Cobrust main repo and missed this leak because audit scope did not include `Cobrust-lang/cobrust-tmlanguage`. F49 codifies that audit scope MUST follow the dispatch's actual mutation surface, not the assumed-default surface.

## §5 Global config — set 2026-05-22 (post-incident escalation)

Initial draft of this finding deferred `~/.gitconfig` edits to user-side decision per CLAUDE.md safety protocol "NEVER update the git config". But after a **second leak** the same day (PLDB PR #643 fork branch in `Hakureirm/pldb` — same root cause, same `/tmp/` fresh-workspace fallback to OS user), the user explicitly authorized the permanent fix:

```bash
git config --global user.name wbj010101
git config --global user.email wbj010101@gmail.com
```

Verified post-set: `git init` in a brand-new `/tmp/` workspace inherits `wbj010101 <wbj010101@gmail.com>` for both author and committer. The §4.1 pre-flight check now resolves as no-op success on this machine — defense in depth.

CLAUDE.md's "NEVER update the git config" protocol applies to autonomous mutation without permission. User explicit authorization (this incident) lifts that restriction for the specific change.

## §6 Family

- **F39** (device names in commit messages / ADR text) — sibling, scope = commit message text
- **F49** (this) — sibling, scope = commit metadata (author / committer fields)
- **F45a** (LLVM wave-3 systemic) — sibling, audit-scope-too-narrow class

The three together define the rule: **F-privacy family = "any artifact reaching public surface — text, metadata, or external repo — must use neutral handles"**.

## §7 Status

RATIFIED 2026-05-22 by user real-time catch + immediate rewrite (force-push `cbc1e0e` → `cd2fe04` on `Cobrust-lang/cobrust-tmlanguage`).

## §7a Incident log (2026-05-22)

Three independent fresh-workspace leaks fired the same day before global config was set:

1. **`Cobrust-lang/cobrust-tmlanguage`** (grammar repo for github-linguist PR #7977 follow-up). `/tmp/cobrust-tmlanguage` workspace created by CTO directly. 3 commits leaked. **RESCUED**: force-push `cbc1e0e` → `cd2fe04` (brand-new repo, no external dependents).

2. **`Hakureirm/linguist` fork branch `add-cobrust-language`** (linguist PR #7977). `/tmp/linguist-clone` workspace created by P7 sub-agent via `gh repo fork --clone`. 1 commit leaked at `2974f8e`. **RESCUED JIT**: force-push `2974f8e` → `09d5e36` while PR was still `OPEN` / `mergedAt: null` / `reviewDecision: REVIEW_REQUIRED`. PR auto-syncs from fork; maintainer sees `wbj010101` author.

3. **`Hakureirm/pldb` fork branch `add-cobrust`** (PLDB PR #643 source). `/tmp/pldb-fork` workspace created by P7 sub-agent. Fork branch tip `94fd077` had real-name leak. PR was MERGED at upstream `breck7/pldb` via GitHub **squash-merge** before leak was caught — fortuitously, squash-merge created a NEW commit `bff882a` with `Hakureirm <wbj010101@gmail.com>` as author (GitHub uses the merger's GitHub identity, not the source-commit author). **RECOVERED — effectively clean**: `breck7/pldb` and `Programming-Language-DataBase/pldb` (the canonical PLDB repos) both have clean `main` history; `94fd077` only lingers in GitHub's `refs/pull/643/head` PR-archive ref (invisible in UI / git-log; reachable only by direct SHA lookup). Post-merge cleanup: `Hakureirm/pldb` fork's `add-cobrust` branch deleted to remove last reachable ref from our side.

**Lesson learned (post-incident)**: GitHub **squash-merge** acts as a natural identity-privacy shield. Future fork-PR submissions should prefer squash-merge over fast-forward / rebase-merge for any external repo where local fork commit author identity may differ from desired public attribution.

Pattern of the 2 sub-agent leaks: dispatch prompt **did not** include the §4.2 identity-config requirement (because §4.2 was being authored just-in-time during the same day). Future dispatch templates must inline the config command, AND now that global config is set, defense in depth means the leak no longer fires by default.

## §8 Cross-refs

- ADR-0001 (license) — neutral identity policy precedent
- `feedback_no_device_names_in_commits` (2026-05-19) — F39 ratification
- `feedback_post_author_audit_mandatory` (2026-05-18) — audit-scope discipline
- F39 finding (commit-text family)
- F45a finding (audit-scope family)
