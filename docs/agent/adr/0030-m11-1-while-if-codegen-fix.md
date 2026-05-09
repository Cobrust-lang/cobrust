---
doc_kind: adr
adr_id: 0030
title: M11.1 — fix while-loop-with-leading-if codegen regression + close audit-#2 (real fizzbuzz / fib)
status: accepted
date: 2026-05-09
last_verified_commit: d178a3f
supersedes: []
superseded_by: []
---

# ADR-0030: M11.1 — fix while-loop-with-leading-if codegen regression

## Context

CTO post-merge audit-#2 acceptance probe on integrated `main`
(HEAD `2af90cc`, 2423 tests green) discovered a Cranelift codegen
regression at the M9 + M12.x interaction surface:

```
while <cond>:
    if <branch>:                  ← FIRST stmt of loop body
        ...
    [else: ...]
    <subsequent stmts>            ← never executed
```

Trigger condition: `if` is the **first** statement of a `while`-loop
body. Workaround: prepend any non-conditional stmt (e.g. `print(...)`)
before the `if`.

7 minimal repros documented in
`docs/agent/findings/m12-x-while-if-codegen-regression.md` §Method:

- ✓ test1 (if/else top-level)
- ✓ test2 (if + modulo)
- ✓ test3 (while + print + mutation, no if)
- ✓ test7 (while + print + if-no-else + mutation) — workaround
- ✗ test6 (while + if/else + mutation) — bug
- ✗ test8 (while + if-no-else + mutation) — bug
- ✗ test4 (full fizzbuzz) — bug

**Why this surfaced now**: M12.x's ADR-0027 §1-§5 corpus tests in
`crates/cobrust-codegen/tests/{aggregate,ref,cast}_corpus.rs` +
`crates/cobrust-stdlib/tests/{for_protocol,fstring}_corpus.rs`
exercised lowering specs in isolation. The M9 baseline's
`codegen_diff_corpus.rs` only checks "compiles + links + exit-0";
empty stdout when an `if` sits at the top of a `while` body passes
exit-0 silently. No corpus combined while + if + mutating loop body.

**Why this matters**: blocks the audit's #2 recommendation (rewrite
`examples/fizzbuzz.cb` and `examples/fib.cb` as real Cobrust
algorithms instead of literal-print constants), documented in
`docs/agent/findings/examples-literal-print-debt.md`. Audit-#2 is
gating "constitution §1.1 (Language & Runtime) is real, not a
demo" — the canonical proof-of-life FizzBuzz currently prints
canned strings rather than executing the algorithm.

## Options considered

1. **Patch in `cobrust-mir/src/lower.rs`** — fix the HIR-to-MIR
   lowering of `Stmt::While` so the body's first `Stmt::If` doesn't
   collapse into a malformed terminator. The MIR `tree.rs`
   `BasicBlock` enum would be unchanged.

2. **Patch in `cobrust-codegen/src/cranelift_backend.rs`** — fix the
   Cranelift translation of `BasicBlock` successors so an `if`-as-
   first-stmt-in-loop is wired correctly. The MIR is unchanged.

3. **Patch both** — if the MIR has an invariant that "first stmt of a
   loop body is always non-terminating" and codegen relies on it,
   fix codegen to handle the new case.

## Decision

**Investigate first, then patch wherever the bug actually lives.** The
diagnosis surface is `cranelift_backend.rs` line ~where `Loop` /
`Goto` / `If` terminators are emitted (search: `Terminator::Loop`,
`Terminator::SwitchInt`, `BasicBlockId`); cross-reference with
`mir/src/lower.rs` (`fn lower_stmt`, `Stmt::While`).

The dispatched agent must:

1. Reproduce the 7 test cases from the finding's §Method as a new
   integration corpus `crates/cobrust-codegen/tests/while_if_corpus.rs`.
   Each case shells out to the `cobrust` binary (via `Command::new`
   per existing `cli_subcommands.rs` pattern), runs the produced
   binary, captures stdout, and asserts equality with the expected
   output.

2. Run the corpus to confirm 4 fail / 3 pass at HEAD `d0f8934`. This
   is the **failing-test-first** discipline (constitution §6).

3. Diagnose the root cause. Likely candidates (CTO-conjectured, agent
   should verify):

   - In MIR, `Stmt::While` lowers to `[header, body, after]`. The
     `header` block's terminator branches on the loop condition. The
     `body`'s first instruction may be an `if`, which itself lowers
     to a sub-block structure. Suspect: when `body` is a single `If`,
     MIR may emit `body == if-header` (alias, not goto), and the
     codegen path that resolves `body`'s `BasicBlockId` to a Cranelift
     `Block` might double-resolve or skip.

   - Or: in codegen, when emitting the `Loop` terminator for the
     while-header, the successor `body` block might be resolved as
     an empty pass-through block when it's actually an `if`-header.

4. Apply the minimal fix. Re-run the 7 corpus cases — all must pass.

5. Rewrite `examples/fizzbuzz.cb` to a real algorithm:

   ```
   let n: i64 = 1
   while n <= 15:
       if n % 15 == 0:
           print("FizzBuzz")
       elif n % 3 == 0:
           print("Fizz")
       elif n % 5 == 0:
           print("Buzz")
       else:
           print(n)
       n = n + 1
   ```

   Build + run produces the canonical 1..15 FizzBuzz output.

6. Rewrite `examples/fib.cb` to recursive form:

   ```
   fn fib(n: i64) -> i64:
       if n < 2:
           return n
       return fib(n - 1) + fib(n - 2)

   fn main():
       print("fib(10) =")
       print(fib(10))
   ```

   Build + run produces "fib(10) =\n55". (Adapt phrasing if
   `fn`-recursion or the `print(int)` call shape needs adjustment.)

7. Update `docs/agent/findings/examples-literal-print-debt.md`:
   change status from "open" to "closed", reference the M11.1
   fix commit + this ADR.

8. Verify cold integrated rebuild: `cargo test --workspace --quiet`
   reports ≥ 2423 tests pass + zero new failures.

9. Verify doc-coverage: `bash scripts/doc-coverage.sh` exits 0.

10. Stamp `last_verified_commit` on this ADR + the
    while-if-codegen-regression finding to the M11.1 fix commit SHA.

## Consequences

- **Audit-#2 closed**: `examples/fizzbuzz.cb` and `examples/fib.cb`
  now execute real algorithms, demonstrating constitution §1.1
  (Language & Runtime half) is functional end-to-end.
- **Codegen confidence**: 7-case while-if corpus prevents regression.
  This is a permanent test-suite addition — never delete.
- **`examples/notebook/` translated bundle**: still has ~1000 literal
  prints (the LLM-translated content). That's a separate audit item
  (translator-real-vs-synthetic), not blocked by M11.1.

## Acceptance gate (Done means)

1. ✅ `crates/cobrust-codegen/tests/while_if_corpus.rs` exists with
   7 test cases (test1..test8 minus the 2 that overlap test7).
2. ✅ `cargo test --workspace --quiet` reports ≥ 2423 tests pass +
   the 7 new cases ≥ 4 of which previously failed all now pass.
3. ✅ `examples/fizzbuzz.cb` source has zero literal-string-canned
   "Fizz"/"Buzz"/"FizzBuzz" if/else without `% 3`/`% 5`/`% 15`
   modulo. `cobrust build examples/fizzbuzz.cb && ./out` produces
   the canonical 1..15 sequence.
4. ✅ `examples/fib.cb` source has a real recursive `fn fib`.
   Build + run produces "55" (or close textual proof).
5. ✅ `docs/agent/findings/examples-literal-print-debt.md` status
   updated to "closed" with cross-ref.
6. ✅ Doc-coverage gate passes; no docs-tree drift.
7. ✅ ADR-0030 + finding both have `last_verified_commit` stamped.
8. ✅ Atomic commits per constitution §6.

## Cross-references

- ADR-0019 §"Definition of usable" — the audit-driven correctness bar.
- ADR-0023 (M9 codegen) — the Cranelift backend being patched.
- ADR-0027 (M12.x amendments) — the prior expansion that didn't
  cover this interaction surface.
- `findings/m12-x-while-if-codegen-regression.md` — the bug doc.
- `findings/examples-literal-print-debt.md` — the audit-#2 finding
  this fix closes.
