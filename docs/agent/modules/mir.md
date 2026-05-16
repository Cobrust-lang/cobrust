---
doc_kind: module
module_id: mod:mir
crate: cobrust-mir
last_verified_commit: e85630f
dependencies: [mod:hir, mod:types, adr:0020, adr:0027, adr:0041]
---

# Module: mir

## Purpose

Mid-level IR: control-flow-explicit form fed to `mod:codegen`. The
form `M9` codegen will consume. Locals + basic blocks + terminators
+ drop schedule + discharged borrow-check obligations.

## Status

- **M8 — delivered.** ADR-0020 pinned the shape; implementation
  matches. 157 tests across `lower_forms / mir_well_formed /
  mir_ill_formed / mir_fuzz` pass.

## Public surface (M8)

```rust
// Top-level entry — typed-HIR → MIR.
pub fn lower(typed: &cobrust_types::TypedModule) -> Result<Module, MirError>;

// Borrow check pass — discharges B1..B5 obligations.
pub fn borrow_check(body: &Body) -> Result<(), MirError>;

// Drop schedule pass — 5-phase, mutates Body to insert Drop terminators.
pub fn compute_drop_schedule(body: &mut Body) -> Result<(), MirError>;

// IR shape.
pub struct Module { pub bodies: Vec<Body> }

pub struct Body {
    pub def_id: DefId,
    pub name: String,
    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub return_local: LocalId,
    pub param_count: usize,
    pub span: Span,
}

pub struct LocalDecl { pub id: LocalId, pub name: String, pub ty: Ty,
                      pub mutable: bool, pub span: Span }
pub struct LocalId(pub u32);
pub struct BlockId(pub u32);

pub struct BasicBlock { pub id: BlockId, pub statements: Vec<Statement>,
                       pub terminator: Terminator, pub span: Span }

pub struct Statement { pub kind: StatementKind, pub span: Span }
pub enum  StatementKind {
    Assign { place: Place, rvalue: Rvalue },
    StorageLive(LocalId),
    StorageDead(LocalId),
    Nop,
}

pub enum Terminator {
    Goto(BlockId),
    SwitchInt { operand: Operand, cases: Vec<(SwitchValue, BlockId)>, otherwise: BlockId },
    Return,
    Call { func: Operand, args: Vec<Operand>, destination: Place,
           target: BlockId, unwind: Option<BlockId> },
    Drop { place: Place, target: BlockId },
    Unreachable,
    Assert { cond: Operand, expected: bool, msg: AssertKind, target: BlockId },
}

pub enum SwitchValue { Bool(bool), Int(i64), Adt(u32) }
pub enum AssertKind { DivisionByZero, Overflow, BoundsCheck, Unreachable }

pub struct Place { pub local: LocalId, pub projections: Vec<Projection> }
pub enum  Projection { Field(usize), Index(Operand), Deref, Discriminant }

pub enum Operand { Copy(Place), Move(Place), Constant(Constant) }

pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    UnaryOp(UnOp, Operand),
    Aggregate(AggregateKind, Vec<Operand>),
    Cast(CastKind, Operand, Ty),
    Ref(BorrowKind, Place),
    Discriminant(Place),
    Len(Place),
    NullaryOp(NullaryOp),
}

pub enum BorrowKind { Shared, Mut }
pub enum BinOp { Add, Sub, Mul, Div, FloorDiv, Mod, Pow, MatMul,
                 Shl, Shr, BitAnd, BitOr, BitXor,
                 Eq, NotEq, Lt, LtEq, Gt, GtEq, And, Or, In, NotIn }
pub enum UnOp { Plus, Neg, BitNot, Not }
pub enum CastKind { IntToFloat, FloatToInt, BoolToInt, IntToBool,
                    StrToBytes, BytesToStr }
pub enum AggregateKind {
    Tuple, List, Set, Dict, Record,
    Adt(u32, u32),
    FormatString,
}
pub enum NullaryOp { SizeOf, AlignOf }

pub enum Constant {
    Bool(bool), Int(i64), Float(u64), Imag(u64),
    Str(String), Bytes(Vec<u8>), None, FnRef(u32),
}

// Errors — ADR-0020 §"Public surface".
pub enum MirError {
    UseAfterMove { local: u32, span: Span },
    UseAfterDrop { local: u32, span: Span },
    ConflictingMutBorrow { local: u32, span: Span },
    SharedMutOverlap { local: u32, span: Span },
    EscapingBorrow { local: u32, span: Span },
    DropMissing { local: u32, span: Span },
    DoubleDrop { local: u32, span: Span },
    FieldOutOfBounds { place: PlaceDebug, span: Span },
    UnresolvedDefId { def_id: u32, span: Span },
    NonExhaustiveSwitch { span: Span },
    Internal(String),
}

pub struct PlaceDebug { pub local: u32, pub projection_chain: String }
```

## Lowering rules (per ADR-0003 form, per ADR-0020 §"Lowering rules")

Module-level (1–6):

| Form | Lowering |
|---|---|
| 1 `module` | one `Module { bodies }` with one `Body` per fn + synthetic init body |
| 2 `import_stmt` | `LocalDecl` for the imported `DefId`; M8 emits no MIR-side semantics |
| 3 `fn_def` | one `Body` per fn; entry block = `BasicBlock(0)` |
| 4 `class_def` | each method becomes a `Body`; class itself is an ADT registration |
| 5 `decorator` | unwrap recursively; inner item lowers as if undecorated |
| 6 `type_alias` | no MIR emission |

Statement (7–19):

| Form | Lowering |
|---|---|
| 7 `let_stmt` | `LocalDecl` + `StorageLive` + `Statement::Assign(Place, Rvalue::Use(...))` |
| 8 `assign_stmt` | `Statement::Assign(target_place, rhs_rvalue)` |
| 9 `if_stmt` | each arm: `SwitchInt(cond, [(true, body)], otherwise: next)`; arms join at merge block |
| 10 `while_stmt` | `pre → header → SwitchInt → [body, exit]`; body ends in `Goto(header)` |
| 11 `for_stmt` | **M12.x** (ADR-0027 §4 for-protocol — `Iterator` trait surface backed by the four stdlib iter types): `_iter = iter_expr`; `Call(__cobrust_iter_init, _iter)` → `_handle`; loop header `Call(__cobrust_iter_next, _handle)` → `_opt`; `SwitchInt(_opt, [Bool(false) → exit], body_block)`; body ends in `Goto(header)` |
| 12 `match_stmt` | decision tree of `SwitchInt` on discriminants + projections |
| 13 `with_stmt` | `Call(__enter__) → body → Call(__exit__)`; binding is a `let` |
| 14 `try_stmt` | each handler = `unwind` target; finally = on every exit edge |
| 15 `return_stmt` | `Statement::Assign(_return, ...)` then `Terminator::Return` |
| 16 `break_continue` | `Goto(loop_exit)` / `Goto(loop_header)` — looks up `(header_bb, exit_bb)` pair from the top of `BodyBuilder::loop_stack` (L201-202 of `lower.rs`). Pushed at While entry (L712) + For entry (L824); popped on natural exit. ADR-0050a §"Semantics": innermost-loop binding is implicit because each loop pushes/pops a fresh pair. |
| 17 `raise_stmt` | `Terminator::Unreachable` (panic helper materialises at M11) |
| 18 `pass_stmt` | `StatementKind::Nop` |
| 19 `expr_stmt` | synthesize `let _tmp = expr` + discard |

Pattern (20): projection-and-test sequences; binding pattern allocates a local, copies via `Field(idx)` projection.

Expression (21–30):

| Form | Lowering |
|---|---|
| 21 `literal_expr` | `Operand::Constant(Constant::*)` |
| 22 `fstring_expr` | `Rvalue::Aggregate(AggregateKind::FormatString, parts)` — codegen materializes via `__cobrust_str_new` + `__cobrust_fmt_int / fmt_float / ...` per ADR-0027 §5; runtime allocation routed through `__cobrust_alloc` |
| 23 `name_expr` | `Operand::Copy(Place)` for Copy types, `Operand::Move(Place)` for owning types |
| 24 `collection_expr` | `Rvalue::Aggregate(Tuple|List|Set|Dict, items)` |
| 25 `comprehension_expr` | desugared to fresh accumulator + loop |
| 26 `lambda_expr` | new `Body` + parent gets `Operand::Constant(Constant::FnRef(...))` |
| 27 `call_expr` | `Terminator::Call { func, args, destination, target, unwind }` |
| 28 `access_expr` | `Place` with `Field(idx)` (attr) or `Index(operand)` (index) projection |
| 29 `binary_unary_expr` | `Rvalue::BinaryOp` / `Rvalue::UnaryOp`; integer division emits `Assert(rhs != 0)` first |
| 30 `await_yield_expr` | `await` → placeholder `Call`; `yield` → no-op M8 (M13 binds runtime) |

## Invariants (M8)

- Every `BasicBlock` has exactly one terminator.
- Every `LocalDecl` carries a fully-resolved `Ty`.
- Drop-schedule pass is idempotent.
- Borrow check is terminating (`O(blocks * locals)`).
- Lowering is total over typed-HIR.
- `Module::display()` is deterministic — same input, same output.

## Borrow-check obligations (B1..B5 per ADR-0020 §"Borrow-check proof obligation list")

| # | Discharged by | Error |
|---|---|---|
| **B1** No use after move | `borrow.rs` move tracker | `UseAfterMove` |
| **B2** No two simultaneous mutable borrows | `borrow.rs` borrow stack | `ConflictingMutBorrow` |
| **B3** No mutable + shared overlap | `borrow.rs` borrow stack | `SharedMutOverlap` |
| **B4** Drop after last use | `drop.rs` verification phase | `UseAfterDrop` |
| **B5** No escaping borrow past its scope | `borrow.rs` (intra-procedural at M8) | `EscapingBorrow` |

ADR-0006 §"Soundness proof obligation list" enumerates 9 type-level obligations; items 4–9 are discharged at type-check time (M2). Items 1–3 (Progress, Preservation, Lowering preservation) are flow obligations and project onto MIR-time as B1..B5.

## M11.3 lower_condition extraction (per ADR-0035)

`if` and `while` heads share a single MIR-level root primitive,
`BodyBuilder::lower_condition`, defined in
`crates/cobrust-mir/src/lower.rs`. The primitive lowers a condition
expression `expr` (which may emit auxiliary blocks for division
asserts on `%` / `/` / `//`, short-circuit Boolean evaluation, etc.)
and returns `(Operand, BlockId)` — the cond's resulting Operand and
the block where the Operand is finally available
(`cond_end_block`). The caller is responsible for terminating
`cond_end_block` with the appropriate branch terminator (typically
`SwitchInt`); `cur_block` is left set to `cond_end_block`.

Pre-condition: `self.cur_block` is set to the block where condition
evaluation should begin.
Post-condition: `self.cur_block == Some(cond_end_block)` and the
caller terminates that block.

### Why the primitive

Pre-M11.3 `lower_if` and `lower_loop`'s While arm had divergent
hand-rolled equivalents. `lower_if` correctly captured
`cond_end_block = current_block_id()` after `lower_expr(cond)`
returned (ADR-0030 fix). `lower_loop` instead reset
`self.cur_block` back to the loop `header` block, blindly assuming
the cond's final assigns lived in `header`. For trivial conds (`n > 0`,
`n == 5`) that assumption held; for `<BinOp> == 0`-shape conds
(LC 263 trigger: `while n % 2 == 0`), the inner `Mod` BinOp's
div-assert chain split cond-evaluation across two blocks
(header → assert_target). The While arm then wrote `SwitchInt(_eq)`
into `header` while `_eq`'s assign lived in `assert_target`, so the
SwitchInt read a stale (zero-initialised) value every iteration and
the body never entered.

### Block-flow shape after the fix

For `while <cond_expr>:` where `<cond_expr>` lowers via
`lower_condition` and may emit N >= 0 auxiliary blocks:

```
pre ──goto──▶ header ──[cond chain across 0..N blocks]──▶ cond_end_block
                                                              │
                                              ┌──SwitchInt───┴───┐
                                              ▼                  ▼
                                            body              exit/else
                                              │
                                              └─goto──▶ header (back-edge)
```

The body's back-edge `Goto(header)` is correct: jumping to header
re-enters the full cond-eval chain (header still ends with whatever
terminator `lower_expr(cond)` placed there — either a `SwitchInt`
when the cond is trivial enough that no auxiliary blocks were
emitted, or `Assert(divcond) -> assert_target` flowing into
`assert_target`'s `SwitchInt` for `<BinOp> == 0` shapes), so each
iteration recomputes the cond's value.

### Interaction with prior ADRs

- **ADR-0033 `inferred_locals` fixed-point** (codegen-side
  `Ty::None` resolution): orthogonal. `lower_condition` operates on
  block-flow shape; ADR-0033 operates on operand-type inference.
  Both apply to the same MIR temps (e.g. `_bin: Ty::None`); neither
  affects the other. Verified by corpus case
  `while_condition_through_inferred_locals_chain` in
  `crates/cobrust-codegen/tests/while_condition_corpus.rs`.
- **ADR-0034 `Constant::FnRef` Call lowering**: orthogonal. A
  function-call inside a condition expression lowers via
  `Terminator::Call` (with its own destination block); the
  `lower_condition` helper sees the call's destination as just
  another part of the cond chain and correctly captures the
  post-call block as `cond_end_block`. Verified by corpus case
  `while_binop_with_function_call`.
- **ADR-0030 M11.1 while-leading-if fix**: sibling. M11.1 fixed
  `lower_if`'s block-id arithmetic when `if` is invoked from inside
  a while body. M11.3 ensures both heads route through the same
  primitive, eliminating the entire class of head-shape drift.

## Drop schedule algorithm

ADR-0020 §"Drop schedule algorithm" — 5 phases:

1. **Initialization** — non-`Copy` locals declared in body (params excluded) marked drop-pending.
2. **Move** — `Operand::Move(place)` references → root local moved out → no longer pending.
3. **End-of-scope** — at every `Goto`/`SwitchInt`/`Return`/`Unreachable` edge, insert `Drop` blocks for still-pending locals (LIFO order).
4. **Divergence** — `Unreachable` blocks skip drop insertion.
5. **Verification** — forward-flow check; pending-on-Return → `DropMissing`; double-drop on path → `DoubleDrop`.

## ADR-0041 §H2 + §H6 — semantic compliance amendments

Two MIR-level lowering paths were corrected to match Python semantics
(per claude-desktop integrated handoff §2):

- **§H2 — short-circuit `and` / `or`** (`lower_short_circuit_bool`).
  Pre-fix: `lower_bin` emitted `BinaryOp(And/Or, lhs, rhs)` which
  codegen lowered to `band` / `bor` — both LHS and RHS are eagerly
  evaluated. Post-fix: when `op ∈ {And, Or}`, MIR allocates a result
  local, evaluates LHS, branches on the result, and *conditionally*
  evaluates RHS only when the LHS does not yet determine the answer.
  CFG shape: `pre → SwitchInt(lhs) → [eval_rhs, merge]; eval_rhs →
  merge`.
- **§H6 — comprehension desugar** (`lower_comprehension`,
  `lower_comp_clauses`, `lower_comp_body`). Pre-fix: `ExprKind::Comp`
  lowered to a fresh empty list, body never emitted. Post-fix: real
  loop+append, mirroring the `LoopKind::For` lowering. Calls
  `__cobrust_list_new(8, 0)` upfront, runs the for-protocol on the
  iterator, evaluates guards inline (continue on falsy), and pushes
  each element via `__cobrust_list_append`. Multi-clause
  comprehensions nest via recursion in `lower_comp_clauses`.

## M-F.3.1 — for-loop length-bound index lowering (per ADR-0050b)

ADR-0050b supersedes the ADR-0027 §4 iter-protocol path for
`LoopKind::For`. The new lowering performs length-bound index
iteration directly, without calling `__cobrust_iter_init / next /
drop`:

```
pre_block:
  iter_local := <iter expr>
  Call __cobrust_list_len(iter_local) -> len_local: i64
after_len:
  idx_local: i64 := 0
  declare var_local (loop-var of element type)
  Goto(header)
header:
  cond_local := BinaryOp(Lt, idx_local, len_local)
  SwitchInt(cond_local, [(Bool(true), body)], otherwise: exit)
body:
  Call __cobrust_list_get(iter_local, idx_local) -> var_local
  [lower user body block]
  idx_local := BinaryOp(Add, idx_local, Constant::Int(1))
  Goto(header)
exit:
  [optional else block]
```

### Why this shape

The ADR-0027 iter-protocol used a `__cobrust_iter_next` runtime helper
that returned `i64`, with `0` reserved as the "exhausted" sentinel. For
`list[i64]` iteration this collided with legitimate `0`-valued
elements — the **first** iteration of `for v in range(0, n):` returned
`0`, which the caller's `SwitchInt(Bool(false))` interpreted as "stop"
and exited the loop immediately. The latent bug surfaced under
M-F.3.1's `range(0, n)` corpus.

The length-bound shape sidesteps the sentinel problem entirely: the
exhaustion condition is the explicit `idx < len` comparison, and the
value channel (`__cobrust_list_get`) is free to return any i64
including 0.

### Composition with Phase G iter-protocol expansion

When Phase G lands user-defined `__iter__` traits, the type checker
dispatches between this length-bound primitive (for `Ty::List<_>` iter
sources) and a generic iter-protocol shape (for arbitrary types
implementing `__iter__`). The two paths coexist; nothing in M-F.3.1
needs to be torn out.

### Interaction with comprehensions

`lower_comprehension` / `lower_comp_clauses` still emit the ADR-0027
iter-protocol path (§H6). Comprehensions are a separate desugar
target and their iter sources today are exclusively list-shaped
container expressions whose runtime values are heap pointers (never
the literal 0), so the latent sentinel bug does not surface there.
Closing comprehensions onto the length-bound primitive is a Phase G
follow-up (out of M-F.3.1 scope).

### `range(a, b)` prelude binding

`range(start, stop) -> list[i64]` ships as a real Cobrust prelude
function (not an intrinsic stub) — see `cobrust-cli/src/build.rs::PRELUDE`.
Its body materialises a `list[i64]` of `stop - start` slots via
`list_new(n)` + a population `while` loop using `list_set`. Calls
inside the body to `list_new` / `list_set` are intrinsic-rewritten
on every callsite per ADR-0044 W2 Phase 3; the `range` body itself
survives the intrinsic-rewrite pass and is compiled through normal
MIR / codegen.

3-argument `range_step(start, stop, step)` is deferred to Phase G
alongside iter-protocol expansion.

## Done means (M8 — DONE)

- [x] Every form in ADR-0003 has an explicit lowering rule.
- [x] All 7 terminator variants used.
- [x] Drop schedule passes verification on every test body.
- [x] Borrow check terminates within `O(blocks × locals)` on every test body.
- [x] 46 lower-form golden tests + 55 well-formed + 50 ill-formed + 6 fuzz properties green.
- [x] `COBRUST_M8_FUZZ_LONG=1` 100k cases / property panic-free.
- [x] `adr:0020` accepted; implementation matches.

## Non-goals

- Inter-procedural lifetime tracking (M9 codegen).
- LLVM-style phi nodes (Cobrust uses rustc-style per-block locals).
- Optimization passes (constant folding, DCE, etc.) — also M9+.
- Generator state-machine lowering (M13 structured concurrency).

## ADR-0050a M-F.3.0 — `break` / `continue` MIR contract

| Surface | Anchor |
|---|---|
| Loop-scope stack | `BodyBuilder::loop_stack: Vec<(BlockId, BlockId)>` (`lower.rs` L201-202) — pair = `(header_bb, exit_bb)`. |
| Break lowering | `lower.rs` L419-427 — `StmtKind::Break` → `Terminator::Goto(exit_bb)`; if `loop_stack.is_empty()` returns `MirError::Internal("break outside loop")` (defensive — types should reject earlier). |
| Continue lowering | `lower.rs` L428-436 — `StmtKind::Continue` → `Terminator::Goto(header_bb)`. |
| While push/pop | `LoopKind::While` arm at L712 pushes, L718 pops. |
| For push/pop | `LoopKind::For` arm at L824 pushes, L830 pops. Once M-F.3.1 lands a richer for desugar, the contract is unchanged — break still binds to the for's exit. |
| Unreachable tail | After `break` / `continue` terminate the current block, any subsequent statements in the same source body run through `ensure_open_block`, lowering into a fresh block with no predecessor. Codegen DCE removes them. |

Test corpus: `crates/cobrust-mir/tests/break_continue_mir_corpus.rs`
— 19 cases including 5-level deep nesting (`m10`, `m15`) and goto-target
bounds verification (`m16`).

## Cross-references

- `adr:0020` — MIR shape, terminator taxonomy, drop schedule, borrow obligations (authoritative).
- `adr:0019` — Phase E roadmap; M8 row.
- `adr:0006` — type-system obligations 1–9; B1..B5 project onto items 1–3.
- `adr:0050a` — break/continue contract seal (MIR loop_stack discipline).
- `adr:0050c` — M-F.3.2 list[str] TD-1 closure (Str ownership flip + drop schedule).
- `mod:types` — input.
- `mod:codegen` — output consumer.
- Constitution `CLAUDE.md` §2.2 (drops including GIL/GC), §4.1 (pipeline), §5.1 (elegance), §5.2 (scientific — enumerated obligations), §7 (M2 done means), ADR-0019 (M8..M14 sequencing).

## ADR-0050c M-F.3.2 — Str ownership + list[str] drop schedule

| Surface | Anchor |
|---|---|
| `is_copy` (drop pass) | `drop.rs:142-152` — `Ty::Str` and `Ty::List(_)` REMOVED from the Copy set. Drop pass enumerates them as drop-eligible. |
| `is_copy_type` (lowering operand) | `lower.rs:1909-1934` — `Ty::Str` non-Copy at operand-read time (Move semantics); `Ty::List(_)` kept Copy-at-operand for shared-borrow shapes like `list_set(xs, i, v)`. Honest-debt: use-after-move on list[str] is not detected. |
| Param exclusion cutoff | `drop.rs:63-78` — `param_cutoff = body.param_count + 1` to cover BOTH the synthetic `_return` slot (LocalId(0)) AND every user param. Pre-fix off-by-one excluded the LAST user param; double-free on helper-fn returns. |
| Return-move upgrade | `lower.rs:408-434` — `StmtKind::Return` upgrades `Operand::Copy(p)` to `Operand::Move(p)` when `p`'s type is drop-eligible. Marks the local as moved in `collect_moves`, so the drop pass excludes it from the auto-drop chain. NRVO-equivalent. |
| Aggregate(List) elem-type synth | `lower.rs:1206-1232` — element type synthesised from first element via `synth_expr_ty`, replacing the legacy `Ty::None` placeholder. |
| For-loop body Str clone | `lower.rs:854-948` — when `var_local`'s type is `Ty::Str`, fetch the raw pointer into a throwaway i64 temp, then `__cobrust_str_clone` into the loop var. Prevents alias with the list slot's owned Str. |
| Index expr Str clone | `lower.rs:1304-1395` — `xs[i]` on a `list[str]` routes through `__cobrust_list_get` + `__cobrust_str_clone`. Returns owned Str via `Operand::Move`. |

Test corpus:
- `crates/cobrust-cli/tests/list_str_e2e.rs` — 33 end-to-end tests (build + run + assert stdout) covering literal construction, iteration, argv interop, helper-fn pass + return, indexing, f-string interpolation, list_is_empty, nested list[list[str]].
- `crates/cobrust-stdlib/tests/list_str_drop_corpus.rs` — 10 C-ABI link-time tests for `__cobrust_str_clone` / `__cobrust_list_drop_elems` / `__cobrust_list_is_empty`.
