# Cobrust LC-100 Stress Corpus — Tier A

ADR-0047 Phase 2 deliverable. 100 oracle-test directories across 4 buckets,
each testing a distinct algorithm category using Cobrust's ADR-0044
stdin/argv source surface.

**Purpose**: language-surface discovery and regression test corpus.  
**Not** a LeetCode mirror — all problem descriptions are paraphrased.

## Bucket index

| Bucket | Categories | Dirs |
|---|---|---|
| B1 | Arrays, Two Pointers, Hash Maps | 001-030 |
| B2 | Stack/Queue, Linked List, Binary Tree | 031-060 |
| B3 | Dynamic Programming, Binary Search, Bit Manipulation | 061-090 |
| B4 | Math, Greedy, Recursion | 091-100 |

## B2 programs (031-060)

### Stack / Queue (031-040)

| Dir | Slug | Difficulty |
|---|---|---|
| 031 | bracket-balancer-extended | Easy |
| 032 | min-stack-pair | Easy |
| 033 | next-greater-element | Easy |
| 034 | stack-sort-ascending | Easy |
| 035 | queue-via-two-stacks | Easy |
| 036 | daily-temperatures | Easy |
| 037 | reverse-polish-eval | Medium |
| 038 | sliding-window-max | Medium |
| 039 | decode-nested-depth | Easy |
| 040 | largest-rectangle-histogram | Medium |

### Linked List (041-050)

| Dir | Slug | Difficulty |
|---|---|---|
| 041 | reverse-linked-list | Easy |
| 042 | linked-list-cycle-detect | Easy |
| 043 | remove-nth-from-end | Easy |
| 044 | middle-of-linked-list | Easy |
| 045 | linked-list-palindrome | Medium |
| 046 | remove-duplicates-linked-list | Easy |
| 047 | merge-k-sorted-lists | Medium |
| 048 | reorder-linked-list | Medium |
| 049 | intersection-two-lists | Easy |
| 050 | rotate-linked-list | Easy |

### Binary Tree (051-060)

| Dir | Slug | Difficulty |
|---|---|---|
| 051 | binary-tree-max-depth | Easy |
| 052 | invert-binary-tree | Easy |
| 053 | symmetric-tree | Easy |
| 054 | path-sum-exists | Easy |
| 055 | count-nodes | Easy |
| 056 | level-order-traversal | Medium |
| 057 | lowest-common-ancestor | Medium |
| 058 | diameter-of-tree | Easy |
| 059 | flatten-tree-to-list | Medium |
| 060 | right-side-view | Medium |

## Per-directory structure

```
<NNN>-<slug>/
  README.md      — paraphrased problem description + I/O format + oracle
  test.toml      — TOML oracle: [[cases]] with input / expected_stdout /
                   expected_exit_code = 0
  solution.cb    — written by P7-B2-DEV (TDD step 2, not present in step 1)
  failure.md     — written by P7-B2-DEV if solution.cb fails gates
```

## Encoding conventions

- **Linked list**: parallel arrays `val[N]` and `next[N]`; head = index 0;
  null/end = -1 sentinel.
- **Binary tree**: parallel arrays `val[N]`, `left[N]`, `right[N]`;
  root = index 0; absent child = -1 sentinel.
- **Stack/Queue**: emulated with `list_new` + integer top/front/back cursors.

## How to run a solution (once DEV step is complete)

```bash
cargo build -p cobrust-cli
cargo run -p cobrust-cli -- build examples/leetcode-stress/<NNN>-<slug>/solution.cb \
    -o /tmp/lc-<NNN>
printf "<input>\n" | /tmp/lc-<NNN>
```
