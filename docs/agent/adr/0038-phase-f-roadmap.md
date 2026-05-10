---
doc_kind: adr
adr_id: 0038
title: "Phase F roadmap with timetable + external success criteria"
status: accepted
date: 2026-05-10
last_verified_commit: TBD-handoff-pack-commit
supersedes: []
superseded_by: []
relates_to: [adr:0019]
discovered_by_review: review-claude 10th strategic review (P10-tier first pass)
---

# ADR-0038: Phase F roadmap — timetable + external success criteria

## Context

ADR-0019 §"Out of scope (defer to Phase F)" lists 7 items as "deferred"
without priority, timetable, or external success definition:

- Self-hosting (constitution §4.4)
- LSP / IDE protocol
- `cobrust debug` subcommand
- WASM target
- Cross-compilation matrix beyond macOS arm64 + Linux x86_64
- M7.6+ A/C buckets (numpy)
- M9.1 LLVM full backend
- More translated libraries
- M14.1 stdlib calls + control flow in REPL

This is a **reactive list**: items added when out-of-scope of Phase E
sprints. It has no notion of:
- Which item should fire when
- What external user / market signal triggers it
- What "done" means in user-visible terms (not internal commit)
- How the items relate to **the project narrative** (why anyone outside
  the project should care)

review-claude 10th strategic review (2026-05-10, P10-tier review #1)
flagged that all 9 prior reviews + 24 hours of intense Phase E sprint
work happened **without any P10 view**. This ADR fixes that gap.

## Options considered

### Option 1 — Keep ADR-0019 reactive list, add items as triggers fire
- Pros: Lowest immediate work
- Cons: Already produced 1 year of unstructured deferred-work backlog;
  no narrative; no public-facing roadmap. **Rejected.**

### Option 2 — Phase F roadmap with priority + condition + done means + 1-year horizon (CHOSEN)
- Pros: External-facing, narrative-aligned, falsifiable
- Cons: ~50 more lines of ADR + needs CTO sign-off on priority calls
- **Chosen** because 0.1.0-beta release requires a public roadmap to
  ship credibly; ADR-0019 reactive list cannot serve that role.

### Option 3 — Multi-year strategic doc separate from ADR
- Pros: More space for narrative
- Cons: ADR system already exists, splitting decision artifacts adds
  cost. **Rejected** — keep within ADR system for traceability.

## Decision

Adopt Option 2. Phase F is partitioned into three tiers (F.1, F.2, F.3)
by 6-month / 1-year / 5-year horizon respectively. Each item carries:

- **Priority** (P0/P1/P2/P3) — within its tier
- **Trigger** — what user / project state must hold before starting
- **Done means** — externally verifiable success criterion
- **Estimated effort** — calibrated against AI-velocity (≥10× human)

### F.1 — Six months (0.1.0-beta → 0.2.x)

Driver: 0.1.0-beta is shipped; project needs to validate the wedge
("AI Python 加速器") with real external users.

| Item | P | Trigger | Done means | Effort |
|---|---|---|---|---|
| F.1.1 GitHub public + first release | P0 | Now (no blocker) | repo public, v0.1.0-beta tagged, first 10 external stars | 1 day |
| F.1.2 tomli end-to-end real-LLM full-library translation | P0 | Now | All public functions of tomli pass L0..L3 + downstream pip-tools tests | 2-3 days |
| F.1.3 install path frictionless | P0 | F.1.1 done | `cargo install cobrust-cli` works; prebuilt binaries on releases | 1 day |
| F.1.4 error UX rewrite | P0 | F.1.1 done | 10 common .cb errors output ≤30 line user-facing message (no Cranelift IR dump) | 1-2 days |
| F.1.5 syntax highlighting | P1 | F.1.1 done | VSCode marketplace listing, 100+ installs | 0.5 day |
| F.1.6 second translated library | P1 | F.1.2 done | textwrap (or base64) translated, L0..L3 PASS | 2 days |
| F.1.7 self-hosting AST printer | P1 | F.1.2 done | unparser of 30 forms re-implemented in `.cb`, round-trip-tested against existing Rust unparser | 3-4 days |
| F.1.8 LSP M0 (hover + go-to-def) | P2 | F.1.5 done | vscode-cobrust extension on marketplace, 1000+ installs | 5-7 days |
| F.1.9 cross-arch CI matrix | P2 | F.1.1 done | GitHub Actions matrix tests macOS-arm64 + linux-x86_64 + linux-aarch64 + windows-x86_64 (best-effort) | 1-2 days |

### F.2 — One year (0.2.x → 0.5.x)

Driver: 100 contributors; 1000 users; 5 production-validated translated
libraries.

| Item | P | Trigger | Done means | Effort |
|---|---|---|---|---|
| F.2.1 Top-10 PyPI translated batch | P0 | F.1.6 done | 10 libraries L0..L3 PASS + downstream-dep PASS, on cobrust-registry | ~2 weeks |
| F.2.2 LSP M1 (full diagnostics + autocomplete) | P0 | F.1.8 done | vscode/helix/nvim parity with rust-analyzer's M1 | 3-4 weeks |
| F.2.3 Debugger (`cobrust debug`) | P1 | F.2.2 done | breakpoint + stack trace + variable inspection in vscode | 2-3 weeks |
| F.2.4 WASM target | P1 | F.1.7 done | hello.cb compiles + runs in browser via wasmtime | 1-2 weeks |
| F.2.5 Self-hosting type checker subset | P1 | F.1.7 done | `cobrust check` for non-generic functions implemented in `.cb` | 3-4 weeks |
| F.2.6 cobrust-registry online | P2 | F.2.1 done | `cobrust install <lib>` fetches from cobrust.dev/registry | 1-2 weeks |
| F.2.7 Consensus-mode multi-provider | P2 | Now-ish | 3-provider consensus reduces translation error rate measurably (paper-quality finding) | 1 week |

### F.3 — Five years (1.0+)

Driver: Cobrust is a household name in Python performance space; companies
deploy translated libraries in production; compiler is 70% self-hosted.

| Item | P | Trigger | Done means | Horizon |
|---|---|---|---|---|
| F.3.1 Top-100 PyPI translated | P0 | F.2.1 done | 100 libraries on registry, 5000+ deploys | year 2-3 |
| F.3.2 Top-1000 PyPI translated | P0 | F.3.1 done | automatable batch translation, 80%+ first-attempt PASS | year 4-5 |
| F.3.3 Compiler 70% self-hosted | P1 | F.2.5 done | Type checker, AST printer, MIR builder all in `.cb` | year 3-4 |
| F.3.4 Build tooling parity with Cargo | P1 | F.2.6 done | publish/yank, security advisories, downstream auditing | year 2-3 |
| F.3.5 Production deployments at scale | P0 | F.3.1 done | 5+ companies running cobrust-translated libraries in production with public case study | year 3-4 |

## Decision (CTO sign-off section)

**Default decision (review-claude proposes; CTO confirm or override)**:

Adopt the F.1 tier as committed; F.2 as planned (subject to F.1 outcomes);
F.3 as aspirational (subject to F.2 outcomes).

Within F.1, **priority ordering for 2-day 0.1.0-beta sprint**:

1. F.1.1 GitHub public — Day 1 morning, CTO solo
2. F.1.2 tomli end-to-end + F.1.3 install + F.1.4 error UX + F.1.5 syntax highlighting — Day 1 afternoon, **4 parallel sub-agents**
3. F.1.6 second library + F.1.7 AST printer self-host start — Day 2-7 (post-beta)
4. F.1.8 LSP M0 — Day 7-14
5. F.1.9 cross-arch CI — Day 2-3

**CTO sign-off**: Cobrust CTO — accept review-claude default ordering. Wedge "AI Python 加速器, auto-translate Python lib + PyO3 wrapper" confirmed; GitHub namespace `Cobrust-lang/cobrust` confirmed by user; Day 1 public per build-in-public preference. (date: 2026-05-10)

If CTO disagrees with the ordering or scope, edit this section before
the 4 sub-agents fire (Day 1 afternoon).

## Consequences

### Positive

- Public-facing roadmap; ships with 0.1.0-beta
- Each F.1 item has a falsifiable success criterion (no "deferred"
  vague-out)
- Reviewer (review-claude or future) can hold the project to its
  external commitments
- Multi-horizon view (6 mo / 1 yr / 5 yr) gives strategic anchor for
  fundraising / hiring / community pitch

### Negative

- ~50 lines of ADR overhead vs ADR-0019 reactive list
- F.2 / F.3 tables likely to drift; needs annual refresh

### Risk

- F.1.2 (tomli full translation) may fail at the prompt-design layer —
  audit-3a's rich-prompt builder is leaf+stateful validated, not
  whole-library-iterating validated. **Mitigation**: T1.1 dispatch
  prompt allows partial PASS (4/5 functions sufficient).
- F.2.1 (Top-10 PyPI) may hit per-library prompt-context limits not
  visible in audit-3a's tomli scope. **Mitigation**: track first 3
  libraries at higher cadence; reassess before committing top-10
  batch.

## Cross-references

- ADR-0019 — Phase E roadmap (this ADR extends F segment)
- ADR-0036 — audit-3a prompt-design fix (anchor for F.1.2 viability)
- finding `audit-1-tomli-real-llm-result.md` — leaf-fn translation evidence
- finding `audit-3a-stateful-prompt-design.md` — stateful-fn translation evidence
- review-claude 10th strategic review — origin of this ADR
- review-claude handoff `COVER_LETTER.md` — 2-day sprint plan that exercises F.1.1-F.1.5

## Why this ADR exists at this exact moment

User said: "我不知道, 没有经验, 但是你有很全面的知识, 你应该知道如何让它
成为 0.1.0-beta 可公开的状态". 0.1.0-beta needs a public roadmap with
falsifiable horizon claims. ADR-0019's deferred list cannot serve that
role. ADR-0038 fills the gap.
