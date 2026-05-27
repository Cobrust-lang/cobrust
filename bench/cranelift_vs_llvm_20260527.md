# Cranelift vs LLVM benchmark — Phase X.1

ADR-0070 §X.3 input: empirical baseline before flipping LLVM-default.

## Methodology

- Both `cobrust` binaries built once at the same workspace HEAD.
- Cranelift binary: `target-cranelift/release/cobrust` (default backend).
- LLVM binary: `target-llvm/release/cobrust` (built with `--features cobrust-codegen/llvm`).
- Per program: `cobrust build <file> -o <out> --release` → run `<out>` → diff stdout.
- Times in milliseconds (wall clock, single sample per program — small-N indicative, not statistically significant).
- Sizes in KB (1 KB = 1024 B).
- Stdout parity per F50: byte-identical via `cmp`.
- F35-sibling: numbers are measured wall time, not extrapolated.

## Corpus

- Total programs: 25
- `examples/leetcode/`: 10 (LC-100 subset)
- `examples/`: 15

## Aggregate stats

| Metric | Cranelift | LLVM | LLVM delta |
|---|---|---|---|
| Mean compile (ms) | 100 | 110 | +10.0% |
| Mean runtime (ms) | 36 | 35 | -2.8% |
| Mean size (KB) | 7766.3 | 7766.5 | +0.0% |

## Failure counts

- LLVM compile failures: 0 / 25
- LLVM runtime failures: 0 / 25
- Stdout parity divergences: 0 / 25

## Per-program results

| Program | C compile (ms) | L compile (ms) | C run (ms) | L run (ms) | C size (KB) | L size (KB) | Parity | Status |
|---|---|---|---|---|---|---|---|---|
| leetcode/binary_search | 266 | 263 | 28 | 28 | 7766.3 | 7766.5 | OK | ok |
| leetcode/climbing_stairs | 86 | 98 | 27 | 28 | 7766.3 | 7766.5 | OK | ok |
| leetcode/fibonacci | 88 | 100 | 27 | 48 | 7766.3 | 7766.5 | OK | ok |
| leetcode/maximum_subarray | 95 | 101 | 28 | 30 | 7766.3 | 7766.5 | OK | ok |
| leetcode/merge_two_sorted_lists | 87 | 103 | 28 | 27 | 7766.3 | 7766.5 | OK | ok |
| leetcode/reverse_string | 130 | 108 | 33 | 29 | 7766.3 | 7766.5 | OK | ok |
| leetcode/roman_to_integer | 96 | 110 | 30 | 28 | 7766.3 | 7766.6 | OK | ok |
| leetcode/stock_best_time | 88 | 100 | 27 | 27 | 7766.3 | 7766.5 | OK | ok |
| leetcode/two_sum | 88 | 111 | 29 | 28 | 7766.3 | 7766.5 | OK | ok |
| leetcode/valid_parentheses | 95 | 105 | 28 | 28 | 7766.4 | 7766.5 | OK | ok |
| examples/bench_array_sum | 90 | 100 | 226 | 169 | 7766.3 | 7766.5 | OK | ok |
| examples/cat | 92 | 104 | 30 | 28 | 7766.3 | 7766.5 | OK | ok |
| examples/csv_sum | 86 | 135 | 27 | 27 | 7766.3 | 7766.5 | OK | ok |
| examples/early_exit | 96 | 106 | 28 | 29 | 7766.3 | 7766.5 | OK | ok |
| examples/echo | 89 | 100 | 29 | 30 | 7766.3 | 7766.5 | OK | ok |
| examples/fib | 95 | 110 | 27 | 44 | 7766.3 | 7766.6 | OK | ok |
| examples/fizzbuzz | 89 | 99 | 48 | 27 | 7766.4 | 7766.5 | OK | ok |
| examples/for_list | 86 | 103 | 29 | 28 | 7766.3 | 7766.5 | OK | ok |
| examples/for_range | 86 | 98 | 28 | 27 | 7766.3 | 7766.5 | OK | ok |
| examples/hello | 132 | 102 | 27 | 27 | 7766.3 | 7766.5 | OK | ok |
| examples/json_pretty | 88 | 97 | 27 | 27 | 7766.4 | 7766.5 | OK | ok |
| examples/regex_grep | 86 | 101 | 27 | 27 | 7766.4 | 7766.5 | OK | ok |
| examples/sort | 88 | 96 | 27 | 27 | 7766.3 | 7766.5 | OK | ok |
| examples/unique_lines | 91 | 104 | 27 | 27 | 7766.4 | 7766.5 | OK | ok |
| examples/wc | 87 | 98 | 28 | 30 | 7766.4 | 7766.5 | OK | ok |

## Top 5 LLVM-faster at runtime (`(L_run - C_run)` most negative)

| Program | C run (ms) | L run (ms) | Delta (ms) |
|---|---|---|---|
| examples/bench_array_sum | 226 | 169 | -57 |
| examples/fizzbuzz | 48 | 27 | -21 |
| leetcode/reverse_string | 33 | 29 | -4 |
| examples/cat | 30 | 28 | -2 |
| leetcode/roman_to_integer | 30 | 28 | -2 |

## Top 5 LLVM-slower at runtime (`(L_run - C_run)` most positive)

| Program | C run (ms) | L run (ms) | Delta (ms) |
|---|---|---|---|
| leetcode/fibonacci | 27 | 48 | 21 |
| examples/fib | 27 | 44 | 17 |
| examples/wc | 28 | 30 | 2 |
| leetcode/maximum_subarray | 28 | 30 | 2 |
| examples/early_exit | 28 | 29 | 1 |

## Interpretation guidance for ADR-0070 §X.3

- **GREEN** (flip default to LLVM): zero LLVM compile-fail + zero parity divergence + LLVM runtime not materially worse (≥ -10% on small programs is noise).
- **YELLOW**: non-zero LLVM failures but workaroundable; LLVM-default OK behind opt-out flag.
- **RED**: parity divergence or systemic LLVM crash → do NOT flip; investigate per F45a.

## Caveats (F35-sibling discipline)

- Single sample per program; no variance estimate. For statistically rigorous numbers a multi-run / hyperfine pass is required (deferred — out of scope for this baseline).
- All programs are tiny (≤ 30 LOC); LLVM optimization headroom is limited. Larger programs (LC-100 expansion / numerical kernels) will show clearer separation.
- Compile-time includes Rust toolchain stdlib link, dominated by linker work. The `cobrust`-internal compile fraction is small.
- Wall-time only; no `cpu-time` / `max-rss` collected.
- Stdout parity uses canonical-path argv[0] (symlink trick) so the bench harness does not falsely register divergence purely from `C_<name>` vs `L_<name>` exe path differences. Backend-level argv[0] semantics are byte-identical.

## Phase X.1 verdict

- Recommendation: **GREEN**
- Rationale (measured):
  - LLVM compile failures: 0 / 25
  - LLVM runtime failures: 0 / 25
  - Stdout parity divergences: 0 / 25
  - Mean compile delta: +10.0%
  - Mean runtime delta: -2.8%
  - Mean size delta:    +0.0%

This file is the empirical input to ADR-0070 §X.3 (LLVM-default flip decision).

