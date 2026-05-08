---
doc_kind: finding
finding_id: m12-x-while-if-codegen-regression
last_verified_commit: ea093ef
dependencies: [adr:0019, adr:0023, adr:0027]
---

# Finding: M12.x while-loop-with-leading-if regression — empty stdout

## Hypothesis

After M12.x merged ADR-0027's five lowering specs into Cranelift codegen
(commit 68171c3 + post-merge integration through 22c6fae), the language
should support a real FizzBuzz program — `while` loop with `if/elif/else`
chain on `%` modulo, mutating a counter. This is the audit-#2 acceptance
test for `examples/fizzbuzz.cb` rewrite.

## Method

CTO post-merge probe on integrated `main` (HEAD `22c6fae`, 2423 tests
green, 4 pre-existing examples regression-free) on 2026-05-09:

| Test | Source pattern | stdout |
|---|---|---|
| test1 | `if n > 3: print("big") else: print("small")` | `big` ✓ |
| test2 | `let r = n % 3; if r == 0: print("Fizz")` | `Fizz` ✓ |
| test3 | `while n < 3: print("loop"); n = n + 1` | `loop\nloop\nloop` ✓ |
| test7 | `while n <= 3: print("loop"); if n == 2: print("two"); n = n + 1` | `loop\nloop\ntwo\nloop` ✓ |
| **test6** | `while n <= 3: if n == 2: print("two") else: print("not-two"); n = n + 1` | **empty** ✗ |
| **test8** | `while n <= 3: if n == 2: print("got two"); n = n + 1` | **empty** ✗ |
| **test4** (full fizzbuzz) | `while n <= 15: if/elif/elif/else; n = n + 1` | **empty** ✗ |

All build cleanly (link succeeds, exit code 0). The codegen-emitted
binary just produces no stdout.

## Result

**M12.x's Cranelift backend has a control-flow regression** when an
`if` (or `if/else` or `if/elif/else`) is the **first** statement of a
`while`-loop body. The trigger condition is:

```
while <cond>:
    if <branch>:                  ← FIRST stmt of loop body
        ...
    [else:]
        ...
    <subsequent stmts>            ← never executed
```

Workaround: prepend any non-conditional statement (e.g. `print(...)`)
before the `if`. Example: test7 passes precisely because `print("loop")`
sits between the `while` header and the `if`.

## Conclusion

ADR-0027 §1-§5 did not cover this control-flow shape; the M12.x corpus
tests in `crates/cobrust-codegen/tests/{aggregate,ref,cast}_corpus.rs`
+ `crates/cobrust-stdlib/tests/{for_protocol,fstring}_corpus.rs` all
exercise their lowering specs in isolation. The interaction between
`Stmt::While` (M9 baseline) and `Stmt::If` (M9 baseline) when the `If`
is the first basic-block successor inside the while-header was not in
any corpus.

The audit's #2 recommendation — rewrite `examples/fizzbuzz.cb` and
`examples/fib.cb` to real Cobrust — **cannot be completed at HEAD
22c6fae**. The codegen surface is insufficient (the bug above) AND
ADR-0027 omitted these examples from the binding deliverable list.

This is a clean instance of constitution §5.2 "Negative results are
documented under findings/, not hidden." It is being documented now so
that the next sprint targeting `examples/fizzbuzz.cb` rewrite has a
concrete bug to fix before re-attempting.

## Actionable consequences

1. **M11.1 sprint** (queue immediately as a Phase E follow-up):
   - Reproduce all 7 test cases above in
     `crates/cobrust-codegen/tests/while_if_corpus.rs`.
   - Identify root cause in
     `crates/cobrust-codegen/src/cranelift_backend.rs` — likely a
     basic-block successor wiring issue when MIR emits the
     `while`-header → `if`-header → `then`/`else` graph as the first
     edge from the loop entry.
   - Fix + verify all 7 cases produce correct stdout.
   - Then proceed to fizzbuzz.cb / fib.cb rewrites per the audit.

2. **Acceptance bar for `examples/fizzbuzz.cb` rewrite** (post-fix):
   - Source has zero literal `print("Fizz")` etc. constants.
   - Source has a real `while` loop with `if/elif/elif/else` over `%`.
   - `cobrust build && run` produces the canonical 1..15 FizzBuzz
     output.

3. **Acceptance bar for `examples/fib.cb` rewrite** (post-fix +
   `Constant::FnRef` Call lowering for user-defined fn, if not
   already supported):
   - Source has real `fn fib(n: i64) -> i64: if n < 2: return n;
     return fib(n - 1) + fib(n - 2)`.
   - `cobrust build && run` produces "fib(10) = 55" computed by
     recursion.

4. **Update `findings/examples-literal-print-debt.md` after M11.1
   ships**: change status from "open" to "closed" with cross-ref to
   M11.1 commit + this finding's bug.

## Cross-references

- ADR-0019 §"Definition of usable" — the audit-driven correctness bar.
- ADR-0023 (M9 codegen) — the Cranelift backend that M12.x extended.
- ADR-0027 (M12.x amendments) — five lowering specs delivered; this
  bug shows the M9 + M12.x interaction surface needs more corpus
  coverage.
- `findings/examples-literal-print-debt.md` — the audit's #2 finding
  this bug is blocking.
- `findings/translator-real-vs-synthetic-status.md` — the audit's #1
  remediation is unblocked (it doesn't depend on this codegen fix).
- Test sources at `/tmp/test{1..8}.cb` were CTO-authored probes; not
  committed (transient). The minimal repro in #1 above is committed
  in this finding's text.
