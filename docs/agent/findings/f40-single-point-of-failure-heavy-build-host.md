---
module_id: findings/f40
title: Single-point-of-failure heavy-build host (DG dead 2026-05-20)
status: filed
date_filed: 2026-05-20
related_findings: [f37-silent-rot-on-accepted-debt, f39-device-name-leakage-in-commits]
related_memory: [feedback_heavy_build_offload_to_workstation.md, reference_x86_workstation.md]
---

# F40 — Single-point-of-failure heavy-build host

## §1 Pattern

Depending on a single self-hosted runner (SSH-reachable workstation) for full-workspace
cargo verification creates a single point of failure. When the host dies — network reset,
OS issue, ISP interruption — the entire offload pipeline collapses with no fallback path.

## §2 Why it is debt

Per CLAUDE.md §3 dispatch reproducibility: verification must be reproducible by any
contributor. An SSH-credential-gated single host is anti-reproducibility. A new
contributor (human or agent) cannot run the heavy-build gates without:

1. SSH credentials to the specific host.
2. The host being alive and reachable.
3. The host having a current repo clone, Rust toolchain, and working PATH.

Any one of those three failing silently stalls a sprint without a clear error message.

## §3 Empirical

2026-05-19 / 2026-05-20: DG-Workstation-2x3090 SSH endpoint failed with
`kex_exchange_identification: read: Connection reset by peer` throughout an 8+ hour
session. Sub-agents kept retrying (per Mode C SOP) instead of escalating, consuming
tool budget on failed SSH invocations. The Mac single-crate per-crate verify was
sufficient to unblock the session but was ad-hoc — no policy existed for "DG is dead,
do this instead."

The host's degradation went unflagged for the full session (F37 sibling: silent failure
without escalation signal).

## §4 Resolution path

**Adopted policy (effective 2026-05-20)**:

- ALL HEAVY full-workspace cargo (`cargo test --workspace`, `cargo build --workspace`)
  routes to GH Actions CI (ubuntu-latest + macos-latest matrix).
- Mac local = single-crate quick-feedback only (`cargo test -p <crate>`).
- No SSH credentials in dispatch templates. No `ssh -p <port> <user>@<host>` patterns.

GH Actions is the authoritative 2-OS matrix verifier. It is reproducible, credential-
free, and available to all contributors.

**Dispatch template change**: replace Mode C `VERIFY LOOP (every change-batch)` SSH
block with "push branch → GH Actions CI passes → merge."

## §5 F-family

Sibling of F37 (silent-rot-on-accepted-debt): the host's degradation was not escalated.
Sub-agents silently retried instead of surfacing "DG unreachable, route to CI."

Related to F39 (device-name-leakage): the same DG host's hostname `DG-Workstation-2x3090`
leaked into commit messages (ADR-0058a wave-1 build verification records) before
abandonment — both are hygiene failures from over-reliance on a named private host.

## §6 Status

Filed 2026-05-20. Resolution adopted immediately (memory + spike doc updated in same
session). No open action items.

Archaeology: `feedback_heavy_build_offload_to_workstation.md` + `reference_x86_workstation.md`
preserve the DG history for root-cause archaeology.
