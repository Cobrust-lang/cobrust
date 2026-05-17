---
doc_kind: adr
adr_id: 0052f
parent_adr: 0052
title: "Parser §8 cap relaxation — `&Call(Attr(...))` (method-form borrow)"
status: proposed
date: 2026-05-17
last_verified_commit: 4e05cbb
supersedes: []
superseded_by: []
relates_to: [adr:0052a, adr:0052d-prereq, adr:0052]
discovered_by: findings/0052d-prereq-impl-blocker.md (Wave-2 prereq DEV cargo-test) + ADR-0052d-prereq §"Cascade enumeration" L320 `f30wit_method_03` deferral
ratification_path: P9 Wave-2 round-2 sub-ADR review (ratifies on impl merge)
---

# ADR-0052f — Parser §8 cap relaxation for `&Call(Attr(...))` form

## 1. Context

ADR-0052a §8 capped Wave-1 borrow operands to `&ident`, `&ident.field`, and `&ident[idx]`. The cap was conservative-by-design: ship the smallest correct surface for the LC-100 honest-debt closure, defer everything else to follow-up sub-ADRs. The `validate_borrow_operand` validator at `crates/cobrust-frontend/src/parser.rs:1134-1139` (HEAD `4e05cbb`) explicitly rejects `ExprKind::Call { .. }` as a borrow operand with the message "borrow of a call-result is not supported in Wave-1".

ADR-0052d-prereq (Wave-2, ratified at `0a90594`) introduced the four per-type method tables (Str / List / Float / Int) on top of the dict-method precedent. The §"Precedence with 0052a `&s`" section (L117-121) claimed that `&s.method()` already parses correctly per ADR-0052 F-G.3 (method-call binds tighter than borrow → `&(s.method())`).

**Empirical reality** (per `findings/0052d-prereq-impl-blocker.md`): the `validate_borrow_operand` cap rejects `&Call(Attr(...))` at parse time, before the borrow-wrapping precedence even applies. Test `f30wit_method_03_borrow_precedence_binds_tighter_than_method_call` (`crates/cobrust-mir/tests/method_dispatch_f30_witness.rs:241`) is `#[ignore]`'d with a clear "deferred to 0052d follow-up parser-cap relaxation sub-ADR" note. ADR-0052d-prereq §"Cascade enumeration" L320-332 documented the forecast miss and noted the cap status MUST be verified at design-time, not assumed (F32 ADSD candidate).

ADR-0052a is **accepted at SHA `8f29189`** (verified per F27 verified-at-HEAD immutability discipline). The §8 cap text in `crates/cobrust-frontend/src/parser.rs:1134-1139` is the live source-of-truth. ADR-0052a's text cannot be edited post-ratification; the correction path is a new sub-ADR (this one).

This ADR is also kept standalone from ADR-0052d round-2 (which will ship `s.method()` migration in examples) to keep PRs lean per the Wave-2 round-1 closure audit recommendation.

## 2. Decision

**Relax `validate_borrow_operand` at `crates/cobrust-frontend/src/parser.rs:1134-1139` to admit `&Call(Attr(...))` where the callee is the method-form (attribute-access shape) and the receiver `base` is itself a borrowable place.**

Other `&Call(...)` forms — free-function calls (`&foo(x)`, callee is `Name`) and unresolved callee shapes — **remain rejected**. The rejection diagnostic stays useful: `&free_fn_call()` is almost always wrong (borrows the temporary return value with no place to anchor); preserving the catch honors §2.5 compile-time-catch.

The relaxation is **parser-only**. The downstream HIR / types / MIR pipeline already handles `Unary(Borrow, Call(Attr(...)))` correctly post-0052d-prereq (the method-table dispatcher rewrites the inner `Call(Attr, ...)` to a PRELUDE-fn `Call(Name, ...)` at type-check time; MIR sees a borrow-of-Call-of-Name shape, which lowers identically to existing borrow operand handling once the parser admits it).

## 3. Semantics — `&s.method()` parsing

Per ADR-0052 F-G.3 precedence amendment: `&s.method(args)` parses as `Unary(Borrow, Call(Attr(s, "method"), args))`. The unary borrow `&` binds **looser** than the method-call attribute-access; matches Rust corpus (`&v.len()` parses `&(v.len())`).

Semantic flow:

1. Parser produces `Unary(Borrow, Call(Attr(s, "method"), args))`.
2. Type-checker enters method-table dispatch via ADR-0052d-prereq `try_synth_*_method` chain.
3. If `method` is in the receiver's method-table, the inner `Call(Attr, ...)` is rewritten to `Call(Name("prelude_fn"), [s, ...args])`.
4. The outer `Unary(Borrow, ...)` then wraps the rewritten PRELUDE-fn call's return type.
5. If the rewritten return type is a place (rare today; PRELUDE-fns mostly return `Int` / `Bool` / `Str`-by-value), the borrow targets it. If not, an existing diagnostic fires (`TypeError::BorrowOfNonPlace` per ADR-0052a §6) — the §2.5 compile-time-catch path is preserved.
6. If `method` is not in any method-table, the existing `TypeError::UnknownMethod` (per ADR-0052d-prereq §"New error variant") fires. The outer borrow does not mask this diagnostic.

In Wave-2 practice, most method-form returns are non-place primitives (`s.len() -> Int`, `xs.is_empty() -> Bool`, `f.floor() -> Float`). Borrowing those is semantically a no-op (the value is already Copy at the MIR layer). The relaxation thus mostly serves **uniformity** — the LLM writing `&s.len()` (because it learned `&` from §2.5 Direction A) does not hit a confusing parse error; the compiler accepts the borrow as cosmetic-but-correct and lowers it to the same MIR as `s.len()`.

The narrow but real semantic case where the borrow matters: when a Wave-2+ method returns a non-Copy type (e.g. future `s.clone() -> Str`), `&s.clone()` borrows the cloned value. ADR-0052a §3 transparency rules apply: PRELUDE-fn formal-param coercion accepts the borrow at call sites; non-call positions follow existing borrow semantics.

## 4. Why standalone ADR

- **F27 verified-at-HEAD immutability**: ADR-0052a is accepted at `8f29189` (frontmatter `ratified_at`). Modifying §8 text in-place violates the F27 rule that accepted ADRs are immutable except for `### Cascade enumeration (post-spike)` addenda. The §8 cap relaxation is a Wave-2-round-2 forward decision, not a Wave-1 closure addendum.
- **ADR-0052d-prereq is also accepted** at `0a90594`. Folding the cap relaxation into the 0052d round-2 follow-on impl ADR would couple two independent concerns (cap relaxation = parser scope; method-form migration in examples = downstream consumer work). Wave-2 round-1 closure audit recommended keeping PRs lean.
- **Cross-referencing**: this ADR's `relates_to` lists `adr:0052a` and `adr:0052d-prereq`; both parent ADRs continue to point at the same accepted SHAs; the relaxation is visible in ADR history as a discrete decision.

## 5. Parser changes

**Single file**: `crates/cobrust-frontend/src/parser.rs`.

The validator at L1134-1139 currently has a blanket reject for `ExprKind::Call { .. }`:

```rust
ExprKind::Call { .. } => Err(ParseError::Syntax {
    message: "borrow of a call-result is not supported in Wave-1 \
              (ADR-0052a §8 cap: borrow operand must be `Name`, `Name.field`, or `Name[idx]`)"
        .to_string(),
    span: operand.span,
}),
```

The relaxation replaces this with a guard that admits `Call(Attr(base, _), _)` when `base` is itself a borrowable place (recursive call to `validate_borrow_operand(base)`), and rejects all other `Call` shapes with an updated diagnostic that points at ADR-0052f:

```rust
ExprKind::Call { callee, .. } => match &callee.kind {
    // Method-form: callee is Attribute-access. Admit if receiver is
    // a borrowable place. Per ADR-0052f §2 cap relaxation.
    ExprKind::Access(AccessKind::Attribute { base, .. }) => {
        Self::validate_borrow_operand(base)
    }
    // Free-fn call or unresolved callee shape: still rejected.
    // Preserves the §2.5 compile-time-catch for `&free_fn(x)`.
    _ => Err(ParseError::Syntax {
        message: "borrow of a free-function call-result is not supported \
                  (ADR-0052f only relaxes method-form `&recv.method(...)`; \
                  borrow operand must be `Name`, `Name.field`, `Name[idx]`, \
                  or `Name.method(...)`)"
            .to_string(),
        span: operand.span,
    }),
},
```

No other parser change. No HIR change. No types change. No MIR change. No codegen change.

## 6. HIR / Types / MIR — no changes

The downstream pipeline is already correct for `Unary(Borrow, Call(Attr(...)))` post-0052d-prereq:

- **HIR lowering** (`crates/cobrust-hir/src/lower.rs:1078-1083` + `HirExprKind::Borrow` from ADR-0052a §8): produces `HirExprKind::Borrow(HirExprKind::Call(HirExprKind::Attr(...), args))`. No new arm required — the recursive lowering already handles nested calls inside borrows.
- **Types** (`crates/cobrust-types/src/check.rs`): the method-table chain dispatcher from ADR-0052d-prereq runs on the inner `Call(Attr, ...)` regardless of the outer borrow wrapper. The outer `ExprKind::Borrow` arm (ADR-0052a §6) computes `Ty::Ref(inner_ty)` from the rewritten PRELUDE-fn call's return type.
- **MIR** (`crates/cobrust-mir/src/lower.rs`): the inner `Call(Name("str_len"), [s])` (post-rewrite) lowers normally; the outer `ExprKind::Borrow` arm from ADR-0052a §7 emits `Operand::Copy` of the call-result place. The use-after-move catch at `borrow.rs:114` does not fire (`Ty::Ref(_)` is Copy).

The parser is the **only** layer that blocks the surface; relaxing it unblocks the entire pipeline.

## 7. F30 shadow-flip dry-run

Surface is small and additive. Grep at HEAD `4e05cbb`:

```bash
$ grep -rn "&[a-z][a-z_]*\.[a-z_]*(" examples/ crates/cobrust-frontend/tests/ crates/cobrust-types/tests/ crates/cobrust-mir/tests/
crates/cobrust-mir/tests/method_dispatch_f30_witness.rs:252 (in #[ignore]'d f30wit_method_03 source string)
```

**One predicted active consumer**: the `f30wit_method_03` test (currently `#[ignore]`'d). Removing the ignore is the single impl validation.

**Latent-consumer enumeration** (5-10 expected patterns post-impl, none currently present):

| # | Latent callsite pattern | Origin |
|---|---|---|
| 1 | `&s.len()` for explicit-borrow length read | LLM-friendly idiom; will appear in LC-100 migrations post-ADR-0052d round-2 |
| 2 | `&xs.get(i)` for borrow of list-element | post-Wave-2 list-method migration |
| 3 | `&s.trim()` borrow of trimmed string | Phase G+ string-pipeline idioms |
| 4 | `&f.floor()` borrow of float-floor (semantic no-op; cosmetic) | uniformity case |
| 5 | `&n.abs()` borrow of int-abs (cosmetic) | uniformity case |
| 6 | `&d.get(k)` borrow of dict lookup | uniformity case + existing dict-method table |
| 7 | `&s.contains(sub)` borrow of bool predicate (cosmetic) | uniformity case |
| 8 | `&s.replace(a, b)` borrow of replaced Str (non-trivial — borrows the returned Str place) | future Phase-G+ string transform |

Rows 1-3 and 8 are the semantically-meaningful cases. Rows 4-7 are cosmetic-but-accepted (LLM might write `&` defensively; parser admits, compiler lowers identically to non-borrowed form). Each row's failure-mode pre-impl is the same parse-reject diagnostic from L1134-1139; post-impl all pass.

**Zero existing program changes meaning.** Method-form callsites today (`d.keys()`, etc.) do not currently use the outer `&` wrapper. The relaxation is forward-looking surface; no behavioural cascade on existing code.

## 8. TEST + DEV PAIR plan

Per F28 strict-separation discipline. Single-file changes throughout; minimal ceremony.

### 8.1 TEST sprint (~30 min, sonnet — well-scoped corpus authoring per `feedback_subagent_model_tier.md` mid-tier)

Extend `crates/cobrust-frontend/tests/borrow_phase_g_parse_corpus.rs` with:

- **Well-typed parse (5-8 cases)**:
  - `&s.len()` (canonical witness)
  - `&xs.get(0)` (list-method receiver)
  - `&f.floor()` (float-method receiver)
  - `&n.abs()` (int-method receiver)
  - `&d.get(k)` (dict-method receiver, dispatcher first-arm)
  - `&s.trim()` (Str method returning Str)
  - `&xs.is_empty()` (List method returning Bool)
- **Ill-typed parse (3-5 cases)**:
  - `&free_fn()` (callee is `Name`, not `Attr` — must reject with ADR-0052f-specific diagnostic mentioning the cap relaxation scope)
  - `&foo(x, y)` (free-fn with args — same rejection)
  - `&(s.len())` parenthesised form (currently rejected; remains rejected post-relaxation since the operand becomes the parenthesised expr, still a `Call` shape, but the explicit-paren case is the same Call structure — must verify the validator recurses correctly; if test reveals different behaviour, classify direct/genuine and update ADR before merge)
  - `&undefined.method()` (parse-OK; defers to type-checker `UnknownName`)
  - `&123.method()` (literal receiver — parser must reject the receiver per existing literal cap before reaching the Call wrapper)

### 8.2 DEV sprint (~30 min, sonnet — single-fn edit per existing ADR)

- Edit `validate_borrow_operand` at `crates/cobrust-frontend/src/parser.rs:1134-1139` per §5 diff above.
- Un-ignore `f30wit_method_03` at `crates/cobrust-mir/tests/method_dispatch_f30_witness.rs:241` (remove the `#[ignore = "..."]` attribute).
- Run `cargo test --package cobrust-frontend --test borrow_phase_g_parse_corpus`, `cargo test --package cobrust-mir --test method_dispatch_f30_witness f30wit_method_03`, and full `cargo test --workspace --no-fail-fast` for cascade enumeration.

### 8.3 Total

~30min TEST + ~30min DEV + ~30min P10 5-gate review = **~1 day P10-direct PAIR wall-time**.

## 9. §2.5 compliance

Per CLAUDE.md §2.5 audit-teammate rubric:

- **Compile-time-catch wins**: `&free_fn_call()` (callee = `Name`) STILL rejected with a specific diagnostic that names ADR-0052f and lists the admitted forms. The diagnostic message tells the LLM the FIX path (use method-form receiver, or borrow the receiver first then call). The §2.5 Direction B "print the FIX, not just the diagnosis" rubric is honored at the diagnostic-message authoring level. No silent acceptance of inappropriate borrows.
- **Training-data overlap matched**: Rust's `&v.method()` parse is the canonical surface across the Rust training corpus. `&s.len()`, `&v.iter()`, `&xs.get(i)` are all in idiomatic Rust. The relaxation closes the §2.5 §B "method-call sugar + explicit borrow co-occurrence" overlap gap that ADR-0052a Wave-1 left as honest-debt.

## 10. Out of scope

- **`&mut s.method()` (mutable borrow of method-form)**: deferred to a future sub-ADR. Wave-2 ships shared-borrow-only per ADR-0052a §12. The validator update preserves this restriction (no `mut` admission added; the existing nested-borrow `&&` rejection at L1111-1115 still fires for any `&mut` shape Wave-1 doesn't model).
- **Complex chained `&a.b.c.method()`**: Wave-1 cap on `&a.b.c` field-chain (per ADR-0052a §8 `&ident.field` Wave-1 single-field-depth rule) still applies. The relaxation here is method-call only — the receiver `base` must itself satisfy the existing `validate_borrow_operand` rules (so single-field receivers like `&p.head.method()` would need the field-chain cap relaxed separately).
- **`&(expr_that_evaluates_to_call)` (parenthesised borrow of arbitrary expression)**: ADR-0052a Wave-1 cap on `&(complex_expr)` is intentional; this ADR does not touch it.
- **Borrow of generic function call**: `&Vec::new()` style. Cobrust has no `::` path syntax and no user-declared generics; out of scope by language surface.
- **LC-100 corpus migration to `&s.method()`**: ADR-0052d round-2 (not yet authored) consumes this relaxation and migrates example sites. Scope split is deliberate.

## 11. Consequences

### Positive

- Closes the deferred `f30wit_method_03` test from ADR-0052d-prereq impl blocker finding; un-ignoring the test is the single impl validation.
- Restores the F-G.3 precedence claim (ADR-0052 F-G.3 amendment + ADR-0052d-prereq §"Precedence with 0052a `&s`") to empirical truth at the parser layer.
- Unblocks ADR-0052d round-2 example migration (e.g. `s.len()` callsites can write `&s.len()` if explicit borrow is preferred for §2.5 LLM-friendliness signaling).
- Single-file change; minimal blast radius; reversible at any commit.
- Diagnostic for rejected `&free_fn()` form gets sharper (calls out ADR-0052f scope explicitly).

### Negative

- Slight asymmetry in the cap: method-form Call admitted, free-fn Call rejected. The asymmetry is principled (method-form has a borrowable receiver place; free-fn return is a temporary), but the LLM may need one round of compile-error feedback to learn the distinction. The §2.5-style FIX-in-diagnostic mitigates.
- Recursive `validate_borrow_operand(base)` from the method-form arm is structurally novel (other arms either accept directly or reject directly). Correctness depends on the recursion terminating on `Name` / `Access` leaves; verified by §8.1 well-typed corpus.

### Neutral

- No HIR / types / MIR / codegen surface. The relaxation is a pure parser-side gate adjustment.
- The §"Cascade enumeration" addendum methodology (per ADR-0052a §13, ADR-0052d-prereq §"Cascade enumeration") is structurally inapplicable here: the change is additive parser admission, not a flip of operand semantics. The F30 dry-run table in §7 + the cargo-test green at impl merge is the cascade evidence. If unexpected failures appear during DEV impl, classify per F30 SOP and append a `### Cascade enumeration (post-spike)` subsection before ratification.

## 12. Dispatch readiness

- **TEST budget**: ~30min (sonnet). 5-8 well-typed parse + 3-5 ill-typed parse extensions to `borrow_phase_g_parse_corpus.rs`.
- **DEV budget**: ~30min (sonnet). Single-fn edit at `parser.rs:1134-1139` + un-ignore `f30wit_method_03`.
- **P10 review + merge**: ~30min (5-gate green at self-hosted runner per `feedback_heavy_build_offload_to_workstation.md`).
- **Total wall-time**: ~1 day P10-direct PAIR.
- **Pre-dispatch checklist**:
  - [ ] ADR-0052f at status `proposed` (this commit).
  - [ ] No feature flag — relaxation is small + reversible.
  - [ ] HEAD anchor verified at `4e05cbb` (parser.rs:1134-1139 is unchanged from ADR-0052a §8 ratification).
  - [ ] `findings/0052d-prereq-impl-blocker.md` cross-referenced (Path A per §"Scope-of-fix analysis").
- **Branch**: `feature/adr0052f-borrow-of-call-relaxation` (P9-Wave2-R2).
- **Merge target**: `main`.
- **Ratification**: on impl merge; status `proposed` → `accepted` with `ratified_at` + `ratified_on` filled in same merge commit.
