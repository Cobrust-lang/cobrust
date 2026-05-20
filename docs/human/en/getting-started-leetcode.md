# LeetCode with Cobrust

> 30 minutes from zero to your first Two Sum.

## Prerequisites

- Cobrust v0.1.2+ installed — see [Getting started — 30-second install](getting-started.md)
- Verify your install:

  ```bash
  cobrust --version
  # Expected: cobrust 0.1.2
  ```

- Requires: `cargo` (to compile `.cb` files from source)

---

## Two ways to run LeetCode in Cobrust (5 minutes)

Cobrust programs can receive input in two ways:

### Path 1: stdin (standard input — the OJ-standard approach)

```bash
printf "4\n2\n7\n11\n15\n9\n" | cobrust run examples/leetcode/two_sum.cb
```

- Uses `input("")` to read one line at a time
- Returns empty string `""` on EOF instead of raising an exception
- Best fit for LeetCode / competitive programming judges

### Path 2: argv (command-line arguments)

```bash
cargo run -p cobrust-cli -- build examples/leetcode/two_sum.cb -o /tmp/two_sum
/tmp/two_sum arg1 arg2
```

- Uses `argv()`, which returns `list[str]`; first element is the program path
- Useful for parameterized invocations or tool scripts

> Use Path 1 (stdin + `input()`) for LeetCode problems — it matches the input format used by online judges.

---

## Problem 1: Two Sum (10-minute walkthrough)

### Problem statement

Given N integers and a target value, find two indices `i < j` such that `nums[i] + nums[j] == target`.

### Input format (stdin, multiple lines)

```
N           <- array length
nums[0]     <- one integer per line
nums[1]
...
nums[N-1]
target      <- the target value
```

**Example input** (N=4, nums=[2,7,11,15], target=9):

```
4
2
7
11
15
9
```

**Expected output**:

```
0
1
```

### Complete Cobrust solution

```cobrust
# LC-01 Two Sum (ADR-0044 W2 Phase 3).
#
# Input  (stdin):
#   Line 1: N
#   Lines 2..N+1: one integer each
#   Line N+2: target
#
# Output: indices i j (i < j) with nums[i]+nums[j]==target, one per line.
#
# Algorithm: O(N²) brute-force scan.

fn main() -> i64:
    let n = parse_int(input(""))
    let nums = list_new(n)
    let i: i64 = 0
    while i < n:
        let v = parse_int(input(""))
        list_set(nums, i, v)
        i = i + 1
    let target = parse_int(input(""))
    let a: i64 = 0
    while a < n:
        let b: i64 = a + 1
        while b < n:
            if list_get(nums, a) + list_get(nums, b) == target:
                print(a)
                print(b)
                return 0
            b = b + 1
        a = a + 1
    return 0
```

### Run it

```bash
cd /path/to/cobrust
printf "4\n2\n7\n11\n15\n9\n" | cargo run -p cobrust-cli -- run examples/leetcode/two_sum.cb
# Expected output:
# 0
# 1
```

Or build first, then run:

```bash
cargo run -p cobrust-cli -- build examples/leetcode/two_sum.cb -o /tmp/two_sum
printf "4\n2\n7\n11\n15\n9\n" | /tmp/two_sum
# Expected output:
# 0
# 1
```

### Cobrust vs Python comparison

| Feature | Python | Cobrust |
|---|---|---|
| Read one line | `s = input()` | `let s = input("")` |
| Parse integer | `int(s)` | `parse_int(s)` |
| Create a list | `nums = [0] * n` | `let nums = list_new(n)` |
| Write to list | `nums[i] = v` | `list_set(nums, i, v)` |
| Read from list | `nums[i]` | `list_get(nums, i)` |
| Print integer | `print(x)` | `print(x)` |
| Print string | `print(s)` | `print(s)` |
| EOF handling | raises `EOFError` | returns `""` |

---

## Full catalog — 10 problems

See [`examples/leetcode/README.md`](../../../examples/leetcode/README.md) for input formats and run commands.

| # | Problem | Difficulty | Key language features |
|---|---|---|---|
| 01 | Two Sum | Easy | `list_new` / `list_get` / `list_set` |
| 02 | Reverse String | Easy | `str_len` / `str_at` / `print_no_nl` |
| 03 | Fibonacci | Easy | recursion / DP `while` loop |
| 04 | Valid Parentheses | Easy | `str_eq_lit` / `list_new` as stack |
| 05 | Merge Two Sorted Lists | Easy | `count_toks` / `parse_int_tok` |
| 06 | Maximum Subarray | Easy | Kadane's algorithm, `while` + local vars |
| 07 | Binary Search | Easy | `while` binary search, return index or -1 |
| 08 | Climbing Stairs | Easy | DP with rolling variables |
| 09 | Best Time to Buy and Sell Stock | Easy | greedy, single pass |
| 10 | Roman to Integer | Easy | `str_ord` / `str_at` character mapping |

---

## Cobrust LeetCode style guide

### Reading input

- **Recommended**: `input("")` — reads one line, strips trailing newline, returns `""` on EOF
- **Not recommended for beginners**: `read_line()` — preserves trailing `\n`, requires manual handling
- Parse an integer: `parse_int(input(""))`
- Parse the i-th space-separated integer on a line (e.g. `"3 5 7"`): `parse_int_tok(line, i)`
- Count tokens on a line: `count_toks(line)`

### Using argv

```cobrust
fn main() -> i64:
    let args = argv()        # list[str]; args[0] is the program path
    let n = parse_int(args[1])
    ...
    return 0
```

### Data type availability

| Type | Status | Notes |
|---|---|---|
| `i64` | Available | 64-bit signed integer |
| `str` | Available | UTF-8 string |
| `list[i64]` | Available | use `list_new` / `list_get` / `list_set` |
| `list[str]` | Available | returned by `argv()` |
| `dict` | Not yet implemented | Phase F roadmap |
| `f64` | Not yet implemented | Phase F roadmap |

### print functions

- `print(s)` — print string with automatic `\n`
- `print(n)` — print integer with automatic `\n`
- `print_no_nl(s)` — print string without `\n` (useful for character-by-character output)

---

## Common gotchas

### 1. No implicit bool

```cobrust
# Wrong — Cobrust does not allow implicit truthiness
if x:
    ...

# Correct
if x > 0:
    ...
if !s.is_empty():
    ...
```

### 2. Reassignment does not need `let mut`

```cobrust
# Correct — just reassign; no mut keyword needed
let x: i64 = 0
x = x + 1

# Type inference works at declaration
let n = parse_int(input(""))
```

### 3. No `is` — use `==`

```cobrust
# Wrong — Cobrust removes the `is` operator entirely
if a is b:
    ...

# Correct
if a == b:
    ...
```

### 4. EOF detection

```cobrust
# input() returns "" on EOF, does not raise
let line = input("")
while !str_eq_lit(line, ""):    # str_eq_lit compares a string to a literal
    # process line
    line = input("")
```

### 5. String comparison

```cobrust
# Correct — compare against a string literal with str_eq_lit
if str_eq_lit(s, "true"):
    print("matched")
```

---

## Next steps

- Browse [`examples/leetcode/README.md`](../../../examples/leetcode/README.md) for all 10 problems with input formats and run commands
- Want to contribute a new problem? See [`CONTRIBUTING.md`](../../../CONTRIBUTING.md)
- Language roadmap: [ADR-0038 Phase F roadmap](../../agent/adr/0038-phase-f-roadmap.md) — more stdlib and Python library translation plans
- Technical details on stdin/argv: [ADR-0044](../../agent/adr/0044-stdin-argv-source-binding.md)
