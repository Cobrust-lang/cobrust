---
doc_kind: adr
adr_id: 0005
title: HIR shape and AST→HIR lowering tables for the static core
status: accepted
date: 2026-04-30
last_verified_commit: afe26db
supersedes: []
superseded_by: []
dependencies: [adr:0003]
---

# ADR-0005: HIR shape and AST→HIR lowering tables for the static core

## Context

Constitution `CLAUDE.md` §7 places `mod:hir` as a M2 deliverable: the
"high-level IR" the type checker (`mod:types`) consumes. ADR-0003
froze the M1 surface — the 30 syntactic forms — but `mod:hir.md`
deliberately deferred the HIR shape to "the M2 ADR." This is that
ADR.

The HIR must:

- **Be smaller than the AST.** The AST has 30 surface forms; many
  collapse during lowering (`with`, comprehensions, f-strings,
  decorators, augmented assignment, walrus). The HIR keeps **only
  semantically distinct constructs**.
- **Carry resolved names.** Every name use points at a unique
  `DefId`; every shadowing question is answered at lowering time, not
  at type-check time.
- **Be hygienic.** Implicit identifiers introduced by desugaring
  (e.g. `__iter_0` for a comprehension) cannot collide with user
  identifiers, regardless of what the user wrote.
- **Carry spans.** Every HIR node carries its origin span so that
  diagnostics from `mod:types` cite the user's source, not the
  lowered form.
- **Be lossless w.r.t. semantics.** Two AST programs that differ only
  in surface sugar lower to identical HIR. Two AST programs that
  differ in semantics never collapse to identical HIR.
- **Honour `CLAUDE.md` §2.2.** Every dropped Python form is unrepresentable
  at the HIR level: `is`, mutable defaults, late closure binding,
  multiple inheritance with MRO, `del`/`global`/`nonlocal`. The
  parser already rejects these per ADR-0003; the HIR rejects them
  again at the layer that the type checker can rely on (defense in
  depth).

## Options considered

1. **No HIR — type-check the AST directly.**
   - Pros: one fewer phase to write.
   - Cons: type rules become entangled with surface sugar (need to
     handle `with`, comprehensions, decorators, walrus, augmented
     assignment, etc. multiple times — wherever they appear).
     Name resolution mixes with type inference. Diagnostics get
     worse, not better. Rejected — the constitution's "elegant" bar
     (§5.1, "one way to do each thing in the core") is violated.

2. **Mirror-the-AST HIR — same node families with `DefId`s
   attached.**
   - Pros: cheap to implement, lowering is identity-shaped.
   - Cons: doesn't actually desugar, so the type checker still has to
     handle every surface form. Buys us nothing structural; it just
     renames the AST. Rejected — the M2 milestone exists *because*
     surface forms do not have first-class type rules.

3. **Reduced HIR — surface sugar collapsed, name resolution baked
   in, every node has a span and a `DefId`-or-resolved-reference.**
   *(chosen)*
   - Pros: the type checker writes its rules against ~12 node families
     instead of 30 surface forms. Diagnostics still point at user
     source via spans.
   - Cons: lowering is non-trivial; we own a per-form desugaring
     specification (this ADR's value).

4. **A-Normal Form (ANF) HIR — every subexpression named.**
   - Pros: makes sequencing explicit, useful for backends.
   - Cons: way too aggressive for type checking; breaks the
     "carry user spans" property because user-visible expressions
     get split. Rejected for M2 — ANF is an M3+ concern at the MIR
     layer.

## Decision

Adopt **option 3**. The HIR is a span-bearing tree with resolved
names. Lowering is a total function `lower(ast::Module) → hir::Module`
that:

- Allocates a `DefId` for every binding site (function parameters,
  let-bindings, for-loop targets, comprehension targets, match-arm
  bindings, class names, type-alias names, import aliases).
- Resolves every `name_expr` (form 23) to either a `DefId` or
  `LoweringError::UnknownName`.
- Desugars sugar into a small core (table below).
- Threads spans through every node — both user-source spans and
  desugared-construct spans (the latter point at the surface form
  that produced them).
- Refuses to admit constitution-dropped forms even if the AST
  somehow contained them — this is defense in depth on top of the
  parser's `ParseError::DroppedByConstitution`.

### HIR node families

The HIR has **fewer node families than the AST**. Each family is
listed with the AST forms it absorbs.

| HIR family    | Absorbs (AST forms)                                                  | Notes |
|---|---|---|
| `Module`      | form 1                                                                | top-level; carries `DefId`s for every item |
| `Item::Fn`    | form 3, form 5 (decorators are lowered around it)                     | docstring, params, return type, body |
| `Item::Class` | form 4, form 5                                                        | single base + trait list, no MRO |
| `Item::TypeAlias` | form 6                                                            | type-parameter list + body type |
| `Item::Import` | form 2                                                               | each `from … import …` becomes one `Item::Import` per target |
| `Stmt::Let`   | form 7, form 8 (when target is a fresh name and op = `Eq`)            | immutable binding |
| `Stmt::Assign` | form 8 (target is an existing l-value, includes augmented ops)       | augmented op is desugared to read-modify-write |
| `Stmt::If`    | form 9                                                                | n-ary `arms: Vec<(cond, body)>` plus optional `else`; `elif` is just an extra arm |
| `Stmt::Loop`  | forms 10, 11 (`while` / `for`); `for` desugars to iterator protocol  | unified loop with optional `else` |
| `Stmt::Match` | form 12, form 20                                                      | exhaustiveness checked at type-check time |
| `Stmt::With`  | form 13                                                               | normalised: multi-binding `with` is left-recursive (each `WithItem` becomes a nested HIR `With`) |
| `Stmt::Try`   | form 14                                                               | exception types appear as type names in the handler list |
| `Stmt::Return`| form 15                                                               | `Option<Expr>` |
| `Stmt::Break` / `Stmt::Continue` | form 16                                            | two distinct kinds for clarity |
| `Stmt::Raise` | form 17                                                               | `(exc, cause?)` |
| `Stmt::Pass`  | form 18                                                               | preserved as a no-op |
| `Stmt::Expr`  | form 19                                                               | bare expression statement |
| `Expr::Lit`   | form 21                                                               | bool, none, int, float, imag, str, bytes |
| `Expr::Format`| form 22                                                               | f-string lowered to `format(template, [{expr}*])` core call |
| `Expr::Name`  | form 23                                                               | resolved `Name { def: DefId, ... }` |
| `Expr::Tuple` / `Expr::List` / `Expr::Set` / `Expr::Dict` | form 24            | discriminated; subkind no longer carried by a single enum |
| `Expr::Loop`  | form 25 (comprehension)                                               | a comprehension lowers to a `Loop`-and-collect block on a fresh accumulator binding |
| `Expr::Lambda` | form 26                                                              | params and body |
| `Expr::Call`  | form 27                                                               | callee + positional/keyword/splat args |
| `Expr::Attr` / `Expr::Index` | form 28                                                | attribute and indexing; slice is its own subnode |
| `Expr::Bin` / `Expr::Un`     | form 29                                                | operator table preserved verbatim (no `is`) |
| `Expr::Await` / `Expr::Yield` / `Expr::YieldFrom` | form 30                       | structured-concurrency primitives, no async-coloring |

Pattern grammar (form 20) lowers verbatim into `hir::Pattern` —
literals, bindings, wildcard, sequence, mapping, class, or-patterns.
**Bindings inside patterns allocate `DefId`s during lowering**, so
the type checker sees pre-resolved names.

### Lowering tables (per-form contracts)

Each row below is a **per-form lowering rule**. Every form in
ADR-0003 has a row. The HIR shape is exactly what these rules emit;
nothing else.

#### Module-level forms (1–6)

| AST form              | HIR shape | Desugaring rule |
|---|---|---|
| 1 `module`            | `hir::Module { docstring, items: Vec<Item>, span }` | docstring is lifted out of the leading expr_stmt; the rest of the items lower one-by-one |
| 2 `import_stmt`       | `Item::Import { kind: ImportKind, span }` where `ImportKind` is `Module(path, alias?)` or `From(path, target_name, target_alias?)` | `from a import x, y` becomes **two** `Item::Import` items (one per target); each binding allocates a `DefId` |
| 3 `fn_def`            | `Item::Fn(FnBody { def_id, name, params, return_type, body, span })` | each `Param` allocates a `DefId`; defaults are evaluated at the call site (literal-only at parse time, semantic check at type-check time) |
| 4 `class_def`         | `Item::Class(ClassBody { def_id, name, base, traits, members, span })` | members are lowered as a list of `Item::Fn` / `Item::Let`; classes do not introduce new scopes for free names — see §"Scoping" |
| 5 `decorator`         | wraps the inner `Item::Fn` / `Item::Class` in `Item::Decorated { decorators: Vec<Expr>, inner: Box<Item>, span }` | exactly equivalent to `inner = decorator_n(...(decorator_1(inner))...)` semantically; desugaring to chained `Call`s is deferred to MIR |
| 6 `type_alias`        | `Item::TypeAlias(AliasBody { def_id, name, type_params: Vec<DefId>, value, span })` | type parameters allocate `DefId`s in the alias scope |

#### Statement forms (7–19)

| AST form              | HIR shape | Desugaring rule |
|---|---|---|
| 7 `let_stmt`          | `Stmt::Let { def_id, pattern, annot, value, span }` | the `let` introduces a binding scope from this point to end-of-block; pattern bindings each get a `DefId` |
| 8a plain assignment   | `Stmt::Assign { target, value, span }` | when the target is a name pre-bound in scope; if not, this is a `LoweringError::AssignToUnknown` |
| 8b augmented `op=`    | `Stmt::Assign { target, value: hir::Expr::Bin { op, lhs: target, rhs: original_rhs }, span }` | `x += e` → `x = x op e`. The lowering emits the **same** `target` HIR twice; type checker treats it normally |
| 9 `if_stmt`           | `Stmt::If { arms: Vec<(Expr, Block)>, else_block: Option<Block>, span }` | `elif` becomes another arm; this is *not* a sequence of nested `if`s |
| 10 `while_stmt`       | `Stmt::Loop(LoopKind::While { cond, body, else_block?, span })` | identity lowering; the `else` clause runs when the loop exits without `break` |
| 11 `for_stmt`         | `Stmt::Loop(LoopKind::For { def_id, pattern, iter, body, else_block?, span })` | type checker enforces `iter: Iter[T]` and `pattern : T`; iterator protocol lookup is a M3+ concern |
| 12 `match_stmt`       | `Stmt::Match { scrutinee, arms: Vec<MatchArm>, span }` | each arm: `(pattern, guard?, body)` — bindings inside `pattern` allocate `DefId`s scoped to `body` |
| 13 `with_stmt` (1+ items) | left-fold: `with a as x, b as y: body` → `with a as x: with b as y: body` | this is a deterministic structural rewrite — no semantic change |
| 14 `try_stmt`         | `Stmt::Try { body, handlers, else_block?, finally_block?, span }` | each `ExceptHandler` keeps its `(exc_type, binding?, body)`; `binding` allocates a `DefId` |
| 15 `return_stmt`      | `Stmt::Return(Option<Expr>)` | identity |
| 16 `break_continue`   | `Stmt::Break` or `Stmt::Continue` | two distinct kinds |
| 17 `raise_stmt`       | `Stmt::Raise { exc?, cause?, span }` | identity |
| 18 `pass_stmt`        | `Stmt::Pass` | preserved (a no-op makes empty bodies legal) |
| 19 `expr_stmt`        | `Stmt::Expr(Expr)` | identity, with module-level docstring lifting handled at form 1 |

#### Pattern form (20)

Patterns lower 1:1; bindings allocate `DefId`s into the pattern's
arm scope. Or-patterns require all branches to bind the same set of
names (enforced at type-check).

#### Expression forms (21–30)

| AST form              | HIR shape | Desugaring rule |
|---|---|---|
| 21 `literal_expr`     | `Expr::Lit(Lit)` where `Lit` mirrors AST `Literal` | identity |
| 22 `fstring_expr`     | `Expr::Format { template: String, parts: Vec<FormatPart>, span }` | each `FStrPart::Lit(s)` becomes `FormatPart::Lit(s)`; each `FStrPart::Expr { expr, debug_equals, format_spec }` becomes `FormatPart::Hole { expr: lowered, debug_equals, format_spec }`. Nested f-strings lower recursively. **No string concatenation**: the format function is the single core primitive |
| 23 `name_expr`        | `Expr::Name { def: ResolvedName, span }` | resolution: nearest enclosing scope wins; not-found is `LoweringError::UnknownName` |
| 24 `collection_expr`  | one of `Expr::Tuple`, `Expr::List`, `Expr::Set`, `Expr::Dict` | dict-spread (`{**x}`) is a structural element, lowered to a `DictEntry::Spread` |
| 25 `comprehension_expr` | `Expr::Comp(Box<Comp>)` where `Comp { kind: CompKind::{List|Set|Dict|Generator}, element: CompElem, clauses: Vec<CompClause>, span }` | bindings inside clauses allocate `DefId`s scoped to subsequent clauses + element; type checker treats this as "construct a fresh `Vec/HashSet/HashMap/iterator` and run the equivalent of nested `for`/`if`" |
| 26 `lambda_expr`      | `Expr::Lambda { params, body, span }` | params get `DefId`s scoped to `body`; closure capture is computed structurally and reified into a `captures: Vec<CaptureSpec>` field at type-check time |
| 27 `call_expr`        | `Expr::Call { callee, args: Vec<CallArg>, span }` | identity over `Positional / Keyword / StarArgs / StarStarKwargs`; routing of args to params is type-checker work |
| 28 `access_expr`      | `Expr::Attr { base, name, span }` or `Expr::Index { base, index, span }` | slice (form 28 sub-grammar) lowers to `IndexKind::Slice { start?, stop?, step? }`; tuple-index lowers to `IndexKind::Tuple(Vec<IndexKind>)` |
| 29 `binary_unary_expr`| `Expr::Bin { op, lhs, rhs, span }` or `Expr::Un { op, operand, span }` | operator table preserved verbatim. **`is` cannot appear** because the parser rejects it; if it somehow does, `LoweringError::DroppedFeature("is")` |
| 30 `await_yield_expr` | `Expr::Await(Box<Expr>)` / `Expr::Yield(Option<Box<Expr>>)` / `Expr::YieldFrom(Box<Expr>)` | identity; structured concurrency means no async-coloring |

### Scoping

Scoping is **lexical and explicit**. The HIR scope rules:

- **Module scope** — every top-level `Item` introduces a `DefId` into
  the module scope. Items shadow imports.
- **Function scope** — a function body opens a fresh scope with its
  parameters bound. Free names look up through the lexical chain;
  unresolved names are `LoweringError::UnknownName`.
- **Class scope** — *not* a name-lookup scope for methods (consistent
  with the constitution's rejection of metaclass-driven name lookup).
  Class members are accessed via `self.x` / `Cls.x` and resolved at
  type-check time, not lowering time.
- **Loop / comprehension target scope** — `for` and comprehension
  targets introduce bindings scoped to the loop body / comprehension
  body. **No leak** outside the loop, unlike Python — this is
  consistent with §2.2's stance on closure binding.
- **Match-arm scope** — pattern bindings scope to the arm body and
  guard.
- **`let` scope** — from the `let` to end-of-block.
- **`with` scope** — the `as` target scopes through the body.

### `DefId` allocation

`DefId` is an opaque, monotonically allocated index. The lowering
session owns the counter. Each binding site allocates exactly one
`DefId`. Resolution maps every `name_expr` to either a `DefId` (in
scope) or the `LoweringError::UnknownName` error.

### Hygienic gensym

When desugaring introduces a fresh identifier (e.g. comprehension
collector, with-resource), the lowering allocates a `DefId` whose
display name is unrepresentable in surface syntax (`__cb_gensym_NN`).
This guarantees no collision with user-supplied identifiers — even
if a user writes `__cb_gensym_42`, the HIR distinguishes them by
`DefId`, not name.

### Error taxonomy (`LoweringError`)

The lowering reports structured errors, not panics. The full set:

- `UnknownName { name: String, span: Span }` — `name_expr` not
  resolvable in any enclosing scope.
- `DroppedFeature { name: &'static str, span: Span }` — defense in
  depth: a constitution-dropped form snuck past the parser.
- `MutableDefault { span: Span }` — semantic re-check of the parser's
  syntactic rejection, in case future AST versions grow non-literal
  defaults.
- `OrPatternBindingMismatch { span: Span }` — an or-pattern's
  branches bind different names.
- `DuplicateBinding { name: String, first: Span, second: Span }` —
  two bindings with the same name in the same scope (e.g. duplicate
  parameter, duplicate let in the same block before any read).

### Lowering totality

The lowering is **total** in the sense that every well-formed AST
either yields a well-formed HIR or yields a `LoweringError`. Lowering
**never panics** on any AST produced by `mod:frontend`. This is a
hard invariant verified by the lowering test-suite (every form in
ADR-0003 has a golden lowering test).

## Consequences

- **Positive**
  - Type checker writes its rules against ~12 HIR families instead
    of 30 surface forms. The "static core" (constitution §7) is
    finally a small core.
  - Spans flow from AST to HIR; type-check diagnostics cite the
    user's source.
  - Every desugaring rule is documented, mechanical, and testable —
    the lowering tests can mirror this ADR row-for-row.
  - Defense in depth: the type checker never has to check that `is`
    didn't sneak in, that a mutable default didn't sneak in, etc.
    Those errors are caught earlier.

- **Negative**
  - The HIR is its own surface; future contributors must learn it.
    The doc burden is real (offset by this ADR's lowering tables).
  - Any future change to the AST families requires a corresponding
    HIR change. The "atomic commit" rule (§6) makes that workable
    but adds discipline cost.

- **Neutral / unknown**
  - Closure capture computation is currently sketched as "structural
    `captures: Vec<CaptureSpec>`" — the exact shape lands in the
    type-checker (ADR-0006) since capture is a typing question. The
    HIR carries a placeholder.
  - `match`-exhaustiveness is *not* checked at lowering time; it is a
    type-check obligation (ADR-0006). The HIR carries each arm
    verbatim.
  - The decorator lowering deliberately keeps `Item::Decorated` as a
    distinct family rather than expanding to nested `Call` forms.
    This preserves the user's intent for diagnostics; full expansion
    happens at MIR time. This is an accepted asymmetry.

## Evidence

- Constitution `CLAUDE.md` §2.2 (dropped Python forms), §5.1
  (elegance: "one way to do each thing"), §7 (M2 done means).
- `docs/agent/modules/hir.md` — module spec, this ADR fills the
  "TBD" placeholder.
- `adr:0003` — the 30 forms enumerated; this ADR's rows reference
  them by number.
- `crates/cobrust-hir/tests/` — golden lowering suite, one test per
  form, mirroring the desugaring tables above.
