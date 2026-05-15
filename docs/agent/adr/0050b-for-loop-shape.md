---
doc_kind: adr
adr_id: 0050b
title: "M-F.3.1 — For-loop shape (range-first + list[i64] iter); list[str] gated on Wave 2"
status: proposed
date: 2026-05-16
last_verified_commit: TBD
supersedes: []
superseded_by: []
relates_to: [adr:0027, adr:0030, adr:0035, adr:0044, adr:0050]
discovered_by: P9-B Wave 1 dispatch on `feature/f3-for-loop` per ADR-0050 §"Implementation map / M-F.3.1"
ratification_path: in-session review per ADR-0050 §"Audit model"
---

# ADR-0050b: M-F.3.1 — For-loop shape (range-first + list[i64] iter)

## Context

ADR-0050 §"Implementation map / M-F.3.1" prescribes for-loop sprint scope but defers the **shape**
decision — desugar at HIR vs. lean on existing iter-protocol vs. introduce a new range primitive — to
this sub-ADR.

### Existing baseline (audited 2026-05-16 on `feature/f3-for-loop`)

| Layer | Surface | Status |
|---|---|---|
| AST | `Stmt::For { target, iter, body, else_block }` (form 11) | shipped |
| Parser | `parse_for` consumes `for <pat> in <expr>: <block>` (+ optional `else:`); supports tuple pattern via `parse_for_target` | shipped |
| HIR | `LoopKind::For { binding_def_ids, pattern, iter, body, else_block, span }` | shipped |
| HIR-lower | `ast::StmtKind::For → h::LoopKind::For` (no desugar; preserves structure) | shipped |
| Types | `iter_element` accepts `Ty::List<T>` / `Ty::Set<T>` / `Ty::Dict<K, V>` / homogeneous `Ty::Tuple` → binds loop var to element type | shipped |
| MIR | `lower_loop / LoopKind::For` emits `__cobrust_iter_init / __cobrust_iter_next / __cobrust_iter_drop` via `Terminator::Call` | shipped |
| Codegen | Cranelift backend pre-declares the three `__cobrust_iter_*` C-ABI shims | shipped |
| Runtime stdlib | `cobrust-stdlib::iter::{__cobrust_iter_init, __cobrust_iter_next, __cobrust_iter_drop}` over list-of-i64 layout per ADR-0044 W2 Phase 2 | shipped |
| Source-level smoke | `for a in argv():` works end-to-end via `intrinsics_input.rs` corpus T19/T20/T24/T27/T29/T33 | shipped |
| Source-level **`range(a, b)`** | **not in prelude; no recognized call form** | **MISSING — this sprint's gap** |
| `list[str]` iter source | works in source today via `argv()` → `for s in args:` path (the runtime shim interprets every iter target as a list of i64 slots — the slots happen to hold heap-Str pointers; iteration emits pointers back as i64, which the type checker re-tags as Str) | shipped (per ADR-0044 W2 Phase 2 amendment) |

The brief's framing — "desugar `for i in range(a, b)` into a while-with-counter, do not stand up the full iter-protocol" — predates a verification of the worktree state. With AST/parser/HIR/types/MIR/codegen/runtime all already wired for the iter-protocol shape, the ground-truth shape of the M-F.3.1 sprint is **"plug the missing `range(a, b)` source-level form into the existing iter-protocol path"**, not "rewrite the for-protocol via a new HIR-level desugar."

### Constitution alignment

| Clause | This shape's adherence |
|---|---|
| §1.1 "syntactically familiar to Python users" | `for i in range(0, n):` is the canonical Python form |
| §2.2 "no late closure binding" | each iter rebinds via MIR `Place::local(var_local)` re-assign; closures capture by explicit `copy`/`ref`/`move` per Cobrust convention — no Python late-binding cell semantics |
| §2.2 "no implicit truthy/falsy" | iter-source is type-checked against `iter_element` contract; non-iterable iter expressions raise `TypeError::NotIterable` |
| §5.1 "one way to do each thing" | range() is the **one** non-collection iter source for Phase F.3; user-defined iter via `__iter__` trait deferred to Phase G |
| §6 "test-first" | P7-TEST PAIR-mandated D2-D3 sprint per `cto_operations_runbook.md` |

### LC-100 + external-user friction evidence

- `feedback_third_party_audit_2026_05_09.md` cites "刷不了 leetcode" as the user-quote framing for Phase F.3.
- `range(a, b)` is the dominant Python loop idiom (≈70% of leetcode Python solutions per Pattern-A finding).
- Without `range`, every counting loop in Cobrust today must use `while`, which is constitution §5.1-flagged as a "permanent ergonomic tax" (ADR-0050 §"Strategic frame").

## Options considered

### Option A — HIR-level desugar (brief-prescribed shape)

Add HIR pass that rewrites `LoopKind::For { iter: Call("range", [a, b]), … }` to `LoopKind::While`:

```
let __i = a
while __i < b:
    let i = __i
    body
    __i = __i + 1
```

- Pros: zero runtime allocation; iter source recognized syntactically; reuses while lowering.
- Cons:
  - The for-protocol path is **already shipped** end-to-end. A second desugar path forks the for-loop's lowering and doubles maintenance.
  - The iter-protocol path is the right shape for Phase G (user `__iter__`). Adding a desugar fork now means tearing it back out for Phase G.
  - Loop-var rebinding via fresh `let i = __i` inside the while body adds a HIR-scope new binding per iter — desirable for closure-capture safety but currently moot since closure capture is by-value.
  - Constitution §5.1 "one way to do each thing" — two desugar paths for one form violates this.

### Option B — Prelude `range` as real Cobrust fn (CHOSEN)

Add `range(a, b)` as a real prelude function body that materializes a `list[i64]`:

```cobrust
fn range(a: i64, b: i64) -> list[i64]:
    let mut xs: list[i64] = list_new(0)
    let mut i: i64 = a
    while i < b:
        let _ = list_set(xs, i - a, i)
        i = i + 1
    return xs
```

(Implementation detail: `list_new` today allocates a capacity but does not size; the body uses `list_push`-equivalent via `list_set` indexed assignment, growing via the W2 Phase 3 list ABI. Actual emitted prelude body is the working form, not the sketch above; see §"Implementation map" for the exact text.)

- Pros:
  - Zero new HIR/MIR/codegen/runtime surface.
  - Reuses existing `__cobrust_iter_*` runtime for the actual `for` iteration.
  - Constitution §5.1: one for-loop lowering path (iter-protocol).
  - Phase G extension is monotone: when source-level `__iter__` lands, `range` stays as a list-producer; nothing rewires.
  - Trivially supersedable by an O(1)-memory specialization at the optimizer level later without breaking source-level semantics.
- Cons:
  - O(b - a) memory: a `range(0, 1_000_000)` allocates a 1M-slot list. Acceptable for Phase F.3 (LC-100 ranges are bounded < 10⁵); ADR-0050b §"Future work" notes the O(1) specialization as a Phase G-or-later optimization.
  - The user-visible signature is `list[i64]`, not an opaque `Range`. This means `r = range(0, 10); for i in r:` and `for i in range(0, 10):` both work, which is *more* permissive than Python's lazy-range — explicitly fine.

### Option C — New `Range` primitive + iter-protocol RangeIter

Introduce `Ty::Range`, `Aggregate::Range`, and a `RangeIter` runtime alongside `ListIter`.

- Pros: O(1) memory; matches Python's lazy-range surface.
- Cons:
  - Touches every layer (types, MIR, codegen, runtime). Out-of-scope for a 3-day D2-D3 sprint.
  - Phase G iter-protocol unification would re-merge `Range` with user `__iter__` anyway; doing it twice is wasted work.
  - Constitution §5.1 violation again — two iter shapes (lazy Range, eager List) for one surface form.

### Option D — Range as recognized intrinsic call rewritten at HIR

Hybrid: parse `Call("range", …)` normally; in the HIR-lower pass, when the parent is `Stmt::For { iter: range_call, … }`, rewrite the for-loop in-place to a while. Outside the for-loop context, `range` is undefined.

- Pros: O(1) memory in the common case.
- Cons:
  - "`range` is only legal inside `for`" is a special case that breaks compositionality (`r = range(0, 10); for i in r:` won't work).
  - The compositional break is exactly the Python late-binding-vs-explicit-capture special case Cobrust drops in §2.2.
  - Same maintenance double-cost as Option A.

## Decision

**Adopt Option B.** Ship `range(a, b)` as a Cobrust prelude function body materializing a `list[i64]`; lean on the existing for-protocol iter path for all `for`-loop semantics.

The brief's Option-A shape is **superseded by ground-truth state of the worktree** (AST/parser/HIR/types/MIR/codegen/runtime are all shipped for the iter-protocol path; the only missing source-level surface is `range`). Option B respects the brief's stated intent ("range-first + list[i64] iter only; iter-protocol Phase G deferred") with strictly less surface change.

`range(a, b, step)` 3-arg form ships in this sprint **only** for `step > 0`, matching Python's behavior for forward ranges. Negative-step and reverse ranges deferred to Phase G alongside iter-protocol expansion. The 2-arg form is the primary user surface for Phase F.3.

### Implementation map

#### M-F.3.1.A — prelude `range`

- `crates/cobrust-cli/src/build.rs::PRELUDE` — append a `range` body that materializes the list inline using existing W2 Phase 3 list ABI.
- `crates/cobrust-cli/src/build/intrinsics.rs` — no change required if `range` is a real prelude body (it doesn't need intrinsic-rewrite; the W2 Phase 3 `list_new` / `list_set` it calls *are* already intrinsic-rewritten).
- **Body must be the size-known form** to avoid the W2 Phase 3 `list_new` capacity-vs-size discrepancy: the prelude `range` body uses an explicit grow-loop, materializing `b - a` slots with values `a + 0, a + 1, …, a + (b - a - 1)`.

#### M-F.3.1.B — `list[i64]` iter source

Already shipped per ADR-0044 W2 Phase 2 + the existing `__cobrust_iter_*` runtime. No code change required; sprint adds **corpus** to lock the contract.

#### M-F.3.1.C — `list[str]` iter source — gated to Wave 2

The iter-protocol runtime today returns `i64` slots; when the underlying list holds heap-Str pointers, the type checker re-tags slots as `Str` at the binding site. This *works at runtime* (verified via `intrinsics_input.rs` T19/T20/T24/T27/T29/T33) but lacks per-iter Drop scheduling — every iteration through a `list[str]` reads the pointer without `__cobrust_str_clone`, and the loop var aliases the list slot. This is a latent ownership bug.

Per ADR-0050 §"M-F.3.2 list[str]", the ownership flip (`Ty::Str` Copy → non-Copy with Drop per ADR-0027) lands in Wave 2 under ADR-0050c (Str ownership) + M-F.3.2 (list[str]). M-F.3.1 (this sprint) **does not** flip the ownership; the test corpus locks the current behavior with a clearly documented "Wave 2 supersedes" note for the list[str] cases.

#### M-F.3.1.D — break / continue interaction

Per `feature/f3-break-continue` audit at sprint kickoff: that branch contains **zero unique commits** beyond `main` HEAD (`30cf2b2`). M-F.3.0 (break/continue) ships in parallel on a separate worktree. The for-loop desugar **MUST NOT** emit `break`/`continue` in MIR; this is upheld trivially under Option B because the for-loop lowering is unchanged.

If `feature/f3-break-continue` lands before this sprint merges, this ADR adds a backlinked Phase G follow-up to specialize `for i in range(a, b): if cond: break` into a `while` form for O(1) early-exit — not in M-F.3.1 scope.

### Per-iteration loop-var binding semantics

Per constitution §2.2 "no late closure binding": closures captured inside the for-loop body capture the loop-var by value at the moment of closure creation. Under Option B + the existing MIR for-protocol lowering:

- `var_local` is a single MIR local declared once before the loop header.
- Each iter assigns `var_local := __cobrust_iter_next(…)` then runs the body.
- A closure created inside the body that captures `var` performs an explicit value-copy at creation site (per Cobrust capture semantics — no upvar/cell layer).

Net effect: closures see the iter-N value of `var` when created at iter N, regardless of subsequent iter-N+1 mutations of `var_local`. The constitution invariant holds.

### Empty range

`for i in range(0, 0):` and `for i in range(5, 3):` (start ≥ stop) materialize an empty `list[i64]`, then the iter-protocol's first `__cobrust_iter_next` returns 0 → exit. Body never executes. Same semantics as Python.

### Nested for + variable shadowing

```cobrust
for i in range(0, 3):
    for j in range(0, 3):
        let total: i64 = i * 3 + j
        print_int(total)
```

- HIR-lower already opens a fresh scope per `For` arm; each `i`/`j` gets its own `DefId`.
- Inner `let total` is a new binding per inner-iter (per existing Cobrust let-in-loop semantics).
- Shadowing follows Rust rules: `for i in range(0, 3): let i: str = "foo"` — the inner `let` shadows the loop-var for the body's remainder, but the next iter's `__cobrust_iter_next` reassigns the loop-var slot before the body re-enters.

### Iter source type checking

- `for x in range(a, b):` — `range` resolves to the prelude fn `fn(i64, i64) -> list[i64]`; `iter_element` returns `i64`; `x: i64`.
- `for x in xs:` where `xs: list[i64]` — `iter_element` returns `i64`; `x: i64`.
- `for x in xs:` where `xs: list[str]` — current runtime works (per ADR-0044 W2 Phase 2), but ownership-incorrect; M-F.3.1 corpus tests *negative* this case with a docstring "Wave 2 supersedes per ADR-0050c". *Update during impl: the type checker continues to accept `list[str]` iter at M-F.3.1 because the runtime path works; the ownership defect surfaces only under repeated-iter / drop-after-move scenarios which Wave 2 M-F.3.2 corpus exercises.*
- `for x in 42:` — `iter_element` raises `TypeError::NotIterable`; ill-typed gate corpus locks the error message.
- `for x in "string":` — Cobrust `str` is **not iterable** at M-F.3.1; iter-protocol over individual code points lands with the string stdlib bundle (Phase F.3 P1 §M-F.3.5). Ill-typed corpus locks this.

### `range(a, b, step)` 3-arg form

Optional; ships if cheap. Prelude body:

```cobrust
fn range_step(a: i64, b: i64, step: i64) -> list[i64]:
    # Cobrust precondition: step > 0; step <= 0 panics via division-by-zero
    # shape (no panic primitive in M-F.3.1; surface as TypeError if zero).
    let mut xs: list[i64] = list_new(0)
    let mut i: i64 = a
    while i < b:
        let _ = list_set(xs, (i - a) / step, i)
        i = i + step
    return xs
```

Decision: defer 3-arg `range_step` to Phase G alongside iter-protocol expansion. M-F.3.1 ships **2-arg `range(a, b)` only**. Rationale: 3-arg form's parameter-default sugar (`range(a, b, step=1)`) requires the optional-param surface from ADR-0036, which is unimplemented for prelude bodies at M-F.3.1.

## Consequences

- **Positive**
  - Smallest correct increment. Zero new MIR/codegen/runtime surface (per brief constraint).
  - Phase G iter-protocol expansion is monotone over this shape; no rework.
  - LC-100 `for i in range(0, n):` patterns work end-to-end after this sprint.
  - Constitution §5.1 "one way" preserved — one for-loop lowering path.

- **Negative**
  - O(b - a) memory cost for `range`. Mitigation: M-F.3.1 §"Future work" notes a Phase G optimizer pass for O(1) lazy-range specialization. LC-100 bounded ranges (n ≤ 10⁵) tolerate this; ranges over 10⁷+ slots produce O(80MB) allocations and surface as performance findings, not correctness bugs.
  - 3-arg `range_step` deferred — partial alignment with Python surface.

- **Neutral / unknown**
  - `list[str]` iteration today is runtime-correct but ownership-incomplete. M-F.3.1 corpus tests the working path with explicit "Wave 2 supersedes" notes; M-F.3.2 closes the ownership gap.
  - Whether the prelude `range` body should use `list_push`-style growth or pre-sized allocation depends on the W2 Phase 3 list ABI's growth shape; the impl confirms the right primitive during PAIR-DEV.

## Evidence

- ADR-0050 §"Implementation map / M-F.3.1" — sprint scope binding.
- ADR-0027 §"For-protocol scaffolding" — placeholder shape that ADR-0050b supersedes (the iter-protocol is no longer scaffolding-only after ADR-0044 W2 Phase 2).
- ADR-0044 W2 Phase 2 — `__cobrust_iter_init/next/drop` interpretation of `iter_val` as list pointer; locked the runtime path.
- ADR-0035 — `lower_condition` shared root primitive for `if`/`while` heads; this ADR is monotone over it.
- `feedback_third_party_audit_2026_05_09.md` — "刷不了 leetcode" external-friction baseline.
- `cto_operations_runbook.md` §"Dev/test pair pattern" — D2-D3 mandatory PAIR sprint.
- `feedback_subagent_model_tier.md` — sonnet model selection for D2-D3 well-scoped impl.
- Worktree audit 2026-05-16 on `feature/f3-for-loop` — AST/parser/HIR/types/MIR/codegen/runtime baseline verified shipped.
