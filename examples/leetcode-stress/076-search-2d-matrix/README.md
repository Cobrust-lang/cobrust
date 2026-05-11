# LC-076 Search 2D Matrix

**Category**: Binary Search
**Difficulty**: Medium

## Algorithm

Given an M x N matrix where each row is sorted in ascending order and the
first element of each row is strictly greater than the last element of the
previous row, determine whether a target value is present in the matrix.
The algorithm must run in O(log(M*N)) time.

The matrix can be treated as a single sorted array of M*N elements. Perform
standard binary search using a virtual index i in [0, M*N-1]. To convert
virtual index i to matrix coordinates: row = i / N, col = i % N. This lets
the entire matrix be searched with one binary search pass.

## Input format

```
Line 1: M N (rows and columns)
Lines 2..M+1: N space-separated integers per row
Line M+2: target
```

## Oracle

- M=3, N=4 matrix [[1,3,5,7],[10,11,16,20],[23,30,34,60]], target=3 → `true`
- Same matrix, target=13 → `false`
- M=1, N=1, [[1]], target=1 → `true`

## Approach hint

Read all matrix values into a flat list of size M*N (row-major order). Binary
search on indices 0..M*N-1. The element at virtual index i is list[i]. Output
"true" on success, "false" on failure.
