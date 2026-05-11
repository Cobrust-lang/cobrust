# LC-047 Merge K Sorted Linked Lists

**Category**: Linked List
**Difficulty**: Medium

## Algorithm

Given K sorted singly linked lists (each encoded as a flat sorted array in
the input), merge them into one sorted list and print the result.

Without a priority queue, the simplest correct approach is repeated pairwise
merging: merge list 1 and list 2 into a result, then merge that result with
list 3, and so on. Each pairwise merge is O(M+N); total cost is O(N*K^2/2)
which is acceptable for small K.

Each individual merge follows the standard two-pointer merge: compare the
current head of each list, output the smaller value, advance that list's
cursor. When one list is exhausted, append the remainder of the other.

## Input format

```
Line 1: K   (number of lists)
Lines 2..K+1:
  first token = length L_i of that list
  next L_i tokens = sorted values of that list (space-separated on one line)
```

## Oracle

K=3, [1,4,5] [1,3,4] [2,6] → `1 1 2 3 4 4 5 6` (one per line)

## Approach hint

Read each list into a separate scratch array. Implement a two-list merge
function that returns a flat merged list. Call it K-1 times, accumulating
results into a single merged array.
