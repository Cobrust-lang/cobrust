---
doc_kind: index
last_verified_commit: ac5636a
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
| [0022](0022-translation-ecosystem-batch.md) | Translation ecosystem batch — cobrust-requests + cobrust-click + L3 closures (dateutil 5/5, msgpack 3/3) | accepted | 2026-04-30 |
| [0021](0021-m7-6-numpy-expansion.md) | M7.6 numpy expansion — Complex dtype widening, FFT + polynomial bindings, reduction extensions | accepted | 2026-04-30 |
| [0020](0020-m8-mir-shape.md) | M8 MIR — node families, terminator taxonomy, drop schedule, borrow-check obligations | accepted | 2026-04-30 |
| [0023](0023-m9-codegen.md) | M9 codegen — backend feature flags, ABI, calling convention, linker delegation, target matrix | accepted | 2026-04-30 |
| [0024](0024-m10-cli-driver.md) | M10 CLI driver — subcommand registry, exit-code scheme, runtime-helper contract for hello-world, package config namespacing | accepted | 2026-04-30 |
| [0025](0025-m11-stdlib-runtime.md) | M11 stdlib + runtime — module surfaces, runtime ABI, drop-schedule fix, codegen amendments, print-intrinsic lift | accepted | 2026-04-30 |
| [0026](0026-m12-package-format.md) | M12 package format — user-crate cobrust.toml schema, lockfile determinism, content-addressed registry, semver resolver, namespace collision (Option C) | accepted | 2026-04-30 |
| [0027](0027-m12-x-codegen-stdlib-amendments.md) | M12.x — codegen + stdlib amendments to lift M11 followups (Aggregate / Ref / Cast / for-protocol / f-string) | accepted | 2026-05-09 |
| [0028](0028-m13-concurrency-runtime.md) | M13 structured-concurrency runtime — tokio binding, JoinHandle/channel/scope/cancel surface, no async/sync coloring | accepted | 2026-04-30 |
| [0029](0029-m14-repl.md) | M14 REPL — interactive shell, directives, multi-line input, evaluation strategy | accepted | 2026-04-30 |
| [0030](0030-m11-1-while-if-codegen-fix.md) | M11.1 — fix while-loop-with-leading-if codegen regression + close audit-#2 (real fizzbuzz / fib) | accepted | 2026-05-09 |
| [0031](0031-audit-5-ledger-provider-kind-field.md) | Audit #5 — bump ledger schema to carry `provider_kind` (anthropic/openai/synthetic) | accepted | 2026-05-09 |
| [0032](0032-audit-1-tomli-real-llm-e2e.md) | Audit #1 — first end-to-end real-LLM translation of `tomli::parse_bool` through L0..L2 with cache discipline (no synthetic, isolated tempdir) | accepted | 2026-05-09 |
| [0033](0033-codegen-float-return-fix.md) | Codegen Ty::None Option C — root-primitive `inferred_locals` + fixed-point; closed Bug A (Float→I8) + Bug B (Conway-toy 4+ block) | accepted | 2026-05-09 |
| [0034](0034-m11-2-fnref-call-lowering.md) | M11.2 — `Constant::FnRef` Call lowering via codegen forward-declaration pass (audit #2 closes from 🟡 PARTIAL to ✅ DONE) | accepted | 2026-05-09 |
| [0035](0035-m11-3-while-condition-lower-primitive.md) | M11.3 — `lower_condition` root primitive shared by `if` + `while` heads (closes review-claude LC 263 P0 + 同 ADR-0033 Option C 精神) | accepted | 2026-05-09 |
| [0036](0036-audit-3a-prompt-design-fix.md) | Audit #3a — production `build_translation_prompt_rich` builder + stateful PASS (§1.2 production-validated) | accepted | 2026-05-09 |
| [0038](0038-phase-f-roadmap.md) | Phase F roadmap — F.1/F.2/F.3 tiers with priority + trigger + done-means + effort matrix; 0.1.0-beta release plan + wedge "AI Python 加速器" | accepted | 2026-05-10 |
| [0037](0037-py-compat-hard-bind.md) | Reserved — py-compat hard-bind to L2 verifier (Phase F.1.x) | proposed | 2026-05-10 |
| [0039](0039-tomli-full-translation-result.md) | T1.1 — tomli full-library real-LLM translation 5/5 PASS production milestone | accepted | 2026-05-10 |
| [0040](0040-honest-gate-verdicts-and-real-llm-wiring.md) | 0.1.0-stable Wave 1A — honest `GateOutcome` verdicts (B2) + production real-LLM router wiring (B1) per claude-desktop integrated handoff §1.B1 + §1.B2 + §10 | accepted | 2026-05-09 |
| [0041](0041-python-semantics-compliance-binding.md) | 0.1.0-stable Wave 2E — Python semantics compliance binding (H1-H8 in one PR per claude-desktop integrated handoff §2 + §10): floor `%`, short-circuit `and`/`or`, honest `UnimplementedBinOp` for `**`/`@`/`in`/`not in`, walrus reject, closure capture walker, comprehension MIR desugar, multi-base class reject, tuple-index constant-fold | accepted | 2026-05-09 |
