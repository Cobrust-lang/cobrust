---
doc_kind: finding
finding_id: f39-device-name-leakage-in-commits
title: "F39: Device-identifying names leaked into git history + repo files"
status: ratified_2026-05-19
date: 2026-05-19
last_verified_commit: d012df9
discovered_by: P10 CTO emergency audit — pre-publish privacy sweep
severity: P1 (privacy / opsec; identifying info in public repo)
related: [finding:f35-sibling-commit-msg-vs-diff-drift]
cross_refs: [memory:feedback_no_device_names_in_commits, memory:reference_x86_workstation]
sourced_from: 2026-05-19 emergency P9 dispatch — Option A force-rewrite authorization
---

# F39: Device-Identifying Names Leaked into Git History + Repo Files

## Pattern

Sub-agents writing commit messages, ADRs, and module docs frequently
embedded **device-identifying** strings from operator memory references
(hostnames, IPs, ports, GPU model SKUs, OS kernel versions, user logins)
into public artifacts that landed on `main`. Pre-publish, this leaked
operator opsec into a soon-public repo.

## Concrete leak inventory (2026-05-19, pre-rewrite)

- **31 commit messages** across `main` + branches contained one or more
  of: `DG-Workstation-2x3090`, `wubingjing`, `112.74.60.44`, `port 10040`,
  `Linux 6.x kernel`.
- **18 repo files** (workflow + 8 ADRs + 2 architecture pages + 4 test
  files + 1 module page + 1 spike) carried the same strings inline.
- **Workflow filename** `.github/workflows/workstation-gates.yml`
  itself hinted at the host identity tier.

## Why the drift happens

- Memory references (`reference_x86_workstation`, `cto_operations_runbook`)
  legitimately store concrete connection info so the human operator can
  reconnect quickly between sessions.
- Sub-agents reading these memory entries treat the literals as
  **publishable detail** (they "ground" the work) rather than
  **opsec-sensitive material**.
- No pre-write rule existed; CI did not grep commit/diff text for these
  patterns.

## Remediation executed (2026-05-19)

- `git filter-repo --replace-text` + `--replace-message` rewrite across
  all branches mapping device-identifying strings to neutral placeholders
  (`<self-hosted-runner>`, `<runner-user>`, `<runner-ip>`,
  `<runner-port>`, `<gpu-host>`, `linux x86_64 host`).
- 18 leftover worktree branches deleted (local + remote).
- Workflow renamed to `.github/workflows/self-hosted-gates.yml`.
- Force-pushed `main` with rewritten history (solo dev, no external
  consumers, user explicit authorization per CLAUDE.md safety protocol).

## Going-forward rule

When writing commit messages, ADRs, module docs, or any other
publishable artifact, **never** embed:

- Specific hostnames (use `<self-hosted-runner>` or `runner host`).
- Specific user logins (use `<runner-user>` or `the operator account`).
- IP addresses (use `<runner-ip>` or `the runner endpoint`).
- SSH port numbers (use `<runner-port>` or `the SSH port`).
- GPU model SKUs as tier identifiers (use `<gpu-host>` or describe
  capability: "x86_64 GPU host with CUDA").
- OS minor version + kernel version (use `linux x86_64 host`).

Initials-only references (`DG verify`, `on DG`) are acceptable when the
two-letter token does not uniquely identify a public-facing artifact.

## CI follow-up (open)

Add a pre-commit / CI grep gate that fails the build if any of the
banned literals reappear. Tracked in next CI hardening sprint.

## Cross-link

This finding mirrors operator memory `feedback_no_device_names_in_commits`
so a future Claude resuming without that memory entry still has the rule
in-repo.
