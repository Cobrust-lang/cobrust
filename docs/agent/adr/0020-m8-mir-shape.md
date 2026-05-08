---
doc_kind: adr
adr_id: 0020
title: M8 MIR — node families, terminator taxonomy, drop schedule, borrow-check obligations
status: accepted
date: 2026-04-30
last_verified_commit: ed07cf0
supersedes: []
superseded_by: []
dependencies: [adr:0003, adr:0005, adr:0006, adr:0019]
---

# ADR-0020: M8 MIR — node families, terminator taxonomy, drop schedule, borrow-check obligations

## Context

Constitution `CLAUDE.md` §4.1 places MIR between HIR and codegen. ADR-0019
§"M8 — MIR" pinned the milestone scope:

> Mid-level IR: control-flow-explicit form fed to codegen. Locals,
> basic blocks, terminators, drop schedule, borrow-check obligations
> discharged.

ADR-0005 froze the HIR shape (~12 node families, sugar-collapsed,
name-resolved). ADR-0006 froze the type system (bidirectional, structural,
no `dyn`, no implicit truthiness, no silent coercion) and pinned a
**9-item soundness proof obligation list**. M2 (`mod:types`) discharged
items 4–9 of that list at type-check time. **Items 1–3 are *flow*
obligations** — Progress, Preservation, Lowering preservation — and
the natural place to discharge them is the MIR layer where every step
is a basic-block edge.

ADR-0019 also pinned MIR design as **Cobrust-native** (do not bind to
`rustc`'s MIR — invariants differ). M8 owns the IR shape. The same
"translate the surface, bind the core" principle from ADR-0012
applies: M9 will bind to Cranelift / LLVM as backends; M8 owns the
IR that those backends consume.

The MIR must:

- **Be control-flow explicit.** Every block ends in a terminator;
  fall-through is desugared into `Goto`.
- **Make ownership visible.** Constitution §2.2 drops Python's GIL +
  GC story; Cobrust uses ownership semantics. The MIR *is* the place
  where moves, borrows, and drops are explicit.
- **Discharge borrow obligations** before codegen sees the IR.
  Codegen receives a *proven* IR — no speculative checks happen
  later.
- **Compute drop schedule.** Every owning local has at least one
  drop point on every reachable exit. No double-drop. No leak on
  divergent control.
- **Cover the 30 forms.** Every form in ADR-0003 has a per-form
  lowering rule from typed-HIR (`mod:types::TypedModule`) to MIR.

## Options considered

1. **SSA in the textbook sense — phi nodes at block joins, no
   basic-block list.**
   - Pros: rich theoretical literature (Cytron, Wegman-Zadeck);
     cleanest dataflow.
   - Cons: phi-node bookkeeping is invasive at lowering time; codegen
     backends (Cranelift, LLVM) prefer the per-block-locals form.
     The 1.5× implementation cost buys nothing for the static core
     M2..M11 fragment. Rejected.

2. **rustc-style MIR — basic blocks + locals + terminators + Place
   projections, no phi nodes; SSA-like only in that each local is
   defined by at most one statement on each path.** *(chosen)*
   - Pros: matches what Cranelift / LLVM consume after their own
     SSA construction; is the form rustc itself uses for borrow
     checking. The borrow checker rules are well-published and
     transferable.
   - Cons: not a true SSA — joins do not have phi nodes; instead
     locals carry their assignments through `Goto` edges. Acceptable
     because borrow check + drop schedule operate on per-block local
     state, not on def-use chains.

3. **CPS / continuation-passing.**
   - Pros: structured-concurrency story (constitution §2.2) maps
     cleanly to CPS continuations.
   - Cons: codegen backends (Cranelift, LLVM) do not consume CPS;
     would require an extra lowering at M9. Premature for M8.

4. **Simple expression tree (no CFG explication).**
   - Pros: trivial to lower from HIR.
   - Cons: defeats M8's whole purpose. Rejected.

## Decision

Adopt **option 2**: rustc-style MIR. Cobrust-native; do not depend on
`rustc_mir`. The implementation lives in `crates/cobrust-mir/`.

### MIR node families

The MIR has **6 primary families**:

| Family | Role | Notes |
|---|---|---|
| `Module` | top-level container | one entry per typed-HIR `Item::Fn` (and one synthetic body for top-level statements) |
| `Body` | per-function CFG | locals, basic blocks, drop schedule, captured-`DefId` map |
| `BasicBlock` | linear statement sequence + terminator | every block ends in exactly one terminator |
| `Statement` | side-effecting non-terminator | assignments, drops marked, no control transfer |
| `Terminator` | control transfer | exhaustive enum: `Goto / SwitchInt / Return / Call / Drop / Unreachable / Assert` |
| `Place` / `Rvalue` / `Operand` | data references | typed access paths and value-producers |

Plus the supporting types (`Local`, `LocalDecl`, `BlockId`, `LocalId`,
`Constant`, `BinOp`, `UnOp`, `Projection`).

### Terminator taxonomy (binding)

The full terminator enum:

```rust
pub enum Terminator {
    /// Unconditional jump.
    Goto(BlockId),
    /// Discriminator switch — `value` matches one of the cases or
    /// falls through to `otherwise`.
    SwitchInt {
        operand: Operand,
        cases: Vec<(SwitchValue, BlockId)>,
        otherwise: BlockId,
    },
    /// Return from the body.
    Return,
    /// Call a callable. Continues at `target` on normal return; if
    /// `unwind` is `Some`, panics propagate to that block.
    Call {
        func: Operand,
        args: Vec<Operand>,
        destination: Place,
        target: BlockId,
        unwind: Option<BlockId>,
    },
    /// Drop the value at `place` (running its destructor) and jump to
    /// `target`. Inserted by the drop-schedule pass; never written by
    /// the lowering directly.
    Drop {
        place: Place,
        target: BlockId,
    },
    /// Statically-known dead code (e.g. after `raise` or `Never`-typed
    /// expression).
    Unreachable,
    /// Runtime assert (e.g. integer-division-by-zero, bounds check).
    /// Continues at `target` on success; panics with `msg` on failure.
    Assert {
        cond: Operand,
        expected: bool,
        msg: AssertKind,
        target: BlockId,
    },
}
```

`SwitchValue` covers `Bool(bool)`, `Int(i64)`, and ADT discriminants
(`Adt(u32)`). `AssertKind` enumerates `DivisionByZero`, `Overflow`,
`BoundsCheck`, and `Unreachable` (the latter for `match`-without-wildcard
when exhaustiveness leans on type information).

### Place semantics

A `Place` is a typed access path:

```rust
pub struct Place {
    pub local: LocalId,
    pub projections: Vec<Projection>,
}
pub enum Projection {
    Field(usize),       // tuple index or record field index
    Index(Operand),     // List / Dict / Str / Bytes index
    Deref,              // pointer / reference deref
    Discriminant,       // ADT discriminant peek
}
```

Projections are **type-driven** — the lowering only emits projections
the typed-HIR justifies. `Field(0)` on `Tuple([Int, Str])` is valid;
`Field(2)` on the same is a lowering bug (caught by `MirError::FieldOutOfBounds`).

### Rvalue / Operand split

```rust
pub enum Rvalue {
    Use(Operand),                                       // copy / move
    BinaryOp(BinOp, Operand, Operand),
    UnaryOp(UnOp, Operand),
    Aggregate(AggregateKind, Vec<Operand>),             // Tuple / List / Set / Dict / Record / Adt
    Cast(CastKind, Operand, Ty),                        // explicit numeric cast (constitution §2.2 — no implicit)
    Ref(BorrowKind, Place),                             // &place / &mut place
    Discriminant(Place),                                // read the ADT tag
    Len(Place),                                         // List / Str / Bytes len
    NullaryOp(NullaryOp),                               // SizeOf / AlignOf placeholder
}

pub enum Operand {
    Copy(Place),    // for Copy types (Bool, Int, Float, ...)
    Move(Place),    // for owning types (List, Set, Dict, Str owned, ...)
    Constant(Constant),
}

pub enum BorrowKind {
    Shared,         // & — many readers
    Mut,            // &mut — one writer, no readers
}
```

Operand-vs-Rvalue split mirrors rustc MIR: Statements assign an
`Rvalue` to a `Place`; Rvalues consume `Operand`s. Operand is
the *leaf* — either a direct `Place` access (Copy/Move) or a literal.

### Drop schedule algorithm

**Goal**: every owning local has exactly one `Drop` on every
reachable exit, and no double-drop on any path (constitution §2.2's
ownership story).

The algorithm operates **after lowering produces the CFG without drops**.

1. **Initialization phase** (per body): scan every `Statement::Assign`
   and `Terminator::Call` whose destination is a fresh local of a
   non-`Copy` type. Mark that local as *owning, drop-pending*.
   `Copy` types (`Bool`, `Int`, `Float`, `Imag`, `None`, `Never`,
   tuples-of-Copy) never go drop-pending.

2. **Move phase**: scan every `Operand::Move(place)`. The `place`'s
   root local *transfers ownership*. If we move out of a local that
   was drop-pending, mark it as *moved* — we no longer drop it.

3. **End-of-scope phase**: at the end of every basic block whose
   terminator is `Goto / SwitchInt / Return / Unreachable`, every
   still-drop-pending local of a scope ending here gets a synthetic
   `Drop` block inserted between the current block and its successor.
   For multiple drops, they chain: `BB → Drop(a) → Drop(b) → ... →
   succ`. The order is *reverse* of declaration order (LIFO),
   matching Rust's drop semantics.

4. **Divergence phase**: blocks that terminate in `Unreachable` skip
   drop insertion (the runtime never reaches them).

5. **Verification phase**: after insertion, every owning local must
   have exactly one drop on every path to `Return`. Algorithm:
   forward-flow analysis on the post-drop CFG; the join lattice is
   `(drop-pending, dropped, moved)`. A local that reaches `Return`
   in `drop-pending` state is `MirError::DropMissing`; a local
   that's `dropped` *and* later read is `MirError::UseAfterDrop`.

The pass is implemented in `crates/cobrust-mir/src/drop.rs`. The
verification phase doubles as a soundness check on the lowering
itself.

### Borrow-check proof obligation list

ADR-0006 enumerated 9 type-system soundness obligations. Items 4–9
were fully discharged at M2 type-check time. Items 1–3 are *flow*
obligations — they project onto MIR-time *borrow-check* obligations.
M8 discharges 5 borrow obligations:

| # | Obligation | Discharged by |
|---|---|---|
| **B1** | **No use after move.** A `Place` whose root local is in `moved` state cannot be read. | `borrow.rs` — `MoveTracker` per BB; on every `Operand::Copy(place)` / `Operand::Move(place)` access verifies the local's flow-state is *not* `moved`. Violations: `MirError::UseAfterMove`. |
| **B2** | **No two simultaneous mutable borrows.** A `BorrowKind::Mut` reference excludes any other borrow on the same root local until the mutable borrow's lifetime ends. | `borrow.rs` — borrow-stack per local. `BorrowKind::Mut` pushes; conflicts (existing `Mut` *or* `Shared` on path) yield `MirError::ConflictingMutBorrow`. |
| **B3** | **No mutable + immutable overlap.** A `BorrowKind::Shared` cannot coexist with a `BorrowKind::Mut` on the same root local. | Same `borrow.rs` borrow-stack — `Shared` pushes; conflicts with active `Mut` yield `MirError::SharedMutOverlap`. |
| **B4** | **Drop after last use.** Every owning local is dropped at the end of its lifetime, but never before its last use. | `drop.rs` — verification phase (above) confirms each owning local reaches the drop point exactly once on each reachable path; reads after drop yield `MirError::UseAfterDrop`. |
| **B5** | **No escape of `&'a T` past `'a`.** A reference cannot outlive the local it references. (M8 enforces *function-local* lifetimes; cross-function lifetime polymorphism is M9+ work.) | `borrow.rs` — *intra-body* lifetime check: every `Ref(_, place)` taken in `Body::N` must have its consuming `Operand` resolved by `Body::N`'s `Return` or earlier. Violations: `MirError::EscapingBorrow`. |

The five obligations form a closed proof for the *intra-procedural
fragment*. Inter-procedural lifetime obligations (lifetime-polymorphic
function signatures, escape via return value) are deferred to M9
when codegen materializes calling conventions. ADR-0006's items
1–3 (Progress, Preservation, Lowering preservation) compose with
B1..B5 to prove the static-core soundness target enumerated by §5.2.

The borrow check pass is **terminating**: each iteration over the
CFG either marks an obligation discharged or yields a `MirError`.
There is no fixpoint — every check is local to a basic block plus
its inputs from immediate predecessors.

### Lowering rules — per ADR-0003 form

Every form in ADR-0003 has a per-form lowering rule from typed-HIR
to MIR. The HIR shape is fixed by ADR-0005; the typed-HIR adds the
`def_types` map of `DefId → Ty`. M8's lowering threads through that
map to drive `Place` typing, drop scheduling, and borrow obligations.

#### Module-level forms (1–6)

| Form | Lowering rule |
|---|---|
| 1 `module` | One `Module { fns }` containing one `Body` per `Item::Fn` plus one synthetic `Body::Init` for module-level statements. The init body's terminator is `Return`. |
| 2 `import_stmt` | A `LocalDecl` for the imported `DefId` initialized lazily (no MIR-side semantics at M8; M11 stdlib will materialize). |
| 3 `fn_def` | One `Body` whose locals correspond to params + locals declared in `let` + synthetic locals from desugaring (e.g. iterator state). Entry block is `BasicBlock(0)`; control terminates in `Return`. |
| 4 `class_def` | Each method becomes a `Body`. The class itself is an ADT registration (no MIR allocation; M9 materializes vtables). |
| 5 `decorator` | `Item::Decorated { decorators, inner }` lowers by *expanding* the decorators: `inner = decorator_n(...(decorator_1(inner))...)`. Each decorator becomes an extra `Call` in the parent body. |
| 6 `type_alias` | No MIR emission — aliases are purely type-level. |

#### Statement forms (7–19)

| Form | Lowering rule |
|---|---|
| 7 `let_stmt` | Allocate a `LocalDecl` per `DefId`; emit `Statement::Assign(Place::local(id), Rvalue::Use(...))`. Mutable container types declare drop-pending. |
| 8 `assign_stmt` | Emit `Statement::Assign(target_place, rhs_rvalue)`. Augmented `op=` was desugared at HIR; here it's a single assign. |
| 9 `if_stmt` | Each arm becomes a `SwitchInt { cases: [(Bool(true), arm_body)], otherwise: next_arm_or_else }`. The `else` block is the otherwise of the last switch. All arms join at a synthetic merge block. |
| 10 `while_stmt` | `header → SwitchInt cond → (body, exit)`. `body` ends in `Goto(header)`. Optional `else` runs only on falsy exit (not `break` exit). |
| 11 `for_stmt` | Lowers via the iterator protocol: synthesize an iterator local, loop fetching `next()`. M8 emits the protocol calls but leaves the actual iterator type to M11 stdlib. The `else` block runs only on natural exhaustion. |
| 12 `match_stmt` | Lowers to a *decision tree* of `SwitchInt` on the discriminant + nested switches on field projections. Or-patterns split into multiple incoming edges to the arm body. |
| 13 `with_stmt` | Lowers context-manager `__enter__` / `__exit__` calls; the binding is a `let` whose drop point lands at `__exit__`. |
| 14 `try_stmt` | Each handler becomes an `unwind` target on `Call` terminators inside `body`. `else_block` runs on no-handler-matched. `finally_block` is a synthetic block on every exit edge (normal and unwind). |
| 15 `return_stmt` | `Statement::Assign(_return_local, ...)` then `Terminator::Return`. The current block ends. |
| 16 `break_continue` | `Goto(loop_exit_block)` for break, `Goto(loop_header_block)` for continue. |
| 17 `raise_stmt` | `Terminator::Call(panic, ...)` followed by `Unreachable`. |
| 18 `pass_stmt` | No emission. |
| 19 `expr_stmt` | Synthesize `let _tmp = expr` and discard. |

#### Pattern form (20)

Patterns lower as a sequence of *projection-and-test* operations on
the scrutinee `Place`. Each test that fails falls through to the
next arm (i.e. the pattern's arm becomes part of the decision tree
in form 12).

#### Expression forms (21–30)

| Form | Lowering rule |
|---|---|
| 21 `literal_expr` | `Operand::Constant(Constant::*)`. |
| 22 `fstring_expr` | `Rvalue::Aggregate(AggregateKind::FormatString, parts)` — the format string runtime helper materializes at M11. |
| 23 `name_expr` | `Operand::Copy/Move(Place::local(def_id))`; choose `Move` if the type is non-`Copy`. |
| 24 `collection_expr` | `Rvalue::Aggregate(Tuple | List | Set | Dict, items)`. Each item is an `Operand`; nested aggregates lower recursively. |
| 25 `comprehension_expr` | Desugars into a fresh accumulator local + a `Loop`-like CFG that pushes elements. Filters become `SwitchInt` on the guard. |
| 26 `lambda_expr` | A new `Body` with the captures listed; the parent body holds an `Operand::Constant(Constant::FnRef(body_id))`. |
| 27 `call_expr` | `Terminator::Call { func, args, destination, target, unwind }`. The current block ends. |
| 28 `access_expr` | Attr → `Place` with `Field(idx)` projection (record/tuple) or call resolution (instance method). Index → `Place` with `Index(operand)` projection. Slice → an aggregate-of-slice via runtime helper. |
| 29 `binary_unary_expr` | `Rvalue::BinaryOp` / `Rvalue::UnaryOp`. Integer division emits an `Assert(rhs != 0)` before the op (constitution §2.2 no-silent-coercion implies no silent NaN). |
| 30 `await_yield_expr` | `await` lowers to a structured-concurrency call (M13 binds the runtime); at M8, emits a `Call` terminator placeholder. `yield` is a structured `Goto` to the generator's resume block; M8 emits the placeholder. |

### Public surface (binding)

```rust
pub fn lower(typed: &cobrust_types::TypedModule) -> Result<Module, MirError>;

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

pub enum MirError {
    UseAfterMove { local: LocalId, span: Span },
    UseAfterDrop { local: LocalId, span: Span },
    ConflictingMutBorrow { local: LocalId, span: Span },
    SharedMutOverlap { local: LocalId, span: Span },
    EscapingBorrow { local: LocalId, span: Span },
    DropMissing { local: LocalId, span: Span },
    DoubleDrop { local: LocalId, span: Span },
    FieldOutOfBounds { place: PlaceDebug, span: Span },
    UnresolvedDefId { def_id: u32, span: Span },
    NonExhaustiveSwitch { span: Span },
    Internal(String),
}
```

### Invariants

- Every `BasicBlock` has exactly one terminator.
- Every `LocalDecl` carries a fully-resolved `Ty` (no `Ty::Var(_)` —
  the lowering rejects unresolved types as `MirError::Internal`).
- Drop-schedule pass is idempotent: running it twice on the same
  body produces the same CFG.
- Borrow-check is terminating; total time is `O(blocks * locals)`.
- Lowering is *total* over typed-HIR: every typed module either
  yields a `Module` or yields a structured `MirError`.
- `Module` can be debug-printed deterministically — same input,
  same byte output.

## Consequences

- **Positive**
  - The MIR shape is fully pinned. M9 codegen consumes a known,
    documented IR.
  - Borrow-check obligations have a single home; the type checker
    no longer has to think about lifetimes.
  - Drop schedule is computed at M8 *before* codegen — codegen
    inserts no additional drops.
  - The 5-obligation list (B1..B5) is enumerable, finite, testable
    — same shape as ADR-0006's 9-item soundness list.

- **Negative**
  - Cobrust now owns 3 IRs (AST, HIR, MIR). Doc burden is real;
    triple-tree sync rule (constitution §3.3) is the offset.
  - Drop schedule is *separate* from borrow check; bugs that
    cross the two passes are harder to localize. Mitigation: each
    pass has its own test suite and the integration suite runs both.
  - rustc-style MIR (no phi nodes) means joins re-read locals
    rather than rendezvous-via-phi. Codegen backends do their
    own SSA construction; this is fine but means M8's MIR is not
    directly a Cranelift IR.

- **Neutral / unknown**
  - Inter-procedural lifetime tracking is deferred to M9. Until
    then, lifetime-escaping returns are accepted but cannot be
    fully verified.
  - Generators (form 30 `yield`) lower as a placeholder at M8;
    M13 will connect them to the structured-concurrency runtime.
  - Closure capture types are recorded in `Body::captures` but
    not yet checked for `copy` / `ref` / `move` mode (constitution
    §2.2 — explicit capture mode lands at M9 alongside calling
    convention).

## Evidence

- Constitution `CLAUDE.md` §2.2 (drops including GIL + GC), §4.1
  (compiler layers), §5.1 (elegance: ownership encoded), §5.2
  (scientific: enumerated obligations), §6 (workflow), §7 (M2 done
  means).
- ADR-0003 — the 30 forms; this ADR's lowering tables cover each.
- ADR-0005 — HIR shape; M8 reads typed-HIR (`mod:types::TypedModule`).
- ADR-0006 — type-system 9-obligation list; M8 discharges B1..B5
  which are flow-obligations 1–3 projected onto MIR.
- ADR-0019 — Phase E roadmap; M8 row binds the scope.
- `crates/cobrust-mir/src/{tree.rs, lower.rs, borrow.rs, drop.rs,
  error.rs}` — implementation pinned to this ADR.
- `crates/cobrust-mir/tests/{lower_forms.rs, mir_well_formed.rs,
  mir_ill_formed.rs, mir_fuzz.rs}` — enforce the lowering rules,
  the 5 borrow obligations, and the drop schedule properties.
