---
doc_kind: adr
adr_id: 0060b
parent_adr: 0060
title: "Phase M wave-2 — syntax trio (None-return + &T-annot + [T;N]-array)"
status: accepted
date: 2026-05-19
ratified_at: 2026-05-19
last_verified_commit: 2d84de5
supersedes: []
superseded_by: []
relates_to: [adr:0060, adr:0060a, adr:0058a, adr:0052a]
discovered_by: P10 Phase M sprint per ADR-0058a §15 gaps #3 #4 #5
---

# ADR-0060b: Phase M wave-2 — syntax trio

## 1. Context

ADR-0058a §15 gaps #3 (`-> None` return type), #4 (`[T; N]` array
literal type), and #5 (`&T` type-annotation) are three **parser-side**
gaps. The type universe already has `Ty::None` and `Ty::Ref(Box<Ty>)`;
only `Ty::Array(Box<Ty>, usize)` is added in this ADR. All three gaps
are unblocked by parser + AST extensions; the type system mostly
reuses existing semantics.

## 2. §2.5 LLM-first ROI

§2.5 §B (training-data overlap) — *all three are high*:

- `-> None` is **the** standard Python return annotation for no-value
  functions. Cobrust forbidding it is a Python-prior shock.
- `&T` in annotation position is the Rust idiom; matches ADR-0051
  Priority A "explicit `&` borrow ergonomics".
- `[T; N]` is the Rust fixed-size array spelling.

§2.5 §A (compile-time-catch) — `[T; N]` adds an out-of-bounds
detection at type-check time when the index is a literal constant
(`xs[5]` on `[i64; 4]` ⇒ compile error, not runtime panic).

## 3. Decision

### 3.1 `-> None` return type (gap #3)

The blocker is `parse_type_atom` calling `expect_ident` which rejects
the `KwNone` keyword. Fix: special-case `KwNone` at the entry of
`parse_type_atom`:

```rust
// In parse_type_atom, before the LParen / Ident branch:
if matches!(self.peek_kind(), TokenKind::KwNone) {
    let span = self.current_span();
    self.bump();
    return Ok(Type {
        kind: TypeKind::Name(vec!["None".to_string()]),
        span,
    });
}
```

The `lower_named_type("None")` lookup in `cobrust-types::check.rs:2323`
already produces `Ty::None`. The LLVM backend already lowers
`Ty::None` return locals to `i64` fallback (`llvm_backend.rs:628`).
Cranelift backend mirrors via `lower_ty_wave1` (returns `INVALID`
sentinel; the body-signature builder rewrites to `i64`).

**Collision check**: does this conflict with `def f(): pass` implicit
None? No — the implicit case has `return_type = None` (Option),
landing at MIR `Ty::None` already. The explicit case is identical
semantically; it's just sugar that doesn't compile today.

### 3.2 `&T` in type-annotation position (gap #5)

The blocker is `parse_type_atom` not accepting `TokenKind::Amp` as
the type-atom prefix. Fix: prepend an `&` prefix branch in
`parse_type_atom`:

```rust
// At entry of parse_type_atom, after the LParen branch:
if self.eat(&TokenKind::Amp) {
    let inner = self.parse_type_atom()?;
    let span = start.merge(inner.span);
    return Ok(Type {
        kind: TypeKind::Ref(Box::new(inner)),
        span,
    });
}
```

Add `TypeKind::Ref(Box<Type>)` variant to AST. HIR mirror:
`h::TypeKind::Ref(Box<h::Type>)`. Typeck `lower_type` extends:

```rust
TypeKind::Ref(inner) => Ty::Ref(Box::new(self.lower_type(inner))),
```

`Ty::Ref` already exists (ADR-0052a Wave-1). Unification rule already
binds (one-way `Ref(T) → T` call-site coercion). Codegen `lower_ty`
already maps `Ty::Ref(inner) → lower_ty(inner)` (transparent), per
`llvm_backend.rs:580`.

Banning `&&T`: not banned at parser-level (left to the type-checker;
the call-site coercion currently doesn't strip nested Ref, so `&&T`
silently fails at use-site unification — acceptable for wave-2 since
no LLM-prior asks for `&&T`).

### 3.3 `[T; N]` fixed-size array type (gap #4)

Add `TypeKind::Array { elem: Box<Type>, len: usize }` to AST.

Parser: in `parse_type_atom`, after the LParen branch (and `&`
branch from §3.2), prepend a `LBracket` branch:

```rust
if self.eat(&TokenKind::LBracket) {
    let elem = self.parse_type()?;
    self.expect(&TokenKind::Semicolon)?;
    let len_tok = self.peek().clone();
    let len = match &len_tok.kind {
        TokenKind::Int(s) => s.parse::<usize>()
            .map_err(|_| ParseError::Syntax {
                message: format!("array length must be a non-negative integer, got `{s}`"),
                span: len_tok.span,
            })?,
        _ => return Err(ParseError::Syntax {
            message: "array type literal expects integer length after `;`".into(),
            span: len_tok.span,
        }),
    };
    self.bump();  // consume the Int token
    self.expect(&TokenKind::RBracket)?;
    return Ok(Type {
        kind: TypeKind::Array { elem: Box::new(elem), len },
        span: start.merge(self.peek().span),
    });
}
```

HIR mirror: `h::TypeKind::Array { elem: Box<h::Type>, len: usize }`.

Types crate: add `Ty::Array(Box<Ty>, usize)` variant. Properties:

- `is_value_type` (Copy fast-path): true iff `elem` is Copy.
- `subst_var` + `collect_vars`: recurse into `elem`.
- Display: `[T; N]`.
- Unify: `Array(t1, n1) ⇔ Array(t2, n2)` iff `n1 == n2 ∧ t1 ⇔ t2`.

Typeck `lower_type`:

```rust
TypeKind::Array { elem, len } => Ty::Array(
    Box::new(self.lower_type(elem)),
    *len,
),
```

Codegen `lower_ty` (LLVM):

```rust
Ty::Array(elem, n) => {
    let elem_ty = self.lower_ty(elem);
    elem_ty.array_type((*n) as u32).as_basic_type_enum()
}
```

Codegen `lower_ty_wave1` (Cranelift): wave-1 narrows to opaque
pointer (heap-managed); the LLVM path is the one exercised by the
gap fixtures.

Array indexing (`xs[i]`) reuses the existing `Place::index` MIR
projection path — no new MIR form. The bounds-check codegen reuses
`__cobrust_panic` (existing runtime helper).

### 3.4 Out-of-bounds compile-time catch (literal-index optimisation)

When `i` is a literal constant and the array length is known, the
type-check inserts a bounds guard at HIR→MIR lowering time:

```rust
if let (ast::ExprKind::Literal(Lit::Int(s)), Ty::Array(_, n)) =
    (&idx.kind, &base_ty)
{
    if let Ok(k) = s.parse::<i64>() {
        if k < 0 || (k as usize) >= *n {
            return Err(MirError::ArrayIndexOob {
                idx: k, len: *n, span: idx.span,
                suggestion: Some(format!("valid range is [0, {}]", n - 1)),
            });
        }
    }
}
```

This is the §2.5 §A "compile-time-catch-errors" payoff for `[T; N]`.

## 4. Surface examples

```cobrust
fn no_return() -> None:
    pass

fn count(s: &str) -> i64:
    return str_len(s)  # ADR-0052a transparency; &str ↔ str at call

fn first(a: [i64; 4]) -> i64:
    return a[0]

fn oob() -> i64:
    let a: [i64; 4] = [0, 0, 0, 0]
    return a[5]  # MirError::ArrayIndexOob
```

## 5. Acceptance

- 8 unit tests in `cobrust-frontend`: `-> None` parse, `&T` annot
  parse (Name + Generic + Tuple inner), `[T; N]` parse (positive +
  bad-length-literal + non-integer-length).
- 4 unit tests in `cobrust-types`: `Ty::Array` unify (eq-len OK,
  mismatch fails); `Ty::Array(Copy)` is Copy; `Ty::Ref(T)` Display.
- 2 codegen corpus fixtures un-ignored:
  - `llvm_type_06_none_return` (renamed from `_int_return_baseline`)
  - `llvm_type_08_array_i64` (un-ignored, gap closure)
  - `llvm_operand_06_deref_ptr` (un-ignored — actually tests `&T`
    annotation passthrough, no Deref op needed since `Ty::Ref` is
    transparent at LLVM level)
- 1 MIR test: array-OOB literal index detection.

## 6. Anchors

- 0060b-F34: syntax trio closure canonical
- 0060b-F35: sibling 0060 + 0060a
- 0060b-F36: 3 fixture renames + 2 un-ignores match behavior
- 0060b-F37: zero `#[ignore]` retained

## 7. Cross-references

- ADR-0060 — Phase M frame
- ADR-0058a §15 #3 #4 #5 — gap queue items
- ADR-0052a Wave-1 §4.1 — expr-position `&` (paired with annot `&T`)
- `cobrust-types::check.rs:2323` — `None` lookup site
- `cobrust-codegen::llvm_backend.rs:580` — `Ty::Ref` transparency

## 8. Cascade addendum (2026-05-19 Phase M follow-up sprint)

Two paired findings RESOLVED:

- `finding:adr0060b-array-indexing-mir-projection-debt` RESOLVED at
  commit **981b577**:
  - `synth_expr` IndexAccess arm now allow-lists `Ty::Array(elem, n)`;
    literal-OOB index fires `TypeError::NotIndexable` per §3.4
    compile-time-catch.
  - `llvm_backend.rs::lower_place_load` emits safe
    `build_extract_value` aggregate-extract for constant-int Array
    index reads (the `forbid(unsafe_code)` crate-level lint blocks
    the inkwell GEP; aggregate-extract requires a compile-time `u32`
    index, which matches the §3.4 literal-OOB compile-time-catch
    surface naturally).
  - F36 rename `llvm_type_08_array_i64` -> `llvm_type_08_array_i64_index`
    + added `llvm_type_08b_array_index_literal_oob`.
  - F37 honest scope: dynamic-index Array reads (non-literal `xs[i]`
    where `i` is an Ident or BinOp) still fall through to the
    wave-1 stub-load and are tracked under "ADR-0060b dynamic-array-index
    queue (deferred to wave-3 cast-surface sub-sprint)". The wave-2
    closure here surfaces the §3.4 compile-time-catch payoff exactly.

- `finding:adr0060b-empty-dict-annotation-k-flow-debt` RESOLVED at
  commit **83ee812**:
  - `StmtKind::Let` annotation site now calls `validate_hashable_dict`
    on the HIR annotation tree before evaluating the RHS, mirroring
    the existing `ItemKind::Let` site. The root-cause turned out to
    be a one-arm symmetry bug between item-level and stmt-level let;
    the empty `{}` literal masking the Array K via fresh-var
    substitution is no longer reachable because the validation runs
    pre-RHS.

Test closures: `pm_b06_array_not_hashable` un-ignored + PASS; added
`pm_b07_array_not_hashable_empty_dict_module_level` (F34 regression
guard rail for the symmetric item-level path).
