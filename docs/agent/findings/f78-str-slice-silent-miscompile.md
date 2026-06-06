---
finding_id: F78
title: str slicing silently miscompiles — "hello"[1:4] evaluates to the WHOLE string (a §2.2 silent-miscompile in a core op)
date: 2026-06-06
status: resolved
resolved_by: adr-0094
resolution_commit: "(dirty tree — CTO commits; mirrors the bytes Phase-2 slice machinery of ADR-0093 §2)"
severity: major
relates_to: [adr:0093, adr:0094, "claude.md:§2.2", "claude.md:§2.5", "finding:f37"]
discovered_by: the bytes Phase 2 (ADR-0093 §2) adversarial audit
---

# F78 — str slicing silently miscompiles to the whole string

## RESOLUTION (ADR-0094, 2026-06-06)

RESOLVED by **ADR-0094** — the `str` index OPERATOR runtime, mirroring
the ADR-0093 §2 `bytes` slice machinery but **codepoint-addressed** (the
load-bearing str-vs-bytes decision: Python `str[i]`/`str[i:j]` index by
Unicode scalar, NEVER splitting a multi-byte UTF-8 codepoint — so a slice
boundary always lands on a `char` boundary and the result is ALWAYS valid
UTF-8, no snap-or-trap needed). Verified vs CPython 3:

- `"hello"[1:4] == "ell"` (was `hello`) ✓
- `"hello"[1] == "e"` (the SIBLING scalar bug — `s[i]` was ALSO the whole
  string; fixed in the same MIR arm for consistency) ✓
- `len("hello"[1:4]) == 3` (was a use-of-moved compile error) ✓
- `"héllo"[1:3] == "él"` (the UTF-8 codepoint case) ✓
- `s[1:]` / `s[:3]` / `s[0:4:2]` / `s[1:-1]` REJECT at `cobrust check`
  via `TypeError::UnsupportedSliceShape` (the ADR-0093 `bytes` reject
  EXTENDED to `Ty::Str`, no new cascade — §2.5-A) ✓

Surface: `__cobrust_str_slice` + `__cobrust_str_char_at` (codepoint, mint-
fresh / borrow-base / drop-once), a `Ty::Str` MIR `Index` arm, a
`(Ty::Str, IndexKind::Slice)` check.rs arm, two codegen externs. The base
`str` is BORROWED (Move→Copy upgrade); the slice/scalar mints a fresh str
dropped once (1000-iter drop-hammer clean). See ADR-0094 for the full
codepoint rationale + the `UnsupportedSliceShape`-to-str extension.

---

## Original report (preserved)

## What (verified at HEAD 5248d8f)

A `str` slice expression silently evaluates to the **whole base string**,
not the slice — a §2.2 silent-miscompile in one of the most common ops.

```
# print("hello"[1:4])           -> prints  hello   (CPython: "ell")
# print("hello"[1:])            -> prints  hello   (CPython: "ello")
# print(len("hello"[1:4]))      -> compile error (use-of-moved-value / len-of-slice)
# let t: str = s[1:4]           -> compile error: use of moved value
```

So the contiguous `lo:hi` form **builds + runs but returns the wrong
value** (the whole string) in a value context (e.g. inside `print(...)`),
while other contexts hit an unrelated move/borrow compile error. Either
way str slicing is unusable + the value-context case is the dangerous one
(exit 0, no diagnostic, wrong answer).

## Why (root cause)

The generic `ExprKind::Index` / slice lowering falls through: a `Slice`
sub-expression collapses to the base operand (or `Constant::Int(0)`) with
**no `__cobrust_str_slice` runtime** — there is no str-slice shim at all.
This is the SAME generic-slice fall-through the bytes Phase 2 work
(ADR-0093 §2) just fixed for `bytes`: bytes now has `__cobrust_bytes_slice`
+ a dedicated MIR `Slice` arm + a `TypeError::UnsupportedSliceShape`
compile-time reject for the non-`lo:hi` shapes. `str` never got that
treatment, so its slice path is still the unsound fall-through. (bytes
`b"hello"[1:4]` correctly returns `b"ell"` now; str does NOT.)

## NOT introduced by bytes

This predates ADR-0093 and is orthogonal — bytes Phase 2 made the bytes
slice CORRECT; it merely surfaced the pre-existing str defect during the
adversarial audit. The bytes commit (5248d8f) explicitly notes it as
out-of-scope.

## Fix (the queued increment — mirror bytes Phase 2 for str)

1. **`__cobrust_str_slice(s, lo, hi) -> *mut str`** in
   `cobrust-stdlib/src/string.rs` — UTF-8-boundary-aware (a str slice must
   not split a multi-byte codepoint; either snap to char boundaries like
   Python's codepoint indexing, or trap on a mid-codepoint cut — decide in
   the ADR; bytes had no such concern).
2. **MIR `Slice` arm for `Ty::Str`** in `lower.rs` (beside the bytes-slice
   arm), base BORROWED, result a fresh str dropped once.
3. **Codegen extern** + **check.rs** `str[lo:hi] -> str` typing.
4. **Extend `TypeError::UnsupportedSliceShape`** to `Ty::Str` for the
   open-ended / stepped / negative shapes (the §2.5-A compile-time-catch),
   so no str slice shape silently miscompiles ever again.
5. Resolve the value-context move issue (`s[1:4]` as a value) — the same
   borrow/Move discipline bytes uses.

A regression e2e MUST include `"hello"[1:4] == "ell"` + `"héllo"[1:3]`
(the UTF-8-boundary case) + the unsupported-shape rejects + a CPython
differential oracle.

## Verification note

Reproduced independently (not just from the audit): two `/tmp` probes at
HEAD 5248d8f — `print("hello"[1:4])` prints `hello`; `print(len("hello"
[1:4]))` is a compile error. The bytes equivalent `b"hello"[1:4]` is
correct (len 3).
