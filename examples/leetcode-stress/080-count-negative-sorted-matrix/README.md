# LC-080 Count Negatives in Sorted Matrix

**Category**: Binary Search
**Difficulty**: Easy

## Algorithm

Given an M x N grid where each row is sorted in non-increasing order and
each column is also sorted in non-increasing order, count the total number
of negative integers in the grid. The sorted structure allows an O(M log N)
or O(M + N) solution, both better than the O(M*N) brute force.

For each row, use binary search to find the first negative number's column
index. All elements from that column to the end of the row are negative.
Summing these counts across all rows gives the total. The binary search per
row searches for the leftmost column where grid[row][col] < 0.

## Input format

```
Line 1: M N
Lines 2..M+1: N space-separated integers per row (non-increasing order)
```

## Oracle

- M=4, N=4, [[4,3,2,-1],[3,2,1,-1],[1,1,-1,-2],[-1,-1,-2,-3]] → `8`
- M=2, N=2, [[3,2],[1,0]] → `0`
- M=1, N=1, [[-1]] → `1`

## Approach hint

For each row, binary search for the first index where the value is negative.
Since the row is non-increasing, this is equivalent to finding the leftmost
position with value < 0. The count for that row is N - first_negative_col.
Sum across rows.
