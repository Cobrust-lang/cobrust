# Phase M — Language-surface gap closure

Phase M closes the 6 source-level gaps queued by ADR-0058a §15. Five
gaps land via three additive sub-sprints; the sixth is formally
out-of-scope.

## Why this matters (LLM-first design ROI)

Per CLAUDE.md §2.5 (LLM-first design), the goal is "the language LLM
agents write correctly on the first try." Phase M tackles syntax
patterns LLM training data overwhelmingly contains:

- `i32` / `i8` — dominant in Rust + C/C++ corpora.
- `-> None` — canonical Python explicit-no-return spelling.
- `&T` in annotation position — Rust idiom; pairs with ADR-0052a
  expression-position `&`.
- `[T; N]` — Rust fixed-size array literal type.

The empirical anchor is `finding:leetcode-corpus-parse-int-tok-use-after-move-fixture-debt
§5.1`: 84 of 100 LC-100 stress fixtures were initially authored
without `&`, requiring 226 mechanical call-site insertions. Lifting
`&T` to annotation position lets the type signature itself encode the
borrow contract, reducing LLM author friction.

## What ships

```mermaid
flowchart LR
    A[ADR-0060<br>Phase M frame] --> B[ADR-0060a<br>narrow ints<br>i8 / i16 / i32]
    A --> C[ADR-0060b<br>syntax trio<br>-> None / &T / [T;N]]
    A --> D[ADR-0060c<br>anonymous struct<br>OUT-OF-SCOPE]
    B --> E[Ty::IntN&lpar;width&rpar;]
    C --> F[Ty::Ref&lpar;inner&rpar;]
    C --> G[Ty::Array&lpar;elem,N&rpar;]
    D --> H[Use tuple / record]
```

## ADR-0060a — narrow-int types

- `Ty::IntN(8 | 16 | 32)` — distinct from `Ty::Int` (i64).
- Unification rule: `IntN(a) ⇔ IntN(b)` iff `a == b`; **no implicit
  widening** to `Ty::Int`.
- Copy: narrow ints are Copy (no drop schedule entry).
- LLVM lowering: `i8_type()` / `i16_type()` / `i32_type()`.
- Cranelift lowering: `types::I8` / `types::I16` / `types::I32`.
- DI lowering: collapses to `DW_ATE_signed` "Int" entry.

## ADR-0060b — syntax trio

- **`-> None`** — `parse_type_atom` now accepts `KwNone` at entry;
  resolves to `Ty::None` via existing `lower_named_type("None")`.
  LLVM backend's existing `Ty::None` → `i64` fallback path covers
  the return.
- **`&T` in annotation** — `parse_type_atom` accepts `&` prefix;
  AST `TypeKind::Ref(Box<Type>)` lowers to `Ty::Ref(inner)`. Ref is
  transparent at LLVM level (recurses into inner per
  `llvm_backend.rs:580`).
- **`[T; N]`** — `parse_type_atom` accepts `LBracket` prefix;
  AST `TypeKind::Array { elem, len: usize }` lowers to
  `Ty::Array(elem, n)`. LLVM lowers to `[N x T]` array type; the
  type-emission shape ships now, indexing follows in a sub-sprint.

## ADR-0060c — out-of-scope memo

Anonymous struct literal `struct{T, U}` will not be added. Use:

- `(T, U)` tuple type for positional access.
- `class Foo: x: T` for named-field access.

Both already lower to LLVM struct types; adding a third spelling
violates CLAUDE.md §5.1 "one way to do each thing."

## What's deferred (honest debt)

Three findings document Phase M wave-2 follow-ups:

1. `finding:adr0060a-binop-on-intn-narrow-int-debt` — BinOp + literal
   coercion on narrow ints. Cast-surface sub-sprint will add the
   `(IntN(w), IntN(w)) -> IntN(w)` arm + literal-fit guard.
2. `finding:adr0060b-array-indexing-mir-projection-debt` — `a[0]`
   indexing on `[T; N]`. Typeck `NotIndexable` predicate + MIR
   `Place::index` + LLVM GEP wiring in a follow-up.
3. `finding:adr0060b-empty-dict-annotation-k-flow-debt` — empty `{}`
   literal with a non-hashable-K dict annotation. Severity P3
   (production non-empty path is correct).

## Verification (DG `1ff7921`)

- 5 / 5 gap fixtures GREEN:
  - `llvm_type_02_i32` (narrow-int signature)
  - `llvm_type_03_i8` (narrow-int signature)
  - `llvm_type_06_none_return` (`-> None` parse + lower)
  - `llvm_type_08_array_i64` (array type-emission)
  - `llvm_operand_06_deref_ptr` (`&i64` annotation passthrough)
- 17 / 17 Phase M parser-corpus tests GREEN.
- 11 / 14 Phase M typeck-corpus tests GREEN; 3 honestly `#[ignore]`'d
  per F37 with finding cross-references.
- Zero regression across Phase H/I/J/K/L baselines (DG full suite).
