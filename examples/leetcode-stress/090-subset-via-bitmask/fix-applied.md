# fix-applied.md — LC-090 subset-via-bitmask

> **FIX APPLIED — LC-100 Tier A Sprint 2 (Pattern A `.rodata` literal misalignment closed via `__cobrust_print_no_nl_lit` C-ABI variant + operand-aware intrinsic-rewrite, ADR-0047 Option H). Test re-enabled; no source-level changes to `solution.cb`.**

## Status

RUNTIME-FAIL (stdlib gap)

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/090-subset-via-bitmask/solution.cb -o /tmp/lc100-090
printf "2\n1 2\n" | /tmp/lc100-090
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Root cause

Same as LC-069 and LC-072. Output format requires space-separated integers
within each subset line (e.g. `"1 2\n"` for subset {1, 2}). This requires
`print_no_nl` with digit-character string literals. Two or more `print_no_nl`
literal calls in the same program trigger a misaligned-pointer panic in the
`__cobrust_print_no_nl` runtime function.

The bitmask enumeration algorithm is correct; the failure is in output formatting.

## Candidate fix tier

codegen / stdlib gap — same fix as LC-069:
- Add `print_int_no_nl(n: i64)` intrinsic, OR
- Fix string-literal alignment in Cranelift codegen for multi-call `print_no_nl`.
