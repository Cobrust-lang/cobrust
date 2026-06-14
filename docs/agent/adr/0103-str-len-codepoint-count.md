---
doc_kind: adr
adr_id: 0103
title: `len(<str>)` returns the codepoint count
status: accepted
date: 2026-06-14
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0103: `len(<str>)` returns the codepoint count

## Context

`len(<str>)` returned the UTF-8 BYTE count, not the Unicode codepoint count
that CPython `len(str)` returns. `len("é") == 2` (é is 2 UTF-8 bytes) where
CPython gives `1`. This was the LAST hold-out of the str codepoint arc:

- F79 / ADR-0094 made `s[i]` and `s[lo:hi]` codepoint-addressed.
- F88 / ADR-0101 made `for c in s:` iterate codepoint-by-codepoint and even
  introduced the `__cobrust_str_char_count` (`chars().count()`) runtime fn for
  its loop bound. ADR-0101 §Consequences explicitly flagged the `len`-byte
  divergence as "separate pre-existing ... Out of scope here."

So for ANY multi-byte string, `len(s)` disagreed with both the `for c in s:`
iteration count and the valid `s[i]` index range — an internal inconsistency
on top of the CPython divergence. The ADR-0080 pit/coil STRING-length
refinement validator (`crates/cobrust-pit/src/validation.rs::check_str_len`)
had ALREADY chosen the codepoint count (`s.chars().count()`, "Python `len()` /
JSON Schema minLength/maxLength semantics — codepoints, not bytes"), so the
SOURCE `len` was also inconsistent with the validator that advertises the same
`len(self)` bound. §2.2 (Python-compatibility correctness) + §2.5
(Maximize-overlap-with-training-data: the LLM expects CPython `len`). See
finding `f91-str-len-byte-count-not-codepoint`.

## Options considered

1. **A new explicit byte-len builtin, keep `len(str)` = bytes.** Rejected:
   inverts the §2.5 default — the common spelling `len(s)` would keep the
   surprising semantics; bytes are the rare case.
2. **Redirect the `len(str)` dispatch (`Kind::LenPoly` Str arm) AND the
   `str_len` PRELUDE shim (`Kind::StrLen`, the target of the `s.len()` method
   form) from `__cobrust_str_len_src` (byte) to `__cobrust_str_char_count`
   (codepoint).** The codepoint runtime fn already exists (F88) and is already
   declared in codegen with the identical `(*mut Str) -> i64` signature, so
   this is a one-symbol redirect on each of the two str length paths — no new
   runtime fn, no new extern, no signature change. `len(bytes)` keeps
   `__cobrust_bytes_len` (bytes ARE bytes). `len(list)` / `len(dict)`
   unchanged.
3. **Desugar / recompute byte length where needed.** Over-scoped; no consumer
   asked for the byte count at the `str` source surface.

## Decision

Option 2. Both str length surfaces become the Python-canonical CODEPOINT
count:

- The free `len(<str>)` builtin — `Kind::LenPoly`, `Ty::Str` arm — emits
  `__cobrust_str_char_count` (was `__cobrust_str_len_src`).
- The `str_len` PRELUDE shim — `Kind::StrLen` — emits the same symbol. The
  `s.len()` METHOD form rewrites to `str_len`
  (`method_form_rewrite_name`, `crates/cobrust-mir/src/lower.rs`), so the
  method form is codepoint-consistent for free, and the MIR callee identity
  (`str_len`) the F30 witness pins is UNCHANGED.

Result: `len(s)` == the number of `for c in s:` iterations == the valid `s[i]`
index range (`s[len(s)-1]` is the last codepoint), all three using
`chars()`-based primitives. `len(bytes)` STAYS the byte count.

The `STR_LEN_RUNTIME_SYMBOL` constant + `__cobrust_str_len_src` runtime fn +
its codegen extern are RETAINED (no source `len`/`str_len` path emits it now;
the io/file shims still read the byte primitive internally, and a future
explicit byte-len builtin would bind to it). The constant carries
`#[allow(dead_code)]` with a rationale doc.

The pit/coil StrLen refinement (ADR-0080) is ALREADY codepoint — this ADR
brings the source `len` into agreement with it, NOT the other way around. No
change to the validator.

## Consequences

- **Positive**
  - CPython-faithful: `len("é") == 1`, `len("héllo") == 5`, `len("a🎉b") == 3`,
    `len("你好") == 2` (§2.2 + §2.5 win).
  - Closes the str codepoint arc: `len`, `s[i]`, `s[lo:hi]`, `for c in s:` all
    agree codepoint-for-codepoint. No more `len(s)` ≠ iteration/index count.
  - The source `len` now matches the ADR-0080 refinement validator's
    `len(self)` semantics (one consistent notion of string length).
  - One-symbol redirect on two paths; no new runtime/extern/signature.
- **Negative / behavior change**
  - This is a BEHAVIOR CHANGE for multi-byte strings. ASCII is UNAFFECTED
    (byte == codepoint). The only corpus assertion that changed:
    `str_mul_e2e_03` (`len("é" * 2)`: 4 → 2), corrected in this change.
  - The free `str_len(s)` shim (internal ADR-0044 W2 helper) now also returns
    codepoints, not bytes. No corpus asserts a multi-byte `str_len`; the
    byte-length primitive stays reachable as `__cobrust_str_len_src` (and via
    the `len(bytes)` / `s.encode()` path at the source level).
- **Neutral / unknown**
  - O(n) codepoint scan per `len(str)` (`chars().count()`) vs the O(1)-ish
    byte `len`. Acceptable for the common short-string case; a cached
    codepoint count is future work, same trade-off F88 already accepted for
    the loop bound.

## Evidence

- `crates/cobrust-cli/src/build/intrinsics.rs` — `Kind::LenPoly` Str arm +
  `Kind::StrLen` now emit `STR_CHAR_COUNT_RUNTIME_SYMBOL`
  (`__cobrust_str_char_count`); `STR_LEN_RUNTIME_SYMBOL` retained
  `#[allow(dead_code)]`.
- `crates/cobrust-stdlib/src/string.rs` — `__cobrust_str_char_count` doc +
  the Rust-side `len` doc clarify byte-vs-codepoint split.
- `crates/cobrust-codegen/src/llvm_backend.rs` — `__cobrust_str_char_count`
  extern (pre-existing, F88).
- `crates/cobrust-cli/tests/len_polymorphic_e2e.rs` — `len_e2e_07..13`
  (multi-byte, emoji, CJK, consistency triple, method form, bytes-stays-byte).
- `crates/cobrust-cli/tests/str_mul_e2e.rs` — `str_mul_e2e_03` corrected
  (codepoint, not byte).
- `crates/cobrust-pit/src/validation.rs::check_str_len` — already codepoint;
  unchanged, now in agreement.
- Finding `docs/agent/findings/f91-str-len-byte-count-not-codepoint.md`.
- Prior art: ADR-0094 (F78/F79 codepoint `s[i]`), ADR-0101 (F88 `for c in s:`,
  which flagged this divergence as out-of-scope and is now closed here).
