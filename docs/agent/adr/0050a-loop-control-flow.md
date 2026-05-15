---
doc_kind: adr
adr_id: 0050a
title: "M-F.3.0 — Loop control flow (`break` / `continue`) semantics + contract seal"
status: proposed
date: 2026-05-16
last_verified_commit: TBD
supersedes: []
superseded_by: []
relates_to: [adr:0003, adr:0005, adr:0006, adr:0020, adr:0027, adr:0030, adr:0035, adr:0050]
discovered_by: ADR-0050 Phase F.3 Wave 1 dispatch — M-F.3.0 break/continue gap-closure sprint
---

# ADR-0050a: M-F.3.0 — Loop control flow (`break` / `continue`) semantics

## Context

ADR-0050 §"Implementation map" §M-F.3.0 mandates `break` / `continue` as the
first Wave 1 sprint. P9 pre-flight diagnosis on `feature/f3-break-continue`
(2026-05-16) revealed the implementation **already exists across every
layer** of the compiler:

| Layer | Surface | Anchor |
|---|---|---|
| Lexer (`crates/cobrust-frontend/src/lexer.rs`) | `KwBreak` + `KwContinue` reserved keywords | shipped pre-M11 (form-16 of core-30, ADR-0003) |
| Tokens (`crates/cobrust-frontend/src/token.rs`) | `TokenKind::KwBreak` + `TokenKind::KwContinue` | shipped pre-M11 |
| AST (`crates/cobrust-frontend/src/ast.rs`) | `StmtKind::BreakContinue(BreakKind)` + `enum BreakKind { Break, Continue }` | shipped pre-M11 |
| Parser (`crates/cobrust-frontend/src/parser.rs` L205-220) | Single-token statement reducer, EOS-terminated | shipped pre-M11 |
| Unparser (`crates/cobrust-frontend/src/unparse.rs` L85-93) | Renders `break\n` / `continue\n` per indent | shipped pre-M11 |
| HIR (`crates/cobrust-hir/src/tree.rs` L154-155) | `StmtKind::Break` + `StmtKind::Continue` (no payload) | shipped pre-M11 |
| HIR lower (`crates/cobrust-hir/src/lower.rs` L517-523) | `BreakContinue(_)` → HIR `Break` / `Continue` | shipped pre-M11 |
| Types (`crates/cobrust-types/src/check.rs` L308-319 + L82-84 + L415-417 + L434-436) | `loop_depth: usize` counter; reject if 0 | shipped pre-M11 |
| Type errors (`crates/cobrust-types/src/error.rs` L100-104) | `BreakOutsideLoop` + `ContinueOutsideLoop` | shipped pre-M11 |
| MIR (`crates/cobrust-mir/src/lower.rs` L201-202 + L419-436 + L712-718) | `loop_stack: Vec<(header_bb, exit_bb)>`; `Break` → `Goto(exit_bb)`, `Continue` → `Goto(header_bb)` | shipped pre-M11 |
| Codegen | Inherits Cranelift `Goto` from M9 baseline — no break/continue-specific surface needed | ADR-0023 / ADR-0030 |

Empirical verification at HEAD `30cf2b2`:

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 10:
        i = i + 1
        if i == 5:
            break
        if i == 3:
            continue
        print_int(i)
    print_int(99)
    return 0
```

Compiles cleanly and prints `1\n2\n4\n99` (verified `target/cobrust/test_break_continue` at HEAD `30cf2b2` on Mac local).

The implementation gap is not in the compiler. It is in the **contract**:

1. **No ADR captures the semantics.** ADR-0003 lists `break_continue` as
   form 16 of the core-30 but does not specify innermost-loop binding,
   scope discipline, or the exhaustive ill-typed rejection surface.
2. **Test coverage is insufficient.** `lower_forms.rs` has 2 cases;
   `control_flow_combinations_corpus.rs` has 6 cases; `ill_typed.rs` has
   2 cases. ADR-0050 §M-F.3.0 mandates ≥30 well-typed + ≥20 ill-typed
   *break/continue-focused* corpus on top of the structural ones.
3. **Triple-tree documentation is missing.** `docs/human/{zh,en}/getting-started.md`
   never mention `break` / `continue`. `docs/agent/modules/{frontend,hir,types,mir}.md`
   mention them only obliquely.
4. **No example.** `examples/` has no program that demonstrates
   loop-early-exit as a teaching artifact.

M-F.3.0 is therefore a **contract-seal sprint**: lock down the existing
behaviour with the ADR + corpus + docs + example so it cannot drift, and
so downstream M-F.3.1 (for-loop) + M-F.3.4 (dict iter) can compose
against a stable foundation.

## Options considered

### Option A — Add a new ADR-0050a contract, expand corpus, ship docs (CHOSEN)

- Pros:
  - Honours constitution §6 "ADR-or-it-didn't-happen" for what is now a
    load-bearing feature.
  - Locks in the innermost-loop binding so future label-syntax (Python's
    `break <label>`, dropped per ADR-0050 §"Sub-ADR slots") cannot
    silently regress.
  - Cleanly seals M-F.3.0 sprint as a wave-1 Phase F.3 deliverable
    without re-implementing working code.
  - Catches latent edge-case behaviours (break inside if-elif-else chain,
    continue at top of body, nested while-while break-innermost) through
    corpus expansion before the for-loop sprint (M-F.3.1) layers on top.
- Cons:
  - Zero functional code change; the sprint output is documentation +
    tests + example. Some reviewers may classify this as "doc-only" and
    deprioritise; the constitution §6 + ADR-0050's M-F.3.0 binding refute
    that — corpus is first-class.

### Option B — Skip ADR-0050a; just expand corpus

- Pros: smallest commit volume.
- Cons:
  - Violates constitution §6 — break/continue's semantics span 4 layers
    + 2 doc trees; an ADR must capture the cross-layer contract or it
    will drift the next time someone touches HIR/MIR.
  - Future audit will surface "where is the break/continue contract
    documented?" and have to be answered with grep.
- **Rejected.**

### Option C — Add `break <label>` and `continue <label>` while sealing the contract

- Pros: closes a Python-shape ergonomic gap in one sprint.
- Cons:
  - Constitution §2.2 + ADR-0050 §"Sub-ADR slots" §"ADR-0050a" explicitly
    **drop** labelled break/continue per "Cobrust drops Python `break label`
    per §2.2 minimalism; bare `break` / `continue` only; nested loops use
    innermost scope".
  - Labelled break is solvable post-fact via an early-return wrapping
    pattern (`fn inner(): … return early`) — the user already has a tool.
  - Adding labels expands the AST + parser + HIR + MIR + types surface
    significantly and would extend the M-F.3.0 sprint from D2 to D3-D4.
- **Rejected.** Honour ADR-0050's binding.

## Decision

Adopt **Option A** — ratify the existing implementation with ADR-0050a,
expand corpus to ADR-0050 §M-F.3.0 minimums, ship triple-tree docs and
an example.

### Semantics (binding contract)

#### Surface syntax

```cobrust
break
continue
```

Bare keywords only. No label. No expression payload. Each statement
must occupy its own line (parser enforces `expect_eos()` after the
keyword bump).

#### Binding rule

Both keywords bind to the **innermost enclosing loop** unconditionally.
A loop is either:

- A `while <cond>:` statement.
- A `for <pat> in <iter>:` statement.

`break` and `continue` inside a nested loop bind to the inner loop, never
the outer:

```cobrust
let i: i64 = 0
while i < 3:
    let j: i64 = 0
    while j < 3:
        if j == 1:
            break          # binds to the inner `while j < 3`
        j = j + 1
    # control resumes here after inner break
    i = i + 1
```

#### Semantics

- `break` skips the remaining loop body **and** the next condition
  recheck → control transfers to the statement immediately following the
  loop (including its optional `else:` block, which per Python tradition
  is **not** executed on break — confirmed in MIR lowering where break
  emits `Goto(exit_bb)`, and the `else` block writes are appended to
  `exit_bb` only on natural cond-false termination of `while`).
- `continue` skips the remaining loop body → control transfers to the
  loop header for the next condition evaluation. For `while`, this means
  re-evaluating the condition expression. For `for`, this means calling
  `iter.next()` again.

#### Scope discipline

`break` / `continue` are valid **only** inside a loop body. Specifically:

- ✗ Module top-level.
- ✗ Function top-level (no enclosing loop).
- ✗ Inside an `if` / `elif` / `else` body that is itself outside a loop.
- ✗ Inside a `match` arm that is outside a loop.
- ✗ Inside a `with` body that is outside a loop.
- ✗ Inside a nested function definition (loop scope **does not cross**
  function boundaries — a nested `fn` resets `loop_depth` to 0).
- ✓ Inside an `if` / `elif` / `else` body that is inside a loop body.
- ✓ Inside a `match` arm that is inside a loop body.
- ✓ Inside a `with` body that is inside a loop body.
- ✓ Inside the loop body of an inner loop (binds to the inner).

Type checker enforces via the `loop_depth: usize` counter
(`crates/cobrust-types/src/check.rs` L82-84). The counter increments on
loop entry (`check_loop` L405-444) and decrements on loop exit. A nested
function call's `check_item` saves and restores the counter — see
`return_stack` discipline for the analogue.

#### Reachability

Both statements diverge — they return `BlockOutcome::Diverges` from
`check.rs` L312 and L318. Statements following a `break` or `continue`
within the same basic block are unreachable; MIR's `lower_stmt` after
`Terminator::Goto` leaves `cur_block = None` and `ensure_open_block` opens
a fresh "dead" block for any tail statements — those statements emit but
flow into a block with no incoming edge, which the codegen DCE eliminates.

#### Empty loop body

```cobrust
while True:
    break
```

is well-typed and well-formed. `while True:` lowers to a header that
unconditionally enters the body block; the `break` terminates that body
with `Goto(exit)`; the exit block ends with `Return` (synthesised when
`return 0` follows the loop).

#### Side-effect ordering

`break` / `continue` emit only their `Goto` terminator. No drops, no
arg evaluations. Per ADR-0020 §"Drop schedule algorithm", any locals that
need dropping at the loop exit are scheduled at the exit block's
StorageDead chain — break and continue are **transparent to drop
scheduling** at the MIR layer because the loop's exit/header blocks are
the same destinations as natural termination.

### Layer contract (binding)

#### Lexer

```rust
"break"    => TokenKind::KwBreak,
"continue" => TokenKind::KwContinue,
```

Reserved across all contexts (cannot be used as identifiers).

#### Parser

```rust
TokenKind::KwBreak => {
    let span = self.bump().span;
    self.expect_eos()?;
    Ok(Stmt {
        kind: StmtKind::BreakContinue(BreakKind::Break),
        span,
    })
}
TokenKind::KwContinue => /* same shape, BreakKind::Continue */
```

`expect_eos()` ensures `break;` or `break foo` reject at parse time —
the keyword stands alone on its line.

#### AST

```rust
pub enum StmtKind {
    /// Form 16 — `break` / `continue` (single form, two keywords).
    BreakContinue(BreakKind),
    // …
}
pub enum BreakKind { Break, Continue }
```

Unspanned payload because both are zero-token statements after the
keyword bump.

#### HIR

```rust
pub enum StmtKind {
    Break,
    Continue,
    // …
}
```

Flattened from AST's tagged enum — HIR no longer needs the tag because
each branch is a distinct statement kind.

#### Types

```rust
StmtKind::Break => {
    if self.loop_depth == 0 {
        return Err(TypeError::BreakOutsideLoop { span: s.span });
    }
    Ok(BlockOutcome::Diverges)
}
// Continue: mirror with ContinueOutsideLoop.
```

Loop entry: `self.loop_depth += 1`. Loop exit: `self.loop_depth -= 1`.
`check_item` for a nested `fn` saves + restores `loop_depth` (currently
implicit because each `Ctx` is per-module and fns are checked in
sequence, but the contract requires the property; ADR-0050a tests it
explicitly).

#### MIR

```rust
loop_stack: Vec<(BlockId, BlockId)>,  // (header_bb, exit_bb)

StmtKind::Break => {
    if let Some((_, exit)) = self.loop_stack.last().copied() {
        self.ensure_open_block();
        self.terminate(Terminator::Goto(exit));
        Ok(())
    } else {
        Err(MirError::Internal("break outside loop".to_string()))
    }
}
// Continue: mirror with header (loop_stack.last().0).
```

Loop entry: `self.loop_stack.push((header, exit_block))`. Loop exit:
`self.loop_stack.pop()`. Both `LoopKind::While` (L671) and `LoopKind::For`
(L726) push/pop their pair.

The `MirError::Internal` arm is defensive — the type checker is the gate
that should reject break-outside-loop; if it ever reaches MIR, the
internal error fires (currently unreachable in well-typed input).

#### Codegen

No surface. The Cranelift backend lowers `Terminator::Goto` to a
single-successor branch via `builder.ins().jump(target_block, &[])`.
break/continue produce no new codegen primitives.

### Test corpus (binding minimum)

Per ADR-0050 §M-F.3.0 binding: ≥30 well-typed + ≥20 ill-typed
break/continue-focused cases. M-F.3.0 ships **as a single corpus crate**
across four files:

1. `crates/cobrust-frontend/tests/break_continue_parse_corpus.rs`
   — parser round-trips + unparse + reject malformed
   (`break foo`, `continue label`, `break;`).
2. `crates/cobrust-types/tests/break_continue_types_corpus.rs`
   — well-typed scope acceptance + ill-typed scope rejection
   (`break` at module top, fn top, inside `if` without loop, inside
   `match` without loop, inside nested fn whose outer is inside a loop).
3. `crates/cobrust-mir/tests/break_continue_mir_corpus.rs`
   — MIR shape assertions: `Goto(exit_bb)` vs `Goto(header_bb)`,
   loop_stack push/pop balance under deep nesting (≥5 levels),
   unreachable-tail removal after a break.
4. `crates/cobrust-cli/tests/cli_break_continue_e2e.rs`
   — end-to-end build + run + stdout match for 10+ programs covering:
   - single-loop break early-exit
   - single-loop continue skip
   - nested-loop break-innermost
   - nested-loop continue-innermost
   - break inside if-elif-else inside loop
   - break + post-loop computation
   - continue at top of body
   - while True: break (infinite-loop guard)
   - deep nesting (3+ levels)
   - break/continue combined with print + assignments

Corpus totals across the four files: ≥30 well-typed + ≥20 ill-typed
(distributed across all four).

### Documentation contract

#### Human tree

- `docs/human/en/getting-started.md` §"Loops" amended with a
  `break` / `continue` subsection (examples-before-abstractions per
  constitution §3.1).
- `docs/human/zh/getting-started.md` 1:1 parallel.

#### Agent tree

- `docs/agent/modules/frontend.md` — row for `BreakContinue` parsing.
- `docs/agent/modules/hir.md` — row for `Break` / `Continue` HIR variants.
- `docs/agent/modules/types.md` — row for `loop_depth` discipline + the
  two error variants.
- `docs/agent/modules/mir.md` — extend the existing form-16 line with
  the loop_stack semantics + Goto target rules.

#### Doc-coverage script

`scripts/doc-coverage.sh` gains an `M-F.3.0` block that:

1. Verifies every doc tree mentions `break` + `continue`.
2. Verifies all four corpus test files exist.
3. Verifies `examples/early_exit.cb` exists.
4. Verifies ADR-0050a is `status: accepted` once the sprint stamps it.

### Example

`examples/early_exit.cb` demonstrates break inside a `while` with a
post-loop print, mirroring the FizzBuzz/Fib example shape (ADR-0030):

```cobrust
fn main() -> i64:
    let i: i64 = 0
    let sum: i64 = 0
    while i < 100:
        i = i + 1
        if i == 7:
            continue
        if sum > 30:
            break
        sum = sum + i
    print(sum)
    return 0
```

Expected output: deterministic integer reflecting the sum after the
early-exit and skip rules.

## Consequences

### Positive

- Constitution §6 "ADR-or-it-didn't-happen" honoured for break/continue.
- Future M-F.3.1 (for-loop) + M-F.3.4 (dict iteration) compose against
  a stable, contract-locked foundation. The for-loop sprint can rely on
  `LoopKind::For` already pushing the loop_stack pair.
- Constitution §3 triple-tree doc sync enforced at the script level for
  this feature.
- The 30+20 corpus catches the latent edge cases (nested fn boundary,
  unreachable-tail removal) before list[str] (M-F.3.2) and dict
  (M-F.3.4) layer their own iteration semantics on top.

### Negative

- Zero functional change. Sprint output is documentation + tests + an
  example. Looks "doc-only" to a sloppy reviewer; constitution §6 and
  ADR-0050's binding refute that classification — corpus is first-class.

### Neutral / unknown

- Labelled `break <label>` / `continue <label>` remains permanently
  out-of-scope per constitution §2.2 minimalism. If a future sprint
  surfaces a concrete need that cannot be expressed via early-return
  wrappers, a new ADR may revisit; the present decision locks "no
  labels" until then.
- Python's `for...else:` and `while...else:` clauses (the else fires on
  natural cond-false termination, **not** on break) — Cobrust supports
  these in the AST (the `else_block: Option<Block>` field on While/For)
  and in MIR (the exit block runs the else_b lower). ADR-0050a does not
  change this; corpus includes 2 cases verifying break skips the else.

## Evidence

- ADR-0050 §"Implementation map" §M-F.3.0 — sprint mandate.
- `crates/cobrust-mir/src/lower.rs` L201-202 + L419-436 + L712-718 —
  existing loop_stack implementation.
- `crates/cobrust-types/src/check.rs` L82-84 + L308-319 + L415-417 —
  existing loop_depth implementation.
- `crates/cobrust-mir/tests/lower_forms.rs` L264-280 — existing 2-case
  MIR coverage (expanded to ≥10 by this ADR).
- `crates/cobrust-codegen/tests/control_flow_combinations_corpus.rs`
  L365-440 — existing 6-case E2E coverage (expanded to ≥10 by this ADR).
- `crates/cobrust-types/tests/ill_typed.rs` L358-374 — existing 2-case
  type rejection (expanded to ≥10 by this ADR).
- Constitution §2.2 — minimalism (no labelled break).
- Constitution §3 + §6 — doc + ADR mandate.
- ADR-0030 §"M11.1 fix" + ADR-0035 §"M11.3 lower_condition primitive" —
  the prior while-codegen ADRs that landed the underlying lowering this
  contract seals.

## Cross-references

- ADR-0003 — core 30 forms (form 16 = break/continue).
- ADR-0005 — HIR shape (StmtKind enumeration).
- ADR-0006 — type system (loop_depth analogue to return_stack).
- ADR-0020 — MIR shape (Terminator::Goto + BasicBlock model).
- ADR-0027 — for-protocol scaffolding (placeholder for-loop the M-F.3.1
  sprint replaces; ADR-0050a's loop_stack contract applies to whichever
  shape the for-loop ultimately desugars to).
- ADR-0030 + ADR-0035 — sibling while codegen ADRs.
- ADR-0050 — Phase F.3 batch (parent).
