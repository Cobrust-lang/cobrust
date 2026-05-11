# LC-078 Koko Eating Speed

**Category**: Binary Search
**Difficulty**: Medium

## Algorithm

Given N piles of items and H hours, find the minimum integer rate K (items
per hour) at which you can finish all piles within H hours. Each pile is
handled individually: a pile of size P takes ceil(P/K) hours at rate K.
You must consume each pile completely before moving to the next.

This is another "binary search on the answer" problem. The minimum feasible
rate is 1; the maximum is max(piles) (finish any pile in one hour). Binary
search for the smallest K in [1, max_pile] such that sum of ceil(pile/K) <= H.
Use integer ceiling: ceil(P/K) = (P + K - 1) / K in integer arithmetic.

## Input format

```
Line 1: N H (number of piles, number of hours)
Line 2: N space-separated pile sizes
```

## Oracle

- N=4, H=5, piles=[3,6,7,11] → `4`
- N=5, H=8, piles=[30,11,23,4,20] → `23`
- N=1, H=3, piles=[10] → `4`

## Approach hint

Binary search lo=1, hi=max(piles). For each mid, compute total hours =
sum of (pile + mid - 1) / mid for each pile. If total_hours <= H, mid is
feasible; record mid as best answer and try lower (hi=mid-1). Otherwise
raise lo=mid+1.
