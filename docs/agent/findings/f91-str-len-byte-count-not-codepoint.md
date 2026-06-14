---
doc_kind: finding
finding_id: f91-str-len-byte-count-not-codepoint
last_verified_commit: TBD
discovered_by: §2.2 Python-compatibility correctness review (str codepoint arc)
severity: P1
related: f78-str-slice-silent-miscompile (ADR-0094 codepoint s[i]/s[lo:hi]), f79-scalar-negative-index-oob-trap, f88-str-for-codepoint-iteration (ADR-0101, introduced __cobrust_str_char_count), ADR-0080 pit StrLen refinement (already codepoint), ADR-0088 (len polymorphic dispatch)
status: closed_by_F91
---

# Finding: `len(<str>)` returned the BYTE count, not the codepoint count

## Hypothesis

`len(<str>)` returned the UTF-8 BYTE count instead of the Unicode CODEPOINT
count CPython returns. `len("é") == 2` (é = 0xC3 0xA9, 2 UTF-8 bytes) where
CPython `len("é") == 1`. A §2.2 Python-compatibility correctness divergence.

This was the LAST inconsistency in the str codepoint arc:

- F79 / ADR-0094 — `s[i]` and `s[lo:hi]` are codepoint-addressed.
- F88 / ADR-0101 — `for c in s:` iterates codepoint-by-codepoint (and added
  the `__cobrust_str_char_count` runtime fn for its loop bound). ADR-0101
  explicitly flagged this `len`-byte divergence as out of its scope.

So for ANY multi-byte string `len(s)` disagreed with BOTH the `for c in s:`
iteration count AND the valid `s[i]` index range — an internal inconsistency
on top of the CPython divergence. Worse, the ADR-0080 pit/coil STRING-length
refinement validator (`check_str_len`) had already chosen codepoints ("Python
`len()` ... codepoints, not bytes"), so the SOURCE `len` even disagreed with
the validator that advertises the same `len(self)` bound.

## Method

`Kind::LenPoly` (`crates/cobrust-cli/src/build/intrinsics.rs`) dispatched the
`Ty::Str` arm to `STR_LEN_RUNTIME_SYMBOL` = `__cobrust_str_len_src`, which
delegates to `__cobrust_str_len` (`fmt.rs`) = `s.len()` (Rust byte length).
The `str_len` PRELUDE shim (`Kind::StrLen`, the rewrite target of the
`s.len()` method form) routed to the same byte symbol. The codepoint runtime
fn `__cobrust_str_char_count` (`chars().count()`) already existed (F88) and
was already declared in codegen with the identical `(*mut Str) -> i64`
signature — but the `len` paths never used it.

## Result (resolution — F91 / ADR-0103)

A one-symbol redirect on each of the two str length paths:

- **intrinsics.rs `Kind::LenPoly` Str arm** → `STR_CHAR_COUNT_RUNTIME_SYMBOL`
  (`__cobrust_str_char_count`), the codepoint count.
- **intrinsics.rs `Kind::StrLen`** (the `str_len` shim / `s.len()` method
  target) → the same symbol, so the method form is codepoint-consistent for
  free and the MIR callee identity (`str_len`) the F30 witness pins is
  UNCHANGED.
- `len(bytes)` STAYS `__cobrust_bytes_len` (bytes ARE bytes). `len(list)` /
  `len(dict)` unchanged.

`len(s)` == the `for c in s:` iteration count == the valid `s[i]` index range
(`s[len(s)-1]` is the last codepoint), all `chars()`-based.

## Codepoint vs byte (load-bearing)

`"héllo"` = 6 UTF-8 bytes, 5 codepoints → `len == 5`. `"a🎉b"` (🎉 = 4 UTF-8
bytes, one codepoint) → `len == 3`. `"你好"` → `len == 2`. ASCII is unchanged
(byte == codepoint). The byte length stays reachable: `__cobrust_str_len_src`
+ its codegen extern are retained (io/file shims read it internally; a future
explicit byte-len builtin would bind it), and at the source level
`len(b"...")` / `s.encode()` give the byte count.

## pit StrLen refinement (ADR-0080) — decided + documented

The pit/coil StrLen refinement validator (`crates/cobrust-pit/src/validation.rs::check_str_len`)
was ALREADY codepoint (`s.chars().count()`, "Python `len()` / JSON Schema
minLength/maxLength semantics — codepoints, not bytes"). F91 brings the
SOURCE `len` into agreement with it (consistency), NOT the other way around —
no validator change. The `len(self)` bound a `where`-clause writes and the
`len(s)` a handler computes now mean the same thing.

## Regression guard (cross-file, F80/F83 lesson)

A len-dispatch change has workspace-wide blast radius. The ONLY corpus
assertion that asserted the old byte length over a multi-byte string:

- `crates/cobrust-cli/tests/str_mul_e2e.rs::str_mul_e2e_03` —
  `len("é" * 2)`: was `4` (byte), now `2` (codepoint). Corrected + doc
  updated to cite F91.

A grep of all `.cb`-source `len(`/`.len()`/`str_len(` over multi-byte string
literals across the corpus found no other byte-len assertion. ASCII tests
(the vast majority) are unaffected.

New corpus: `crates/cobrust-cli/tests/len_polymorphic_e2e.rs::len_e2e_07..13`
— single multi-byte codepoint, mixed ASCII+multibyte, emoji-is-one-codepoint,
CJK, the `len == for-c-in-s == s[i]-range` consistency triple, the `s.len()`
method form, and `len(bytes)` staying the byte count.

## §2.5 minor (opportunistic, deferred)

The `NotIterable` reject hint "use a list / dict / range / str — primitives
cannot iterate" over-promises in the comprehension / `in` context (str
iterates ONLY in a `for`-loop, F88). A context-split hint was prototyped but
deferred: the self-host `.cb` parity corpus
(`crates/cobrust-types-cb/tests/check_parity_corpus.rs::test_synth_comp_not_iterable_fail`)
pins the identical literal, so splitting only the Rust side would drift the
two checkers. The hint was refactored to a single `not_iterable_hint` binding
(harmless DRY) and the context-split is deferred to the change that teaches
the comprehension / `in` MIR paths to iterate `str`.
