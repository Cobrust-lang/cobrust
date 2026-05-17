---
doc_kind: adr
adr_id: 0052
title: "Phase G — LLM-friendliness sprint batch frame (explicit borrow / error UX rewrite / @py_compat L2 bind / method-call sugar)"
status: proposed
date: 2026-05-16
last_verified_commit: 7ab04a4
supersedes: []
superseded_by: []
relates_to: [adr:0037, adr:0050, adr:0050a, adr:0050b, adr:0050c, adr:0050d, adr:0050e, adr:0050f, adr:0051]
discovered_by: P10/user 2026-05-16 — codification of CLAUDE.md §2.5 into Phase G operational batch
parent_adr: 0051
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; sub-ADRs 0052a..0052d ratify on dispatch
---

# ADR-0052: Phase G — LLM-friendliness sprint batch frame

## Context

### Strategic frame (2026-05-16)

Phase F.3 closed at `aa74063` (v0.2.0 release notes) + `7ab04a4` (release prep + version bump to 0.2.0). Five P0 language features (break/continue, for-loop, f64, list[str], dict) plus two P1 stdlib surfaces (string stdlib, file IO) shipped under the §"v0.2.0 stable tag binding" defined in ADR-0050. ADR-0051 then codified `CLAUDE.md §2.5` as the constitutional north star: **"Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try."**

Phase G operationalizes §2.5 as a batch sprint. The four priority directions named in §2.5 + ADR-0051 §"Decision" §3 — explicit `&` borrow, F.1.4 Error UX rewrite, `@py_compat` L2 hard-bind, method-call sugar — become four sub-ADRs (0052a..d) dispatched in two parallelizable waves.

This ADR is the **batch frame** — same structural role ADR-0050 played for Phase F.3. It does NOT decide individual designs; sub-ADRs 0052a..d own those at dispatch time. It DOES decide: scope, prerequisites, wave ordering, audit gates, §2.5 compliance rubrics, and the verified-at-HEAD scaffolding map each sub-ADR will modify.

### Constitutional citation (§2.5 verbatim)

Quoted verbatim from `CLAUDE.md` §2.5 (HEAD `7ab04a4` lines 67-87):

> **Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try.**
>
> This sentence binds every design trade-off. When a choice pits "elegance for humans" against "the LLM gets it right ex ante", the latter wins.
>
> Two operational selection rules:
>
> - **Compile-time-catch-errors**: prefer designs that surface bugs at type-check / borrow-check / parse time over designs that defer to runtime. The LLM's compile-error feedback loop is its strongest correction signal. Every `TypeError::*` variant + every `MirError::*` variant is a successful catch.
> - **Maximize-overlap-with-training-data**: prefer syntax + semantics that occur frequently in Python + Rust training corpora. LLMs write correctly when the surface matches their priors.

The four priority directions Phase G operationalizes (cited from `CLAUDE.md` §2.5 lines 78-83):

- **A. Explicit `&` borrow / let-rebind shortcut**: eliminates `clone()` clutter; the LARGEST current LLM-friendliness deficit per LC-100 honest-debt empirical baseline. Phase G P0.
- **B. F.1.4 Error UX rewrite**: error messages MUST print the FIX, not just the diagnosis.
- **C. `@py_compat` tier hard-bind to L2 verifier** (ADR-0037 activation).
- **D. Method-call sugar priority**: `s.split(",")` over `split(s, ",")`. Closer to LLM training data distribution.

### Phase F.3 closure baseline

Phase F.3 delivered the language-half soundness baseline (M-F.3.0..M-F.3.6, ADR-0050a..f all `accepted`). Two structural debts carry into Phase G as motivation:

1. **LC-100 honest-debt** — `findings/lc100-str-use-after-move-regression-from-adr0050c.md` Path D + the long-term-deferral addendum: Str=non-Copy uniformly (ADR-0050c Option A) produces `clone(s)` clutter every multi-use Str pattern. P10/user disposition is "leave LC-100 alone until the language matures"; Direction A is the load-bearing fix for the underlying ergonomic.
2. **`clone()` proliferation across M-F.3.5 surfaces** — every M-F.3.5 PRELUDE call documented in ADR-0050e §"Decision 3" Table consumes Str args. Idiomatic programs (ADR-0050e example at L344-354) string `clone(s)` calls before each non-final PRELUDE call. This is the empirical baseline showing the §2.5 "training-data overlap" cost: Python source code rarely has `clone(s)` calls; LLM agents emit Python-shaped code that hits `UseAfterMove` on first compile.

### Phase F.3 vs Phase G framing

| Axis | Phase F.3 (closed) | Phase G (this ADR) |
|---|---|---|
| Mandate | §1.1 language-half completeness — "the language being a language" | §2.5 LLM-friendliness — "the language LLM agents write correctly on the first try" |
| Audience | external user trial of v0.2.0 | the LLM agent emitting v0.2.0+ source |
| Surface posture | breadth (5 P0 features) | depth (4 P0 ergonomic + verification axes) |
| Trade-off rule | language-half soundness > wedge cosmetics (LC-100 deferral) | LLM-friendliness > human-elegance instinct (§2.5 binding) |
| Audit rubric | ADR-0050 §"Audit model" + ADSD F27 verified-at-HEAD | ADR-0050 §"Audit model" + §2.5 compliance check per priority |

## Options considered

### Option A — sequential Phase G (one sub-ADR at a time)

- **Pros**: lowest peak Opus token burn; clean merge order; minimum risk of cross-sub-ADR interaction bugs.
- **Cons**: ~8-10 week wall time; Directions A (predicate flip) + D (method dispatch) are independent and can run in parallel; underutilizes DG workstation.
- **Rejected.** Per `feedback_autonomous_self_drive.md`, default CTO mode is parallel-when-independent.

### Option B — single-mega-sprint (all four sub-ADRs as one batch)

- **Pros**: maximal atomicity; one merge.
- **Cons**: Direction A (predicate flip) requires F30 shadow-flip dry-run before §"Consequences" enumeration finalization; cannot share a single sprint with Directions B/C/D which are non-predicate-flip work. Mixing predicate-flip + non-predicate-flip in one sprint defeats the F30 SOP.
- **Rejected.**

### Option C — two-wave parallel dispatch (CHOSEN)

- **Pros**: respects the F30 predicate-flip vs non-predicate-flip boundary (Direction A in Wave 1 with shadow-flip; Directions B/C/D in Wave 2 parallel). Wave 1 closes the load-bearing ergonomic fix first (Direction A unblocks LC-100 + M-F.3.5 `clone()` clutter retroactively). Wave 2's three sub-ADRs are independent of each other.
- **Cons**: 4 sub-sprints + Method-dispatch infra ADR (Direction D prereq) is a 5-deliverable batch; Opus budget heavier than ADR-0048's single-batch or ADR-0050's 3-wave.
- **Chosen.** Direction A's Wave 1 buys retroactive cleanup for LC-100 + M-F.3.5; Wave 2 then ships ergonomic surface + verification depth in parallel.

### Option D — defer Direction C (`@py_compat` L2 bind) to Phase H

- **Pros**: smaller Phase G; ADR-0037 has been `proposed reserved` for ~6 weeks without urgency.
- **Cons**: §2.5 §C explicitly names this as a Phase G priority; deferral re-derives the prioritization. ADR-0037's "L2.behavior gate is not enforced" gap blocks the LLM Router from routing on tier (consensus-mode for `strict` tier; cheaper single-model for `numerical`). Direction C is the verification-side LLM friendliness fix.
- **Rejected.**

## Decision

Adopt **Option C** — two-wave parallel dispatch on `feature/g-*` branches off `main`, integrated wave-by-wave via `git merge --no-ff` after independent 5-gate verification per integrated `main`.

### Wave structure

| Wave | Sub-ADRs dispatched in parallel | Duration | DG-load |
|---|---|---|---|
| **Wave 1** | **0052a** explicit `&` borrow / let-rebind (Direction A) — design-then-impl with F30 shadow-flip | ~5-7 days | 1 P9-Opus design spike + 1 P10-direct PAIR (TEST opus + DEV opus) for impl |
| **Wave 2** | **0052b** Error UX rewrite (Direction B) · **0052c** `@py_compat` L2 hard-bind (Direction C) · **0052d** method-call sugar (Direction D, prereqed by Method-dispatch infra ADR) | ~7-10 days | 3 P9 spikes + 3 P10-direct PAIRs concurrent + 1 prereq ADR (Method-dispatch infra) |

Direction D's Method-dispatch infra ADR (working slot: **0052d-prereq**) is a Wave-2 design-only spike (P9-Opus solo, doc-only) that MUST land **before** sub-ADR 0052d. Per ADR-0050e §"Option A" L180-184: "Needs method-dispatch infrastructure that does not yet exist. `crates/cobrust-mir/src/lower.rs` has zero method-call support". Direction D's actual scope is gated on resolving the dispatch-table architecture (per-type method tables vs name-mangled global resolver) which is itself a sub-decision worth a doc-only spike.

### Sub-ADR roster

- **ADR-0052a — Explicit borrow / let-rebind shortcut** (Wave 1 P0 — Direction A). Design + impl. F30 shadow-flip dry-run mandatory. Owner: P9 opus design spike → P10-direct PAIR impl (DEV opus + TEST opus parallel).
- **ADR-0052b — F.1.4 Error UX rewrite — suggestion-bearing diagnostics** (Wave 2 P0 — Direction B). Design + impl. ANSI rendering + `suggestion` field on every `TypeError::*` variant + every `MirError::*` variant. Owner: P9 sonnet design (well-scoped per existing `error_ux.rs` scaffolding) → P10-direct PAIR impl (DEV sonnet + TEST sonnet).
- **ADR-0052c — `@py_compat` tier hard-bind to L2.behavior verifier** (Wave 2 P1 — Direction C). Design + impl. Activates ADR-0037 from `proposed reserved` to `accepted`. Tier-aware acceptance threshold in `BehaviorVerifier`. Owner: P9 opus design (touches translator pipeline + router) → P10-direct PAIR impl (DEV opus + TEST opus).
- **ADR-0052d — Method-call sugar PRELUDE-form alias** (Wave 2 P0 — Direction D). Design + impl. Prereqed by Method-dispatch infra ADR (Wave-2 first). Owner: P9 opus design (HIR + types method-dispatch resolver) → P10-direct PAIR impl (DEV opus + TEST opus).

### Sub-ADR prerequisites

```
ADR-0052a (Wave 1) ──┐
                     ├─→ retroactively unblocks LC-100 + M-F.3.5 clone() clutter; Wave 2 sub-ADRs land on top
0052d-prereq ────────┴─→ 0052d method-call sugar (Wave 2)
ADR-0037 (reserved) ─→ 0052c py_compat L2 hard-bind (Wave 2; this ADR sub-classes ADR-0037)
                       (independent) ─→ 0052b error UX (Wave 2)
```

## §2.5 compliance check per priority

Per ADR-0051 §"Consequences" L82 audit-teammate rubric, every sub-ADR ratifies under the two operational selection rules. This frame ADR pre-applies the rubric to lock the §2.5 binding ex-ante.

### Direction A — Explicit borrow / let-rebind

- **Compile-time-catch gained**: today `let n = str_len(s); let c = str_at(s, i)` produces `MirError::UseAfterMove { local }` at `crates/cobrust-mir/src/borrow.rs:114`. Under explicit borrow form (`&s`), the use-after-move catch becomes a *first-class signal* the LLM can decode: stderr says "use `&s` not `s` for read-only borrow", LLM retries with `&s`, compiler accepts. Today the catch fires but the fix path is `clone(s)` (heap allocation) — wrong signal for the LLM.
- **Training-data overlap matched**: Rust `&str` is one of the most-common-in-training tokens; Python doesn't have it but the LLM has Rust priors that fire under §2.5 §B's "Rust + Python training corpora". Reference idiom: Rust `fn f(s: &str)` vs Cobrust today `fn f(s: str)`. The `&` glyph reads as borrow in Rust priors; Phase F.3's M-F.3.5 surface lacks it.
- **Reference idioms**: Rust `let n = s.len(); let c = s.chars().nth(i)` (works without clone). Python `n = len(s); c = s[i]` (works because Python is reference-semantics for str). Cobrust today: `let n = str_len(clone(s)); let c = str_at(s, i)` (clone burden). After Direction A: `let n = str_len(&s); let c = str_at(&s, i)`.

### Direction B — Error UX rewrite (suggestion-bearing diagnostics)

- **Compile-time-catch gained**: `TypeError::ImplicitTruthiness` at `crates/cobrust-types/src/error.rs:64` already fires on `if x:` where `x: Int`. The catch is real. Today's user-facing diagnostic at `crates/cobrust-cli/src/error_ux.rs:620-631` already produces a suggestion ("change to 'if x != 0:' or 'if x.is_some():'") — BUT the suggestion is hard-coded English prose, not a structured field on the `TypeError::ImplicitTruthiness` variant. LLM consuming stderr cannot reliably parse English suggestions across all 30+ variants. Direction B's gain: every error variant gets a structured `suggestion: Option<String>` field at the error-construction site, not the rendering site — making the suggestion auditable + testable + LLM-machine-parseable.
- **Training-data overlap matched**: Rust compiler's `help:` notes are in every Rust training corpus. Reference: Rust `error[E0308]: mismatched types ... help: consider using \`.to_string()\``. The "help:" line is canonical Rust diagnostic shape. Direction B's ANSI rendering matches.
- **Reference idioms**: Rust diagnostic JSON output (`--error-format=json`) emits structured `children` with `level: "help"` per primary span. Direction B mirrors this for Cobrust's `cobrust-cli` JSON output (post-Phase G LSP work consumes this).

### Direction C — `@py_compat` L2 hard-bind

- **Compile-time-catch gained**: today `BehaviorVerifier::AcceptAll` at `crates/cobrust-translator/src/pipeline.rs:278-288` returns `Skip { reason: "AcceptAll — no L2.behavior gate wired" }`. Under Direction C, a tier-aware `BehaviorVerifier` returns `Fail { reason: "py_compat=strict requires byte-identical oracle match; observed divergence at exemplar N" }`. This is a runtime catch at L2 verification time, but it converts to a *compile-time catch from the translator's POV*: the L2.behavior gate is the LLM Router's compile-error feedback signal for translated functions. Without it, translations that fail strict tier silently pass; LLM router has no signal to re-translate.
- **Training-data overlap matched**: Python `numpy.testing.assert_allclose(rtol=1e-7)` is in every numerical-Python training corpus; Python `unittest.assertEqual` is in every Python training corpus. Direction C's tier-aware threshold matches these idioms verbatim (`strict` → `assertEqual`, `numerical(rtol=1e-7)` → `assert_allclose(rtol=1e-7)`).
- **Reference idioms**: `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs:799` already classifies divergence under tier taxonomy — the infrastructure exists; Direction C wires it into the gate decision.

### Direction D — Method-call sugar

- **Compile-time-catch gained**: today `s.split(",")` parses as `Call { callee: Access(Attribute { base: s, name: "split" }), args: [","] }` at `crates/cobrust-frontend/src/parser.rs:1156-1167`. The HIR lowering at `crates/cobrust-hir/src/lower.rs:1078-1083` produces `ExprKind::Attr { base, name }`. The type checker at `crates/cobrust-types/src/check.rs:780-787` returns a fresh inference variable for `Attr` — i.e. accepts any attribute name on any base type, defers resolution to inference. This is *the opposite of §2.5 compile-time-catch*: the language accepts a malformed program. Direction D's gain: method resolution against a per-type method table (mirroring `try_synth_dict_method` at `check.rs:907-915`) makes `s.bogus_method(",")` an immediate `TypeError::UnknownMethod { base_ty: Str, name: "bogus_method", span }` — compile-time catch.
- **Training-data overlap matched**: Python `s.split(",")` is in every Python training corpus. Rust `s.split(",")` is in every Rust training corpus. Cobrust today `split(s, ",")` is in neither corpus — it's a Cobrust-original surface inherited from W2 PRELUDE-fn-form per ADR-0050e §"Decision 1". Direction D unifies the surface with both training distributions.
- **Reference idioms**: Python `"hello,world".split(",")`. Rust `"hello,world".split(",").collect::<Vec<_>>()`. Cobrust after Direction D: `"hello,world".split(",")` (alias to `split(s, ",")` PRELUDE form).

## Verified-at-HEAD scaffolding map (F27 SOP)

Per `findings/adr-scope-reality-divergence.md` F27 verified-at-HEAD discipline, each sub-ADR cites file:line anchors at HEAD `7ab04a4` that the sub-ADR will modify. Sub-ADRs spike-time re-verifies; this frame ADR pre-grounds the design surface so sub-ADR dispatch prompts can use it as required reads.

### Sub-ADR 0052a (Direction A) scaffolding anchors

- **The predicate**: `crates/cobrust-mir/src/lower.rs:2147-2188` `fn is_copy_type(ty: &Ty) -> bool`. This is the F30 predicate. Today Str is non-Copy uniformly; List + Dict are Copy@operand non-Copy@drop. Direction A introduces an explicit-borrow form (`&s` / `let s = &s` rebinding) that suppresses the Move-on-read predicate WITHOUT changing the type's Copy-ness.
- **The use-after-move catch fire site**: `crates/cobrust-mir/src/borrow.rs:114` `MirError::UseAfterMove`. Direction A keeps this fire site live for non-borrowed reads; borrowed reads emit `Operand::Copy` instead of `Operand::Move` so the catch does not fire.
- **The C-ABI clone shim Direction A retires from idiomatic programs**: `crates/cobrust-stdlib/src/fmt.rs:306` `__cobrust_str_clone`. The shim stays (still emitted by `Aggregate` lowering per `borrow.rs:120-129` Phase 4 note) but explicit `&s` programs avoid it.
- **F30 shadow-flip target**: feature flag `cobrust_borrow_phase_g` gating the parser+HIR+types+MIR diff. ADR-0052a dispatch prompt MUST require shadow-flip dry-run per `findings/predicate-flip-cascade-discovery-deficit.md` L45-51 SOP.

### Sub-ADR 0052b (Direction B) scaffolding anchors

- **Error variant definition site**: `crates/cobrust-types/src/error.rs:64` `TypeError::ImplicitTruthiness { actual: Ty, span: Span }`. Direction B adds a `suggestion: Option<&'static str>` field at construction time (NOT rendering time). Each of the ~30 `TypeError::*` variants + ~10 `MirError::*` variants similarly extended.
- **Current rendering site**: `crates/cobrust-cli/src/error_ux.rs:620-631` produces the English suggestion via hard-coded match. Direction B's rendering site shifts to reading the structured `suggestion` field, eliminating the hard-coded prose duplication.
- **Construction site (one of many)**: `crates/cobrust-types/src/check.rs:1532` `Err(TypeError::ImplicitTruthiness { actual, span })` (per Wave 2 audit F2 amendment 2026-05-17; original anchor `check.rs:1473` was the parent helper region). Direction B updates every construction call site to populate `suggestion: Some("change to 'if x != 0:' or 'if x.is_some():'")`.

### Sub-ADR 0052c (Direction C) scaffolding anchors

- **The skip-by-default verifier**: `crates/cobrust-translator/src/pipeline.rs:278-288` `impl BehaviorVerifier for AcceptAll`. Direction C either replaces `AcceptAll` with a tier-aware `TierVerifier` OR adds `TierVerifier` as a sibling and makes the default opt-in via `cobrust.toml`.
- **The tier field**: `crates/cobrust-translator/src/spec.rs:48` `pub py_compat: String` in `FunctionSpec`. Today serialized as `"strict"` / `"numerical(rtol=1e-7)"` / `"semantic"` / `"none"`. Direction C parses this into a structured `enum PyCompatTier { Strict, Semantic, Numerical { rtol: f64 }, None }`.
- **The router prompt-context handoff**: `crates/cobrust-translator/src/translate.rs:349-350` already passes `tier` into the L1 prompt. Direction C does NOT change this; it adds the matching gate-side enforcement on the verdict.
- **The empirical precedent**: `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs:799` `Classify a single case's divergence under the @py_compat tier`. Direction C lifts this audit-test logic into production `TierVerifier`.

### Sub-ADR 0052d (Direction D) scaffolding anchors

- **The parser surface (already shipped)**: `crates/cobrust-frontend/src/parser.rs:1239-1249` parses `s.method(args)` as `Call { callee: Access(AccessKind::Attribute { base: s, name: "method" }), args }` (per Wave 2 audit F1 amendment 2026-05-17; original anchor `parser.rs:1156-1167` was the lambda body parser region). No parser change needed.
- **The HIR lowering (already shipped)**: `crates/cobrust-hir/src/lower.rs:1078-1083` produces `ExprKind::Attr { base, name }`. No HIR change needed.
- **The type-check resolver gap**: `crates/cobrust-types/src/check.rs:780-787` `ExprKind::Attr { base, name }` returns `self.fresh_var()` — accepts any attribute. Direction D adds method-name lookup against a per-type method table HERE.
- **The dict-method precedent**: `crates/cobrust-types/src/check.rs:907-915` `fn try_synth_dict_method` shows the per-type method resolver shape. Direction D generalizes this from `Ty::Dict` to `Ty::Str`, `Ty::List(_)`, `Ty::Float`, `Ty::Int` and reroutes the existing PRELUDE-fn intrinsics through it.
- **The intrinsic-rewrite anchor**: `crates/cobrust-cli/src/build/intrinsics.rs:1011` `fn kind_for_name(name: &str) -> Option<Kind>` and the `Kind` enum at line `941`. Direction D's MIR-level lowering rewrites method-form `s.split(",")` → `__cobrust_str_split(s, ",")` symbol — same C-ABI as PRELUDE-fn form. No codegen change.

## Dispatch wave plan (F28 P10-direct PAIR pattern)

Per `findings/adsd-pair-pattern-impl-gap.md` F28 binding (Wave 2 + Wave 3 of Phase F.3 confirmed empirically) and ADR-0050 §A7, single-layer Claude Code sub-agent architecture requires P10-direct dispatch of TEST + DEV agents for impl sprints. Phase G follows the same shape:

### Wave 1 — Direction A only

| Phase | Pattern | Duration |
|---|---|---|
| 0052a design spike | P9 opus solo (doc-only); includes F30 shadow-flip dry-run + §"Consequences" enumeration | ~2-3 days |
| 0052a impl | P10-direct PAIR (TEST opus + DEV opus parallel); branch `feature/g-explicit-borrow` | ~3-4 days |
| Merge + audit | P10 merge; audit-teammate run per §"Audit gates" below | ~half day |

### Wave 2 — Directions B + C + D parallel

| Phase | Pattern | Duration |
|---|---|---|
| 0052d-prereq Method-dispatch infra ADR | P9 opus solo (doc-only); enumerates per-type method-table architecture | ~1-2 days (must complete before 0052d impl can start; can run in parallel with 0052b + 0052c design) |
| 0052b design + impl | P9 sonnet design (well-scoped per error_ux.rs scaffolding) + P10-direct PAIR (TEST sonnet + DEV sonnet); branch `feature/g-error-ux` | ~3-4 days |
| 0052c design + impl | P9 opus design (translator-side, touches pipeline + router contract) + P10-direct PAIR (TEST opus + DEV opus); branch `feature/g-py-compat-l2` | ~5-7 days |
| 0052d design + impl | P9 opus design (per-type method-dispatch resolver) + P10-direct PAIR (TEST opus + DEV opus); branch `feature/g-method-call-sugar`; blocks on 0052d-prereq | ~5-7 days |
| Merge + audit | P10 merges Wave 2 in dependency order (0052b → 0052c → 0052d); single audit-teammate spawn post-Wave-2 | ~half day |

**Total Phase G ≈ 3-4 weeks**, in line with ADR-0050 Phase F.3 revised wave timing (A2 amendment).

### Host routing (per `feedback_heavy_build_offload_to_workstation.md`)

| Sprint | Host | Mode |
|---|---|---|
| 0052a design (doc-only + shadow-flip dry-run) | Mac local | direct (shadow-flip cargo test runs on DG) |
| 0052a impl | DG primary | Mode C |
| 0052d-prereq Method-dispatch infra ADR | Mac local | direct |
| 0052b design + impl | Mac local then DG verify | Mode C |
| 0052c design + impl | DG primary | Mode C |
| 0052d design + impl | DG primary | Mode C |

Every `cargo build --workspace` + `cargo test --workspace` invocation runs on DG per heavy-build offload binding policy. Mac local stays for doc-only spikes + targeted single-crate verification.

## Audit gates

### Per-Wave audit (post-merge teammate spawn)

Per ADR-0050 §"Audit model — teammate-in-session", each wave's audit-teammate spawn at merge time receives the following compliance check matrix in addition to ADSD F-catalogue + constitution §2.2 + §5:

| Compliance rubric | Source | Pass criterion |
|---|---|---|
| **§2.5 compile-time-catch rule** | `CLAUDE.md §2.5` line 75 | Each sub-ADR enumerates 1+ compile-time catch gained; audit verifies the catch fires on a deliberately-malformed test input |
| **§2.5 training-data-overlap rule** | `CLAUDE.md §2.5` line 76 | Each sub-ADR cites 1+ Rust + Python idiom whose surface the sub-ADR matches; audit verifies the idiom by Cobrust-source `examples/` snippet |
| **F27 verified-at-HEAD** | `findings/adr-scope-reality-divergence.md` | Every `file:line` anchor in the sub-ADR confirmed by grep at the sub-ADR's `last_verified_commit` |
| **F30 shadow-flip dry-run** (Direction A only) | `findings/predicate-flip-cascade-discovery-deficit.md` L45-51 | Direction A sub-ADR ratification includes a shadow-flip artifact: cargo test diff under feature flag with latent-consumer enumeration |
| **F28 P10-direct PAIR** | `findings/adsd-pair-pattern-impl-gap.md` | Each impl sprint commit history shows TEST + DEV parallel dispatch by P10 (not P9 nested) |

### Post-Phase-G integrated audit

After Wave 2 merges, a final audit-teammate spawn evaluates Phase G holistically against:

1. **§2.5 compliance scoreboard** — did all four sub-ADRs respect both operational selection rules?
2. **Cross-sub-ADR interaction** — does Direction A's `&s` + Direction D's `s.method()` combine cleanly (`(&s).split(",")` parses + type-checks correctly)?
3. **Phase F.3 retro-cleanup** — does Direction A close the LC-100 honest-debt receipt at `findings/lc100-str-use-after-move-regression-from-adr0050c.md` or does the receipt remain open with a Phase H pointer?
4. **v0.3.0 readiness** — does Phase G's net delta justify a v0.3.0 tag, or does the package stay at v0.2.x patch level?

## Out of scope (for v0.3.0)

The following are explicit non-goals for Phase G; Phase H+ takes them up.

1. **LC-100 long-term deferral disposition reaffirmed**. Per `findings/lc100-str-use-after-move-regression-from-adr0050c.md` §"Long-term deferral — addendum 2026-05-16 (P10/user re-disposition)", LC-100 corpus stays as indefinite long-term tech debt. Direction A's explicit-borrow form *enables* a future LC-100 rewrite but does NOT trigger one. The Phase G batch does not retroactively edit LC-100 programs; users + Phase H+ sprint maintainers can opt in.
2. **Numerical tier (M7+ numpy core)**. Constitution §"Milestones" reserves M7+ for `numpy`. Direction C's `@py_compat` L2 hard-bind covers `numerical(rtol=…)` *thresholding* but does NOT ship numpy translation. M7+ remains gated on M6 (first native-extension library), which itself follows v0.3.0.
3. **Phase 7.5 recursive struct types**. ADR-0050 §A3 already deferred Phase 7.5 out of v0.2.0; this remains Phase H+ scope per ADR-0050 §A3 reasoning (dict-keyed indirection covers F24 user-facing ergonomic gap).
4. **REPL + LSP + LLVM backend swap**. Per ADR-0050 §M-F.3.8 + §M-F.3.9, these are Phase F.5+ scope. Phase G's error-UX work (Direction B) is forward-compatible with LSP consumption but does NOT ship LSP.
5. **Full Unicode case-folding / grapheme indexing**. ADR-0050e §"Decision 3" notes 9/10 ASCII-fast-path; full Unicode is Phase G+ deferred but NOT in Phase G's binding scope.
6. **Method-form retrofit on PRELUDE-fn surface**. ADR-0050e §"Option C" recommended both forms coexist (PRELUDE-fn stays as alias; method-form is sugar). Direction D ratifies this — the existing PRELUDE-fn form `split(s, ",")` does NOT get deprecated. Future Phase H+ may revisit but Phase G does not.
7. **Self-hosting (constitution §4.4 post-M5 deferral)**. Phase G ships ergonomic + verification depth in Rust-implemented compiler; self-hosted compiler stages remain post-M5 / post-M11 deferred per constitution §4.4. **F-G.2 amendment.**

## Consequences

### Positive

- §2.5 LLM-friendliness binding becomes operational reality, not just constitutional aspiration. Four load-bearing ergonomic + verification axes ship in one batch.
- Direction A retroactively cleans up M-F.3.5 `clone()` clutter (ADR-0050e §"Decision 3" example) without re-editing the PRELUDE surface. Users opt into `&s` ergonomics; old programs still compile.
- Direction B converts the unstructured English suggestion prose to a structured `suggestion` field, making error messages JSON-serializable for future LSP + LLM router stderr-consumption.
- Direction C activates ADR-0037 from `proposed reserved` to `accepted`, closing a 6-week-old reserved slot and unblocking the LLM Router's tier-based routing (consensus-mode for strict; cheap single-model for numerical).
- Direction D unifies the surface with both Python + Rust training-data distributions per §2.5 §B rule, eliminating Cobrust-original PRELUDE-fn-form-only friction.
- Phase G batch sets the precedent that constitutional pillars (§2.5) drive batch sprints, not external user feedback alone. Phase H+ inherits this pattern.

### Negative

- Four sub-sprints + 1 prereq ADR = 5 deliverables; heaviest Opus budget batch yet (vs ADR-0048's 9-surface but single-sprint; ADR-0050's 3-wave but mostly verified-at-HEAD scaffolding).
- Direction A's predicate flip risks F30 latent-consumer cascade despite shadow-flip SOP; impl wall-time uncertainty is the largest of the four sub-ADRs. The shadow-flip itself buys ~10x payback per `findings/predicate-flip-cascade-discovery-deficit.md` L51 but the upfront cost is real.
- Direction D's method-dispatch infra ADR (0052d-prereq) is a new architectural surface — per-type method tables. The architecture decision can produce `BLOCK-WITH-FINDINGS` at design-time audit, slipping Wave 2 dispatch.
- Direction C's tier-aware verifier may surface latent translator-pipeline bugs (today's `AcceptAll` masks real divergences in the translator test corpus). Direction C closure may require recovering 1-2 translator regressions surfaced by the tighter gate.
- v0.3.0 tag binding is implicit (Phase G closure → version bump); explicit tag binding criteria lives in the eventual v0.3.0 release-readiness sprint, not this ADR.

### Neutral / unknown

- ~~Whether Direction A's explicit-borrow form should be `&s` (Rust-glyph) or `ref s` (named keyword).~~ **F-G.1 amendment (post-audit 2026-05-16)**: Pre-committed to `&s` per §2.5 maximize-training-data-overlap rule (Rust `&str` is canonical Rust-corpus surface; cited Direction A §"Reference idioms" L128-129). Sub-ADR 0052a MAY propose `ref s` ONLY by documenting a specific §2.5 cost preventing `&s` adoption; otherwise `&s` is the binding default.
- Whether Direction D method-call form should also work for `&s.method(args)` (borrow + method together). Cross-sub-ADR interaction; post-Phase-G integrated audit checks. **F-G.3 amendment**: precedence is fixed per Rust-corpus default — `&s.method()` parses as `&(s.method())` (method-call binds tighter than borrow). Sub-ADR 0052d ratifies this in the method-dispatch infra ADR (0052d-prereq).
- Whether Phase G needs a v0.3.0-alpha intermediate tag (mirroring ADR-0048's v0.2.0-alpha plan that ADR-0050 superseded). Probably not — Phase G ships ergonomic + verification depth, not new language surface, so v0.3.0 stable tag is the natural endpoint.
- Whether Direction C's tier-aware verifier upgrade triggers a re-run of all existing tomli + python-dateutil translation manifests under the stricter gate. Likely yes; sub-ADR 0052c §"Migration" owns the disposition.

## Documentation mandate (per constitution §3)

Each sub-ADR's commit ships triple-doc updates:

- `docs/human/zh/` — sub-ADR rationale + new ergonomic surface ("使用 `&s` 借用而不必 `clone(s)`") + example programs.
- `docs/human/en/` — English mirror, one-to-one with zh per §3.1 binding.
- `docs/agent/` — agent-tree updates: this batch frame's `last_verified_commit` stamps to merge SHA; each sub-ADR cross-refs back to this ADR via `parent_adr: 0052`.

Phase G triple-doc deliverable scope per sub-ADR:

| Sub-ADR | Bilingual surfaces added |
|---|---|
| 0052a | `getting-started.md` §"Explicit borrow" + `design-philosophy.md` §"Why `&s` not `clone(s)`" |
| 0052b | `error-reference.md` rewrite: every error variant gets its suggestion documented |
| 0052c | new `docs/human/{zh,en}/translation-guide.md` §"`@py_compat` tiers and L2 verification" |
| 0052d | `getting-started.md` §"Method-call form" + `design-philosophy.md` §"Method-form vs PRELUDE-form: training-data alignment" |

## Why this ADR now

Phase F.3 closed at HEAD `7ab04a4` with v0.2.0 release prep complete. ADR-0051 codified §2.5 as the constitutional north star. The four priority directions in §2.5 + ADR-0051 are the load-bearing scope decisions for the next batch; without ADR-0052 as the Phase G frame, sub-ADRs 0052a..d would re-derive prioritization from §2.5 in each spike, re-derive wave ordering, re-derive audit rubrics, and risk drift across the four parallel dispatches. Codifying Phase G's batch frame now binds the four sub-sprints to §2.5 + ADR-0051 before any Wave 1 dispatch.

This ADR's `status: proposed` reflects that the batch frame itself ratifies on the first sub-ADR dispatch — ADR-0052a Wave 1 design spike's first commit references this frame as `parent_adr: 0052` and the frame's `last_verified_commit` stamps to that commit. Sub-ADRs 0052a..d each independently ratify (`status: accepted` with `last_verified_commit`) on their respective merge SHAs.

## Evidence

- `CLAUDE.md` §2.5 (HEAD `7ab04a4` lines 67-87) — the constitutional citation.
- ADR-0051 — constitutional-layer ratification of §2.5.
- ADR-0050 — Phase F.3 batch frame template + §A1 verified-at-HEAD SOP precedent + §A7 P10-direct PAIR binding.
- ADR-0050e §"Option A" L180-184 — method-dispatch infrastructure absence baseline for Direction D.
- ADR-0050e §"Decision 2" — `clone()` mitigation for ADR-0050c Option A; Direction A retires this idiom.
- ADR-0050f §"Option B" L176-184 — method-dispatch Phase G ETA confirmation.
- ADR-0037 — reserved placeholder Direction C activates.
- `findings/lc100-str-use-after-move-regression-from-adr0050c.md` Path D + long-term deferral addendum — Direction A empirical baseline.
- `findings/predicate-flip-cascade-discovery-deficit.md` — F30 SOP Direction A sub-ADR 0052a dispatch must follow.
- `findings/adsd-pair-pattern-impl-gap.md` — F28 P10-direct PAIR pattern Phase G impl sprints follow.
- `findings/adr-scope-reality-divergence.md` — F27 verified-at-HEAD SOP Phase G sub-ADRs follow.
- `feedback_cobrust_llm_first_design_principle.md` — memory pin of §2.5 binding (cross-session persistence).
- `feedback_heavy_build_offload_to_workstation.md` — DG-primary routing for heavy sprints.
- `feedback_subagent_model_tier.md` — opus/sonnet model selection per sub-ADR tier.

— P9 Tech Lead, 2026-05-16
