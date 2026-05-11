# failure.md — LC-099 generate-parentheses

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/099-generate-parentheses/solution.cb -o /tmp/lc100-099
printf "2\n" | /tmp/lc100-099
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Suspected root cause

Same root cause as LC-093: `print_no_nl(literal_str)` is broken.

The solution stores the generated string as a list of i64 codes (0=open,
1=close), then at print time emits `print_no_nl("(")` or `print_no_nl(")")`.
These literal string arguments cause `__cobrust_print_no_nl` to receive a
misaligned `.rodata` pointer instead of a 8-byte-aligned `*mut StringBuffer`,
triggering the panic in `fmt.rs:194`.

Generating parentheses combinations requires printing multiple characters on
the same line without trailing newlines. This is only possible with `print_no_nl`
on literal characters, which is currently broken.

## Candidate fix tier

Same as LC-093: codegen gap.
Add `__cobrust_print_no_nl_lit(ptr, len)` C-ABI variant that accepts raw
`(*const u8, usize)` pairs like `__cobrust_println`, bypassing the
StringBuffer cast for literal strings.

## Notes

The compile succeeded; the failure is at runtime.
The recursive backtracking algorithm is correct; the gap is in the
print-without-newline intrinsic for literal strings.
