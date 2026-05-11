# Cobrust LeetCode Examples

W2 Phase 3 deliverable (ADR-0044). Ten LeetCode-style programs written in
Cobrust demonstrating stdin/argv plumbing landed in Phase 2.

Each program reads its input from stdin, computes the result, and writes to
stdout — matching the oracle format expected by
`crates/cobrust-cli/tests/leetcode_corpus_e2e.rs`.

## Prerequisites

Build the Cobrust compiler:

```bash
cargo build -p cobrust-cli
```

## How to run

```bash
# Build a .cb file to an executable:
cargo run -p cobrust-cli -- build examples/leetcode/<problem>.cb -o /tmp/<problem>

# Pipe stdin and capture stdout:
printf "<input>\n" | /tmp/<problem>
```

---

## LC-01 Two Sum

**Problem**: Given N integers and a target, find two indices i < j such that
`nums[i] + nums[j] == target`.

**Input format**:
```
N
nums[0]
nums[1]
...
nums[N-1]
target
```

**Oracle**: N=4, [2,7,11,15], target=9 → `0\n1\n`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/two_sum.cb -o /tmp/two_sum
printf "4\n2\n7\n11\n15\n9\n" | /tmp/two_sum
# Output: 0
#         1
```

---

## LC-02 Reverse String

**Problem**: Reverse the input string.

**Input format**: one line with the string.

**Oracle**: `"hello"` → `"olleh\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/reverse_string.cb -o /tmp/reverse_string
printf "hello\n" | /tmp/reverse_string
# Output: olleh
```

---

## LC-03 Fibonacci

**Problem**: Compute F(N) where F(0)=0, F(1)=1, F(N)=F(N-1)+F(N-2).

**Input format**: one line with integer N.

**Oracle**: N=10 → `"55\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/fibonacci.cb -o /tmp/fibonacci
printf "10\n" | /tmp/fibonacci
# Output: 55
```

---

## LC-04 Valid Parentheses

**Problem**: Given a string of `()[]{}`, determine if it is balanced.

**Input format**: one line with the bracket string.

**Oracle #1**: `"()[]{}"` → `"true\n"`
**Oracle #2**: `"(]"` → `"false\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/valid_parentheses.cb -o /tmp/valid_parentheses
printf "()[]{}\n" | /tmp/valid_parentheses   # true
printf "(]\n" | /tmp/valid_parentheses       # false
```

---

## LC-05 Merge Two Sorted Lists

**Problem**: Merge two sorted integer lists into one sorted list.

**Input format**:
```
N M
list1[0] list1[1] ... list1[N-1]
list2[0] list2[1] ... list2[M-1]
```

**Oracle**: N=3 M=3, [1,3,5] [2,4,6] → `"1\n2\n3\n4\n5\n6\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/merge_two_sorted_lists.cb -o /tmp/merge_two_sorted_lists
printf "3 3\n1 3 5\n2 4 6\n" | /tmp/merge_two_sorted_lists
# Output: 1 2 3 4 5 6 (one per line)
```

---

## LC-06 Maximum Subarray (Kadane's Algorithm)

**Problem**: Find the contiguous subarray with the largest sum.

**Input format**:
```
N
nums[0] nums[1] ... nums[N-1]
```

**Oracle**: N=9, [-2,1,-3,4,-1,2,1,-5,4] → `"6\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/maximum_subarray.cb -o /tmp/maximum_subarray
printf "9\n-2 1 -3 4 -1 2 1 -5 4\n" | /tmp/maximum_subarray
# Output: 6
```

---

## LC-07 Binary Search

**Problem**: Search for a target in a sorted array; return its 0-based index
or -1 if not found.

**Input format**:
```
N
sorted_nums[0] ... sorted_nums[N-1]
target
```

**Oracle**: N=6, [-1,0,3,5,9,12], target=9 → `"4\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/binary_search.cb -o /tmp/binary_search
printf "6\n-1 0 3 5 9 12\n9\n" | /tmp/binary_search
# Output: 4
```

---

## LC-08 Climbing Stairs

**Problem**: Count distinct ways to climb N stairs taking 1 or 2 steps.

**Input format**: one line with integer N.

**Oracle**: N=5 → `"8\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/climbing_stairs.cb -o /tmp/climbing_stairs
printf "5\n" | /tmp/climbing_stairs
# Output: 8
```

---

## LC-09 Best Time to Buy and Sell Stock

**Problem**: Find the maximum profit from one buy-sell transaction.

**Input format**:
```
N
prices[0] prices[1] ... prices[N-1]
```

**Oracle**: N=6, [7,1,5,3,6,4] → `"5\n"` (buy at 1, sell at 6)

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/stock_best_time.cb -o /tmp/stock_best_time
printf "6\n7 1 5 3 6 4\n" | /tmp/stock_best_time
# Output: 5
```

---

## LC-10 Roman to Integer

**Problem**: Convert a Roman numeral string to its integer value.

**Input format**: one line with the Roman numeral (e.g. `MCMXCIV`).

**Oracle**: `"MCMXCIV"` → `"1994\n"`

**How to run**:
```bash
cargo run -p cobrust-cli -- build examples/leetcode/roman_to_integer.cb -o /tmp/roman_to_integer
printf "MCMXCIV\n" | /tmp/roman_to_integer
# Output: 1994
```

---

## Language features demonstrated

All 10 programs use the ADR-0044 W2 Phase 2/3 source-level surface:

| Feature | Example |
|---|---|
| `input("") -> str` | Reading a line from stdin |
| `parse_int(s) -> i64` | Parsing integer from string |
| `parse_int_tok(line, i) -> i64` | i-th space-separated int |
| `count_toks(line) -> i64` | Count of tokens in line |
| `str_len(s) -> i64` | Length of string |
| `str_at(s, i) -> str` | Character at position i |
| `str_ord(c) -> i64` | ASCII code of first byte |
| `str_eq_lit(a, "b") -> i64` | Compare against literal |
| `print_no_nl(c)` | Print without trailing newline |
| `list_new(n) -> list[i64]` | Pre-allocated mutable list |
| `list_set(lst, i, v)` | Write to list position |
| `list_get(lst, i) -> i64` | Read from list position |
| `print_int(n)` | Print integer + newline |
| `while` / `if` / `elif` / `else` | Control flow |
| `fn name(args) -> ret:` | User-defined functions |
