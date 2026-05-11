# failure.md — LC-093 integer-to-roman

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/093-integer-to-roman/solution.cb -o /tmp/lc100-093
printf "3\n" | /tmp/lc100-093
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Suspected root cause

`print_no_nl(literal_str)` is broken — when `print_no_nl` receives a
Constant::Str (string literal), codegen passes a raw `.rodata` byte pointer
to `__cobrust_print_no_nl`, which immediately casts it to `*const StringBuffer`.
The `.rodata` pointer is not aligned to 8 bytes (StringBuffer's alignment),
causing the misaligned-pointer panic in `fmt.rs:194`.

This is the same alignment bug as LC-024 (group anagrams) but for a different
call path. LC-024 used `str_at("literal", i)` → the result is unaligned.
LC-093 uses `print_no_nl("M")` directly → `print_no_nl` receives the literal
pointer directly as if it were a `*mut StringBuffer`.

`print(literal_str)` works because it goes through `__cobrust_println(ptr, len)`
with explicit `(ptr, len)` expansion — no `StringBuffer` cast needed.
`print_no_nl` uses `__cobrust_print_no_nl(*mut StringBuffer)` — requires a
heap-allocated, 8-byte-aligned StringBuffer, not a raw `.rodata` pointer.

Integer-to-Roman output requires printing multiple characters on the same line
without trailing newlines. This requires `print_no_nl` on literal char strings
(`"M"`, `"C"`, `"D"`, etc.), which is currently broken.

## Candidate fix tier

codegen / stdlib gap (same root as LC-024)

- Fix: Add a new code path in the intrinsic rewrite pass that, when
  `print_no_nl` receives a `Constant::Str`, allocates a heap StringBuffer,
  fills it with the literal bytes via `__cobrust_str_new` +
  `__cobrust_str_push_static`, and passes the aligned buffer pointer.
  Alternatively, add a `__cobrust_print_no_nl_lit(ptr, len)` C-ABI variant
  (analogous to `__cobrust_println(ptr, len)`) that accepts raw bytes.

## Notes

The compile succeeded; the failure is at runtime.
The algorithm is correct; the gap is `print_no_nl` on literal-derived strings.
