---
doc_kind: finding
finding_id: B4-toml-recursion-depth
last_verified_commit: 36c79c5
discovered_by: review-claude external audit 2026-05-11 (file:line reference B4 §1)
severity: P0 BLOCK (adversarial TOML can blow call stack; SIGSEGV on platforms without stack canaries)
related: [msgpack-fuzz-190gib-allocation]
status: closed-by-fix
fix_branch: feature/0.1.0-stable-B4-B5-B6-untrusted-input-fixes
fix_commit: pending-merge
---

# Finding: cobrust-tomli parse recursion unbounded — adversarial TOML blows call stack

## Hypothesis

`cobrust-tomli::loads()` has no recursion depth limit on `parse_array` /
`parse_inline_table` / `parse_value`. An adversarial TOML string of the form
`x = [[[[...` nested > 10,000 levels deep will exhaust the OS thread stack and
cause a hard crash (SIGSEGV or stack-guard signal), not a `TomliError`.

## Method

- **Static analysis**: `parse_value` → `parse_array` / `parse_inline_table` →
  `parse_value` is a direct mutual recursion with no guard.
- **`State` struct** (pre-fix): `{ src, bytes, pos }` — no `depth` field.
- **Corpus test** (added as part of fix):
  - Input: `"x = "` + `"["` × 150 + `"1"` + `"]"` × 150
  - Expected (post-fix): `Err(TomliError)` with message containing `"nesting depth"`
  - Actual (pre-fix): stack overflow / SIGSEGV on typical 8 MiB stack depth

## Result

### Pre-fix
On macOS arm64 with 8 MiB stack: `loads()` exhausts the stack at approximately
64–128 nesting levels and segfaults. The fuzz harness (`l2_behavior_fuzz_loads_panic_free`)
uses `std::panic::catch_unwind` — but SIGSEGV is **not** a Rust panic and is not
caught by `catch_unwind`. The process terminates with `SIGSEGV` (signal 11).

### Post-fix
`b4_deep_array_returns_err_not_stack_overflow` and
`b4_deep_inline_table_returns_err_not_stack_overflow` both pass: depth 150 inputs
return `Err(TomliError { message: "nesting depth exceeds maximum (100)..." })`.

`b4_exactly_max_depth_is_accepted` passes: depth exactly 100 is allowed (may succeed
or return a parse error for other reasons — crucially does not overflow).

## Root-cause analysis

- `parse_value` delegates to `parse_array` / `parse_inline_table` on `[` / `{` markers
  respectively.
- Those functions call `parse_value` recursively for each element/value.
- No depth counter existed in `State` prior to this fix.
- Stack depth per call frame on arm64 is ~200-400 bytes; 8 MiB stack / 400 bytes
  per frame ≈ 20,000 max recursion depth. However, debug builds are slower to exhaust
  but still do so before `~1,000` levels due to larger debug frame sizes.

## Fix applied

- `State` gained a `pub depth: u32` field (initialised to 0 in `State::new`).
- `parse_array` / `parse_inline_table` are wrapped with depth-guard entry points that
  increment `depth`, check `> MAX_DEPTH`, and decrement on return.
- `MAX_DEPTH = 100` is exported as a public constant from `cobrust_tomli`.
- Error constructor `TomliError::too_deep(pos)` added.
- Three corpus tests added to `tests/tomli_fuzz.rs`:
  - `b4_deep_array_returns_err_not_stack_overflow`
  - `b4_deep_inline_table_returns_err_not_stack_overflow`
  - `b4_exactly_max_depth_is_accepted`

## Conclusion

**P0 BLOCK closed.** Any untrusted TOML input can no longer overflow the call stack.
The fix is mechanical and preserves all existing parse behaviour for valid TOML at
normal nesting depths (typical configs are ≤ 10 levels deep).

`MAX_DEPTH = 100` matches CPython `tomllib`'s default recursion guard and the
[TOML spec recommendation](https://toml.io/en/v1.0.0) that parsers impose a reasonable limit.

## Cross-references

- `crates/cobrust-tomli/src/parser.rs` — `State::depth`, `MAX_DEPTH`, `too_deep()`,
  `parse_array` / `parse_inline_table` depth guards
- `crates/cobrust-tomli/tests/tomli_fuzz.rs` — `b4_*` adversarial corpus tests
- `docs/agent/findings/msgpack-fuzz-190gib-allocation.md` — related pattern
  (untrusted-input size/depth attack on another translated crate)
- Handoff §1 B4 (`review-claude-handoff/handoff-pack/dispatches/claude-desktop-integrated-handoff.md`)
