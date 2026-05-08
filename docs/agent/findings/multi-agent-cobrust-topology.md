---
doc_kind: finding
finding_id: multi-agent-cobrust-topology
last_verified_commit: 7a51f8c
dependencies: [adr:0002, adr:0019]
---

# Finding: Multi-agent worktree topology — 100+ commits' worth of operational lessons

## Hypothesis

The Cobrust project (constitution §1, dual mandate) is large enough that
a single-agent CTO cannot deliver M0..M14 in a useful timeframe. CTO
hypothesised that a worktree-isolated, P9-Tech-Lead-led, 4-way parallel
multi-agent topology would scale linearly enough that wall-clock
delivery fits within a single user session window (per
`feedback_user_sleep_mode`).

This finding documents what worked and — critically — what *didn't* and
how the failure modes were diagnosed and standardised. It is published
externally because the auditing process (audit #6, see
`feedback_third_party_audit_2026_05_09`) suggested the methodology is
*more publishable than the language itself*. The document below is a
cleaned, externalised distillation of the full operational runbook
maintained in CTO memory at
`memory/cto_operations_runbook.md`.

## Method

**Topology** (per ADR-0002):

- 1 CTO agent (Opus 4.7) on `main` — never writes production code. Owns:
  - ADR drafting + acceptance gates
  - P9 dispatch (worktree creation, branch naming, prompt authorship)
  - Merge-time integration (union-merge resolution, doc-coverage repair)
  - Memory + state snapshot for compaction-resilience
- 1+ P9 Tech Lead agents (Sonnet 4.6 typical) per active milestone.
  Each owns:
  - One worktree at `/Users/hakureirm/codespace/Study/cobrust-<id>`
  - One feature branch
  - Full impl + tests + per-language doc trees + ADR
  - Self-verification of `fmt + clippy + build + test + doc-coverage`
- 0..3 general-purpose agents per active "spike" or "two-phase recovery."

**Sequencing**: CTO sequences P9 dispatches per ADR-0019's roadmap. Up
to 4 worktrees can run in parallel; 5+ degrades on macOS arm64 dev host
(cargo registry lock contention; see Gotcha #1 below).

**Cumulative scale at HEAD `7a51f8c`**:

- 125+ commits cumulative on `main`
- 30 ADRs accepted + 6 findings landed
- 2423 tests passing on cold integrated rebuild (M14 baseline)
- 15 workspace crates (5 product translations + 9 compiler/runtime + 1
  numpy)

## Result — six recurring failure modes (and their SOPs)

Each is observed ≥ 3 times during M0..M14. Each is standardised into a
named SOP in the operations runbook.

### Failure 1: Cargo registry lock contention (parallel P9 cargo test)

**Symptom**: P9 worktree gate run exits 144 (SIGUSR2). Cargo can't
acquire the global registry lock because a sibling worktree's
`cargo test` holds it.

**SOP**: Defer full `cargo test --workspace` to **integrated main**
post-merge. P9 worktree only runs:
- `cargo build` for the P9's specific crate
- `cargo test` for the P9's specific crate (not workspace)
- `cargo fmt --check` on whole workspace (lock-free)
- `cargo clippy` on whole workspace (lock-aware)

The 5-gate full verification runs once, on the post-merge integrated
main. This serializes the long-running gate and prevents the lock
contention.

### Failure 2: P9 stream-idle timeout during heavy ADR drafting

**Symptom**: P9 stops emitting tokens for 25+ minutes (sometimes 32+
min) while drafting a 700-line ADR + provenance manifest. Eventually
the runtime times out and the agent terminates with no commit.

**SOP**: **Two-phase dispatch**:
- Phase 1 — CTO drafts the ADR + corpus skeleton in `main`,
  spike-commits it.
- Phase 2 — CTO dispatches a `general-purpose` agent (not `P9`) with the
  ADR as authoritative input. Impl is bounded scope, no large prose
  generation, no timeout window.

This was the recovery for M7.2 (40-minute first attempt → 25 minutes
spike + 12 minutes impl), M12 (32 minutes stalled → resumed 18 minutes
impl), M11.1 (current sprint — applied prophylactically).

### Failure 3: Clippy `--locked -D warnings` whack-a-mole on test files

**Symptom**: P9's worktree-local `cargo clippy` passes; integrated
main's `--locked -D warnings` fails on test files because workspace
locks downloads and pulls in newer clippy-pedantic lint variants. P9
investigates per-callsite, exhausts 600s watchdog.

**SOP**: **18-lint test-only allow header**. Mandatory paste at the top
of every `tests/*.rs` from Day 1. List composed empirically from M4..M12
sweep:

```rust
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
```

Crates with fluent-builder patterns (e.g. `cobrust-click`) need an
additional `#![allow(clippy::return_self_not_must_use)]`. M12.x
amendments added `approx_constant` + `stable_sort_primitive` to the
test-only set after a stall.

### Failure 4: Workspace fmt drift between worktree and integrated main

**Symptom**: P9's `cargo fmt --check` passes locally; integrated
main's `--check` fails. Diff is 0-2 lines, typically import ordering or
trailing newlines, caused by rustfmt cache state interacting with
worktree's borrowed cargo state.

**SOP**: Post-merge on integrated main, **first** `cargo fmt --all`
(apply, no `--check`), **then** `--check`. Commit the fmt sweep as a
trailing housekeeping commit if needed.

### Failure 5: Doc-conflict union-merge produces invalid shell syntax

**Symptom**: 4-way parallel merge resolves doc conflicts via union-merge
(strip `<<<`/`===`/`>>>` markers, keep both halves). For
`scripts/doc-coverage.sh` this can leave one branch's `if ...` block
with the *other* branch's `done; fi` consumed, producing
"unexpected end of file."

**SOP**:
1. Run union-merge Python script (3 sed-equivalent regexes).
2. **Always** `bash -n scripts/doc-coverage.sh` to syntax-check.
3. If fail, `awk` for unclosed `if` blocks, manually insert the missing
   `fi` + `echo` line between the parallel additions.

For `last_verified_commit:` field with different SHAs from each branch:
union-merge keeps both lines (invalid YAML). Manually fix to single line
with the most recent or `TBD`.

### Failure 6: Worktree state pollution + cargo target/ collisions

**Symptom**: Worktree's `target/` ends up containing build artifacts
from sibling worktrees if the user accidentally `cd ../ && cargo
build`. Or: worktree branch becomes stale because parallel P9 force-
pushed (rare; not encountered in M0..M14, but anticipated).

**SOP**:
- `git worktree list` regularly; remove merged worktrees with
  `git worktree remove --force <path>`. Branches retained (for audit);
  worktree directories not.
- Each worktree has its own `target/` (cargo workspace doesn't share
  target across worktrees by default — verified empirically).
- Background bash shells in CC's UI persist after their subprocess
  dies; `ps aux | grep -E "(cargo|git merge)"` is the truth source.

## Conclusion

The Cobrust topology is reproducible by other complex-codebase teams
running multi-agent CC sessions. The six gotchas above are
**generalisable** — they're not Cobrust-specific. A team running a
similar Rust monorepo with 5-15 crates under multi-agent CC should
expect to encounter all six within their first 100 commits.

The CTO operations runbook (private, in CTO memory) is the full
in-conversation playbook. This finding's purpose is to externalise the
*structural* lessons so the methodology can be adopted without
needing access to one CTO's memory directory.

## Actionable consequences

1. Future Cobrust contributors (human or agent) can read this finding
   to understand why test files have a 18-lint header without
   assuming malice or laziness.
2. External teams adopting this topology can reference this finding
   when designing their own dispatch + integration loops.
3. CTO memory's `cto_operations_runbook.md` and
   `feedback_p9_*.md` files remain the authoritative
   in-conversation source — those decay rapidly with milestone-specific
   amendments. This finding crystallises the structural part that
   should not decay.

## Cross-references

- ADR-0002 — multi-agent topology + sequencing ground rules.
- ADR-0019 — Phase E roadmap that drove M8..M14 multi-agent execution.
- `feedback_p9_clippy_stall_pattern` (CTO memory) — Failure 3 detail.
- `feedback_p9_two_phase_dispatch` (CTO memory) — Failure 2 detail.
- `feedback_third_party_audit_2026_05_09` (CTO memory) — audit #6 that
  recommended this finding be published externally.
- `findings/m13-sync-bridge-cost.md` — example of a benchmark-driven
  finding that landed alongside this multi-agent process.
