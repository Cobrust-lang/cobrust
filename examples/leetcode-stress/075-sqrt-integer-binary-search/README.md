# LC-075 Integer Square Root via Binary Search

**Category**: Binary Search
**Difficulty**: Easy

## Algorithm

Given a non-negative integer X, compute the floor of its square root — that
is, the largest integer R such that R * R <= X. Do not use any floating-point
operations; use only integer arithmetic. The algorithm should run in O(log X)
time.

Binary search on the answer range [0, X]. Maintain lo=0, hi=X. At each step,
compute mid=(lo+hi)/2. If mid*mid <= X, mid is a candidate answer (update
best=mid) and search right (lo=mid+1). If mid*mid > X, search left (hi=mid-1).
To avoid overflow for large X, clip hi to X/2+1 after X>1.

## Input format

```
Line 1: X (non-negative integer)
```

## Oracle

- X=4 → `2`
- X=8 → `2`
- X=0 → `0`

## Approach hint

Cap the search range at hi=X/2+1 for X>1 (since sqrt(X) <= X/2 for X>=4).
The best variable tracks the largest mid where mid*mid<=X. Use i64 arithmetic
throughout. Handle X=0 and X=1 as base cases if preferred.
