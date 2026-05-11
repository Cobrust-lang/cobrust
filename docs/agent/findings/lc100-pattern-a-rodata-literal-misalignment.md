---
doc_kind: finding
finding_id: lc100-pattern-a-rodata-literal-misalignment
last_verified_commit: e91caed
dependencies: [adr:0047, adr:0044]
related: [examples-literal-print-debt, lc100-pattern-b-list-of-str-gap, lc100-pattern-c-test-corpus-defects]
discovered_by: lc-100-tier-a-stress-sweep
---

# Finding: LC-100 Pattern A — `.rodata` literal-pointer misalignment in `print_no_nl` / `str_at` family

## Hypothesis

ADR-0047 Phase 2 dispatched 4 P7 sonnet TDD pairs against a 100-program
LeetCode stress corpus. The corpus exercises Cobrust's source-level
stdin/argv surface (ADR-0044) across 10 algorithm categories. The
hypothesis: some failures will cluster on a single shared codegen /
stdlib defect rather than 23 independent gaps. This finding aggregates
those clustered failures into one diagnosis.

## Method

- Read all 23 `examples/leetcode-stress/<NNN>-<slug>/failure.md` files
  on `feature/lc100-stress-sweep` at HEAD `e91caed`.
- Independent grep for the panic signature
  `misaligned pointer dereference` across the failure.md corpus.
- Cross-referenced the panic site against `crates/cobrust-stdlib/src/fmt.rs`
  line 194 and traced the C-ABI entry point.

## Result

### Affected programs (8 of 23 failures)

```
examples/leetcode-stress/024-hashmap-group-anagrams/failure.md
examples/leetcode-stress/056-level-order-traversal/failure.md
examples/leetcode-stress/069-pascal-triangle-row/failure.md
examples/leetcode-stress/072-find-first-last-position/failure.md
examples/leetcode-stress/090-subset-via-bitmask/failure.md
examples/leetcode-stress/093-integer-to-roman/failure.md
examples/leetcode-stress/099-generate-parentheses/failure.md
examples/leetcode-stress/100-subsets-recursive/failure.md
```

Bucket distribution: B1 = 1 (024), B2 = 1 (056), B3 = 4 (069, 072,
090, 099), B4 = 2 (093, 100). Touches all 4 buckets, but concentrated
in B3 where bit-manipulation + binary-search outputs demand
inline-formatted (no-trailing-newline) prints.

### Common panic signature (verbatim from 8 failure.md)

```
thread '<unnamed>' panicked at crates/cobrust-stdlib/src/fmt.rs:194:22:
misaligned pointer dereference: address must be a multiple of 0x8
but is 0x<runtime-pointer>
thread caused non-unwinding panic. aborting.
```

### Root cause (verified by source trace at HEAD `e91caed`)

The `__cobrust_print_no_nl(buf: *mut u8)` C-ABI entry at
`crates/cobrust-stdlib/src/io.rs:594` calls
`str_buf_as_str_phase3(buf)` at line 599, which calls
`crate::fmt::__cobrust_str_len(buf)` at io.rs:535. That function at
`crates/cobrust-stdlib/src/fmt.rs:194` executes:

```rust
let b = unsafe { &*buf.cast::<StringBuffer>() };
```

`StringBuffer` is 8-byte aligned. When `buf` is a raw `.rodata`
byte pointer produced by Cranelift codegen for a `Constant::Str`
literal (e.g., the literal `"M"` or `" "`), the pointer carries the
alignment of its byte payload (1) rather than the alignment required
by the cast target (8). The cast `buf.cast::<StringBuffer>()` then
dereferences a misaligned pointer, and the Rust 1.78+ UB-detection
panic fires.

Two distinct source-code call paths surface the same defect:

1. **`print_no_nl("literal")`** — Cobrust source passes a string
   literal directly to `print_no_nl`. The Cranelift backend lowers
   the literal to a `.rodata` byte pointer and passes that pointer
   as `*mut u8` to `__cobrust_print_no_nl`. Programs 056, 069, 072,
   090, 093, 099, 100 hit this path.

2. **`str_at(literal_var, i)`** — Cobrust source assigns a string
   literal to a variable (`let alpha = "abc..."`) and then calls
   `str_at(alpha, i)` to extract a character. The returned `str`
   inherits the literal's `.rodata` alignment. Program 024 hits
   this path (and additionally hits a structural gap — see
   `lc100-pattern-b-list-of-str-gap.md`).

The defect is **codegen-level**: the literal-to-runtime-string
boundary does not allocate a 8-byte-aligned StringBuffer for
literals, instead passing the raw byte pointer as if it were one.

### Why `print("literal")` works but `print_no_nl("literal")` panics

`print(s: str)` lowers to `__cobrust_println(ptr: *const u8, len: usize)`
— the codegen expands the literal to its `(ptr, len)` pair at the
call site. No `StringBuffer` cast happens. The literal's natural
alignment is sufficient for raw byte access.

`print_no_nl(s: str)` lowers to `__cobrust_print_no_nl(buf: *mut u8)`
— the runtime treats `buf` as `*mut StringBuffer`. Codegen does not
heap-allocate a StringBuffer for literals, so it passes the raw
`.rodata` pointer, triggering the misalignment.

This asymmetry is the actionable defect: either the codegen must
allocate a StringBuffer for literals at the `print_no_nl` call site,
or the runtime must expose a `__cobrust_print_no_nl_lit(ptr, len)`
variant analogous to `__cobrust_println`.

## Conclusion — actionable fix candidates

Three remediation candidates ordered by estimated complexity:

### Candidate F1 (preferred) — add raw-bytes runtime variant

Add `__cobrust_print_no_nl_lit(ptr: *const u8, len: usize)` to
`crates/cobrust-stdlib/src/io.rs` (~10 LOC), exactly analogous to
`__cobrust_println`. Cranelift backend lowering for
`print_no_nl(Constant::Str)` routes to the new C-ABI; lowering for
`print_no_nl(<runtime str>)` continues to use the StringBuffer entry.

- Estimated effort: 2-4 hr (1 sonnet sprint)
- Touches: `crates/cobrust-stdlib/src/io.rs`,
  `crates/cobrust-codegen/src/cranelift_backend.rs` (intrinsic
  dispatch + signature decl)
- Risk: low — additive, no semantic change to existing path

### Candidate F2 (fallback) — codegen heap-allocates StringBuffer

At the call site of `print_no_nl(Constant::Str)`, Cranelift codegen
emits `__cobrust_str_new` + `__cobrust_str_push_static(literal_ptr,
literal_len)` and then passes the resulting StringBuffer pointer to
`__cobrust_print_no_nl`. Single allocation per literal-print call.

- Estimated effort: 4-6 hr (1 sonnet sprint)
- Touches: `crates/cobrust-codegen/src/cranelift_backend.rs` only
- Risk: low-medium — heap allocation in a hot path; mitigated by
  small literal sizes typical in LeetCode programs

### Candidate F3 (nuclear) — add `print_int_no_nl(n: i64)` intrinsic

Skip the string-formatting layer entirely. Add a new runtime helper
`__cobrust_print_int_no_nl(n: i64)` and source-level wrapper
`print_int_no_nl(n: i64)`. Programs that need to print
space-separated integers (069, 072, 090, 099, 100) use this directly
instead of `print_no_nl(str(n))`-style construction.

- Estimated effort: 1-2 hr (1 sonnet sprint)
- Touches: `crates/cobrust-stdlib/src/io.rs` (+ helper),
  `crates/cobrust-codegen/src/cranelift_backend.rs` (signature decl),
  ADR-0044 prelude expansion
- Risk: low — additive; does not subsume Pattern A's deeper gap
  but eliminates it for the LC-100 affected programs by source-level
  rewrite
- Caveat: 024 and 093 still need Pattern A fix because they format
  alpha chars / Roman digits, not bare integers. Pattern A repair
  is the more general fix.

### Recommendation

**Land F1 first** as the principled fix (eliminates the defect at the
boundary); **add F3 in the same sprint** as an ergonomic helper for
the common case of integer formatting (avoids needing `str(n)` +
`print_no_nl(s)` indirection in user programs).

Estimated combined fix-pack: 4-6 hr (1 P7 sonnet sprint). Closes 8
LC-100 failures + closes Pattern A as a class.

### Pattern A's contribution to LC-100 pass rate

With Pattern A fixed:
- 8 failures → 0 failures (programs 056, 069, 072, 090, 093, 099,
  100 turn green)
- Program 024 partial — still blocked by Pattern B (list[str] gap)
  even with Pattern A fix
- Pass rate moves from 77/100 → 84/100 (8 - 1 partial = 7 net).

The 8-program count assumes the underlying algorithm in each .cb
file is correct — Phase 2 P7 DEV reports state this explicitly
("algorithm correct; failure is in output formatting") for all 8.
Post-fix re-baseline can falsify this assumption with one e2e run.

## Cross-references

- ADR-0047 §"Phase 3 done means" — codifies finding authorship
- ADR-0044 — source-level stdin/argv surface that pattern A failures
  share
- `crates/cobrust-stdlib/src/io.rs:594` — `__cobrust_print_no_nl`
  C-ABI entry
- `crates/cobrust-stdlib/src/io.rs:533-547` —
  `str_buf_as_str_phase3` accessor that calls into fmt.rs:194
- `crates/cobrust-stdlib/src/fmt.rs:194` — exact panic site
  (`buf.cast::<StringBuffer>()`)
- `crates/cobrust-codegen/src/cranelift_backend.rs:2001-2002` —
  `print_no_nl` signature declaration
- Finding `examples-literal-print-debt.md` — earlier debt on
  literal-print examples (M11.x sprint, closed). Pattern A is a
  distinct defect (codegen-level alignment), not the same as the
  earlier debt (lack of real algorithmic examples).
- Finding `lc100-pattern-b-list-of-str-gap.md` — co-occurs in 024
- Finding `lc100-pattern-c-test-corpus-defects.md` — sister cluster
  in this sweep
