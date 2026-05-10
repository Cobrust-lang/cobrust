---
doc_kind: finding
finding_id: B6-msgpack-pos-overflow
last_verified_commit: 36c79c5
discovered_by: review-claude external audit 2026-05-11 (file:line references B6 parser.rs:423,437,658,675)
severity: P0 BLOCK (usize arithmetic overflow on 32-bit targets; potential OOB read)
related: [msgpack-fuzz-190gib-allocation]
status: closed-by-fix
fix_branch: feature/0.1.0-stable-B4-B5-B6-untrusted-input-fixes
fix_commit: pending-merge
---

# Finding: cobrust-msgpack `pos + length` usize arithmetic can overflow on 32-bit targets

## Hypothesis

Four unpack functions in `cobrust-msgpack/src/parser.rs` compute `pos + length`
(or `pos + n_bytes`) using plain `usize` addition without overflow checks. On a
32-bit target, where `usize = u32`, an adversarial msgpack payload with a large
`length` field (e.g., ARRAY_32 / MAP_32 / BIN_32 / STR_32 markers with 4-byte
`0xFFFFFFFF` lengths) can cause this addition to wrap around to a small value,
bypassing the subsequent `> data.len()` bounds check and potentially causing an
out-of-bounds read or incorrect slice bounds.

## Method

- **Static analysis**: `unpack_bin` (parser.rs:423), `unpack_float` (parser.rs:437),
  `unpack_str` (parser.rs:658), `unpack_uint` (parser.rs:675) — all contained
  `if pos + length > data.len()` without overflow guards.
- **32-bit overflow scenario** for `unpack_bin`:
  - `pos = 1` (after reading the BIN_32 marker byte)
  - `length = 0xFFFFFFFF` (max u32, from adversarial 4-byte length field)
  - On 32-bit: `1usize + 0xFFFFFFFFusize` = `0x0` (wraps to 0)
  - `0 > data.len()` = `0 > 5` = **false** — bounds check PASSES
  - `data[1..0]` is an empty slice (Rust panics on invalid range or returns empty,
    depending on slice implementation) — incorrect but not necessarily safe

- **Corpus tests** added:
  - Input: `[0xdd, 0xff, 0xff, 0xff, 0xff]` (ARRAY_32 + max-length)
  - Input: `[0xdf, 0xff, 0xff, 0xff, 0xff]` (MAP_32 + max-length)
  - Input: `[0xc6, 0xff, 0xff, 0xff, 0xff]` (BIN_32 + max-length)
  - Input: `[0xdb, 0xff, 0xff, 0xff, 0xff]` (STR_32 + max-length)

## Result

### Pre-fix (64-bit host, corpus tests)

On 64-bit hosts, `1usize + 0xFFFFFFFFusize` = `0x1_0000_0000` (no wrap).
The bounds check `0x1_0000_0000 > 5` = true → `Err(MsgError::unpack("truncated ..."))`.
The corpus tests pass pre-fix on 64-bit because the 64-bit path happens to be safe.

### Pre-fix (32-bit target)

On `i686-unknown-linux-gnu` (Cobrust's 32-bit CI matrix target per L4 finding):
`1u32 + 0xFFFFFFFFu32` wraps to `0`. The bounds check `0 > 5` = false → proceeds to
`data[1..0]`. Rust's slice indexing panics on a reversed range: **panic in `unpack_bin`**,
violating the `MsgError` contract.

### Post-fix

All four B6 corpus tests return `Err` (either `OverflowSize` on 32-bit or `Unpack` on
64-bit). The `MsgErrorKind::OverflowSize` variant surfaces cleanly with correct Display.

## Root-cause analysis

The `unpack_*` functions receive `length` as `usize` (cast from `u32` via `as usize`
in the calling `unpack_one`). On 64-bit, this cast is lossless and the subsequent
`pos + length` is safe. On 32-bit, `u32::MAX as usize = u32::MAX` (same bit width),
and `pos + length` overflows `usize` silently in release mode (undefined in debug via
overflow check abort).

The prior fix for `msgpack-fuzz-190gib-allocation` added `Vec` prealloc caps in
`unpack_array` / `unpack_map` but did not address the arithmetic overflow in the
lower-level `unpack_bin` / `unpack_float` / `unpack_str` / `unpack_uint` primitives.

## Fix applied

All four vulnerable functions now use `checked_add`:

```rust
let end = pos.checked_add(length).ok_or_else(MsgError::overflow_size)?;
if end > data.len() {
    return Err(MsgError::unpack("truncated ..."));
}
```

- `MsgErrorKind::OverflowSize` variant added to `MsgErrorKind`.
- `MsgError::overflow_size()` constructor added.
- `Display` for `OverflowSize` → `"overflow size"`.
- Four B6 corpus tests in `tests/msgpack_fuzz.rs`:
  - `b6_array32_adversarial_length_returns_err`
  - `b6_map32_adversarial_length_returns_err`
  - `b6_bin32_adversarial_length_returns_err`
  - `b6_str32_adversarial_length_returns_err`
  - `b6_overflow_size_error_display`

## Conclusion

**P0 BLOCK closed.** The `pos + length` overflow path is now guarded by `checked_add`
in all four primitive unpack functions. On 32-bit targets (and on 64-bit with crafted
near-`usize::MAX` values), adversarial inputs return `Err(MsgError)` instead of
panicking, corrupting the slice, or producing incorrect results.

The handoff (§1 B6) specifically called out ARRAY_32 / MAP_32 / BIN_32 / STR_32
and FIXEXT / EXT_8/16/32 paths. FIXEXT and EXT_* markers are outside the M6 scope
window (the crate returns `Err(MsgError::unpack("unknown marker ..."))` for those
markers) so they do not reach the arithmetic paths and are not affected.

## Cross-references

- `crates/cobrust-msgpack/src/parser.rs`:
  - `unpack_bin` — `checked_add` for BIN length
  - `unpack_float` — `checked_add` for float n_bytes
  - `unpack_str` — `checked_add` for STR length
  - `unpack_uint` — `checked_add` for uint n_bytes
  - `MsgErrorKind::OverflowSize`, `MsgError::overflow_size()`
- `crates/cobrust-msgpack/tests/msgpack_fuzz.rs` — `b6_*` adversarial corpus tests
- `docs/agent/findings/msgpack-fuzz-190gib-allocation.md` — predecessor finding
  (OOM via large `Vec::with_capacity`; different attack surface, same crate)
- Handoff §1 B6 (`review-claude-handoff/handoff-pack/dispatches/claude-desktop-integrated-handoff.md`)
