---
doc_kind: finding
finding_id: lc024-already-passing-stale-ignore
last_verified_commit: 8f63132
dependencies: [adr:0047, finding:lc100-pattern-a-rodata-literal-misalignment, finding:lc100-pattern-b-list-of-str-gap]
discovered_by: P7 lc024 root-cause sprint 2026-05-19 — last LC-100 hold-out diagnosis (Pre-state: main 8f63132, 99 PASS + 1 ignored)
severity: P2 (stale honest-debt; not a language gap; F37 silent-rot caught at sprint scope)
status: resolved
related: [adr:0047, finding:lc100-pattern-a-rodata-literal-misalignment, finding:lc100-pattern-b-list-of-str-gap]
---

# Finding: LC-024 `#[ignore]` is stale — fixture passes at HEAD `8f63132`

## §1. Pre-state assertion vs empirical reality

The P10 directive cited lc024 root cause as `str_at-on-literal +
missing-list[str]` (failure.md §"Suspected root cause"). Empirical
reproduction at HEAD `8f63132` on Mac aarch64 contradicts the pre-state:

```
$ cobrust build examples/leetcode-stress/024-hashmap-group-anagrams/solution.cb -o /tmp/lc024-test
cobrust build: linked /tmp/lc024-test

$ printf "6\neat\ntea\ntan\nate\nnat\nbat\n" | /tmp/lc024-test
eat
tea
ate

tan
nat

bat
EXIT=0
```

All three assertion inputs in `tests/lc100_stress_e2e_b1.rs::test_lc024_hashmap_group_anagrams`
produce byte-exact expected output:

| Input | Expected | Got | Result |
|---|---|---|---|
| `6\neat\ntea\ntan\nate\nnat\nbat\n` | `eat\ntea\nate\n\ntan\nnat\n\nbat\n` | identical | PASS |
| `1\nabc\n` | `abc\n` | identical | PASS |
| `2\nab\nba\n` | `ab\nba\n` | identical | PASS |

lc024 is **already green**. The `#[ignore]` is silent stale debt.

## §2. Why the original failure.md is no longer reproducible

The original `failure.md` (HEAD pre-`e91caed`) traced the panic to:

> `crates/cobrust-stdlib/src/fmt.rs:194:22:
> misaligned pointer dereference: address must be a multiple of 0x8 but is 0x1048fb141`

This panic site is the `StringBuffer` cast in `__cobrust_str_len`
(`buf.cast::<StringBuffer>()`). The hypothesis was that
`str_at("literal", i)` passed a raw `.rodata` byte pointer (1-byte
aligned) to `__cobrust_str_at`, which forwards through
`str_buf_as_str_phase3 → __cobrust_str_len → buf.cast::<StringBuffer>`
and trips Rust 1.78+ UB misalignment detection.

Two upstream changes (both predating HEAD `8f63132`) closed the path:

### §2.1 `materialize_str_buffer` allocates a real StringBuffer

`crates/cobrust-codegen/src/cranelift_backend.rs:1023-1037` —
`materialize_str_buffer(payload)` calls `__cobrust_str_new` then
`__cobrust_str_push_static(buf, ptr, len)` to populate the
StringBuffer with the literal bytes. The returned `buf` is a real
8-byte-aligned heap pointer to a `StringBuffer`, NOT the raw
`.rodata` pointer.

When codegen lowers `str_at(LIT, idx)` (args.len()=2, sig_param_count=2
for `__cobrust_str_at(*mut u8, i64)`), neither `expand_str_to_ptr_len`
nor `expand_trailing_str_len` fires — the `Constant::Str` first-arg
flows through `materialize_str_buffer`. The runtime receives a
properly-aligned StringBuffer pointer, and `__cobrust_str_len` no
longer panics.

### §2.2 `__cobrust_print_no_nl_lit` for the downstream `print_no_nl(c)`

When `c = str_at(LIT, idx)`, `c` is the return value of the runtime
shim `__cobrust_str_at` (io.rs:512-523), which returns
`alloc_str_buffer(&s[idx..=idx])` — also a real StringBuffer.
`print_no_nl(c)` operand-aware dispatch (intrinsics.rs:1584-1626)
routes the non-`Constant::Str` operand to `__cobrust_print_no_nl`
(StringBuffer entry, safe for runtime-allocated buffers).

No misalignment occurs anywhere in the call chain.

## §3. Why the `#[ignore]` was added and never re-verified

The `#[ignore]` was authored during the LC-100 stress sweep
(`e91caed` and earlier) before:
- Pattern A `__cobrust_print_no_nl_lit` landed
- `materialize_str_buffer` was confirmed to allocate a real
  StringBuffer for literal first-args

The +83 PASS LC-100 stress sprint (`8f63132`) refactored 84 fixtures
mechanically with the &-borrow precedent but did not re-verify
lc024 (the failure.md cited a different root cause class —
`str_at`-on-literal misalignment + missing-list[str] — outside the
borrow-refactor scope). Result: the `#[ignore]` survived as stale
honest-debt.

## §4. Pattern B (list[str] gap) status

The original failure.md additionally cited "missing `list[str]`" as
a structural blocker. Inspection of `solution.cb` shows the
algorithm **already works around** the gap using a flat `list[i64]`
encoding (each word stored as 32 chars-per-row * M rows, with a
parallel `lens: list[i64]` for actual lengths). Pattern B is real
language-surface debt (finding `lc100-pattern-b-list-of-str-gap.md`
still applies for future programs) but does NOT block lc024 —
the fixture authored a workaround.

No change to ADR-0058a §15 language-surface roster is needed for
this sprint; Pattern B remains in the existing queue.

## §5. Resolution

- **Action**: remove `#[ignore]` annotation from
  `tests/lc100_stress_e2e_b1.rs::test_lc024_hashmap_group_anagrams`.
- **Replace surrounding comment block**: cite this finding instead
  of the obsolete root-cause hypothesis.
- **No language change**: codegen + stdlib are correct.
- **No fixture change**: `solution.cb` algorithm is correct.

## §6. F36 + F37 compliance

- **F36 (fixture name vs behavior)**: lc024 fixture is named
  `024-hashmap-group-anagrams`; the algorithm correctly groups
  anagrams. No rename needed.
- **F37 (silent-rot on accepted debt)**: this finding explicitly
  reverses the stale `#[ignore]`. The pre-state assertion in the
  P10 directive was empirically wrong; this doc records why. No
  new `#[ignore]` added; the test flips from ignored to PASS.

## §7. LC-100 stress final state

```
PRE  (8f63132):   99 passed;  0 failed;  1 ignored
POST (this fix): 100 passed;  0 failed;  0 ignored
```

LC-100 stress reaches **100/100** with this `#[ignore]` removal.

## §8. Cross-references

- ADR-0047 §"Phase 2 Pattern A fix" — operand-aware
  `print_no_nl_lit` dispatch
- Finding `lc100-pattern-a-rodata-literal-misalignment.md`
  §"Candidate F1" — the upstream `__cobrust_print_no_nl_lit`
  shim that closed Pattern A as a class
- Finding `lc100-pattern-b-list-of-str-gap.md` — list[str]
  remains a forward-looking language-surface gap; not blocking
  for lc024 due to fixture workaround
- `crates/cobrust-codegen/src/cranelift_backend.rs:1023` —
  `materialize_str_buffer` (the actual fix for `str_at(LIT, idx)`)
- `crates/cobrust-stdlib/src/io.rs:512` — `__cobrust_str_at` C-ABI
- `examples/leetcode-stress/024-hashmap-group-anagrams/solution.cb`
  line 28 — the only `str_at("literal", i)` call site in the
  repo; verified working
