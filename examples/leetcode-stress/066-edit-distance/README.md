# LC-066 Edit Distance

**Category**: Dynamic Programming
**Difficulty**: Medium

## Algorithm

Given two strings, compute the minimum number of single-character edit
operations needed to transform the first string into the second. The three
allowed operations are: insert a character, delete a character, or replace
one character with another. Each operation costs 1.

The standard 2D DP table has dimensions (len(s1)+1) x (len(s2)+1). The base
cases are: dp[i][0] = i (delete i characters from s1 to get empty string),
dp[0][j] = j (insert j characters to build s2 from empty). For dp[i][j]:
if s1[i-1] == s2[j-1], no extra cost, inherit dp[i-1][j-1]. Otherwise,
take 1 + min(dp[i-1][j-1], dp[i-1][j], dp[i][j-1]) for replace/delete/insert.

## Input format

```
Line 1: string s1
Line 2: string s2
```

## Oracle

- s1="horse", s2="ros" → `3`
- s1="abc", s2="abc" → `0`
- s1="", s2="abc" → `3`

## Approach hint

The 2D table can be space-optimized to two 1D rows (current and previous).
Allocate a list of size len(s2)+1. Initialize the first row to 0,1,2,...
For each character of s1, scan s2 updating in-place with a saved diagonal.
