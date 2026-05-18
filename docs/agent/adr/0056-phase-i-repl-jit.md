---
doc_kind: adr
adr_id: 0056
parent_adr: 0054
title: "Phase I frame — REPL JIT (M14.1): Cranelift JIT runtime invoke for incremental REPL eval"
status: proposed
date: 2026-05-18
last_verified_commit: 2a710d3
supersedes: []
superseded_by: []
relates_to: [adr:0054, adr:0029, adr:0034]
discovered_by: P10/user 2026-05-18 — Phase I scoping spike `dispatches/2026-05-18-phase-i-repl-jit-poc-plan.md` ratifies here
ratification_path: P9 frame-ADR review; ratifies on first sub-ADR (0056a) dispatch
---

# ADR-0056: Phase I frame — REPL JIT (M14.1) Cranelift JIT runtime invoke

## 1. Context

### 1.1 Phase G closure baseline

Phase G (ADR-0052 batch frame) closed at HEAD `8b4366c` with the
v0.3.0 stable tag binding satisfied: Wave 1 (ADR-0052a explicit
`&` borrow), Wave 2 round 1 (ADR-0052b error UX, 0052c
`@py_compat` L2 hard-bind, 0052d-prereq method-dispatch infra),
and Wave 2 round 2 (ADR-0052f parser cap relaxation, 0052g
`&CallResult` type-check) all `accepted`. Cargo.lock regenerated
for v0.3.0 release (`8b4366c`).

### 1.2 The M14.1 deferral being un-deferred

ADR-0029 (M14 REPL, `accepted` 2026-04-30) ships an AST-walking
HIR interpreter scoped to literals + arithmetic + bound-var read
+ `let` write + `print(<literal>)` via one-shot AOT delegation.
Its §"Evaluation surface" table (verified at HEAD `8b4366c` lines
249-253) marks three rows `❌ — defer to M14.1`:

- Function calls (user-defined)
- Loops / if-else / match
- Comprehensions / collections

ADR-0029 §"Honesty audit" explicitly notes "Full Turing-complete
session evaluation is M14.1 follow-up." ADR-0054 §4 "Phase I"
(verified at HEAD `8b4366c` lines 89-108) un-defers M14.1 with a
1-week agent-velocity wall and a single-sub-ADR scope claim. This
frame ADR refines that claim post-scoping-spike: the POC plan
`dispatches/2026-05-18-phase-i-repl-jit-poc-plan.md` decomposes
the 1-week wall into 4 sub-ADRs (frame + 3 sub-stages).

### 1.3 Constitutional anchors

- **CLAUDE.md §2.5** (LLM-first design principle, ADR-0051):
  Phase I delivers medium §2.5 ROI per ADR-0054 §2 ranking table —
  speeds the L1 translation pipeline iteration loop without
  shipping a new LLM-surface contract. The contract Phase J (LSP)
  consumes is the **incremental `TypeCheckCtx`** Phase I produces
  as a side-effect; see §6 handoff.
- **CLAUDE.md §7** — M14 REPL milestone; M14.1 = this ADR.
- **ADR-0054 §9** — Phase H + Phase I OVERLAP (different code
  paths: H touches `crates/cobrust-types-cb/` new; I touches
  `crates/cobrust-cli/src/repl.rs` + `cobrust-codegen` JIT add).

## 2. §2.5 citation

ADR-0054 §2 ranks Phase I §2.5 ROI as **medium** with rationale:

> Translation pipeline L1 loop speedup (one-shot AOT call per
> stmt → JIT dispatch). Speeds the translation closed-loop
> measurably but no new LLM-surface contract.

The one-shot AOT delegation path in ADR-0029 §"Negative"
(verified at HEAD `8b4366c`) costs ~50ms per stdlib call on cold
caches — acceptable for `print` but kills tight-loop REPL eval.
Phase I drops this to <50ms warm via lazy `JITModule` init,
making the L1 translation closed-loop "tweak fn → re-eval →
diff against oracle" iteration tight enough that the
agent-velocity compression ratio (~6x per ADR-0054 §8.5) holds in
practice for Phase J onward.

## 3. Decision

Adopt the POC plan's 6-day implementation path verbatim per
`dispatches/2026-05-18-phase-i-repl-jit-poc-plan.md` §6. The plan
is design-only at scoping-spike classification; this ADR ratifies
it as the binding decision.

Path summary:

```
User input line
  → parse_str → ast::Module (synthetic `fn __repl_eval_NNNN()`)
  → hir::lower → mir::lower (existing pipelines)
  → cranelift-jit::JITModule.define_function(id, sig, body)
  → JITModule.finalize_definitions()
  → fn_ptr: extern "C" fn() -> i64 = transmute(get_finalized_fn(id))
  → result = fn_ptr();
  → persist let-binding into Session::type_ctx + globals
```

`CodegenMode { Aot, Jit }` switch at the codegen entry; both
modes share the same `Function`-level lowering loop. Module type
swap from `cranelift-object::ObjectModule` to
`cranelift-jit::JITModule` is sufficient — both implement
`cranelift_module::Module`, so existing per-MIR-form lowering
re-binds without surface rewrites.

## 4. Sub-ADR roster

Four sub-ADRs land under this frame:

| ADR | Role | Day budget |
|---|---|---|
| **0056** | Phase I frame (this ADR) — `CodegenMode { Aot, Jit }` switch, `JITModule` lifetime, lazy-init policy, REPL session integration. | day 7 ratify |
| **0056a** | Cranelift JIT crate wire — `cranelift-jit = "0.131"` dep add, `JITBuilder` + symbol resolution, minimal arithmetic round-trip (`1 + 2 * 3` JIT-evaluated). | day 1-2 |
| **0056b** | Control-flow + stdlib lowering reuse — verify existing MIR → Cranelift lowering binds against `JITModule` for `if` / `while` / `for` + PRELUDE intrinsic-rewrite; removes one-shot AOT delegation entirely. | day 3-5 |
| **0056c** | REPL session state machine — `Session::user_funcs: HashMap<String, FuncId>` + `globals: HashMap<String, JitGlobalSlot>` + `type_ctx: TypeCheckCtx` cross-turn persistence + redefinition semantics. | day 6 |

Sub-ADRs ratify sequentially. Frame ADR (this one) ratifies on
0056a dispatch per `ratification_path`.

## 5. Risk register

Three top risks per scoping spike §8 (verbatim from
`dispatches/2026-05-18-phase-i-repl-jit-poc-plan.md`):

1. **Cranelift JIT API surface vs. `cranelift-object` divergence.**
   Both implement `cranelift_module::Module`, but `JITModule`
   exposes `get_finalized_function(id) -> *const u8` (raw-pointer
   `transmute`) while `ObjectModule` emits relocatable bytes.
   JIT-mode error semantics (panic on `transmute`-mismatch,
   SIGSEGV on signature drift) differ fundamentally. **Mitigation:**
   extensive `extern "C"` ABI validation tests; pin one fn
   signature per primitive return type (i64 / f64 / *const u8 /
   unit); reject any input whose return type doesn't match the
   4-arm table → fall back to interpreter. Owned by 0056a.

2. **Persisting MIR locals across REPL turns.** Each
   `__repl_eval_NNNN` is a fresh JIT compilation; locals do NOT
   persist — only `globals` table entries do. REPL must rewrite
   any user-typed `EXPR` to address globals at HIR-lower time
   (bare `x` → `__globals.x` if `x` is in `Session::type_bindings`).
   Failure mode: silent stale-value reads on shadowing miss.
   **Mitigation:** comprehensive shadowing corpus (let-rebind,
   scoped if-block-binding, fn-arg shadowing) in 0056c.

3. **Fn-redefinition mid-call sees old FuncId.** If `fact(5)` is
   in-flight and user redefines `fact` — Cranelift JIT does NOT
   swap pointers atomically; in-flight calls return via the old
   FuncId. A recursive `fact` mid-call still sees the OLD body.
   **Mitigation:** document via ADR-0056c §"Decision"; reject
   re-definition of currently-on-stack fns with a clear
   diagnostic; matches Python REPL semantics.

## 6. Phase I × Phase J handoff

Phase J (LSP, ADR-0057 per ADR-0054 §5) blocks on Phase I for one
specific contract: the **incremental `TypeCheckCtx`** that
survives across REPL turns.

Per ADR-0054 §9.1 "OVERLAP rules":

> Phase J blocks on Phase I. LSP `textDocument/hover` +
> `textDocument/completion` need incremental type-check context
> that Phase I produces (REPL Session state machine is the
> precedent for incremental update).

Operational requirement: `Session::type_ctx` (ADR-0056c §5) MUST
be `Clone + Send` so the LSP server (Phase J) can fork a
per-`textDocument/hover` snapshot without contending on the live
REPL session. The shape contract:

```rust
pub struct Session {
    type_ctx: TypeCheckCtx,            // Clone + Send required
    user_funcs: HashMap<String, FuncId>,
    globals: HashMap<String, JitGlobalSlot>,
    // ... ADR-0029 fields preserved
}

impl Session { pub fn type_ctx(&self) -> &TypeCheckCtx; }
```

ADR-0056c is the binding sub-ADR for this contract; Phase J
ADR-0057b consumes it. Failure to ship `Clone + Send` here forces
Phase J to re-derive the entire ctx per LSP request — defeats the
incremental-typing premise.

## 7. Pre-dispatch acceptance gate

Per scoping spike §9, four gates must be 🟢 at dispatch eve:

- **Phase G fully closed** ✓ — v0.3.0 shipped at `8b4366c`
  (Cargo.lock regenerated for release). Wave-2 round-2 ADR-0052g
  + ADR-0052d method-call-sugar impl all `accepted`.
- **M11.2 FnRef::Call path verified** ✓ — ADR-0034 `accepted` at
  `ea15683`; `examples/fib.cb` recursive form 🟢 DONE per
  ADR-0034 §"Decision Option 3" forward-declaration trick.
  Phase I gains no new obligation on this path — reuse-only.
- **`cranelift-jit = "0.131"` dep adds clean** — 30-min cargo POC
  on self-hosted runner (per ADR-0054 §10 bullet 2). Confirms no
  version-skew vs the already-pinned
  `cranelift-{codegen,frontend,module,object} = "0.131"` quartet
  in `crates/cobrust-codegen/Cargo.toml` (verified at HEAD
  `8b4366c`). Cold-cargo-build wall-time delta <30s required.
- **`cobrust check` + `cobrust build` + `cobrust repl` smoke** all
  🟢 on M14 50-session corpus at Phase I dispatch eve.

If any gate fails: Phase I slips one wave per CLAUDE.md §6
"Provenance-or-it-didn't-happen". Do not dispatch under
amber-gate state.

## 8. Consequences

### 8.1 Positive

- M14.1 deferral (ADR-0029 §"Evaluation surface" ❌ rows) closes
  cleanly via 4-sub-ADR sequence.
- L1 translation closed-loop iteration speeds ~50ms/call →
  <50ms/turn warm — material agent-velocity multiplier for Phase J+.
- `Session::type_ctx: Clone + Send` contract is the shipped
  Phase J input; incremental type-check infrastructure shakes
  out under the lighter REPL surface before LSP consumes it.
- Reuse-only on M11.2 FnRef path (ADR-0034) — zero new MIR or
  HIR surface obligations.
- Cranelift JIT integration is well-trodden (used by
  `rustc_codegen_cranelift` + `wasmtime`); ADR-0054 §4.4 risk
  rating "Low" holds.

### 8.2 Negative

- `cranelift-jit` adds ~150KB to release-mode `cobrust repl`
  binary; cold-start budget (ADR-0029 <200ms bar) must hold via
  lazy `JITModule` init on first non-introspection statement.
- ABI mismatch in JIT mode is SIGSEGV not type-error;
  validation-test surface in 0056a is non-trivial.
- Fn-redefinition mid-call diagnostic (risk 3) is a new
  error-class users must learn — minor §2.5 cost (training data
  for Python REPL is silent on this).

### 8.3 Neutral

- `CodegenMode { Aot, Jit }` enum is internal to
  `cobrust-codegen`; no public API surface change.
- 50-session golden corpus (ADR-0029 §"Tab completion sources"
  precedent) extends by ~20 sessions covering M14.1 forms;
  existing 50 stay green by construction (directives never trigger
  JIT; simple expr eval can stay on interpreter fast-path).
- v0.4.0 release tag binding on Phase H + Phase I joint closure
  (per ADR-0054 §9 critical-path); not on Phase I alone.

## 9. Dispatch readiness

Per ADR-0054 §4.2 + §8.5 (agent-velocity ~8x compression vs
~2-month human estimate):

| Sub-ADR | TEST hours | DEV hours | Wall |
|---|---|---|---|
| 0056a (JIT crate wire) | 4 | 8 | 1-2 days |
| 0056b (control-flow + stdlib) | 8 | 16 | 3-5 days |
| 0056c (session state machine) | 4 | 8 | 1 day |
| 0056 frame ratify | 2 | 0 | 1 day |
| **Total** | **18** | **32** | **~1 week** |

Matches ADR-0054 §4.2 "1 week agent-velocity" claim.

## 10. Why now

- Phase G closed (v0.3.0 shipped at `8b4366c`).
- Phase H + Phase I can run **parallel** (different crates per
  ADR-0054 §9.1) — Phase I dispatch unblocks 1-week wall of
  productive overlap.
- Scoping spike (`dispatches/2026-05-18-phase-i-repl-jit-poc-plan.md`)
  ratifies POC viability; 4-sub-ADR roster is design-locked.
- ADR-0029 §"Evaluation surface" honesty audit has marked M14.1
  rows ❌ since 2026-04-30 — un-deferral is overdue and unblocks
  Phase J `textDocument/hover` incremental-ctx contract (§6).

— P9 Tech Lead, 2026-05-18
