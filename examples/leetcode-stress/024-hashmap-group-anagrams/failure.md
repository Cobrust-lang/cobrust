# failure.md — LC-024 hashmap-group-anagrams

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/024-hashmap-group-anagrams/solution.cb -o /tmp/lc100-024
printf "6\neat\ntea\ntan\nate\nnat\nbat\n" | /tmp/lc100-024
```

## Raw stderr

```
thread '<unnamed>' (64896196) panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x1048fb141
thread caused non-unwinding panic. aborting.
```

## Suspected root cause

`str_at(s, i)` only works correctly when `s` is a string returned by `input("")`.
When `s` is a string variable assigned from a string literal (e.g.
`let alpha = "abcdefghijklmnopqrstuvwxyz"`) the returned `str` from `str_at` is
misaligned, causing a panic in `print_no_nl`.

Additionally, the solution requires storing M input words for later output, but
`list[i64]` is the only list type available — there is no `list[str]`. Without
the ability to store input strings in a list, words must be reconstructed from
stored character codes, which requires the `str_at`-on-literal path that is broken.

## Candidate fix tier

codegen / stdlib gap

- Fix 1: Make `str_at` work correctly on string literals (not just `input()`-returned strings).
  This would allow reconstructing words from stored ASCII codes via an alphabet literal.
- Fix 2 (preferred): Add `list[str]` support to Cobrust — needed for any algorithm that
  stores multiple input strings.

## Notes

The compile succeeded; the failure is at runtime.
The algorithm is correct; the gap is in `str_at` on literal-derived strings + absence of `list[str]`.
