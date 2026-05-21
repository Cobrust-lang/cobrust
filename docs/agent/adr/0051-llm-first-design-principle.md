---
doc_kind: adr
adr_id: 0051
title: "LLM-first design principle — Cobrust's constitutional north star"
status: accepted
date: 2026-05-16
last_verified_commit: 85548f1
supersedes: []
superseded_by: []
relates_to: [adr:0001, adr:0019, adr:0037, adr:0038, adr:0044, adr:0048, adr:0050, adr:0050a, adr:0050b, adr:0050c, adr:0050d, adr:0050e, adr:0050f]
discovered_by: P10/user 2026-05-16 directive "LLM Agent 一次写对" — strategic reframe after Phase F.3 closure
ratification_path: user-directed; in-session amendment to CLAUDE.md §2.5
parent_adr: none (constitutional layer)
---

# ADR-0051: LLM-first design principle — Cobrust's constitutional north star

## Context

Phase F.3 closed (`6acf678 + b50d000`) with 5 P0 language features + 2 P1 stdlib surfaces + 4 ADSD upstream methodology candidates filed. The audit returned **GO-v0.2.0-STABLE-WITH-OBSERVATIONS** — no blockers.

At that closure point P10/user surfaced the load-bearing strategic frame that must now bind every future design decision:

> **Cobrust 不是为人类写得最爽的语言, 是为 LLM 一次写对的语言。**
>
> Translation: Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try.

This was always implicit in the constitution (§1.1 "AI-friendly Python successor" + §1.2 "AI-native compiler"), but it had never been explicitly stated as the **prioritization rule** for every design trade-off. With Phase F.3 closed and the constitution about to guide Phase G + v0.3.0+, this needs to be a constitutional pillar, not a tacit understanding.

## Options considered

### Option A — leave the principle implicit; trust the existing §1 / §2 framing

- Pros: zero churn; constitution already mentions "AI-friendly".
- Cons: every future design discussion re-derives the principle from scratch. Sub-agents reading the constitution cold have no single sentence to anchor on. The principle gets diluted over time.
- **Rejected.**

### Option B — Amend CLAUDE.md with a new §2.5 + file this ADR (CHOSEN)

- Pros: one-sentence constitutional pillar that every future ADR / sub-agent / audit teammate can cite. Captures the four priority directions P10 enumerated. Mirrors the ADR-0048 reframe pattern.
- Cons: adds load to the constitution. Requires sub-agents reading §2.5 alongside §2.1-§2.4.
- **Chosen.**

### Option C — file the principle as a feedback memory only

- Pros: smaller surface; cross-session persistent.
- Cons: doesn't bind sub-agents (memory files aren't required reads for dispatched agents); doesn't show up in ADR cross-references.
- **Rejected** as the sole channel; will land as a feedback memory *in addition to* the constitutional amendment for cross-session persistence.

## Decision

Adopt **Option B**. Amend `CLAUDE.md` with a new §2.5 "LLM-first design principle" sub-section under §2 "Design Philosophy" that:

1. States the one-sentence principle: "Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try."
2. Names the two operational selection rules every design trade-off should follow:
   - **Compile-time-catch-errors**: prefer designs that surface bugs at type-check time over designs that defer to runtime. The LLM's compile-error feedback loop is its strongest correction signal.
   - **Maximize-overlap-with-training-data**: prefer syntax + semantics that occur frequently in Python + Rust training corpora. LLMs write correctly when the surface matches their priors.
3. Locks the four priority directions P10/user identified as "biggest current LLM-friendliness deficits":
   - **A. Explicit `&` borrow / let-rebind shortcut** (Phase G priority): eliminates `clone()` clutter; LLM-generated code reads ~half the length + correctness up-shifts. Currently the LARGEST LLM-friendliness deficit per LC-100 honest-debt empirical baseline.
   - **B. F.1.4 Error UX rewrite** — error messages must print the FIX, not just the diagnosis. Today: `TypeError::ImplicitTruthiness { actual: Int, span: ... }`. Tomorrow: `TypeError::ImplicitTruthiness { actual: Int, suggestion: "change to 'if x != 0:'", span: ... }`. LLM consumes stderr to decide next step; making the fix path explicit means LLM corrects on first retry.
   - **C. `@py_compat` tier hard-bind to L2 verifier** (ADR-0037 reserved → activate): translation pipeline strict/semantic/numerical tier becomes a contract the LLM router can route on. Higher correctness on real-library translation.
   - **D. Method-call sugar priority** — `s.split(",")` over `split(s, ",")`. Closer to LLM training data distribution. ADR-0050e Q10 + ADR-0050f Phase G method-form path identifies this; Phase G should ship it as a P0 ergonomic.
4. Explicitly subordinates other design instincts to this principle. When a trade-off pits "elegance for humans" against "compile-time catch + training-data overlap", the latter wins.

## Consequences

### Positive

- Every future ADR has a constitutional citation for design trade-offs. Sub-agents can answer "why was this chosen?" with a one-line reference to §2.5 rather than re-deriving the philosophy.
- The four priority directions get explicit phase prioritization. Phase G batch ADR (now slotted as ADR-0052) prioritizes (A) and (B) over feature breadth.
- Future audit teammates have a concrete rubric: "did this design respect §2.5's compile-time-catch + training-data-overlap rules?"
- ADSD F30 candidate (predicate-flip cascade discovery deficit) gains a constitutional rationale: predicate flips that *fail* the LLM-first test (i.e. break PRELUDE ergonomics that LLMs default to) need the shadow-flip dry-run SOP.

### Negative

- Constitution gains weight; sub-agents must internalize §2.5 alongside §2.1-§2.4.
- Some design choices that score well on "human elegance" may now lose to "LLM-friendliness" even when humans would prefer the elegant option. Trade explicit.
- "Maximize-overlap-with-training-data" is fuzzy by construction — LLM training corpora aren't a static target. Phase G+ may need to refresh the surface as model generations change.

### Neutral / unknown

- Whether §2.5 binds tightly enough to actually change behavior, or whether sub-agents continue defaulting to whatever ADR cross-references they happen to read. Audit teammates will check compliance going forward.
- Whether "compile-time-catch" can be quantified — e.g. a "F-pattern catch rate" metric that the audit can measure across ADRs.

## Implementation map

This ADR ships as 4 atomic deliverables in a single commit:

1. **Author this ADR** at `docs/agent/adr/0051-llm-first-design-principle.md` (this file).
2. **Amend `CLAUDE.md`** with new §2.5 "LLM-first design principle" inserted between §2.4 (Cobrust originals) and §3 (Documentation Mandate). Length: ~30-40 lines.
3. **Update `docs/agent/adr/README.md`** index to include ADR-0051 + bump the future Phase G batch frame slot to ADR-0052.
4. **File feedback memory** at `/Users/hakureirm/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/feedback_cobrust_llm_first_design_principle.md` for cross-session persistence — newer sessions read this alongside `project_state_snapshot.md` + `cto_operations_runbook.md`.

Cross-references to update opportunistically (NOT in this ADR's commit; let future ADR amendments cite §2.5):
- ADR-0037 (`@py_compat` hard-bind) — §2.5 Direction C activates this from `reserved` → `proposed`.
- Phase G batch ADR-0052 (when authored) — §2.5 Directions A, B, D all prioritize Phase G P0 items.

## Evidence

- User directive 2026-05-16: 6-paragraph strategic message during Phase F.3 closure (verbatim transcript captured in memory file deliverable #4 above).
- ADR-0050 Phase F.3 batch closure metrics — 6 P0/P1 features shipped with explicit "no implicit truthy/falsy", "Result default", "Aggregate::Dict structural rvalue not block-syntax sugar", "f-string strict precision" trade-offs all rationalized post-hoc as "LLM-friendliness wins". §2.5 makes this rationale ex-ante.
- ADR-0050c Option A + ADR-0050d symmetric walk-back — chose Str/List/Dict ownership-semantics that catch use-after-move at compile time over Python-mutable-by-default. §2.5 Direction A names this as the foundation but also names the cost (`clone()` clutter) that Phase G's explicit borrow form will eliminate.
- LC-100 honest-debt long-term-deferral disposition — codified per `findings/lc100-str-use-after-move-regression-from-adr0050c.md` Path D. The user attestation "nobody uses LC-100 today" combined with §2.5 makes the deferral defensible: ergonomic wedge cosmetics lose to §1.1 language-half soundness which itself serves §2.5.
- F-pattern findings catalogue (F27 / F28 / F29 / F30) — each is a methodology-level catch for LLM-driven sub-agent dispatch correctness. §2.5 makes the catalogue's purpose explicit: harden the LLM agentic dispatch loop, not just the language surface.

## Why this ADR now

Phase F.3 just closed. Phase G is about to start. The four priority directions P10/user named are the load-bearing scope decisions for Phase G. Without §2.5 codified, Phase G ADR-0052 would re-derive the prioritization from scratch + risk drift. Codifying §2.5 now binds Phase G to the LLM-first frame before any new sprint dispatches.

— P10 CTO, 2026-05-16 night
