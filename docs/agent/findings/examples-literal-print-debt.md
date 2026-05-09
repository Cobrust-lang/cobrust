---
doc_kind: finding
finding_id: examples-literal-print-debt
last_verified_commit: TBD
status: closed
closed_by: M11.2 sprint (ADR-0034)
dependencies: [adr:0019, adr:0023, adr:0025, adr:0030, adr:0033, adr:0034]
---

# Finding: M11/M12 examples are literal-print decorations, not real Cobrust algorithms

## Hypothesis

When CTO declared "ADR-0019 §"Definition of usable for most projects" all
three lines met" at the M12 merge, the implicit claim was that the working
.cb programs (hello/fizzbuzz/fib/notebook) demonstrate Cobrust's expressive
power — arithmetic, control flow, recursion, multi-module composition,
package management.

## Method

Read the actual `examples/{hello,fizzbuzz,fib}.cb` and `examples/notebook/src/main.cb`
source files (third-party audit pass on 2026-05-09).

## Result

The four "working programs" all sidestep real Cobrust expression by
emitting literal strings through `print()`:

- `examples/fizzbuzz.cb`: 15 sequential `print("1") / print("2") /
  print("Fizz") ...` calls. **No `if`. No `%`. No loop.** The output
  matches a real FizzBuzz program byte-for-byte but the source carries
  zero algorithmic content.
- `examples/fib.cb`: a single `print("fib(10) = 55")`. **No recursion.
  No function call.** The docstring honestly admits the deferral to
  "M11.x scope per ADR-0025 §"Codegen amendments" Constant::FnRef row".
- `examples/notebook/src/main.cb`: ~1000 lines of literal
  `print("notebook: …")`. The docstring at L25-29 admits the design
  ("why fixed-string print(...) calls instead of f-strings + loops?").
- `examples/hello.cb`: legitimately a single `print("hello, world")`.

The notebook was sized to ≥1000 .cb LOC for ADR-0019 §"Definition of
usable" line 3 by inflating the literal-print body, not by writing
non-trivial Cobrust.

## Conclusion

ADR-0019's three lines are **textually** met but the spirit ("usable
for most projects" implies a Python user could write a real algorithm)
is not. The Constitution §1.1 promise ("syntactically familiar to
Python users") evaporates the moment a Python user opens fizzbuzz.cb.

This is a documented blocker on:

- M11 followups (Aggregate/Ref/Cast Rvalue + for-protocol + f-string)
  per ADR-0025.
- M9 followups (Constant::FnRef Call lowering for non-runtime-helper
  callees) per ADR-0023 §"Followups".
- M2 BinOp / branch lowering at codegen — currently the M9 Cranelift
  backend stubs `Rvalue::BinaryOp` for non-int paths and most
  arithmetic Rvalues (`(n - 1)` etc).

P9-M12.x (currently in flight at `feature/m12-x-codegen-amendments`,
ADR-0027) is the named sprint to lift four of these (Aggregate / Ref
/ Cast / for-protocol / f-string). Function-call Rvalue
(`Constant::FnRef` for user-defined fn) is **not** explicitly in
ADR-0027; it must be added to M12.x scope or land as a separate
M11.1 sprint per the third-party audit's recommendation.

## Actionable consequences

1. **M12.x scope amendment** (in-flight): if M12.x's codegen pass
   does not also lift `Constant::FnRef` Call lowering, the deferred
   examples (`fib` real recursion + `fizzbuzz` real `if/elif/%`)
   stay literal. CTO should send a SendMessage to M12.x agent
   amending scope, OR land a follow-up M11.1 sprint immediately.

2. **Acceptance bar for M12.x merge**: post-merge, fizzbuzz.cb and
   fib.cb must be rewritten to use real Cobrust constructs. The
   acceptance test is "the .cb source has no literal `print("Fizz")`
   for the FizzBuzz example; FizzBuzz is computed."

3. **Notebook example revisit**: the notebook's literal-print bulk
   is a workaround. After M12.x, the notebook should be rewritten to
   exercise actual stdlib (List append + Dict insert + iter + format)
   without inflating LOC artificially.

4. **README.md correction**: the README still says "M0 — repository
   skeleton; compiler / runtime / AI translation subsystem are not
   yet implemented" (verified). This must be updated to reflect the
   real state: compiler skeleton ships M9, runtime ships M11, but
   the language is at M11+M12-baseline expressive depth (literal
   prints work; real algorithms TBD at M12.x).

## Closed by M11.1 (ADR-0030) — partial; finalized by M11.2 (ADR-0034)

The M11.1 sprint (ADR-0030) fixed the while-loop-with-leading-if codegen
regression that blocked this finding's remediation, then rewrote both
examples:

- `examples/fizzbuzz.cb`: real while + if/elif/elif/else + `%` algorithm.
  `cobrust run examples/fizzbuzz.cb` produces the canonical 1..15 FizzBuzz
  sequence computed, not printed as literals.
- `examples/fib.cb`: 🟡 PARTIAL post-M11.1 — real *iterative* algorithm
  (while loop + mutation) only. The recursive form was deferred to
  M11.2 (`Constant::FnRef` Call lowering per ADR-0025 §"Codegen
  amendments"); audit-#2 review-claude review item #2 explicitly
  flagged this as the gap.

The M11.2 sprint (ADR-0034) closed the gap. `examples/fib.cb` now
implements the canonical recursive form:

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

`cobrust build examples/fib.cb && ./target/cobrust/fib` produces stdout
**bit-identical** to `fib(10) =\n55\n` (verified via `cmp` against an
explicit expected fixture during M11.2 sprint).

Status: ✅ DONE. Both fizzbuzz and fib now exercise real Cobrust
algorithms; the audit-#2 spirit-of-usable check is closed.

`examples/notebook/` remains as a separate audit item
(`findings/translator-real-vs-synthetic-status.md`).

## Cross-references

- Constitution `CLAUDE.md` §1.1, §5.2 (negative results documented in
  findings/, not hidden — this finding implements that obligation).
- ADR-0019 §"Definition of usable for most projects".
- ADR-0023 (M9 codegen) §"Followups" — Constant::FnRef Call lowering.
- ADR-0025 (M11 stdlib + runtime) §"Codegen amendments" deferral list.
- ADR-0027 (M12.x codegen + stdlib amendments) §"Lowering specifications" —
  the in-flight sprint that lifts four of five followups.
- ADR-0030 (M11.1 fix) — the sprint that closed this finding.
- Third-party audit `review-claude` 2026-05-09 (in-message, not committed).
