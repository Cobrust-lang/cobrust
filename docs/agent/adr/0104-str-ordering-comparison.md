---
doc_kind: adr
adr_id: 0104
title: `str` ordering comparison (`<` `<=` `>` `>=`) — lexicographic
status: accepted
date: 2026-06-14
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0104: `str` ordering comparison (`<` `<=` `>` `>=`)

## Context

`"abc" < "abd"` (and `>`, `<=`, `>=`) CRASHED the `cobrust build` compiler
with a codegen panic (build exit 101). `==` / `!=` on `str` already WORKED
(ADR-0078 retargets them to `__cobrust_str_eq`); only the four ORDERING ops
crashed.

Root cause — the F85/F87/F92 codegen-panic class:

- The type checker ACCEPTS `str < str`. `synth_bin`'s comparison arm
  (`crates/cobrust-types/src/check.rs`, `BinOp::Eq | NotEq | Lt | LtEq | Gt
  | GtEq`) `unify(Str, Str)` succeeds and returns `Ty::Bool`, identically
  to the already-working `str == str`.
- So the program type-checks, then reaches codegen's `lower_binop`
  (`crates/cobrust-codegen/src/llvm_backend.rs`), whose `Lt/LtEq/Gt/GtEq`
  arms call `into_int_value()` / `into_float_value()` on the operands.
  A `str` is an OPAQUE POINTER, not an int/float, so inkwell panics with
  `expected the IntValue variant` — a raw ICE, NOT a Cobrust diagnostic.

This violates §5.1 ("the compiler must not panic on type-checked input")
and §2.5 (an LLM agent writes `s1 < s2` constantly — sorting, ordering,
binary search; Python performs lexicographic str comparison, so the §2.5
LLM-first fix is to IMPLEMENT it, not reject it). See finding
`f92-str-ordering-comparison-codegen-panic`.

## Options considered

1. **Reject `str < str` at type-check (§2.5-B fix-printing diagnostic).**
   Rejected: Python SUPPORTS lexicographic str comparison; rejecting it
   inverts §2.5 (the common, training-data-frequent spelling would fail).
   This is the posture `bytes` ordering still holds (ADR-0093 deferral),
   but `str` is the high-frequency case and warrants the real impl.
2. **Retarget the four ordering ops in MIR lowering to a runtime
   `__cobrust_str_cmp(a, b) -> i64` (sign of `a.cmp(b)`: -1/0/+1), then
   materialise the bool by comparing that i64 against 0 with the SAME
   ordering op.** A direct sibling of the ADR-0078 `str == str` retarget
   (call-then-compare shape). Codegen's existing INTEGER `Lt/LtEq/Gt/GtEq`
   arms then handle `cmp <OP> 0` with no new codegen comparison logic.
3. **Add a str comparison arm directly in codegen's `lower_binop`.**
   Rejected: the MIR retarget is where `str == str` already lives, keeps
   the borrow/drop discipline in one place, and reuses the integer
   comparison codegen unchanged.

## Decision

Option 2. F92 adds a `str` ordering arm in `lower_bin`
(`crates/cobrust-mir/src/lower.rs`), placed immediately BELOW the existing
`str == str` arm:

```
if matches!(op, Lt | LtEq | Gt | GtEq) && matches!(lhs_ty, Ty::Str) {
    // lhs/rhs lowered with upgrade_move_to_copy_handle (BORROW: __cobrust_str_cmp
    //   reads, does not consume — the source str locals survive + drop once)
    _strcmp: Ty::Int = call __cobrust_str_cmp(lhs, rhs)   // -1 / 0 / +1
    _strcmpb: Ty::Bool = BinaryOp(<same op>, _strcmp, Int(0))  // cmp <OP> 0
    return _strcmpb
}
```

The source ordering `a OP b` is exactly `cmp(a, b) OP 0`
(`a < b` ⇔ cmp < 0, `a >= b` ⇔ cmp >= 0, …), so the SAME `bin_to_mir(op)`
is reused against the integer constant 0 — handled by the existing integer
`lower_binop` arm (SLT/SLE/SGT/SGE).

Runtime: `__cobrust_str_cmp(a: *mut u8, b: *mut u8) -> i64`
(`crates/cobrust-stdlib/src/io.rs`, beside `__cobrust_str_eq`) returns the
i64 sign of Rust `a.cmp(b)` (`Ordering::{Less,Equal,Greater}` → -1/0/+1).
Null operands are treated as `""`. The codegen extern is declared beside
`__cobrust_str_eq` (`crates/cobrust-codegen/src/llvm_backend.rs`,
`(*mut Str, *mut Str) -> i64`).

### Codepoint vs byte order (the correctness note)

Python compares `str` lexicographically by CODEPOINT. Rust `str` `Ord` is
BYTE-lexicographic over the UTF-8 encoding. **UTF-8 is order-preserving**:
for any two valid UTF-8 strings, byte-lexicographic order equals
codepoint-lexicographic order (a defining property of the UTF-8 encoding —
higher codepoints always encode to byte sequences that sort after lower
ones). Every `str` in Cobrust is valid UTF-8 (the runtime `StringBuffer`
holds UTF-8, `str_buf_as_str_phase3` is `from_utf8`). Therefore
`a.cmp(b)` yields the SAME ordering as CPython, confirmed by the
`str_cmp_e2e_04` unicode case (`"é"`(U+00E9) > `"f"`(U+0066) by both byte
and codepoint order).

### `bytes` ordering — out of scope, REJECTS cleanly

`bytes < bytes` (and all `bytes` comparison ops) remain an ADR-0093
deferral: the comparison arm in `check.rs` REJECTS them at type-check with
a fix-printing diagnostic (exit 2), NOT a codegen panic. F92 confirms this
reject is clean (`str_cmp_e2e_09`) and leaves `bytes` for a future
`__cobrust_bytes_cmp` follow-up.

## Consequences

- **Positive**
  - CPython-faithful lexicographic `str` ordering: `"abc" < "abd"`,
    prefix-is-less (`"ab" < "abc"`), empty-is-minimum (`"" < "a"`), unicode
    codepoint order — all match CPython 3 (§2.2 + §2.5 win).
  - Eliminates a §5.1 compiler panic on type-checked input (exit 101 → a
    working program). Closes a high-frequency LLM first-try failure.
  - No new codegen comparison logic — reuses the integer `lower_binop`
    arm via `cmp OP 0`. Mirrors the ADR-0078 `str == str` retarget exactly,
    keeping str (in)equality + ordering co-located.
- **Negative / behavior change**
  - A previously-CRASHING program now COMPILES and RUNS. No prior corpus
    asserted the panic (it was a finding), so no assertion flips.
  - One runtime call per `str` ordering comparison (`__cobrust_str_cmp`);
    O(min(len)) byte scan via `str::cmp`. Same cost class as `str == str`.
- **Neutral / unknown**
  - `bytes` ordering is still unimplemented (clean reject). A future ADR
    can add `__cobrust_bytes_cmp` with the identical pattern.

## Evidence

- `crates/cobrust-mir/src/lower.rs` — `lower_bin` str ordering arm
  (`Lt|LtEq|Gt|GtEq` + `Ty::Str` → `__cobrust_str_cmp` then `cmp OP 0`),
  sibling of the `str == str` arm.
- `crates/cobrust-stdlib/src/io.rs` — `__cobrust_str_cmp` (sign of
  `a.cmp(b)`; codepoint/byte order-preservation note).
- `crates/cobrust-codegen/src/llvm_backend.rs` — `__cobrust_str_cmp`
  extern declaration (beside `__cobrust_str_eq`).
- `crates/cobrust-types/src/check.rs` — comparison arm UNCHANGED;
  `unify(Str, Str)` already accepts ordering (confirmed), mixed
  `str < int` still `unify`-rejects (`str_cmp_e2e_08`), `bytes` still
  fix-printing-rejects (`str_cmp_e2e_09`).
- `crates/cobrust-cli/tests/str_cmp_e2e.rs` — `str_cmp_e2e_01..09`
  (four ops, prefix/empty, equal inclusive-vs-strict, unicode codepoint,
  str variables in `if`, numeric-unchanged regression, `==`/`!=`-unchanged
  regression, mixed-reject, bytes-reject).
- Finding `docs/agent/findings/f92-str-ordering-comparison-codegen-panic.md`.
- Prior art: ADR-0078 (`str == str` / `str + str` retarget — the direct
  pattern sibling), ADR-0093 (`bytes` comparison deferral), ADR-0094 /
  ADR-0101 / ADR-0103 (the str codepoint arc this ordering joins).
