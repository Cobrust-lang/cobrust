# LC-049 Intersection Node of Two Linked Lists

**Category**: Linked List
**Difficulty**: Easy

## Algorithm

Given two singly linked lists that may share a common tail suffix (i.e.,
merge at some node), find the value at the first shared node, or print -1
if they do not intersect.

The two-pointer length-equalization approach is O(N+M) with O(1) space.
Compute the lengths of both lists. Advance the cursor in the longer list
by the length difference. Then walk both cursors together one step at a time
until they land on the same node index. That node is the intersection.

For this parallel-array formulation, "same node" means the same index value
in the next[] array — not the same value, but the same structural position
at which the two lists join.

## Input format

```
Line 1: A B C   (A = length of first list exclusive of shared tail;
                 B = length of second list exclusive of shared tail;
                 C = length of shared tail; 0 means no intersection)
Lines 2..A+1: values of first list (non-shared prefix)
Lines A+2..A+B+1: values of second list (non-shared prefix)
Lines A+B+2..A+B+C+1: values of shared tail
```

## Oracle

A=2 B=3 C=3, prefix1=[4,1] prefix2=[5,6,1] tail=[8,4,5]
→ first shared value = `8`

No intersection A=2 B=3 C=0 → `-1`

## Approach hint

Build the combined next[] structure in a flat array. Pointers from the last
node of each prefix chain into the shared tail. Walk both to find the
intersection index, then print `val[intersection_index]`.
