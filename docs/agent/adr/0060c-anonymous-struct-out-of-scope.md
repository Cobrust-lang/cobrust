---
doc_kind: adr
adr_id: 0060c
parent_adr: 0060
title: "Phase M wave-3 — anonymous struct literal OUT-OF-SCOPE"
status: accepted
date: 2026-05-19
ratified_at: 2026-05-19
last_verified_commit: 2d84de5
supersedes: []
superseded_by: []
relates_to: [adr:0060, adr:0058a, adr:0006]
discovered_by: P10 Phase M sprint per ADR-0058a §15 gap #6
---

# ADR-0060c: Phase M wave-3 — anonymous struct literal OUT-OF-SCOPE

## 1. Context

ADR-0058a §15 gap #6 records the missing source-level spelling
`struct{i64, i64}` for anonymous structural types. The F36 fixture
rename in `codegen_diff_corpus.rs:517` (`llvm_type_09_tuple_two_i64`)
already rewrote the original "struct"-claiming fixture to the
**tuple** form — which is the same LLVM struct-type lowering path.

This ADR formally declares **anonymous struct literal will NOT be
added** to Cobrust source syntax. The §15 queue item #6 is closed by
this OOS memo, not by impl.

## 2. §2.5 LLM-first ROI

§2.5 §D (one-way-to-do-each-thing) — *negative ROI for adding*:

- `Ty::Tuple(items)` already provides positional anonymous product
  types. Lowers to LLVM `struct {...}`.
- `Ty::Record(fields)` already provides named anonymous product
  types. Lowers to LLVM `struct {...}` with named GEP.
- Adding `struct{T, U}` literal syntax would create **three** ways to
  spell anonymous products — violating CLAUDE.md §5.1.

§2.5 §B (training-data overlap) — *neutral*:

- LLM corpus: tuples are dominant for ad-hoc anonymous products.
- Rust uses `(T, U)` for the same. C/C++ uses `std::pair`.
- No measurable LLM-prior pressure for `struct{T, U}` literal form.

## 3. Decision

**Anonymous struct literal syntax `struct{T, U}` will not be added
to Cobrust.** Use one of:

- `(T, U)` tuple type for positional access (`t.0`, `t.1`).
- `Record { name: T, ... }` (or equivalently `class Foo: name: T`)
  for named-field access.

The F36 rename `llvm_type_09_tuple_two_i64` is permanent. The
ADR-0058a §15 queue marks gap #6 as **out-of-scope-closed**.

If a future ADR overturns this decision (e.g. a downstream library
translation needs C-struct interop literal sugar), it must:

1. Document why tuple + record are insufficient.
2. Cite at least one canonical Python or Rust corpus pattern that
   the LLM-prior expects.
3. Supersede this ADR explicitly.

## 4. Surface examples (current state — permanent)

```cobrust
# positional anonymous product:
fn point() -> (i64, i64):
    return (3, 4)

# named anonymous product (via class):
class Point:
    x: i64
    y: i64

# rejected (never added):
# fn point() -> struct{i64, i64}: ...   # ERROR
```

## 5. Acceptance

- ADR landed at status `proposed → accepted`.
- ADR-0058a §15 amendment cross-references this ADR for gap #6
  closure.
- No code change. No test change.

## 6. Anchors

- 0060c-F34: anonymous struct OOS canonical
- 0060c-F35: sibling 0060 frame
- 0060c-F36: `llvm_type_09_tuple_two_i64` rename permanent
- 0060c-F37: no `#[ignore]` involved

## 7. Cross-references

- ADR-0060 — Phase M frame
- ADR-0058a §15 #6 — gap queue item
- CLAUDE.md §5.1 — one-way-to-do-each-thing
- `cobrust-codegen::cranelift_backend.rs` Tuple lowering — the
  existing LLVM struct emit path
