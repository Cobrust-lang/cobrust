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
| [0005](0005-hir-shape.md) | HIR shape and AST→HIR lowering tables for the static core | accepted | 2026-04-30 |
| [0006](0006-type-system.md) | Type system shape, inference algorithm, and proof obligations for the static core | accepted | 2026-04-30 |
| [0007](0007-translator-pipeline.md) | Translator pipeline — L0 spec, L1 translation, provenance manifest, synthetic-LLM mode, PyO3 wrapper | accepted | 2026-04-30 |
| [0008](0008-l2-perf-and-repair-loop.md) | L2.perf benchmark harness, repair loop, and L2/L3 escalation pipeline | accepted | 2026-04-30 |
| [0009](0009-downstream-validation.md) | L3 downstream-dependents validation — corpus, scope, and partial coverage policy | accepted | 2026-04-30 |
| [0010](0010-native-ext-translation.md) | Native-extension translation methodology — msgpack-python, Cython sources, perf threshold relaxation, perf-gate fail-on-threshold-miss routing, downstream widening | accepted | 2026-04-30 |
| [0011](0011-pyo3-build-path.md) | PyO3 build path for translated crates — `--features pyo3`, cdylib emission, dual-mode test harness | accepted | 2026-04-30 |
| [0012](0012-m7-numpy-plan.md) | M7 numpy core — sub-milestone plan and backend strategy | accepted | 2026-04-30 |
| [0013](0013-m7-0-ndarray-foundation.md) | M7.0 ndarray foundation — crate layout, dtype tier, ndarray backend pin, ownership model, differential strategy | accepted | 2026-04-30 |
| [0014](0014-m7-1-ufuncs-broadcasting.md) | M7.1 universal functions, broadcasting, type promotion — dispatch model + numpy-compat semantics | accepted | 2026-04-30 |
| [0015](0015-m7-2-indexing.md) | M7.2 indexing — view/copy taxonomy, ArrayView ownership, IndexError, np.where | accepted | 2026-04-30 |
| [0016](0016-m7-3-reductions.md) | M7.3 reductions — kind taxonomy, axis semantics, pairwise summation, ddof, empty-array behavior | accepted | 2026-04-30 |
| [0017](0017-m7-4-linalg.md) | M7.4 linalg subset — ops surface, backend strategy, error semantics, rtol gate | accepted | 2026-04-30 |
| [0018](0018-m7-5-random.md) | M7.5 random — Generator type, PCG64 backend, seed semantics, distribution surface, KS-test acceptance gate | accepted | 2026-04-30 |
| [0019](0019-phase-e-language-runtime-roadmap.md) | Phase E — Language + runtime roadmap (M8..M14) to "usable for most projects" | accepted | 2026-05-08 |
| [0020](0020-m8-mir-shape.md) | M8 MIR — node families, terminator taxonomy, drop schedule, borrow-check obligations | accepted | 2026-04-30 |
