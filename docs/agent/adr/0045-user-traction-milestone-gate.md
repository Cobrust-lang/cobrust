---
doc_kind: adr
adr_id: 0045
title: User-traction milestone gate — each release binds to ≥1 user-scenario done-means
status: accepted
date: 2026-05-11
last_verified_commit: 9caef99
supersedes: []
superseded_by: []
---

# ADR-0045: User-traction milestone gate

## Context

Over a 24-hour window 2026-05-10..2026-05-11, three milestone-vs-user-traction drift signals fired in rapid succession:

1. **v0.1.0 stable** (commit `c40623e`, 2026-05-10) — declared shipped with 5-gate local green. CI on the tag was actually **red 13/14** (M10 sub-agent hallucinated GitHub Actions SHA pins). Defect surfaced only when user manually navigated to the public repo and saw "1/14" in the CI badge. **Internal milestone green vs external badge red.**
2. **v0.1.1 hot-fix tag** (commit `769a5d8`, 2026-05-11) — declared shipped with CI 7/7 green. review-claude independently verified the public artifacts and discovered: (a) prebuilt binary URLs in release notes were stale (cobrust-0.1.1-* vs cobrust-v0.1.1-* asset names), (b) `cargo install cobrust-cli` failed because workspace 0.0.1 path-deps weren't on crates.io. **Internal CI green vs external install path 404.**
3. **W2 leetcode wedge sprint** — originated from user remark "刷不了 leetcode, 很多东西都没做完". M0..M14 milestone roadmap + Phase F.1 wedge had all been declared "done" over 9 days, yet the project owner could not use Cobrust to do a single LeetCode problem. **9-day internal milestone delivery vs 0 user-feature reachability.**

The systemic pattern: each milestone passed its declared internal gates (5-gate, P9 sub-agent verdict, ADR done-means checklist), but the deliverable failed at the user-facing surface (install path, binary version embed, source-level capability for a representative user scenario). Sub-agent self-reports + CTO local gates were insufficient to catch the drift. Only third-party (review-claude) independent verification or user direct complaint surfaced the gap.

This ADR adds a **mandatory user-traction gate** to every release-bearing milestone. The gate is not a replacement for the internal 5-gate; it is an **additional binding** on the milestone's "done means" definition.

### Prior art in the project

- ADR-0019 §"Definition of usable" four-tier anchor (Literal / Spirit / Mechanism / Production-validated / Full) addresses *what the language can do internally*. ADR-0045 is the **complementary anchor for what an external user can do**.
- `cto_operations_runbook.md` §"Release-readiness agent" (added 2026-05-11 mid-Option-C) is the **executable** form of this ADR's gate. ADR-0045 codifies the policy that runbook executes.
- ADSD `failure-modes-catalogue.md` F19 candidate ("public-facing onboarding text written but never independently install-tested") is the same failure pattern at the per-incident level; ADR-0045 names the systemic prevention.

## Options considered

### Option A — Status quo: no user-traction gate

- Pros: zero process overhead.
- Cons: empirically demonstrated insufficient. Three drift incidents in 24h. Sub-agent self-report has a structural conflict-of-interest (the same context that wrote the deliverable also writes the "done" claim).

**Rejected.**

### Option B — User-scenario binding + release-readiness verify per release (CHOSEN)

Every git tag prefixed `v0.X` or `v1.X` MUST bind to at least one external-user-scenario "done means" that a non-Cobrust-contributor can execute end-to-end from a clean environment.

The bound user-scenario MUST:
- Be reproducible by a fresh shell with only public artifacts (no in-repo working tree assumed).
- Cite exact commands from the public release notes and/or README quickstart, verbatim.
- Have a deterministic expected output (exit code + stdout/stderr match).
- Be executed by an **independent verifier** (not the agent that authored the milestone): either review-claude third-party audit, the release-readiness P7 sonnet sprint per runbook, or the project owner.

The release tag MUST NOT be considered shipped until the bound user-scenario returns GO from an independent verifier. Internal 5-gate green is necessary but not sufficient.

- Pros: empirically demonstrated to catch real drift. The §A.3 BLOCK → §A.4 fix → §A.5 GO cycle is the canonical example: §A.3 release-readiness P7 sonnet (independent verifier) caught the prebuilt-binary version-string mismatch that internal 5-gate did not.
- Cons: ~20-30 min release-readiness verify per release. Acceptable given that catching one F19-class defect at the release tag saves the equivalent of an entire re-release cycle (v0.1.1 → v0.1.2 took ~3 hours of compound sprint work).

**Chosen.**

### Option C — Stricter: per-feature acceptance test against a real external user before merge

- Pros: catches drift even earlier (at feature merge, not release tag).
- Cons: requires having an actual external user committed to per-merge testing, which the project doesn't have yet. Premature for a pre-1.0 project with project-owner-as-first-user.

**Deferred to post-v1.0 ADR-XXXX (out of current scope).**

## Decision

Adopt Option B. Codify in `cto_operations_runbook.md` §"Release tag SOP" (existing as of 2026-05-11):

1. Bump workspace version (the `Cargo.toml` `version` field is the source of truth; binary embeds it via cargo build).
2. Commit the bump.
3. Author release notes that include the bound user-scenario verbatim (cite commands + expected output).
4. Tag at the bumped-and-noted commit.
5. Push tag → release.yml fires → tier-1 binaries built.
6. **Spawn release-readiness P7 sonnet** (D0 sonnet solo per `feedback_subagent_model_tier.md` D-matrix) per runbook §"Release-readiness agent": clean-shell verify each user-scenario command end-to-end + return GO or BLOCK.
7. If GO → release is shipped. If BLOCK → fix-pack sprint per the BLOCK reason; bump again; re-tag (e.g. v0.1.1 → v0.1.2 cycle).

The bound user-scenario MUST be one of:

| Tier | Scenario type | Examples |
|---|---|---|
| **Install** | `cargo install --git <repo> <crate>` or `curl <tarball> &#124; tar xz && ./binary --version` succeeds + binary reports the tag version | v0.1.1 / v0.1.2 had this scenario |
| **Capability** | A representative `.cb` program compiles + runs with expected stdout given documented stdin | W2 wedge's `examples/leetcode/two_sum.cb` is the canonical post-W2 scenario |
| **Library translation** | A `cobrust-<lib>` PyO3-wrapped library passes its CPython differential gate | post-T1.1: `cobrust-tomli` parses real-world TOML matching CPython's tomllib output |

The scenario tier is recorded in the release notes under `## What v0.X delivers (user-scenario)`. The release-readiness verifier MUST quote the scenario verbatim in its GO/BLOCK report.

## Consequences

### Positive

- Closes ADSD F19 (public-onboarding-install-not-tested) systemically. Independent verification at the release tag is mandatory; sub-agent self-report is not the gate.
- Establishes a project-wide invariant: every published tag has a public, executable, end-to-end user-scenario as its acceptance criterion. Future review-claude / external audits / project-owner can re-run the bound scenario against any historical tag for regression detection.
- Aligns with constitution §5.2 "Scientific" — every release ships with a falsifiable claim of user reachability, not just an internal gate count.
- Empirically validated within 24h of being implemented: §A.3 P7 sonnet release-readiness BLOCK caught the prebuilt-binary version-string mismatch on v0.1.1 that internal 5-gate did not catch. §A.5 P7 sonnet release-readiness GO confirmed v0.1.2 fix is real.

### Negative

- ~20-30 min release-readiness verify per release (one P7 sonnet D0 sprint). Acceptable.
- One extra commit cycle per release for any caught defect (v0.1.1 → v0.1.2 was the empirical case). Acceptable; far cheaper than shipping a broken release publicly.

### Neutral / unknown

- The 24-hour incident cluster pre-dates the first independent verifier dispatch (which itself was prompted by user complaint). Will the gate stay sufficient as the project scales beyond project-owner-as-first-user? Unknown until external contributors appear. ADR-0045 can be amended to Option C when the user community supports per-merge testing.
- The scenario tiers (Install / Capability / Library translation) are an initial taxonomy. A future ADR may expand to cover Deploy / Cross-arch / Numerical-correctness tiers as the project matures.

## Evidence

- `docs/releases/v0.1.1-release-notes.md` (current state with deprecation banner) and `docs/releases/v0.1.2-release-notes.md` (current canonical) — both bind explicit Install + Capability user-scenarios.
- `cto_operations_runbook.md` §"Release-readiness agent" — the executable form of this ADR's gate.
- §A.3 release-readiness BLOCK verdict (P7 sonnet `a14ba05e19f21ff68`, 2026-05-11) — empirical proof that independent verification catches drift internal gates miss.
- §A.5 release-readiness GO verdict (P7 sonnet `ac01ceb557550fb5c`, 2026-05-11) — empirical proof that the fix path closes the defect.
- W2 wedge merge commit `9caef99` includes `examples/leetcode/two_sum.cb` + `docs/human/{zh,en}/getting-started-leetcode.md` — first Capability-tier user-scenario for post-W2 releases.
- Memory `feedback_quantitative_claims_verify.md` §"Extension 2026-05-11: sub-agent self-audit can lie" — documents the same anti-pattern at the sub-agent self-report level; ADR-0045 enforces independent verification at the release level.

## Cross-references

- ADR-0019 §"Definition of usable" — internal four-tier anchor, complementary to this ADR's external-user anchor.
- ADR-0038 §F.1 wedge "AI Python 加速器" — strategic context for why user-traction matters as a milestone gate.
- ADR-0042 — snapshot-lint enforcement closes F1.1 (declared invariant without machine enforcement); ADR-0045 closes F19 (declared user-facing claim without independent verification). Same systemic anti-pattern, different surface.
- ADR-0044 — W2 wedge binding; ADR-0045 ensures future releases that touch user-facing surface get verified the same way.
- ADR-0046 — release.yml asset consolidation; ADR-0045 ensures the bound user-scenario's URLs are tier-1 contract-protected.
- Memory `feedback_p10_post_compaction_identity_recovery.md` — same family of "declared without independent verification" anti-pattern at the agent-identity level.
- ADSD `failure-modes-catalogue.md` F19 confirmed entry — `Cobrust-lang/agent-driven-development` commit `60dd769` (ADSD catalogue 第 21 entry; review-claude promoted from candidate to confirmed 2026-05-11 post §A.3→§A.5 empirical cycle).

## Why this ADR now

The 24-hour cluster of three drift incidents (v0.1.0 CI red badge, v0.1.1 install 404, W2 user-can't-leetcode) is the strongest empirical signal in the project's history that internal-gate-only "done" definitions are insufficient. Without this ADR, the next milestone declared "shipped" will continue to risk public-facing reachability defects. With this ADR, every release tag carries a falsifiable user-scenario as part of its definition of done — and a binding independent-verifier step that surfaces drift before the release lands in user hands.
