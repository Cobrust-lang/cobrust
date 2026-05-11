# fix-applied.md — LC-072 find-first-last-position

> **FIX APPLIED — LC-100 Tier A Sprint 2 (Pattern A `.rodata` literal misalignment closed via `__cobrust_print_no_nl_lit` C-ABI variant + operand-aware intrinsic-rewrite, ADR-0047 Option H). Test re-enabled; no source-level changes to `solution.cb`.**

## Status

RUNTIME-FAIL (stdlib gap)

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/072-find-first-last-position/solution.cb -o /tmp/lc100-072
printf "6\n5 7 7 8 8 10\n8\n" | /tmp/lc100-072
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Root cause

Same as LC-069. Output format requires `"first last\n"` (two integers separated
by a space on one line). This requires `print_no_nl` with digit-character literals,
which crashes due to misaligned string literal addresses when multiple `print_no_nl`
calls are in the same program.

The binary-search algorithm (lower_bound + upper_bound) is correct; the failure
is purely in the output formatting.

## Candidate fix tier

codegen / stdlib gap — same fix as LC-069:
- Add `print_int_no_nl(n: i64)` intrinsic, OR
- Fix string-literal alignment in Cranelift codegen for `print_no_nl` multi-call.
