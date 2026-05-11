# fix-applied.md — LC-100 subsets-recursive

> **FIX APPLIED — LC-100 Tier A Sprint 2 (Pattern A `.rodata` literal misalignment closed via `__cobrust_print_no_nl_lit` C-ABI variant + operand-aware intrinsic-rewrite, ADR-0047 Option H). Test re-enabled; no source-level changes to `solution.cb`.**

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/100-subsets-recursive/solution.cb -o /tmp/lc100-100
printf "2\n1 2\n" | /tmp/lc100-100
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Suspected root cause

Same root cause as LC-093 and LC-099: `print_no_nl(literal_str)` is broken.

The subset output format requires space-separated integers on each line.
To print integers without trailing newlines, the solution uses a user-defined
`print_int_no_nl` helper that calls `print_no_nl("0")` through `print_no_nl("9")`
for each digit, and `print_no_nl(" ")` for the space separator.

All of these `print_no_nl(literal)` calls hit the misaligned pointer bug:
`__cobrust_print_no_nl` receives a raw `.rodata` pointer and casts it to
`*const StringBuffer` (alignment 8), causing the panic.

## Candidate fix tier

Same as LC-093: codegen gap.
Add `__cobrust_print_no_nl_lit(ptr, len)` C-ABI variant.

Alternative workaround (no compiler change): if subset elements were always
single-digit, one could store digit chars in a list of str from input — but
elements can be multi-digit, and there's no multi-char non-newline print path.

## Notes

The compile succeeded; the failure is at runtime.
The DFS include/skip backtracking algorithm is correct; the gap is
`print_no_nl` on literal strings.
