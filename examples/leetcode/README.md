# examples/leetcode — 10 LeetCode Programs in Cobrust

> **Phase 3 DEV sprint** — `.cb` files are coming. This directory is a
> placeholder created in the Phase 3 TEST (TDD step 1) commit.
> The Phase 3 DEV P7 sonnet agent creates all 10 `.cb` programs
> until the test corpus in `crates/cobrust-cli/tests/leetcode_corpus_e2e.rs`
> turns green.

## 10 Problems

| # | Name | File | Stdin format | Oracle |
|---|------|------|-------------|--------|
| 01 | Two Sum | `two_sum.cb` | N, then N ints, then target | 0-indexed pair |
| 02 | Reverse String | `reverse_string.cb` | one line | reversed string |
| 03 | Fibonacci | `fibonacci.cb` | N | F(N) |
| 04 | Valid Parentheses | `valid_parentheses.cb` | bracket string | `true` or `false` |
| 05 | Merge Two Sorted Lists | `merge_two_sorted_lists.cb` | `N M`, then N ints, then M ints | merged sorted list, one per line |
| 06 | Maximum Subarray | `maximum_subarray.cb` | N, then N ints | max subarray sum |
| 07 | Binary Search | `binary_search.cb` | N, then N sorted ints, then target | index or `-1` |
| 08 | Climbing Stairs | `climbing_stairs.cb` | N | ways to climb |
| 09 | Best Time to Buy/Sell Stock | `stock_best_time.cb` | N, then N ints | max profit |
| 10 | Roman to Integer | `roman_to_integer.cb` | roman numeral string | integer |

## Run a problem

```bash
# Example: Two Sum
echo "4
2
7
11
15
9" | cobrust run examples/leetcode/two_sum.cb
# Expected output:
# 0
# 1
```

## Test oracle

```bash
cargo test -p cobrust-cli --test leetcode_corpus_e2e --locked
```

All 12 tests must pass after Phase 3 DEV completes.
