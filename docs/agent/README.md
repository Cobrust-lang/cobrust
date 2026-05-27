---
doc_kind: index
last_verified_commit: d035d9d
---

# Cobrust Agent Documentation

Audience: LLM agents resuming work on Cobrust mid-task with no prior
context. This tree is dense, deterministic, and stable-id-cross-referenced.
Human-facing prose lives in `docs/human/`; do not duplicate.

## Tree map

- `conventions.md` — required reading; format and citation rules
- `modules/` — per-module specifications (one file per workspace crate)
- `adr/` — architecture decision records
- `findings/` — negative results and dead ends

## Reading order for a fresh agent

1. `/CLAUDE.md` — the project constitution (repo root)
2. `conventions.md` — how to read and write these docs
3. `modules/<your_target>.md` — the module you are touching
4. `adr/` — search by stable ID for decisions relevant to your task

## Stable IDs

All cross-references use stable IDs, never page positions:

- Modules: `mod:<name>` (e.g. `mod:llm_router`)
- ADRs: `adr:NNNN` (e.g. `adr:0001`)
- Findings: `find:<slug>`

## Module index

| Stable ID | File | Crate | Lands at |
|---|---|---|---|
| `mod:cli` | [`modules/cli.md`](modules/cli.md) | `cobrust-cli` | M0 stub → M1 |
| `mod:frontend` | [`modules/frontend.md`](modules/frontend.md) | `cobrust-frontend` | M1 |
| `mod:hir` | [`modules/hir.md`](modules/hir.md) | `cobrust-hir` | M2 |
| `mod:types` | [`modules/types.md`](modules/types.md) | `cobrust-types` | M2 |
| `mod:mir` | [`modules/mir.md`](modules/mir.md) | `cobrust-mir` | M3+ |
| `mod:codegen` | [`modules/codegen.md`](modules/codegen.md) | `cobrust-codegen` | M3+ |
| `mod:llm_router` | [`modules/llm-router.md`](modules/llm-router.md) | `cobrust-llm-router` | M3 |
| `mod:translator` | [`modules/translator.md`](modules/translator.md) | `cobrust-translator` | M4+ |
| `mod:nest` | [`modules/nest.md`](modules/nest.md) | `cobrust-nest` (translated tomli) | M4 |
| `mod:molt` | [`modules/molt.md`](modules/molt.md) | `cobrust-molt` (translated dateutil) | M5 |
| `mod:scale` | [`modules/scale.md`](modules/scale.md) | `cobrust-scale` (translated msgpack) | M6 |
| `mod:coil` | [`modules/coil.md`](modules/coil.md) | `cobrust-coil` (translated numpy) | M7.0 |
| `mod:strike` | [`modules/strike.md`](modules/strike.md) | `cobrust-strike` (translated requests) | M-batch |
| `mod:den` | [`modules/den.md`](modules/den.md) | `cobrust-den` (translated sqlite3) | v0.7.0 |
| `mod:hood` | [`modules/hood.md`](modules/hood.md) | `cobrust-hood` (translated click) | M-batch |

## ADR index

See [`adr/README.md`](adr/README.md).

## Findings index

See [`findings/README.md`](findings/README.md).
