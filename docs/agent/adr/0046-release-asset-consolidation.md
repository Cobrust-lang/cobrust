---
doc_kind: adr
adr_id: 0046
title: "release.yml asset consolidation + tier-1 platform contract"
status: accepted
date: 2026-05-11
last_verified_commit: 03c70f2
supersedes: []
superseded_by: []
relates_to: [adr:0044, finding:m10-sha-pin-hallucination, finding:f19]
---

# ADR-0046: release.yml asset consolidation + tier-1 platform contract

## Context

Within a 24-hour window (2026-05-10..2026-05-11), two distinct release artifact
friction events compounded each other:

1. **v0.1.1 release notes URL mismatch** — install instructions pointed to
   `cobrust-0.1.1-*` (no `v` prefix) while GitHub uploaded assets as
   `cobrust-v0.1.1-*`. Every prebuilt binary curl command in the release notes
   was a 404. review-claude back-patched `-fsSL` and the correct naming in
   `4baea69`. Root pattern: ADSD F19 — "public-facing onboarding text written
   but never independently install-tested."

2. **v0.1.2 aarch64-unknown-linux-gnu gap** — the `release.yml` at the time had
   `aarch64-unknown-linux-gnu` classified as best-effort (`continue-on-error: true`).
   When this target succeeded and uploaded an artifact, it appeared in the release
   assets but was absent from the release-notes binary install block (which only
   listed macOS arm64 + Linux x86_64). Users on Linux arm64 could not follow any
   install instruction. Root pattern: same F19 — release notes were not derived
   from the actual tier-1 contract.

The systemic gap: `release.yml` had an implicit two-tier model (tier-1 =
`aarch64-apple-darwin` + `x86_64-unknown-linux-gnu`; best-effort =
`aarch64-unknown-linux-gnu` + `x86_64-pc-windows-msvc`) but this contract was
never written down as a binding reference. The release notes generator, the
release-readiness agent, and any future contributor had to infer it from the
YAML matrix structure. Inference is fragile.

**ADR-0045** (user-traction milestone gate) addressed the policy — mandatory
independent release-readiness verification before merge. **ADR-0046** addresses
the single-source-of-truth problem: codify which platforms are tier-1 in a
contract comment block at the top of `release.yml`, and derive all downstream
artifacts (release notes URLs, release-readiness curl list) from that contract.

### Historical evidence

- `docs/agent/findings/m10-sha-pin-hallucination.md` — SHA-pin hallucination
  2026-05-10: sub-agent generated plausible-looking SHAs that resolved 404 on
  GitHub API. Same root: declared-without-execution-verification on a
  user-first-contact surface.
- ADSD `failure-modes-catalogue.md` F19 — "public-facing onboarding text
  written but never independently install-tested." Both v0.1.1 and v0.1.2
  incidents are instances of F19.
- v0.1.2 `9caef99` — W2 wedge merge; `aarch64-unknown-linux-gnu` promoted to
  tier-1 in the release matrix. The release notes at `9caef99` now include all
  three tier-1 URLs.

## Options considered

### Option A — No explicit contract; derive from YAML matrix at read time

- Current state at v0.1.1 tag.
- Readers must parse `strategy.matrix.include` to infer tier-1 vs best-effort.
- `continue-on-error: true` is the only signal; easy to miss or misread.
- Release notes author must manually mirror the YAML matrix into the body text.
- **Rejected**: brittle, already caused two incidents.

### Option B — Separate `tier1-contract.yml` file imported by `release.yml`

- A dedicated file `/.github/release-tier1.yml` lists tier-1 targets; release.yml
  imports it via `fromJson` + `strategy.matrix`.
- Pros: machine-parseable by external tools.
- Cons: YAML anchors / imports are not supported in GitHub Actions matrix syntax
  without an extra job step to read+parse the file. Adds complexity. For 3 targets
  the overhead is not worth it.
- **Rejected**: complexity exceeds benefit at current scale.

### Option C — Inline contract comment block at top of `release.yml` (CHOSEN)

- A structured comment block immediately below the file header declares:
  - Tier-1 targets: must build, must appear in release notes, must be verified
    by the release-readiness agent (per ADR-0045 §"curl × 3").
  - Tier-2 targets: best-effort, may fail without blocking release.
  - Queued targets: documented intent, not yet built (available via
    `cargo install --git`).
- The comment block is the single source of truth. Release notes body text
  and the release-readiness agent's curl list are hand-derived from it, but the
  derivation is now explicit and auditable.
- Future tooling (Option B) can graduate to machine-parsing if the platform
  count grows beyond ~6.
- **Chosen.**

## Decision

Adopt **Option C**. Add a tier-1 contract comment block to the top of
`.github/workflows/release.yml`, establishing the following platform tiers for
v0.1.2+:

**Tier-1 (must build, must appear in release notes, release-readiness agent
must curl-verify all three):**
- `aarch64-apple-darwin` — macOS arm64 (Apple Silicon)
- `aarch64-unknown-linux-gnu` — Linux arm64 (cross-compiled via `cross`)
- `x86_64-unknown-linux-gnu` — Linux x86_64

**Tier-2 (best-effort, `continue-on-error: true`, no release-readiness gate):**
- none currently

**Queued (documented intent, not yet built; available via
`cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli`):**
- `x86_64-apple-darwin` — macOS x86_64 (Intel; available via `cargo install --git`)
- `windows-*` — Windows x86_64 (MSVC or GNU; deferred to Phase F.2.x)

### YAML matrix alignment

`release.yml` build-tier1 matrix at v0.1.2 HEAD (`9caef99`) matches the
above tier-1 contract:
```
- aarch64-apple-darwin   os: macos-latest
- aarch64-unknown-linux-gnu  os: ubuntu-latest  use_cross: true
- x86_64-unknown-linux-gnu   os: ubuntu-latest
```

The best-effort job (`build-best-effort`) currently carries
`x86_64-pc-windows-msvc`; its `continue-on-error: true` semantics are
preserved and documented under "queued" above (Windows target is best-effort
until a Windows runner is stable).

## Consequences

### Positive

- **Single source of truth**: anyone editing `release.yml` sees the contract
  comment before touching the matrix. Tier promotions require updating the
  comment → forces conscious decision.
- **Release-readiness agent anchored**: ADR-0045 requires "curl × 3 tier-1
  URLs"; this ADR defines which 3. The agent's curl list is now derivable
  from a single paragraph, not from reading the YAML matrix structure.
- **Release notes generator anchored**: the `body:` block in the `release` job
  step lists exactly the tier-1 targets. When a target is promoted from queued
  to tier-1, the comment update + matrix update + body update is a 3-line
  change with a checklist.
- **F19 systemic closure (partial)**: the v0.1.1 + v0.1.2 incidents were
  both caused by a mismatch between the internal build matrix and the
  user-facing install instructions. Having an explicit contract comment does
  not prevent all mismatches, but it makes the mismatch immediately visible on
  the next read.

### Negative / trade-offs

- **Comment drift risk**: if someone updates the YAML matrix without updating
  the comment block, the contract becomes stale. Mitigation: doc-coverage CI
  check is the long-term enforcement path (Phase F follow-up: add a
  `grep "tier-1 platform contract" release.yml` assertion to `doc-coverage.sh`
  or a dedicated lint).
- **Hand-derivation still required**: release notes body text is still
  hand-written (or generated by the release job's `body:` field). Option B
  (machine-parseable contract) would eliminate this; deferred until ≥6
  platforms make the manual process painful.

### Neutral

- Windows tier status: `x86_64-pc-windows-msvc` is currently in `build-best-effort`
  with `continue-on-error: true`. ADR-0046 documents it as "queued" because
  the binary is not advertised in the release notes body. This is the correct
  state until a stable Windows runner + test suite lands (Phase F.2.x).

## Evidence

### v0.1.2 tier-1 asset verification (Batch 3 of this sprint)

Release-readiness agent ran after this ADR commit landed:

```
curl -fsSL -o /dev/null -w "HTTP %{http_code} aarch64-apple-darwin\n" \
  https://github.com/Cobrust-lang/cobrust/releases/download/v0.1.2/cobrust-v0.1.2-aarch64-apple-darwin.tar.gz
# HTTP 200 aarch64-apple-darwin

curl -fsSL -o /dev/null -w "HTTP %{http_code} aarch64-unknown-linux-gnu\n" \
  https://github.com/Cobrust-lang/cobrust/releases/download/v0.1.2/cobrust-v0.1.2-aarch64-unknown-linux-gnu.tar.gz
# HTTP 200 aarch64-unknown-linux-gnu

curl -fsSL -o /dev/null -w "HTTP %{http_code} x86_64-unknown-linux-gnu\n" \
  https://github.com/Cobrust-lang/cobrust/releases/download/v0.1.2/cobrust-v0.1.2-x86_64-unknown-linux-gnu.tar.gz
# HTTP 200 x86_64-unknown-linux-gnu
```

All 3 tier-1 URLs return HTTP 200. Contract verified against v0.1.2 artifacts.

## Cross-references

- `docs/agent/findings/m10-sha-pin-hallucination.md` — SHA hallucination
  finding, same declared-without-verification pattern (F13 + F1.1).
- ADSD `failure-modes-catalogue.md` F19 — "public-facing onboarding text
  written but never independently install-tested." This ADR is the structural
  prevention at the release.yml level.
- ADR-0045 — user-traction milestone gate policy; ADR-0046 is the
  `release.yml`-side binding that ADR-0045's release-readiness agent checks.
- ADR-0044 — `aarch64-unknown-linux-gnu` promoted to tier-1 in the W2 merge
  (`9caef99`); this ADR codifies that promotion in the contract comment.
- `cto_operations_runbook.md` §"Release-readiness agent" — the runbook section
  that executes the curl × 3 gate mandated by ADR-0045.
