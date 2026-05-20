---
doc_kind: adr
adr_id: 0064
name: "0064"
title: print_int debt removal + polymorphic print monomorphization
status: accepted
phase: "Phase N (post-Phase-M)"
date: 2026-05-19
last_verified_commit: 46c0946
supersedes: []
superseded_by: []
relates_to: [adr:0050e, adr:0051, adr:0052b, adr:0058d]
---

# ADR-0064: print_int Debt Removal + Polymorphic `print` Monomorphization

## §1 Motivation

**User directive 2026-05-19.** The functions `print_int`, `print_str`, `print_bool`, `print_float` were introduced in Phase E as scaffolding for the wave-1 codegen demo. They were never intended to be source-face PRELUDE API; they were codegen-internal primitives named by type-shape (`verb_type`). They got fossilized when wave-2 onward never audited "is this source-face or codegen-internal?".

This is **F38** — source-surface leakage of a codegen internal primitive. It is a sibling of F36 (fixture-name-vs-behavior drift) and F37 (silent-rot-on-accepted-debt): all three share the root cause of wave-1 demo-ware fossilizing without an audit checkpoint.

**§2.5 training-data-overlap violation (ADR-0051 binding):**  
Every LLM trained on Python or Rust writes `print(x)`, never `print_int(x)`. Exposing `print_int` at source-face means:
- LLM generates `print(x)` → type error at source → LLM is confused by the gap between its prior and the actual API.
- Migration cost grows quadratically: examples, fixture files, cobrust-first-try skill, LC-100 stress corpus — all accumulate `print_int` calls that will each need a migration fix.
- Zero benefit: monomorphization already happens post-typecheck from static types; source-face polymorphism is strictly more correct.

The optimization argument ("keep for monomorphization") is falsified:  
Static types already know `x`'s type at the call site → codegen can monomorphize `print(x: Int)` → `__cobrust_print_int` internally. ADR-0050e PRELUDE-fn + Phase G Direction D (method-call sugar, ADR-0051) already prove this dispatch pattern works.

---

## §2 §2.5 LLM-First Audit

| Design question | Keep print_int | Polymorphic print(x) |
|---|---|---|
| LLM writes correctly on first try? | No — LLM prior is `print(x)` | Yes |
| Compile-time error catch? | Type mismatch surfaces but misleadingly | Wrong-type print → clear TypeError |
| Training-data-overlap score | Low — `print_int` does not appear in Python/Rust corpora | High — `print(x)` is universal |
| Migration cost trend | Grows with every new example/fixture | Stable |

Removing `print_int` IS the training-data-overlap intervention. This ADR directly executes ADR-0051 §2.5 Direction A (maximize-overlap-with-training-data).

---

## §3 Scope

### 3.1 PRELUDE table — remove 4 source-face entries

Remove the following from the PRELUDE source-face table (the table that controls what names are in scope for user `.cb` files):

- `print_int`
- `print_str`
- `print_bool`
- `print_float`

These become codegen-internal symbols only, never user-visible.

### 3.2 Add `print(x)` polymorphic dispatch in type-checker

Add a single PRELUDE entry: `print` with polymorphic dispatch.  
Type-checker rule: `print(expr)` — typecheck `expr`, record its resolved type, pass to codegen as `PrintCall { ty: ResolvedType, expr }`.  
No overloads at source face; single name, single dispatch path.

### 3.3 Codegen monomorphization

Post-typecheck, lower `PrintCall { ty, expr }` to the appropriate C-ABI internal symbol:

| Source type | Internal C-ABI symbol |
|---|---|
| `Int` | `__cobrust_print_int` |
| `Str` | `__cobrust_print_str` |
| `Bool` | `__cobrust_print_bool` |
| `Float` | `__cobrust_print_float` |

This mirrors the `lower_body_wave1` monomorphization pattern established in ADR-0058d. The `__cobrust_print_*` ABI symbols are **not changed** (see §4 Non-goals). Only the dispatch path changes: from user-visible name → internal routing post-typecheck.

### 3.4 Mechanical refactor sprint

All `.cb` source files under `examples/`, `tests/`, `src/` (fixture `.cb` strings), and the `cobrust-first-try` skill must be updated:

- `print_int(x)` → `print(x)`
- `print_str(x)` → `print(x)`
- `print_bool(x)` → `print(x)`
- `print_float(x)` → `print(x)`

Estimated ~50–100 call sites. Pattern mirrors the LC-100 stress 226-call-site `&borrow` refactor (commits b2618f3 + 8f63132).

---

## §4 Non-Goals

- **No ABI change** to existing `__cobrust_print_int` / `__cobrust_print_str` / `__cobrust_print_bool` / `__cobrust_print_float` C symbols. They remain as-is; only the user-face routing changes.
- **No source-face polymorphic dispatch beyond `print`** in this sprint. Other shadowy PRELUDE leakage (if any) stays for follow-up ADRs. Scope is tightly bounded to the `print_*` family.
- **No runtime polymorphism** — `print(x)` is resolved statically at type-check time. `dyn` is never involved.
- **No change to `@py_compat` tier assignment** for `print` itself.

---

## §5 Implementation Plan

### 5.1 Phase 1 — PRELUDE table edit (~30 LOC delta)

- Locate the PRELUDE registration table in the type-checker crate.
- Remove entries for `print_int`, `print_str`, `print_bool`, `print_float`.
- Add entry for `print` with polymorphic dispatch annotation.
- Verify: `print_int(42)` now produces a `NameError` at source; `print(42)` resolves.

### 5.2 Phase 2 — `print` polymorphic dispatch in type-checker + codegen (~200 LOC)

- Type-checker: add `PrintCall` HIR node or reuse existing call dispatch; resolve `print(expr)` → `PrintCall { ty: expr.resolved_type(), expr }`.
- Codegen: add match arm in `lower_expr` (or equivalent) for `PrintCall`; emit the appropriate `__cobrust_print_*` call based on resolved type.
- Add 5+ integration tests covering all four types plus a computed expression (`print(fib(10))`).

### 5.3 Phase 3 — Mechanical refactor sprint (~50-100 call sites)

- `grep -r 'print_int\|print_str\|print_bool\|print_float' examples/ tests/ src/` to enumerate all sites.
- Batch-replace with `print(...)` stripping the type suffix.
- Update `cobrust-first-try` skill documentation to reflect polymorphic `print`.
- Update any human docs (`docs/human/zh/` + `docs/human/en/`) that show `print_int` examples.

### 5.4 Phase 4 — F38 finding ratify

After sprint closes with concrete LOC numbers and zero remaining `print_int` references in `.cb` source files:
- Update `docs/agent/findings/f38-source-surface-leakage-codegen-primitive.md` status: `candidate` → `ratified`.
- Record final call-site count and commit SHA in F38 §3 Empirical section.

---

## §6 Acceptance Gate

All of the following must pass before this ADR transitions to `accepted`:

1. `print(42)` compiles and outputs `42` — Int path.
2. `print("hello")` compiles and outputs `hello` — Str path.
3. `print(True)` compiles and outputs `True` — Bool path.
4. `print(3.14)` compiles and outputs `3.14` — Float path.
5. `print(fib(10))` compiles and outputs correct value — computed expr path.
6. `print_int(42)` in a `.cb` source file produces `NameError: print_int is not defined` (or equivalent) — removal confirmed.
7. LC-100 100/100 maintained.
8. `grep -r 'print_int\|print_str\|print_bool\|print_float' examples/` returns zero results.

---

## §7 Risk Register

| Risk | Likelihood | Mitigation |
|---|---|---|
| PRELUDE refactor breaks other PRELUDE-dependent tests | Medium | Run full test suite after Phase 1 before touching codegen |
| `cobrust-first-try` skill not updated → future LLM agents still emit `print_int` | High if missed | Phase 3 explicitly includes skill update; commit gate enforces `.cb` clean |
| Downstream `examples-literal-print-debt.md` finding overlaps scope | Low | Check findings/examples-literal-print-debt.md before Phase 3; may already cover some sites |
| F35-sibling discipline on commit msgs | Medium | Each phase commit msg must describe final-form scope, not original spec; no telescoping |

---

## §8 LOC Estimate

| Component | Estimate |
|---|---|
| PRELUDE table edit (remove 4, add 1) | ~30 LOC delta |
| Type-checker polymorphic dispatch (`PrintCall` HIR + resolve) | ~80 LOC |
| Codegen monomorphization (match arm + emit) | ~120 LOC |
| Integration tests (5 required, target 8) | ~80 LOC |
| Mechanical refactor (~50–100 call sites across examples/tests/skill) | ~100–200 LOC churn |
| **Total net delta** | **~400–600 LOC** |

---

## Options Considered

1. **Keep `print_int` family, add `print` alias** — rejected; doubles PRELUDE surface, trains LLMs on both forms, migration cost never closes.
2. **Remove at source, polymorphic dispatch post-typecheck (this ADR)** — chosen; zero user-visible change to output behavior, clean §2.5 compliance.
3. **Remove entirely, require explicit type conversion** — over-engineered; `print(x)` with static dispatch is simpler and has higher training-data-overlap.

## Decision

Remove `print_int`, `print_str`, `print_bool`, `print_float` from the PRELUDE source-face table. Add a single polymorphic `print(x)` that resolves the internal C-ABI symbol post-typecheck based on the statically resolved type of `x`. Mechanical refactor of all `.cb` source files follows. ABI symbols unchanged.

## Consequences

- **Positive**
  - §2.5 training-data-overlap restored: `print(x)` matches LLM priors exactly.
  - Migration cost curve broken: new examples/fixtures write `print(x)` naturally.
  - Simpler PRELUDE table: 4 entries removed, 1 added (net -3).
  - F38 finding resolved; detection rule (CI gate candidate) documented.
- **Negative**
  - Phase 3 mechanical refactor is ~50–100 sites: non-trivial but bounded and scriptable.
  - Any external user who somehow adopted `print_int` in their own `.cb` files will break — acceptable given pre-1.0 status.
- **Neutral / unknown**
  - Type inference edge cases: if `print(x)` where `x` has ambiguous type (e.g., unresolved generic) — need a clear error message pointing to type annotation requirement. ADR-0051 §2.5 Direction B (error UX rewrite) covers this.

## Evidence

- ADR-0051 §2.5: "Maximize-overlap-with-training-data: prefer syntax + semantics that occur frequently in Python + Rust training corpora."
- ADR-0050e: PRELUDE-fn cleanup precedent; method-call sugar dispatch pattern.
- ADR-0058d: `lower_body_wave1` monomorphization — exact pattern reused in §3.3.
- LC-100 &borrow refactor (b2618f3 + 8f63132): mechanical 226-site batch precedent for §3.4.
- F38 finding: `docs/agent/findings/f38-source-surface-leakage-codegen-primitive.md`.
- User directive 2026-05-19: authoritative trigger.
