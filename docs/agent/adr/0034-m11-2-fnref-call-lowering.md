---
doc_kind: adr
adr_id: 0034
title: M11.2 — Constant::FnRef Call lowering for user-defined fns
status: accepted
date: 2026-05-09
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0034: M11.2 — Constant::FnRef Call lowering for user-defined fns

## Context

Constitution §1.1 ("syntactically familiar to Python users") is not
fully realised at HEAD `540ed65`: the canonical proof-of-life
recursion example `examples/fib.cb` still runs in **iterative**
form because user-defined fn calls (i.e. `Constant::FnRef(u32)`
operands) lower to a zero-pointer placeholder in
`crates/cobrust-codegen/src/cranelift_backend.rs:1414`, and
`lower_call` line 843-845 explicitly comments that
`Constant::FnRef` is deferred ("M11 will materialize the FnRef
path").

Concretely, today only:
- `Constant::Str(name)` callees → real Cranelift `call` via
  `extern_funcs` (ADR-0024) or `runtime_funcs` (ADR-0027 §4)

are wired. User-defined cross-fn calls (e.g. `fib(n-1)` calling
the same module's `fib`) silently take the zero-pointer
placeholder and never produce a real call instruction.

Audit #2 (review-claude 二次审计 2026-05-09 §2) flagged this:
`examples/fib.cb` is currently **🟡 PARTIAL** in `findings/
examples-literal-print-debt.md` — fizzbuzz is full real-algorithm
✅ but fib is iterative (workaround) until `Constant::FnRef` Call
lowering lands. `M11.1` (ADR-0030, while-if codegen fix) did NOT
touch this surface; ADR-0033 (Option C root-primitive type
inference) also did NOT touch this surface. M11.2 is the smallest
sprint that can lift the audit-#2 partial closure to ✅ DONE.

ADR-0033 just landed (commit `3392eb5`) bringing
`inferred_locals` fixed-point inference. **The recursive fn
landscape interacts with this fix**: a recursive fn returning
through a chain of temps still needs `inferred_locals` to
converge. Now is the right moment for M11.2 because the type
inference plumbing is done; we just need to wire the call symbol.

## Options considered

1. **Lower in HIR** — synthesise an intrinsic-style call from the
   HIR `Call` node; have HIR resolve user fn names to a synthetic
   `Constant::Str` callee that codegen already handles via
   `extern_funcs`. **Rejected** — dirty: it conflates user-defined
   fns (compile-time-known) with `extern` symbols (link-time-known)
   and loses the structural FnRef distinction the IR carefully
   makes.

2. **Lower in MIR** — change MIR's `Call` lowering to materialise
   `Constant::FnRef(id)` into a fully-typed call node carrying the
   target fn's signature inline. **Rejected** — out-of-scope MIR
   refactor: MIR already represents the call correctly; the gap
   is purely in codegen's translation layer.

3. **Lower in codegen via forward-declaration pass** — extend
   codegen with a two-pass walk per `Module`:
   - **Pass 1 (declare)**: iterate all `Function` declarations,
     call `module.declare_function(name, Linkage::Export, sig)`
     to obtain FuncId; store in a per-module
     `user_funcs: HashMap<u32, FuncId>` keyed on the
     `Constant::FnRef(u32)` id. This is the "forward declaration"
     trick — by the time pass 2 starts, every user fn's FuncId
     exists, so any fn body that calls another (or itself) can
     resolve the callee unambiguously.
   - **Pass 2 (define)**: iterate fn bodies in their existing
     order, calling `module.define_function(...)`. Inside
     `lower_call`, add a new branch:
     ```rust
     if let Operand::Constant(Constant::FnRef(id)) = func {
         let func_id = user_funcs.get(id).copied().unwrap();
         let func_ref = obj.declare_func_in_func(func_id, builder.func);
         // … lower args, emit call, etc — mirror the
         // extern_funcs branch (line 535-547) bytewise.
     }
     ```

   **Chosen.** Surgical (only codegen layer touched). Solves
   recursion via classical compiler forward-declaration. No MIR
   refactor. No HIR re-routing.

## Decision

**Option 3.** Implementation map:

```
crates/cobrust-codegen/src/cranelift_backend.rs
  ├── struct ObjectModuleBackend (or whatever the per-module
  │   compilation context is named)
  │   + user_funcs: HashMap<u32, cranelift_module::FuncId>   // NEW
  │
  ├── fn compile_module (or equivalent entry point)
  │   + Pass 1: for each Function in module, declare_function
  │     and populate user_funcs                              // NEW
  │   + Pass 2: for each Function, define_function (existing
  │     loop), with user_funcs available to lower_call
  │
  └── fn lower_call
      + branch: Operand::Constant(Constant::FnRef(id))       // NEW
        → declare_func_in_func + ins().call (mirrors extern
          path at line 535-547)
```

### Interaction with ADR-0033 `inferred_locals` fixed-point

ADR-0033 added per-fn `inferred_locals: HashMap<LocalId, ir::Type>`
that converges via fixed-point during pass 2's body lowering.
M11.2's forward-declaration pass operates **at the fn-signature
boundary** (return type, parameter types) which is statically
declared in the AST/HIR — these never participate in
`inferred_locals` because `inferred_locals` is local-level, not
fn-level. The two layers are **orthogonal**:

- Forward declaration: provides the *callee's* return type via
  the declared sig.
- Fixed-point inference: resolves the *caller's* `Ty::None` temps
  that hold the call's result.

In the recursive case (`fib(n-1) + fib(n-2)`):
- The callee `fib` has declared sig `(i64) -> i64`. Forward
  declaration pins this in pass 1.
- The caller's `_T = call fib(n-1)` has `_T: Ty::None` →
  `inferred_locals` learns `_T = I64` from the call's signature
  return type during fixed-point pass 1 of body lowering. No
  regression.

**Acceptance: a corpus case must specifically exercise this
interaction** (`fnref_inferred_locals_recursive_chain` per the
test list below) so that any future regression of either layer
is caught immediately.

## Done means

1. `examples/fib.cb` rewritten back to recursive form per
   `findings/examples-literal-print-debt.md` §"Acceptance bar":
   ```cobrust
   fn fib(n: i64) -> i64:
       if n < 2:
           return n
       return fib(n - 1) + fib(n - 2)

   fn main() -> i64:
       print("fib(10) =")
       print_int(fib(10))
       return 0
   ```
   `cobrust build examples/fib.cb && ./target/cobrust/fib` produces
   stdout **bit-identical** to `fib(10) =\n55\n` (newline-terminated).

2. New regression corpus
   `crates/cobrust-codegen/tests/fnref_call_corpus.rs` with **≥10
   cases** including all of:
   - `fnref_single_arg_recursive` (fib)
   - `fnref_multi_arg_recursive` (truncated ackermann)
   - `fnref_zero_arg_recursive` (depth-counter)
   - `fnref_direct_recursion` (fib structural variant)
   - `fnref_mutual_recursion` (is_even / is_odd) — verifies
     forward declaration enables BOTH directions
   - `fnref_chain_call` (a → b → c → leaf, no recursion)
   - `fnref_inferred_locals_recursive_chain` — recursive fn whose
     return value passes through a `Ty::None` temp before being
     returned; verifies ADR-0033 + ADR-0034 interaction is
     regression-free
   - `fnref_no_args_no_return` (`fn side_effect() -> ()`-style
     unit-return semantics in Cobrust)
   - `fnref_returns_call_of_other` (return another fn's result
     directly, no temp)
   - `fnref_negative_arg` (recurse with `n - 1`-style; specifically
     exercises operand chain through arithmetic Ty::None temp)

3. `findings/examples-literal-print-debt.md` status updated
   from 🟡 PARTIAL to ✅ DONE with cross-ref to this ADR's commit.

4. ADR-0034 stamped `last_verified_commit` to the merge SHA.

5. All 5 standard gates green:
   - `cargo fmt --all --check`: 0
   - `cargo clippy --workspace --all-targets --locked -- -D warnings`: 0
   - `cargo build --workspace --all-targets --locked`: 0
   - `cargo test --workspace --locked`: 0; total count goes UP
     (≥ 1,783 + 10 new corpus = ≥ 1,793)
   - `bash scripts/doc-coverage.sh`: 0

6. Triple-tree doc sync:
   - `docs/agent/modules/codegen.md`: append note on `user_funcs`
     two-pass approach + cross-ref to ADR-0034 + ADR-0033
   - `docs/human/zh/architecture.md` + `docs/human/en/architecture.md`:
     M11.2 row in milestones table (if present)
   - `scripts/doc-coverage.sh` extended with M11.2 surface check
     if any new public item shows up; otherwise no change required

## Consequences

### Positive

- Audit #2 (`examples-literal-print-debt`) lifts to ✅ DONE.
- Constitution §1.1 "syntactically familiar to Python users" is
  more truthfully realised: real recursion now works.
- Mutual recursion enabled (forward declaration is what makes
  this possible at all).
- `Constant::FnRef` no longer a stub — closes the M11 amendment
  TODO at `cranelift_backend.rs:843-845`.

### Negative

- Codegen complexity grows: a new two-pass invariant ("declare
  before define") must be respected by future codegen edits.
  Document inline in code + ADR.
- Edge: Cobrust's `Module` may grow to N user-defined fns; pass
  1 cost is O(N) declare calls. Negligible.

### Neutral

- `Constant::FnRef` continuing to lower to `iconst(ptr, 0)` in
  `lower_constant` (line 1414) is preserved as fallback for
  first-class FnRef use (e.g. fn-as-value if Cobrust ever
  supports it). The short-circuit in `lower_call` ensures the
  zero-pointer never executes when FnRef is the callee — only
  when it's a value being stored / passed.

### Risk

- **ADR-0033 regression**: if `infer_local_types` doesn't see the
  call's declared sig as the temp's incoming type, the fixed-point
  may converge to a wrong default. Mitigation: corpus case
  `fnref_inferred_locals_recursive_chain` directly exercises this
  path; gate failure means CTO blocks merge.

## Evidence

- ADR-0023 (M9 codegen) + ADR-0024 (M10 CLI driver / codegen
  amendments) + ADR-0027 (M12.x codegen amendments) + ADR-0030
  (M11.1 while-if fix) + ADR-0033 (M11.2 prerequisite — Ty::None
  Option C root primitive)
- `crates/cobrust-codegen/src/cranelift_backend.rs:283` (FnRef
  type stub), `:843-845` (lower_call deferred comment), `:1414`
  (lower_constant zero-pointer placeholder), `:377` + `:555`
  (existing `declare_function` API usage), `:562-586`
  (existing FuncRef map management as template)
- `crates/cobrust-mir/src/tree.rs:320` (`FnRef(u32)` definition)
- `findings/examples-literal-print-debt.md` (audit #2 anchor)

## Cross-references

- ADR-0019 §"Definition of usable" three-tier anchor — M11.2
  consolidates the **spirit** tier (real algorithm in fib too,
  not just fizzbuzz)
- ADR-0033 (depended-on; orthogonality argument here)
- ADR-0030 (template — last codegen sprint with same shape)
- `findings/examples-literal-print-debt.md` (audit #2 closure)
- `findings/m12-x-while-if-codegen-regression.md` (M11.1 sibling)
