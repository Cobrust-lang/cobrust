---
doc_kind: dispatch
title: "Phase I — REPL JIT (M14.1 closure) POC plan"
date: 2026-05-18
status: planning
parent_adr: 0054
sub_adrs: [0056, 0056a, 0056b, 0056c]
last_verified_commit: bc10842
relates_to: [adr:0019, adr:0023, adr:0029, adr:0034, adr:0051, adr:0054]
authored_by: P9 Tech Lead
classification: scoping spike (NOT an ADR)
---

# Phase I — REPL JIT (M14.1 closure) POC plan

Pre-Phase-I scoping spike per ADR-0054 §10 bullet 2. Design-only.
Sub-ADRs 0056 / 0056a / 0056b / 0056c land sequentially under the
Phase I frame post-acceptance.

## 1. Goal

Lift `cobrust repl` from ADR-0029's narrow HIR-interpreter (literals
+ arithmetic + bound-var + let only) to full Turing-complete evaluation
via a Cranelift JIT runtime. User experience target:

```text
>>> let x = 1 + 2
>>> x.pow(3)
27
>>> fn fact(n: int) -> int: return 1 if n <= 1 else n * fact(n - 1)
>>> fact(10)
3628800
```

Each statement compiles + runs **<50ms warm** (lazy `JITModule` init on
first non-introspection stmt). ADR-0029 cold-start ~10ms release
preserved — `:type / :ast / :hir / :mir` never trigger JIT bring-up.

## 2. Surface today (what ADR-0029 ships)

- AST-walking HIR interpreter via `repl::Session::step()` returning
  `StepResult::{Done, Continue, Quit, Error}`.
- Evaluation surface per ADR-0029 §"Evaluation surface" table: literals,
  binary arithmetic, comparisons, boolean ops, var-read, `let X = EXPR`,
  `print(<literal>)` via one-shot AOT delegation.
- **Missing** (deferred to M14.1 per ADR-0029 ❌ rows): `if` / `else`,
  `while`, `for`, user `fn`-def, comprehensions, collections, stdlib
  calls beyond `print(<literal>)`.
- One-shot AOT path for stdlib delegation is ~50ms cold cache —
  acceptable for `print`, not for tight loops or `fact(10)` recursion.

## 3. JIT minimal viable path

```
User input line
  → parse_str → ast::Module (synthetic `fn __repl_eval_NNNN() -> T: …`)
  → hir::lower → mir::lower (reuses existing pipelines)
  → cranelift-jit::JITModule.define_function(__repl_eval_NNNN, sig, body)
  → JITModule.finalize_definitions()
  → let fn_ptr: extern "C" fn() -> i64 = mem::transmute(get_finalized_function(id))
  → let result = fn_ptr();
  → print formatted result; persist let-binding into TypeCtx
```

Per-line wrap shape:

- Expression `x.pow(3)` → `fn __repl_eval_0001() -> int: return x.pow(3)`.
- `let X = EXPR` → `fn __repl_eval_0002() -> T_X: return EXPR`; capture
  return value into `Session::type_ctx` binding for `X`.
- `fn fact(...)` → standalone JIT fn registered in
  `Session::user_funcs: HashMap<String, FuncId>` for later call resolution.

Prior bindings (`x = 3`) are captured **by-name via global slot lookup**,
NOT closure-over-stack. Each REPL turn allocates fresh JIT memory;
prior values live in `Session::globals: HashMap<String, JitGlobalSlot>`
(`Module::declare_data` + `define_data` per binding). The synthetic body
reads `x` via the global-slot dispatch — no implicit closure capture.

## 4. Reuses ADR-0034 (M11.2 FnRef::Call lowering)

ADR-0034 §"Decision Option 3" forward-declaration trick is the load-
bearing reuse: every user `fn` is `module.declare_function(name, Export,
sig)` at definition time, populating `user_funcs: HashMap<u32, FuncId>`.
At call site, `Operand::Constant(Constant::FnRef(id))` in MIR lowers to
`module.declare_func_in_func(...)` + `ins().call(...)` — branch already
shipped (verified at HEAD `bc10842`).

Phase I swaps the **module type** from `cranelift-object::ObjectModule`
to `cranelift-jit::JITModule`. Both implement `cranelift_module::Module`,
so existing lowering paths re-bind to JIT without form-level rewrites.
A new `enum CodegenMode { Aot, Jit }` switch at the backend entry
selects; both modes share the same `Function`-level lowering loop.

## 5. Incremental type-context

REPL session persists an evolving `TypeCheckCtx` across statements:

- After each `let X = EXPR`, `Session::type_ctx` gains `X: Ty`. ADR-0029
  threads `bindings: HashMap<String, Value>`; Phase I adds parallel
  `type_bindings: HashMap<String, Ty>` via stateful re-entrant
  `cobrust-types::check::TypeCheckCtx` (currently `:type EXPR` builds a
  fresh ctx; Phase I mutates a long-lived one).
- Re-definition (`let x = "abc"` after `let x = 1`) shadows old; global-
  slot table retains new `Ty` + JIT data slot.
- Re-definition of a `fn` supported via `Module::clear_context` + re-
  declare; existing call sites keep the old FuncId until next REPL turn
  (matches Python REPL semantics).

## 6. POC scope (~1 week)

| Day | Deliverable | Sub-ADR |
|---|---|---|
| 1-2 | `cranelift-jit = "0.131"` dep landed; arithmetic-only `eval_expr` JIT path; single-fn compile + invoke for `1 + 2 * 3`. | 0056a |
| 3-4 | Control-flow forms (`if` / `else` / `while` / `for`) JIT-evaluable via existing MIR lowering; cross-form correctness on M2 corpus. | 0056b |
| 5 | Stdlib calls (`print`, `len`, `int`, `str`, `float`, `bool`, …) via existing PRELUDE intrinsic-rewrite path. Removes one-shot AOT delegation entirely. | 0056b |
| 6 | User `fn`-def in REPL. `Session::user_funcs` table + `FnRef` resolution via ADR-0034. Recursive + mutually-recursive both green. | 0056c |
| 7 | 5-gate (build / behavior on extended corpus / cold-start budget / <50ms warm / bilingual docs); ratify ADR-0056 frame + sub-ADRs. | 0056 |

ADR-0029's 50-session golden corpus extends by ~20 sessions covering
M14.1 forms; existing 50 stay green by construction (directives never
touch JIT; simple expr eval can stay on interpreter fast-path).

## 7. Sub-ADR roster

| ADR | Role |
|---|---|
| 0056 | Phase I frame — `CodegenMode { Aot, Jit }` switch, `JITModule` lifetime, lazy-init policy, REPL session integration. |
| 0056a | Cranelift JIT crate wire — `cranelift-jit = "0.131"` dep, `JITBuilder` + symbol resolution, minimal arithmetic round-trip. |
| 0056b | Control-flow + stdlib lowering reuse — verify existing MIR → Cranelift binds against `JITModule` without surface change. |
| 0056c | REPL session state machine — `Session::user_funcs` + `globals` + `type_ctx` cross-turn persistence semantics. |

## 8. Risk register (top 3)

1. **Cranelift JIT API surface vs. `cranelift-object` divergence.** Both
   implement `cranelift_module::Module`, but `JITModule` exposes
   `get_finalized_function(id) -> *const u8` (raw-pointer `transmute`)
   while `ObjectModule` emits relocatable bytes. JIT-mode error semantics
   (panic on `transmute`-mismatch, SIGSEGV on signature drift) differ
   fundamentally. Mitigation: extensive `extern "C"` ABI validation tests;
   pin one fn signature per primitive return type (i64 / f64 / *const u8
   / unit); reject any input whose return type doesn't match the 4-arm
   table → fall back to interpreter.

2. **Persisting MIR locals across REPL turns.** Each `__repl_eval_NNNN`
   is a fresh JIT compilation; locals do NOT persist — only `globals`
   table entries do. REPL must rewrite any user-typed `EXPR` to address
   globals at HIR-lower time (bare `x` → `__globals.x` if `x` is in
   `Session::type_bindings`). Failure mode: silent stale-value reads on
   shadowing miss. Mitigation: comprehensive shadowing corpus
   (let-rebind, scoped if-block-binding, fn-arg shadowing) in 0056c.

3. **Error-recovery when user redefines a fn mid-session.** If
   `fact(5)` is in-flight and user redefines `fact` — Cranelift JIT
   does NOT swap pointers atomically; in-flight calls return via the
   old FuncId. A recursive `fact` mid-call still sees the OLD body.
   Mitigation: document via ADR-0056c §"Decision"; reject re-definition
   of currently-on-stack fns with a clear diagnostic.

## 9. Pre-dispatch acceptance gate

Before ADR-0056 frame dispatches:

- **Phase G fully closed.** Wave-2 round-2 ADR-0052g landed at `bc10842`
  ✓ ; ADR-0052d method-call sugar impl is the only remaining Phase G
  item — Phase I BLOCKS on its closure (else `x.pow(3)` doesn't lower).
- **M11.2 FnRef::Call path verified end-to-end.** ADR-0034 accepted at
  `ea15683`; `examples/fib.cb` recursive form 🟢 DONE per ADR-0034
  §"Evidence". Phase I gains no new obligation — reuse-only.
- **`cranelift-jit = "0.131"` dep adds clean.** 30-min cargo POC on DG
  workstation (per ADR-0054 §10 bullet 2). Confirms no version-skew vs
  the already-pinned `cranelift-{codegen,frontend,module,object} = "0.131"`
  quartet in `crates/cobrust-codegen/Cargo.toml`. Cold-cargo-build wall-
  time delta <30s required.
- **`cobrust check` + `cobrust build` + `cobrust repl` smoke** all 🟢 on
  M14 50-session corpus at Phase I dispatch eve.

If any gate fails: Phase I slips one wave; do not dispatch under amber-
gate state (per CLAUDE.md §6 "Provenance-or-it-didn't-happen").

— P9 Tech Lead, 2026-05-18
