---
doc_kind: index
last_verified_commit: TBD
---

# Architecture Decision Records

Every decision affecting two or more files is documented here. Adding,
mutating, or superseding an ADR is itself a code change and ships in the
same atomic commit as the change it justifies.

## How to add an ADR

1. Copy `_template.md` to `NNNN-short-slug.md`, picking the next
   available `NNNN` (zero-padded, monotonic).
2. Fill in frontmatter (`adr_id`, `title`, `status`, `date`).
3. Set status to `accepted` only when implementation lands.
4. Update the index below.
5. Commit ADR + implementation + doc-coverage updates atomically.

## Status legend

- `proposed` — under discussion; do not implement yet.
- `accepted` — current truth; implementation matches.
- `superseded` — replaced; see `superseded_by` frontmatter.
- `deprecated` — wound down without replacement.

## Index

| ADR | Title | Status | Date |
|---|---|---|---|
| [0001](0001-license.md) | Apache-2.0 OR MIT dual license | accepted | 2026-04-30 |
| [0002](0002-multi-agent-topology.md) | Multi-agent topology and milestone sequencing for autonomous delivery | accepted | 2026-04-30 |
| [0003](0003-core-30-forms.md) | Cobrust core 30 syntactic forms (M1 frontend scope) | accepted | 2026-04-30 |
| [0004](0004-llm-router-architecture.md) | LLM Router architecture — provider trait, error taxonomy, retry, cache key, ledger schema, consensus tie-breaking | accepted | 2026-04-30 |
