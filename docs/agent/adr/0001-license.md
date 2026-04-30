---
doc_kind: adr
adr_id: 0001
title: Apache-2.0 OR MIT dual license
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0001: Apache-2.0 OR MIT dual license

## Context

Cobrust is open-source and intends to be reused widely across the Rust
ecosystem and downstream Cobrust packages. The license must:

- Permit commercial use without restriction.
- Provide an explicit patent grant.
- Be compatible with mainstream open-source projects (including GPL-2.0
  consumers via the MIT branch).
- Match the conventions of the Rust ecosystem so that contributors do
  not need to learn a new license scheme.

The constitution (`CLAUDE.md` §0) names "Apache-2.0 + MIT dual" as the
default and instructs M0 to formalize it via this ADR.

## Options considered

1. **MIT only** — minimal, permissive, but no explicit patent grant.
2. **Apache-2.0 only** — patent grant, but incompatible with GPL-2.0
   downstream consumers.
3. **Apache-2.0 OR MIT (dual)** — Rust ecosystem standard. Users pick
   the branch they need; Apache covers patents, MIT preserves
   GPL-2.0 reach.
4. **MPL-2.0** — file-level copyleft, less common in Rust, friction for
   contributors.
5. **AGPL-3.0** — strong copyleft, would deter adoption as a runtime
   dependency.

## Decision

Adopt **Apache-2.0 OR MIT** dual licensing for the entire Cobrust
project, including translated stdlib and registry artifacts. Both
license texts ship at the repository root as `LICENSE-APACHE` and
`LICENSE-MIT`. Workspace `Cargo.toml` declares
`license = "Apache-2.0 OR MIT"`, which is inherited by every member
crate via `license.workspace = true`.

Each contribution is implicitly offered under the same dual license
unless the contributor explicitly states otherwise (Apache-2.0 §5
applies). The README's contributor section makes this explicit so
contributors know the expectation.

## Consequences

- **Positive**
  - Maximum ecosystem reach: any downstream user can pick the branch
    that fits their license stack.
  - Apache-2.0 patent grant protects Cobrust users from patent claims
    by contributors.
  - Mirrors `rustc`, `cargo`, `tokio`, `serde`, etc. — no surprise for
    Rust contributors.
- **Negative**
  - Slightly more legal boilerplate (two license files, dual headers
    in source if we adopt them later).
- **Neutral / unknown**
  - **AI-translated artifacts**: their derivative status under the
    Cobrust license is settled by this ADR. The *training* status of
    the LLMs used by the translation subsystem is an evolving legal
    question. Translation manifests must record the LLMs used so
    consumers can make their own risk assessment. Tracked separately
    as a future ADR (TBD).

## Evidence

- Rust API guidelines on license:
  https://rust-lang.github.io/api-guidelines/necessities.html
- `rust-lang/rust` license file structure (Apache-2.0 OR MIT).
- Constitution `CLAUDE.md` §0 names this as the default.
- `LICENSE-APACHE` and `LICENSE-MIT` at the repo root carry the
  canonical texts.
