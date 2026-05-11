---
doc_kind: finding
finding_id: lc100-pattern-b-list-of-str-gap
last_verified_commit: e91caed
dependencies: [adr:0047, adr:0044]
related: [lc100-pattern-a-rodata-literal-misalignment]
discovered_by: lc-100-tier-a-stress-sweep
---

# Finding: LC-100 Pattern B — `list[str]` type missing from Cobrust language surface

## Hypothesis

ADR-0044 introduced `list[i64]` as the canonical heap-allocated
homogeneous list type at the source level. The hypothesis: this
single list-of-integer type is sufficient for typical algorithmic
programs because strings can be projected to integer codepoint
arrays and reconstructed at print time. LC-100 falsifies this
hypothesis for at least one algorithm class: those that store
multiple input strings for later output (group-by, deduplication,
sorting of strings).

## Method

- Read `examples/leetcode-stress/024-hashmap-group-anagrams/failure.md`
  on `feature/lc100-stress-sweep` at HEAD `e91caed`.
- Cross-referenced ADR-0044 prelude surface and
  `crates/cobrust-stdlib/src/list.rs` / `crates/cobrust-stdlib/src/list_ops.rs`
  for available list types.
- Surveyed the remaining 99 LC-100 programs for similar gaps that
  remained latent because the test corpus avoided string-storing
  algorithms.

## Result

### Direct evidence — program 024

`examples/leetcode-stress/024-hashmap-group-anagrams/failure.md`
identifies two compounding blockers for the group-anagrams
algorithm:

1. **Pattern A** (codegen): `str_at("alphabet-literal", i)` returns
   a misaligned pointer (see
   `lc100-pattern-a-rodata-literal-misalignment.md`).
2. **Pattern B** (language surface): even with Pattern A fixed,
   the algorithm requires storing M input strings to print them
   grouped by anagram signature. Cobrust has no `list[str]` —
   the only list type accepts `i64`. The algorithm therefore would
   need to reconstruct each input string from stored character
   codes at print time, which forces it back through Pattern A.

The two patterns intersect on program 024: Pattern B is the
structural blocker; Pattern A is the surface symptom. Fixing
Pattern A alone allows the codepoint-reconstruction workaround to
proceed, but produces an awkward solution that does not feel
Python-equivalent. Adding `list[str]` is the principled fix.

### Latent evidence — other 99 programs

A targeted grep of P7-B1/B2/B3/B4-TEST corpus README.md inputs
shows zero other programs that REQUIRE storing input strings for
later string output. Most LC-100 programs either:

- consume strings transiently (parsing, character iteration without
  storage) — e.g. 022 valid-anagram counts chars, doesn't store
  strings;
- store strings as fixed-size character code lists with bounded
  length (e.g. 020 backspace-compare maintains two character
  stacks via `list[i64]`);
- print integers / booleans / sentinel values, not strings.

This is **survivorship bias**: the LC-100 corpus was authored
under the implicit constraint "use what's available", so P7 test
agents avoided string-storing algorithms by selecting alternates.
The 24-anagram case is a minimum-instance reveal of a structural
gap that would re-surface at Tier B / C with frequency proportional
to string-heavy algorithm prevalence (estimated 5-15% of medium
tier).

### Why the LC-024 P7 DEV did not extend the language

ADR-0047 §"Compiler / stdlib touch list — NONE expected" binds
P7 DEV agents to NOT extend the compiler during Phase 2. The agent
correctly wrote `failure.md` documenting the gap rather than
shipping a `list[str]` patch on a side-branch. This is the
designed-in behavior: Phase 3 (this finding) escalates the gap
to an ADR proposal candidate.

## Conclusion — actionable proposal

### Severity: BLOCK for string-heavy algorithm classes

Pattern B blocks any algorithm that requires:
- Storing M input strings of variable length
- Sorting / grouping / deduplicating strings as values
- Returning collections of strings as output

These are common patterns in Python: `dict[str, list[str]]`,
`Counter` of strings, `sorted(words, key=...)`. Without
`list[str]`, Cobrust cannot express them ergonomically.

### Estimated complexity: ≥ 1 day (sonnet sprint, likely opus)

Adding `list[str]` is non-trivial:

1. **HIR/MIR**: parametric list type `list<T>` where `T` is `i64`
   today and `str` tomorrow. ADR-0044 prelude treats list as a
   monomorphic type; needs at least dyadic generic support or
   two concrete list types (`list_i64` + `list_str`) wired through
   the type checker.
2. **Runtime**: `crates/cobrust-stdlib/src/list_ops.rs` operations
   (list_get, list_set, list_push, list_len) need string-aware
   variants. Each element holds a StringBuffer pointer; reference
   counting or ownership semantics required (drop semantics
   non-trivial).
3. **Codegen**: list-of-pointer layout; allocator path for
   string elements; bounds-checked indexing returns `str` not
   `i64`.
4. **Source-level surface**: `list_new(<str>)`,
   `list_push(l, s: str)`, `list_get(l, i) -> str`. ADR-0044
   prelude amendment + e2e tests + at least 1 LC-100 program
   rewrite.
5. **Ownership / aliasing**: list operations must not invalidate
   element references. Either copy-on-push semantics or
   refcount-on-element. ADR-0019 ownership/borrowing implications.

This is conservatively a 1-2 day sprint, possibly 2-3 days if
the generic-list refactor is undertaken (preferred long-term).
**Opus-grade work** by memory `feedback_subagent_model_tier.md`
("real LLM / multi-crate refactor" = Opus).

### Recommendation: defer Pattern B to a dedicated ADR

Rather than bundling Pattern B fix into a LC-100 fix-pack, write
a standalone ADR (e.g., ADR-0048 "Parametric list type for
heterogeneous algorithm corpora") that surveys the design
tradeoffs (monomorphic-per-type vs generic) and lands as its own
multi-day sprint. This unblocks LC-024 + a broader algorithm
class (estimated 5-15% of Tier B medium / hard tier).

### Pattern B's contribution to LC-100 pass rate

With Pattern B fixed (alongside Pattern A):
- Program 024 turns green
- Pass rate moves from 84/100 (post-Pattern-A) → 85/100

The marginal contribution of Pattern B fix to LC-100 is only 1
program. Pattern B's value is **forward-looking**: it removes a
structural blocker for Tier B / C string-heavy algorithms, not
for marginal LC-100 coverage.

## Cross-references

- ADR-0047 §"Compiler / stdlib touch list — NONE expected" — the
  Phase 2 binding that prevented P7 DEV from extending
- ADR-0044 — current list type surface (`list[i64]` only)
- `crates/cobrust-stdlib/src/list_ops.rs` — current list runtime
  helpers
- `crates/cobrust-stdlib/src/list.rs` — current list type module
- Finding `lc100-pattern-a-rodata-literal-misalignment.md` —
  co-occurs in program 024
- Finding `lc100-pattern-c-test-corpus-defects.md` — sister
  cluster in this sweep
