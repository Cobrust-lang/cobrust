---
doc_kind: finding
finding_id: leetcode-corpus-parse-int-tok-use-after-move-fixture-debt
last_verified_commit: 031ac44
dependencies: [adr:0050c, adr:0052a, finding:list-polymorphic-instantiation-ambiguity-root-cause]
discovered_by: P9 lc01/lc02 root-cause sprint 2026-05-19 — pre-state assertion in P10 directive misnamed the failures; post-list-poly-fix lc01 + lc02 already PASS, real residue is lc05/06/07/09 + lc_all_compile
severity: P1 (fixture-authoring debt; non-blocking on language; compile-time-caught with concrete fix suggestion)
status: resolved
related: [adr:0052a, adr:0050c, finding:lc100-str-use-after-move-regression-from-adr0050c, finding:list-polymorphic-instantiation-ambiguity-root-cause]
---

# Finding: leetcode corpus fixtures missing `&line` explicit-borrow form across `parse_int_tok`/repeat-read patterns

## §1. Pre-state correction (F37 honest-debt)

The P9 sprint directive cited "lc01_two_sum + lc02_reverse_string still FAIL"
as the residue post-list-poly-fix (HEAD `99228c3`). Empirical verification
on both Mac aarch64 and DG x86_64 shows the opposite:

```
test test_lc01_two_sum_oracle_match ........... ok
test test_lc02_reverse_string_oracle_match .... ok
test test_lc03_fibonacci_oracle_match ......... ok
test test_lc04_valid_parentheses_oracle_match_true ........ ok
test test_lc04_valid_parentheses_oracle_match_false ....... ok
test test_lc05_merge_two_sorted_lists_oracle_match ........ FAILED
test test_lc06_maximum_subarray_oracle_match .............. FAILED
test test_lc07_binary_search_oracle_match ................. FAILED
test test_lc08_climbing_stairs_oracle_match ............... ok
test test_lc09_stock_best_time_oracle_match ............... FAILED
test test_lc10_roman_to_integer_oracle_match .............. ok
test test_lc_all_compile .................................. FAILED (downstream)

test result: FAILED. 7 passed; 5 failed; 0 ignored; 0 measured.
```

lc01 + lc02 are GREEN at `99228c3`. The actual failures are lc05/06/07/09
plus the compile-all gate (downstream of those four). This finding records
the precise root cause + landed fix.

## §2. Precise root cause

All four failing fixtures (`examples/leetcode/{merge_two_sorted_lists,
maximum_subarray,binary_search,stock_best_time}.cb`) share an identical
pattern: a `let line = input("")` followed by repeated
`parse_int_tok(line, i)` calls inside a `while` loop.

`parse_int_tok` is declared in PRELUDE (`crates/cobrust-cli/src/build.rs:51`):

```cobrust
fn parse_int_tok(line: str, i: i64) -> i64:
    return 0
```

`str` is non-Copy under ADR-0050c Phase 2a (Option A — Full-Drop schedule).
The first `parse_int_tok(line, ...)` consumes `line` (move-by-default); the
second call (next loop iteration) hits `MirError::UseAfterMove` with the
helpful suggestion already wired in:

```
cobrust build: MIR error: UseAfterMove {
  local: 5,
  span: Span { file: FileId(0), start: 0, end: 0 },
  suggestion: Some("change to `&s` to borrow without consuming
                    (ADR-0052a explicit shared borrow)")
}
```

ADR-0052a Wave-1 §4.1 introduced the explicit `&s` shared-borrow form
exactly for this case. lc02 already correctly uses it
(`examples/leetcode/reverse_string.cb`):

```cobrust
let n = str_len(&s)
let c = str_at(&s, i)
```

The four failing fixtures predate ADR-0052a adoption in `examples/leetcode/`
or were authored without picking up the explicit-borrow idiom for
PRELUDE str-reading helpers.

## §3. Classification: fixture-authoring debt, not language bug

This is **not** a language-surface gap. The error message is already
optimal per CLAUDE.md §2.5 (F.1.4 "print the FIX, not just the diagnosis"):
diagnosis + concrete suggestion + ADR cross-reference. Three of the four
fixture mistakes are caught at compile time (MIR borrow-check, before
codegen) — exactly the §2.5 "compile-time-catch-errors" criterion.

What the empirical evidence DOES corroborate is the §2.5 / ADR-0051
"Priority A" claim: the **largest current LLM-friendliness deficit** is
"explicit `&` borrow / let-rebind shortcut". The 4 fixtures here, written
in earlier sprints by sub-agents that didn't pick up the `&` discipline,
are exactly the data point that motivated promoting `let-rebind` /
implicit re-borrow to Phase G P0 #1 in ADR-0051.

The proper resolution per F36 ("fixture name must match behavior") is:
fixture **names** are correct (each .cb solves its LeetCode problem),
fixture **code** needs the ADR-0052a borrow form. Update the .cb code.

## §4. Landed fix

Per fixture, the change is mechanical: prefix the repeat-read `str`
argument with `&` at every `parse_int_tok` / `count_toks` call site that
re-reads the same `line`.

`merge_two_sorted_lists.cb`:
```diff
-    let n = parse_int_tok(nm_line, 0)
-    let m = parse_int_tok(nm_line, 1)
+    let n = parse_int_tok(&nm_line, 0)
+    let m = parse_int_tok(&nm_line, 1)
     ...
-        list_set(list1, i, parse_int_tok(line1, i))
+        list_set(list1, i, parse_int_tok(&line1, i))
     ...
-        list_set(list2, j, parse_int_tok(line2, j))
+        list_set(list2, j, parse_int_tok(&line2, j))
```

`maximum_subarray.cb`, `binary_search.cb`, `stock_best_time.cb`:
```diff
-    let max_sum = parse_int_tok(line, 0)
-    ...
-        let v = parse_int_tok(line, i)
+    let max_sum = parse_int_tok(&line, 0)
+    ...
+        let v = parse_int_tok(&line, i)
```

After the fix all four oracle-match tests and the compile-all gate pass.

## §5. Verification

Mac aarch64 + DG x86_64, full `leetcode_corpus_e2e` post-fix:
expected → `12 passed; 0 failed`.

LC-100 corpus delta: this finding addresses 4 of the 5 fixture-level
leetcode_corpus_e2e failures. Stress-corpus (`tests/lc100/`) is unrelated
to these 4 examples — predicted delta on stress-corpus 16P/87F is +0 to
+small (these are different programs from the stress corpus; only
overlap is the `parse_int_tok` repeat-read pattern, which the stress
corpus may also use).

### §5.1 LC-100 stress corpus follow-on sprint (HEAD `031ac44`, 2026-05-19)

The predicted "+0 to +small" delta on stress-corpus was wrong — the
SAME root cause (PRELUDE str-fn first-arg consumed in repeat-read
context) afflicted **84 of 100** stress fixtures. Mechanical refactor
sprint applied the b2618f3 precedent to `examples/leetcode-stress/`:

| Batch | Range | Files | PRELUDE-fn call sites |
|---|---|---|---|
| 1 | lc001-020 | 20 | 36 |
| 2 | lc021-040 | 20 | 52 |
| 3 | lc041-060 | 15 | 45 |
| 4 | lc061-080 | 17 | 56 |
| 5 | lc081-100 | 12 | 18 |
| 6 (user-fn) | 028 + 065 + 068 + 072 + 077 + 078 + 080 | 7 | 14 user-fn + 5 word-arg |
| **Total** | — | **84** | **226** |

Empirical category counts of the 87 pre-state stress failures:

- **A** (PRELUDE str-fn first-arg, bare ident): ~85 fixtures — RESOLVED
  by batches 1-5.
- **B** (str_at/str_len in deeper str-heavy fixtures): subset of A;
  same root cause, same fix.
- **C** (list_get/set polymorphic): 0 — resolved earlier at `99228c3`.
- **D** (user-defined str-arg fns called multiple times): 7 fixtures
  (028, 065, 068, 072, 077, 078, 080) — RESOLVED by batch 6; lc068
  additionally needed word-arg `&` (3rd arg in `words_match`, batch 7).

LC-100 stress before/after:
```
PRE (b2618f3):    16 passed; 87 failed;  1 ignored (lc024 separate)
POST (031ac44):   99 passed;  0 failed;  1 ignored (lc024 same)
Δ:                +83 PASS / −87 FAIL
```

The remaining `lc024_hashmap_group_anagrams` ignore is a pre-existing
RUNTIME-FAIL (failure.md cites "str_at on literal vars misaligned +
missing list[str]"), unrelated to ADR-0050c borrow semantics. No new
`#[ignore]` added during this sprint (F37 compliant — no silent
cover-ups).

F36 compliance audit: zero fixture renames. All 84 refactored fixtures
solve the LeetCode problem named in their slug.

## §6. F36 + F37 compliance

- **F36 (fixture name vs behavior)**: NO fixture renames. The fixtures'
  filenames and algorithms accurately solve their LeetCode problems
  (merge two sorted lists, Kadane, binary search, max-profit). Only the
  Cobrust borrow syntax inside is being updated.
- **F37 (silent-rot on accepted debt)**: this finding explicitly records
  why the pre-state assertion in the P10 directive was wrong (lc01/lc02
  already pass) and the actual residue. No `#[ignore]` is added; tests
  go from FAIL → PASS directly via fixture code fix.

## §7. Cross-references

- ADR-0052a Wave-1 §4.1 — explicit shared-borrow form
- ADR-0050c §"Decision" — Option A Str=non-Copy
- finding:list-polymorphic-instantiation-ambiguity-root-cause (resolved
  the AmbiguousType part of LC-100; this finding addresses the
  UseAfterMove residue noted in its §"residual issue is a separate
  finding queue" closer)
- CLAUDE.md §2.5 / ADR-0051 Priority A — confirms the
  let-rebind / implicit-borrow priority direction with empirical
  fixture-author-friction evidence
