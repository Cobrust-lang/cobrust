# fix-applied.md — LC-056 level-order-traversal

> **FIX APPLIED — LC-100 Tier A Sprint 2 (Pattern A `.rodata` literal misalignment closed via `__cobrust_print_no_nl_lit` C-ABI variant + operand-aware intrinsic-rewrite, ADR-0047 Option H). Test re-enabled; no source-level changes to `solution.cb`.**

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/056-level-order-traversal/solution.cb -o /tmp/lc100-056
printf "7\n1 1 2\n2 3 4\n3 5 6\n4 -1 -1\n5 -1 -1\n6 -1 -1\n7 -1 -1\n" | /tmp/lc100-056
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Suspected root cause

`print_no_nl(s: str)` panics at runtime when called with short string literals
(e.g. "0"–"9" single-char strings, or " " space). The `__cobrust_print_no_nl`
C-ABI shim dereferences the string pointer with 8-byte alignment requirement,
but short string literals in .rodata are not guaranteed 8-byte aligned.

This is the same misalignment bug documented in `024-hashmap-group-anagrams/failure.md`
for `str_at` on literal variables. The level-order traversal requires printing
integers and spaces on the same line without trailing newlines, which is only
achievable via `print_no_nl`. Without a working `print_no_nl`, the space-separated
level output format cannot be produced.

## Candidate fix tier

codegen / stdlib gap

Fix: Ensure string literals used in `print_no_nl` calls are 8-byte aligned in
the .rodata section. Alternatively, add a `print_int_no_nl(n: i64)` intrinsic
that avoids the string alignment issue entirely.

## Notes

The BFS algorithm itself is correct (verified by test 060-right-side-view which
uses a similar BFS without print_no_nl). The failure is entirely due to the
`print_no_nl` alignment bug.
