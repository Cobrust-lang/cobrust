# LC-068 Word Break DP

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

Given a string and a set of dictionary words, determine whether the string
can be partitioned into one or more segments where each segment is a word in
the dictionary. Words can be reused any number of times. The partitioning
must cover the entire string with no leftover characters.

The DP array dp[i] indicates whether the prefix of length i can be fully
segmented. dp[0] = true (empty prefix). For each position i from 1 to len(s),
check all starting positions j from 0 to i-1: if dp[j] is true and the
substring s[j..i] is in the dictionary, then dp[i] = true. Answer is dp[len(s)].

## Input format

```
Line 1: the string to segment
Line 2: W (number of dictionary words)
Lines 3..W+2: one dictionary word per line
```

## Oracle

- s="applepenapple", dict=["apple","pen"] → `true`
- s="catsandog", dict=["cats","dog","sand","and","cat"] → `false`
- s="abc", dict=["ab","c"] → `true`

## Approach hint

Since Cobrust lacks a built-in hash set, store dictionary words in a list and
write a helper `word_in_dict(dict, W, word_start, word_end, s)` that linearly
scans the word list and compares substrings using str_at. This makes the
algorithm O(n^2 * W) but correct on small inputs.
