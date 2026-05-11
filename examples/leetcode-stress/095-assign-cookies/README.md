# LC-095 Assign Cookies

**Category**: Greedy
**Difficulty**: Easy

## Algorithm

You have a list of children, each with a minimum greed factor (the smallest
cookie size that satisfies them), and a list of cookies each with a given size.
Each child can receive at most one cookie; a cookie satisfies a child only if
its size is at least the child's greed factor. Maximize the number of satisfied
children.

The greedy insight is straightforward: sort both lists in ascending order. Use
two pointers, one for children and one for cookies. If the current cookie is
large enough for the current child, assign it and advance both pointers (one
more child satisfied). Otherwise the cookie is too small for this child — it
cannot satisfy any remaining child either — so skip it by advancing only the
cookie pointer. Continue until either list is exhausted.

This greedy choice is locally optimal and globally optimal: assigning the
smallest sufficient cookie to the least demanding child leaves larger cookies
available for more demanding children.

## Input format

```
Line 1: G — number of children
Line 2: G space-separated greed factors (one per child)
Line 3: S — number of cookies
Line 4: S space-separated cookie sizes
```

## Oracle

- G=3 greed=[1,2,3], S=3 cookies=[1,1,2] → `2`
- G=2 greed=[1,2], S=3 cookies=[1,2,3]   → `2`

## Approach hint

Sort both arrays (selection or insertion sort using list_get/list_set).
Walk with two indices `ci` (children) and `si` (cookies): if
`cookie[si] >= greed[ci]` increment both and count; otherwise increment `si`
only. Output the count.
