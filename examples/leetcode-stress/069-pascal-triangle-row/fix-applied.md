# fix-applied.md — LC-069 pascal-triangle-row

> **FIX APPLIED — LC-100 Tier A Sprint 2 (Pattern A `.rodata` literal misalignment closed via `__cobrust_print_no_nl_lit` C-ABI variant + operand-aware intrinsic-rewrite, ADR-0047 Option H). Test re-enabled; no source-level changes to `solution.cb`.**

## Status

RUNTIME-FAIL (stdlib gap)

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/069-pascal-triangle-row/solution.cb -o /tmp/lc100-069
printf "3\n" | /tmp/lc100-069
```

## Raw stderr

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8 but is 0x...
thread caused non-unwinding panic. aborting.
```

## Root cause

The output format requires space-separated integers on one line (e.g. `"1 3 3 1\n"`),
which requires printing integers without a trailing newline. The only available
mechanism is `print_no_nl(s: str)`, but calling `print_no_nl` with a string literal
(e.g. `print_no_nl("0")`, `print_no_nl("3")`) causes a misaligned-pointer panic in
`cobrust-stdlib/src/fmt.rs:194` when the program contains two or more such calls.

Root cause: string literal values embedded in programs with multiple `print_no_nl`
calls are placed at misaligned stack/data addresses by the current Cranelift codegen.
The `__cobrust_print_no_nl` runtime function requires 8-byte aligned `Str` structs,
but the second (and subsequent) string literal is not aligned to 8 bytes.

`print_int(n)` always emits a trailing newline and cannot be used for inline integers.

## Candidate fix tier

codegen / stdlib gap

- Fix: Ensure string literals passed to `print_no_nl` are aligned to 8 bytes in
  Cranelift codegen (same fix needed for all multi-literal `print_no_nl` uses).
  Alternatively: add a `print_int_no_nl(n: i64)` intrinsic that formats an integer
  and writes it without a trailing newline, avoiding string literals entirely.
