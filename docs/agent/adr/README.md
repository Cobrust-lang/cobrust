---
doc_kind: index
last_verified_commit: 7ab04a4
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
| [0042](0042-snapshot-lint-enforcement.md) | snapshot-lint enforcement — close F1.1 for snapshot schema (pre-commit hook + CI-mode script) | accepted | 2026-05-11 |
| [0043](0043-pyo3-023-upgrade.md) | pyo3 0.22 → 0.23+ workspace upgrade — spike + migration plan | proposed | 2026-05-11 |
| [0044](0044-stdin-argv-source-binding.md) | Source-level stdin + argv binding for Cobrust user programs (W2 LeetCode wedge) — `input()` + `read_line()` + `argv()` via PRELUDE + intrinsic-rewrite + 4 new runtime helpers (W2 Phase 2 scope cap: `read_line() -> str`; typed-Result deferred to ADR-0044a) | accepted | 2026-05-11 |
| [0045](0045-user-traction-milestone-gate.md) | User-traction milestone gate — each release binds to ≥1 user-scenario done-means (systemic F19 closure) | accepted | 2026-05-11 |
| [0046](0046-release-asset-consolidation.md) | release.yml asset consolidation + tier-1 platform contract (codifies F19 prevention) | accepted | 2026-05-11 |
| [0047](0047-leetcode-coverage-strategy.md) | LeetCode coverage strategy — Tier A discovery (100 programs, 10 categories × 10) + B/C ramp decision gate (≥90% SKIP / 70-89% conditional GO / <70% HOLD); evidence-driven ramp policy + IP-boundary discipline | accepted | 2026-05-11 |
| [0047a](0047a-verify-py-mandate.md) | Tier B P7-TEST mandate — verify.py independent oracle for every program (closes F23-A oracle-authoring defect after Tier A 65% rate) | accepted | 2026-05-12 |
| [0048](0048-ai-native-framing-reframe.md) | Cobrust framing reframe — "AI-friendly Python successor with AI-native stdlib in development" (Phase F.2 M-AI.0..M-AI.6 + TD-Recursive-Types Phase 7.5) + v0.2.0-alpha tag binding | accepted | 2026-05-12 |
| [0049](0049-alpha-honesty-and-onboarding-hardening.md) | Alpha honesty and onboarding hardening before external AI-surface exposure | accepted | 2026-05-13 |
| [0050](0050-phase-f3-language-completeness-batch.md) | Phase F.3 — language completeness batch (dict, f64, list[str], break/continue, for) and v0.2.0 stable tag | proposed | 2026-05-16 |
| [0050a](0050a-loop-control-flow.md) | M-F.3.0 — Loop control flow (`break` / `continue`) semantics + contract seal | accepted | 2026-05-16 |
| [0050b](0050b-for-loop-shape.md) | M-F.3.1 — for-loop shape (range-first + list[i64] iter); list[str] gated on Wave 2 | accepted | 2026-05-16 |
| [0050c](0050c-str-ownership.md) | M-F.3.2 prereq — Str ownership flip (TD-1 closure): Option A Full-Drop schedule, explicit `__cobrust_str_clone`, F29 enumeration covers 27 Str / List consumers + `__cobrust_list_is_empty` F5 addendum | accepted | 2026-05-16 |
| [0050d](0050d-dict-design.md) | Dict design — `{k: v}` literal, `d[k]` panic + `.get` safe escape, `indexmap` insertion-order, K∈{i64,str}, NotHashable rejects f64, 7-sub-sprint Wave-3 implementation map | accepted | 2026-05-16 |
| [0050e](0050e-string-stdlib-m-f-3-5.md) | M-F.3.5 string stdlib design — 10 PRELUDE-fn surface (`split` / `join` / `replace` / `trim` / `find` / `contains` / `starts_with` / `ends_with` / `lower` / `upper`) + `clone()` builtin scoped IN to retroactively close LC-100 honest-debt; Option C PRELUDE-now / method-form-Phase-G | accepted | 2026-05-16 |
| [0050f](0050f-file-io-completion-m-f-3-6.md) | M-F.3.6 file IO completion design — 7 PRELUDE-fn surface (`read_file` / `read_file_lines` / `write_file` / `append_file` / `stdin_read_all` / `stdout_write` / `stderr_write`); Option C PRELUDE-now / method-form-Phase-G; i64-sentinel error reporting mirroring ADR-0044 W2 Phase 2 cap | accepted | 2026-05-16 |
| [0051](0051-llm-first-design-principle.md) | LLM-first design principle — constitutional north star: "Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try." 4 Phase G+ priority directions (explicit `&` borrow / F.1.4 error UX / `@py_compat` L2 hard-bind / method-call sugar). Amends CLAUDE.md §2.5 | accepted | 2026-05-16 |
| [0052](0052-phase-g-llm-friendliness-batch.md) | Phase G — LLM-friendliness sprint batch frame: 4 sub-ADRs (0052a explicit borrow / 0052b error UX FIX-text / 0052c `@py_compat` L2 bind / 0052d method-call sugar with method-dispatch infra prereq); two-wave dispatch plan (Direction A solo Wave 1 — predicate-flip; B+C+D parallel Wave 2); §2.5 compliance scoreboard rubric; F30 shadow-flip + F28 P10-direct PAIR + F27 verified-at-HEAD SOPs bound; LC-100 cleanup deferred (not in Phase G scope) | proposed | 2026-05-16 |
| [0052a](0052a-explicit-borrow-let-rebind.md) | Wave 1 Direction A — explicit `&s` borrow / let-rebind shortcut; one-way call-site coercion at `synth_call_args` (NOT bidirectional `Ref↔T` unify — v1+v2 cascade root per §13 design lesson); 12 0052a-prefix residuals classified as TEST-author-pattern-errors per Phase F.3 honest-debt precedent; 0 non-0052a regression vs main HEAD baseline | accepted | 2026-05-16 |
| [0052b](0052b-error-ux-fix-suggestions.md) | Wave 2 Direction B — Error UX rewrite: errors print the FIX. Add `suggestion: Option<&'static str>` to 24 TypeError + 11 MirError variants; CLI renderer reads structured field instead of hard-coded match prose; ~55 construction sites updated; ADR-0052a precedent (`BorrowOfNonPlace` + `UseAfterMove` already ship `&s` suggestion forward-compat) | proposed | 2026-05-17 |
| [0052c](0052c-py-compat-tier-l2-bind.md) | Wave 2 Direction C — `@py_compat` tier hard-bind to L2 verifier; activates ADR-0037; parse `py_compat: String` into `enum PyCompatTier { Strict / Semantic / Numerical { rtol } / None }`; replace `AcceptAll` BehaviorVerifier with `TierVerifier`; tomli + dateutil + msgpack migration plan | proposed | 2026-05-17 |
| [0052d-prereq](0052d-prereq-method-dispatch-infra.md) | Wave 2 Direction D prerequisite — per-type method-dispatch infrastructure: generalize `try_synth_dict_method` (check.rs:920) pattern to Str (10 methods) + List (5) + Float (5) + Int (5) = 25 new method-form entries. Method-form is sugar over PRELUDE-fn (no vtable, statically resolved). Gates 0052d. New `TypeError::UnknownMethod` variant. `&s.method()` parses as `&(s.method())` per ADR-0052 F-G.3 | proposed | 2026-05-17 |
