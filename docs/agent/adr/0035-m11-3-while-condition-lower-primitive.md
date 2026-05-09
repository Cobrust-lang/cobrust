---
doc_kind: adr
adr_id: 0035
title: M11.3 — `lower_condition` root primitive shared by `if` + `while` heads
status: accepted
date: 2026-05-09
last_verified_commit: cfb7fd0
supersedes: []
superseded_by: []
---

# ADR-0035: M11.3 — `lower_condition` root primitive shared by `if` + `while` heads

## Context

Per `findings/while-binop-eq-zero-condition-miscompile.md` (P0,
discovered_by: review-claude LC 263 farm Round 1 on 2026-05-09):

```cobrust
fn main() -> i64:
    let n: i64 = 6
    if n % 2 == 0:
        print("if-yes")        # PRINTS ✓
    while n % 2 == 0:
        print("while-yes")     # NEVER PRINTS ✗
        n = 9999
    print_int(n)               # outputs 6, not 9999
    return 0
```

The same boolean expression `n % 2 == 0` evaluates **truthfully** in an
`if` head but **falsely** in a `while` head, on the first iteration with
the same `n` value. Verified at HEAD `ce0cf23` by CTO; build clean,
linker clean, output silently wrong.

Probe matrix (per finding §"Probe matrix"):
- ❌ `while n % 2 == 0` — body NEVER entered
- ✓ `while m == 0` (precomputed `let m = n % 2`) — body enters
- ✓ `while n != 1` — body enters
- ✓ `while n > 0` — body enters

Trigger: **`while` head with a `<BinOp> == 0` (or `!= 0`) condition
where the LHS is a non-trivial BinOp expression**. The `if` head with
the same condition shape works correctly.

This is the **24-hour 3rd independent `while` codegen bug** (after
M11.1 while-leading-if + M12.x 4-block i8/i64 narrow-type). Three
adjacent bugs in `while`-head lowering strongly suggest a single
under-tested primitive; per `findings/two-bugs-one-fix-option-c-pattern.md`
the right move is to **find the root primitive, not patch the surface**.

## Options considered

1. **Surgical patch in `lower_while_head` only** — find the specific
   `<BinOp> == 0` short-circuit (suspected: `while` path fuses
   `BinOp == 0` into `not BinOp` truthy check, dropping the integer-
   equality semantic). Change to emit a proper `icmp eq` then `brif`.
   **Rejected** — patches the surface. The `while` head will keep
   diverging from `if` head; next regression in this region is just a
   matter of time.

2. **Root primitive `lower_condition`** — extract a single shared
   helper:
   ```rust
   fn lower_condition(&mut self, expr: &Expr) -> ir::Value {
       // Lower `expr` to an i64 SSA value, then if the static type is
       // bool emit it directly; if integer, emit `icmp ne 0` so
       // downstream `brif` has a 1-bit predicate per Cranelift contract.
   }
   ```
   Refactor both `lower_if_head` and `lower_while_head` to call
   this helper. Same primitive emits the same IR for both — by
   construction the bug shape from the finding's probe matrix
   cannot recur in either head independently.
   **Chosen.** Same spirit as ADR-0033 Option C (`inferred_locals`
   threading + fixed-point) and the methodology finding
   `two-bugs-one-fix-option-c-pattern.md` §4.5 decision flowchart
   (5 early signals: same error class? — yes, both produce wrong
   bool; same trigger pattern? — both touch BinOp+`==0`; shared
   layer? — codegen condition lowering; threshold/depth? — bug
   only fires on non-trivial BinOps).

3. **MIR-level normalisation** — fold `<BinOp> == 0` into a canonical
   form during MIR lowering, so codegen sees the same shape for both
   heads. **Rejected** — out-of-scope MIR refactor; the bug is in
   codegen translation, fix it there.

## Decision

**Option 2.** Implementation map:

```
crates/cobrust-codegen/src/cranelift_backend.rs
  ├── fn lower_condition(&mut self, expr: &Expr) -> Result<ir::Value, _>  // NEW
  │     // Returns a 1-bit SSA value suitable for use as `brif` predicate.
  │     // Integer LHS → emit `icmp ne 0`; bool LHS → use directly.
  │     // BinOp condition like `a == b` → emit `icmp eq lower(a), lower(b)`.
  │
  ├── fn lower_if (existing) → call lower_condition for the head
  │
  └── fn lower_while (existing) → call lower_condition for the head
```

### Interaction with prior ADRs

- **ADR-0033 (Option C inferred_locals fixed-point)**: orthogonal.
  `lower_condition` consumes already-typed SSA values from
  `lower_expr`. The fixed-point `inferred_locals` map is consulted
  inside `lower_expr` for `Ty::None` temps; that pre-condition is
  unchanged. Verified by adding a corpus case
  `while_condition_through_inferred_locals_chain` that exercises a
  `Ty::None` temp inside a while head BinOp.

- **ADR-0034 (M11.2 Constant::FnRef Call lowering)**: orthogonal.
  Function-call returns flow through `lower_expr` like any other
  RHS; the condition layer doesn't see the call shape directly.

- **ADR-0030 (M11.1 while-leading-if)**: orthogonal.
  M11.1 fixed `Stmt::While` body's first `Stmt::If` block-id resolution
  (which lower goes where in MIR). M11.3 fixes the head condition's
  IR shape (what bytes get emitted for the comparator). Two different
  axes of the same statement.

## Done means

1. New helper `lower_condition` in `crates/cobrust-codegen/src/cranelift_backend.rs`.
2. Both `lower_if` and `lower_while` rewritten to call `lower_condition`.
3. The LC 263 minimal repro from
   `findings/while-binop-eq-zero-condition-miscompile.md` §"Minimum
   reproducer" produces the **expected** stdout:
   ```
   if-yes
   while-yes
   final n =
   9999
   ```
   (or equivalent for the canonical print sequence that finding
   specifies). Verified by `cmp` against expected stdout.

4. New regression corpus `crates/cobrust-codegen/tests/while_condition_corpus.rs`
   with **≥ 12 cases**:
   - `while_binop_mod_eq_zero` — the LC 263 trigger (`while n % 2 == 0`)
   - `while_binop_mod_ne_zero` — `while n % 2 != 0`
   - `while_binop_add_eq_zero` — `while a + b == 0`
   - `while_binop_sub_ne_zero` — `while a - b != 0`
   - `while_binop_mul_eq_zero` — `while a * b == 0`
   - `while_binop_div_eq_zero` — `while a / b == 0`
   - `while_compare_lt` — `while n < 10` (existing happy path)
   - `while_compare_eq` — `while n == 5`
   - `while_through_temp` — `let m = n % 2; while m == 0:` (probe 1
     workaround — must continue working)
   - `while_nested_binop` — `while (a + b) % c == 0:`
   - `while_binop_with_function_call` — `while fact(n) == 0:` (verifies
     ADR-0034 FnRef interaction)
   - `while_condition_through_inferred_locals_chain` — `Ty::None`
     temp through the while head (verifies ADR-0033 interaction)

5. The above 12 corpus cases must work in TWO directions:
   - **As `while` head** (covered above)
   - **As `if` head** with the same expression (parallel sibling tests
     `if_<same_name>_corpus`) to verify the shared primitive doesn't
     regress `if`. ≥ 12 sibling cases.

6. `findings/while-binop-eq-zero-condition-miscompile.md` status
   updated from `open` to `closed_by: <fix commit SHA>` with a
   §"Resolution" section.

7. ADR-0035 stamped `last_verified_commit` to merge SHA.

8. All 5 standard gates green:
   - `cargo fmt --all --check`: 0
   - `cargo clippy --workspace --all-targets --locked -- -D warnings`: 0
   - `cargo build --workspace --all-targets --locked`: 0
   - `cargo test --workspace --locked`: 0; total count goes UP
     (≥ 2,495 baseline + 24 new corpus = ≥ 2,519)
   - `bash scripts/doc-coverage.sh`: 0

9. Triple-tree doc sync:
   - `docs/agent/modules/codegen.md`: append note on shared
     `lower_condition` primitive + cross-ref to ADR-0035, ADR-0033,
     ADR-0034
   - `docs/human/{zh,en}/architecture.md`: M11.3 row in milestones if
     present
   - `scripts/doc-coverage.sh`: extend if new public surface

## Consequences

### Positive

- LC 263 + GCD-via-Euclid + factor-reduction + bit-traversal idioms
  unblocked. Number-theory algorithm class works correctly.
- `if` and `while` heads share a single primitive — eliminates
  drift class permanently. Future `for` head (when added) routes
  through the same primitive.
- Externally validated by review-claude organic stress test —
  publishable as `findings/two-bugs-one-fix-option-c-pattern.md`
  applied a second time.

### Negative

- Codegen complexity: 1 new helper fn + 2 sites refactored. Net
  ~50-80 lines change in `cranelift_backend.rs`. Acceptable.

### Neutral

- ADR-0035 takes the audit-#3 slot that was previously hypothesised
  for `@py_compat` hard-bind. Audit #3 itself splits per review-claude
  handoff §A.3 into:
  - **Audit #3a** (prompt-design fix in `build_translation_prompt`)
    — distinct sprint, Task #36 description should be amended
  - **Audit #3b** (`@py_compat` hard-bind, queued until stateful
    function audit produces concrete divergence)
  ADR-0036 reserved for #3a; ADR-0037 for #3b when their anchors land.

### Risk

- **`if` regression**: refactoring `lower_if` could regress the
  M11.1-fixed while-leading-if path or other `if` corner cases.
  Mitigation: keep the M11.1 corpus + M11.1.1 corpus + while-condition
  parallel `if`-sibling corpus all green at every step.

## Layer correction (post-merge addendum, 2026-05-09)

ADR-0035 §"Context" + §"Decision" hypothesised the bug lived in
`crates/cobrust-codegen/src/cranelift_backend.rs` (an `if`-vs-`while`
divergence in head IR emission). **The empirical fix landed in
`crates/cobrust-mir/src/lower.rs`** instead.

Per the M11.3 P9 sub-agent's CLIF + MIR dump diagnosis: the bug
was `BodyBuilder::lower_loop`'s While arm resetting `cur_block =
header` before the SwitchInt terminator was written. For
`<BinOp> == 0` shapes, `lower_bin`'s Mod path emits an Assert(NotEq
rhs 0) on a div-guard block, advancing `cur_block` to its successor
where the final `_eq` lives. The While arm's reset overwrote
`header`'s Assert terminator with SwitchInt(_eq), orphaning `_eq`
in an unreachable block. Each iteration read `_eq`'s pre-init
zero (false), body never entered.

The fix's location (MIR not codegen) is a **layer correction**
relative to ADR-0035 §"Decision" Implementation map. The
`lower_condition` root primitive is materialised in MIR — the
helper extracts the post-`lower_expr` block + operand pair and
both `lower_if` and `lower_loop` consume it identically.

Same pattern as ADR-0034 §"Implementation map" amendment (M11.2 P9
also found the actual fix needed an MIR-side `lower_call` extension
beyond the codegen-only constraint). **Lesson for future codegen
ADRs**: an "if-vs-while" or "call-vs-non-call" divergence
hypothesis at the codegen layer should default to **also
checking MIR lowering** before locking the implementation map.

This corrects the audit trail without invalidating §"Decision"'s
Option-2 root-primitive choice — the choice was correct; the
layer was wrong.

## Cross-references

- finding `while-binop-eq-zero-condition-miscompile` — bug doc
- finding `two-bugs-one-fix-option-c-pattern` — methodology that motivates Option 2
- ADR-0033 — same-spirit prior root-primitive fix
- ADR-0034 — orthogonal Constant::FnRef Call lowering
- ADR-0030 — sibling while codegen fix (M11.1)
- LC 263 farm: `/Users/hakureirm/codespace/Study/Cobrust-leetcode-farm/lc_263_ugly/`
- review-claude handoff: `/Users/hakureirm/codespace/Study/review-claude-handoff/README.md` §A.1
