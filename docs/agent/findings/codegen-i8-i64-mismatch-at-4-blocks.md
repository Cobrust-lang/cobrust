---
doc_kind: finding
finding_id: cobrust-codegen-i64-i8-mismatch-at-4-similar-blocks
last_verified_commit: d178a3f
discovered_by: review-claude (third-party audit window)
discovered_during: Conway-toy external-user stress test (out-of-workspace .cb program)
related: m12-x-while-if-codegen-regression, m11-1-1-control-flow-corpus
---

# Finding: Cranelift verifier rejects iadd.i8 with i64 operand at 4+ identical inline compute blocks

## Hypothesis

After M11.1's while-leading-if codegen fix (commit `ea093ef`) closed
`m12-x-while-if-codegen-regression`, the Cobrust codegen surface for
"deep arithmetic inside a `while` body" was assumed clean. The M11.1.1
control-flow corpus agent (currently running in worktree
`feature/m11-1-1-control-flow-corpus`) is enriching the regression
net by enumerating while/if combinations.

This finding records a parallel bug surface — **arithmetic-block
repetition inside one `while` body** — that the corpus enumeration
will need to cover.

## Method

Out-of-workspace package `Conway-cobrust-toy/` (path
`/Users/hakureirm/codespace/Study/Conway-cobrust-toy/`) implementing
Wolfram Rule 30 cellular automaton, width 5, 30 generations, all in
one `main` fn (forced because `Constant::FnRef` Call lowering for
user-defined fns is M11.2 deferred).

Each cell's next-bit computation is a 5-line block of identical shape:

```cobrust
l_i = (s / X) % 2
m_i = (s / Y) % 2
r_i = (s / Z) % 2
or_i = m_i + r_i - m_i * r_i        # OR via i64 arithmetic
n_i = (l_i + or_i) % 2              # XOR via i64 arithmetic
```

5 cells × 5 lines = 25 statements, all inside `while g < 30:`. All
variables hoisted to outer scope (`let x: i64 = 0` declarations
before the loop), inside the loop only reassignment.

## Result

`cobrust build` fails with:

```
cobrust build: Cranelift error: Verifier errors: - inst441
  (v520 = iadd.i8 v515, v518): arg 1 (v518) has type i64, expected i8
```

Despite the verifier error, the lowering **continues** and emits an
executable that produces wrong output (smoke run on a 4-cell version
with `s = 30` should yield `result = 3`; emits `5` instead).

Binary search isolates the threshold:

| Cells | Build verifier | Binary correctness |
|---|---|---|
| 1 | ✓ pass | ✓ correct |
| 2 | ✓ pass | ✓ correct |
| 3 | ✓ pass | ✓ correct |
| 4 | ✗ iadd.i8/i64 mismatch | ✗ wrong (still emits) |
| 5 | ✗ same | ✗ wrong (still emits) |

The cells use only:
- integer arithmetic (`+ - * / %`)
- mutable reassignment of pre-declared `let x: i64 = 0` slots
- no `if/else`, no calls

So the bug is **not** the M11.1-fixed while-leading-if path. It is
in either:
1. SSA phi-merge type narrowing across the loop back-edge when the
   number of i64 phi inputs exceeds some threshold, OR
2. A constant-folding / partial-evaluation pass that selects `i8`
   for some expression with values bounded to `{0, 1}` (e.g. mod-2
   results) but a downstream consumer still expects `i64`.

The fact that mod-2 values are arithmetically bounded to {0, 1} —
which `i8` could represent — but the program declares them `: i64`
hints at hypothesis 2.

## Reproduction (minimal)

`Conway-cobrust-toy/src/main.cb`, 4-cell version:

```cobrust
fn main() -> i64:
    let s: i64 = 30
    let m0: i64 = s % 2
    let r0: i64 = (s / 2) % 2
    let or0: i64 = m0 + r0 - m0 * r0
    let n0: i64 = or0 % 2
    let l1: i64 = s % 2
    let m1: i64 = (s / 2) % 2
    let r1: i64 = (s / 4) % 2
    let or1: i64 = m1 + r1 - m1 * r1
    let n1: i64 = (l1 + or1) % 2
    let l2: i64 = (s / 2) % 2
    let m2: i64 = (s / 4) % 2
    let r2: i64 = (s / 8) % 2
    let or2: i64 = m2 + r2 - m2 * r2
    let n2: i64 = (l2 + or2) % 2
    let l3: i64 = (s / 4) % 2
    let m3: i64 = (s / 8) % 2
    let r3: i64 = (s / 16) % 2
    let or3: i64 = m3 + r3 - m3 * r3
    let n3: i64 = (l3 + or3) % 2
    let result: i64 = n0 + n1 * 2 + n2 * 4 + n3 * 8
    print_int(result)
    return 0
```

Note: this version has **no `while` loop**. The bug fires on
straight-line code too — eliminating the loop-phi hypothesis (1).

## Conclusion

- **Two distinct bugs** exposed:
  1. Cranelift verifier rejects an IR where Cobrust codegen has
     selected mismatched integer types for `iadd`. The verifier
     correctly catches it.
  2. Cobrust's CLI does not abort on verifier rejection — it
     proceeds to emit an executable. Linker accepts what Cranelift
     produced anyway, and the executable runs but with wrong
     output. This is a **silent miscompilation surface** and is the
     more dangerous of the two.
- **Threshold:** 4 identical 5-line inline compute blocks within one
  `fn main`. 3 blocks pass, 4+ fail.
- **Independence of `while`:** The bug fires on straight-line code
  too. The conjunction (`while` + 4 blocks) was the original repro;
  isolating to straight-line confirms the loop phi is not implicated.

## Recommended actions

1. **CLI hardening (P0, mechanical):** make `cobrust build` exit
   non-zero on Cranelift verifier rejection. Currently it prints the
   error to stdout but proceeds; this masks miscompilation under
   `&& ./binary` chains.
2. **Codegen investigation (P1):** narrow which Cobrust codegen pass
   selects `i8` for an expression typed `: i64`. Hypothesis: a
   constant-folding pass observing `% 2` values bound to `{0,1}`
   over-eagerly narrows the result type before phi-merge. Locate via:
   - Search `crates/cobrust-codegen/src/cranelift_backend.rs` for
     `i8`, `Type::I8`, or any narrow-result branch.
   - Diff the IR dump (`cobrust build` already emits the IR before
     verifier failure) between 3-block (passes) and 4-block (fails)
     versions to localize.
3. **Corpus addition (P2, M11.1.1 agent's lane):** add an
   "N-similar-blocks" axis to the control-flow corpus, sweeping
   N ∈ {3, 4, 5, 8} × {with-while, straight-line} × {with-mod, no-mod}.
   This finding's straight-line 4-block example is the seed.
4. **Audit-1 caution:** the `tomli` real-LLM E2E translation (Audit
   #1, Task #35) is likely to produce LLM-generated code that
   replicates this pattern (parsers do this kind of multi-field
   conditional). If audit-1 fails L2.behavior with wrong output but
   no crash, this codegen bug is the first place to look — not the
   LLM's translation quality.

## Cross-references

- `m12-x-while-if-codegen-regression` — sibling bug, M11.1 closed it.
- `m11-1-1-control-flow-corpus` — M11.1.1 agent's domain; this
  finding extends its scope.
- `examples-literal-print-debt` — audit-#2 closure that revealed
  M12.x had to lower control flow first; this finding shows the
  lowering is incomplete past 3 blocks.
