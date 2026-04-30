# Cobrust English Documentation

> Cobra 🐍 + Rust 🦀 — A Rust-implemented Python successor with an AI-native compiler that closed-loop translates the entire Python ecosystem.

## Document map

- [Overview](overview.md) — one-sentence understanding of what Cobrust is
- [Design philosophy](design-philosophy.md) — what we keep, what we drop, why
- [Architecture](architecture.md) — compiler layers + AI translation subsystem
- [Milestones](milestones.md) — roadmap from M0 to M7+
- [Getting started](getting-started.md) — from source build to first translation

## Reading paths

| Who you are | Suggested order |
|---|---|
| First-time engineer | overview → design-philosophy → architecture → milestones → getting-started |
| Want to build it now | getting-started → overview → architecture |
| Want to understand a specific decision | `docs/agent/adr/` |
| Continuing an LLM agent's work | `docs/agent/` |

## Documentation contract

- **Bilingual parity**: this tree (`docs/human/en/`) and the Chinese tree (`docs/human/zh/`) are one-to-one
- **Triple sync**: any public item must exist in zh / en / agent trees simultaneously
- **CI-enforced**: doc-coverage must pass for any PR to merge
- **Style**: lists over prose; mermaid diagrams for any non-trivial flow; examples before abstractions

> This tree targets human engineers who want to understand what Cobrust is. The agent tree (`docs/agent/`) targets LLM agents resuming work mid-task. Different style; do not mix.
