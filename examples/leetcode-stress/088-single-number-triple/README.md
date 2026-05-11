# LC-088 Single Number in Triples

**Category**: Bit Manipulation
**Difficulty**: Medium

## Algorithm

Given a list of integers where every element appears exactly three times
except for one element that appears exactly once, find that unique element.
The O(1) space constraint rules out a simple hash count.

The bit-by-bit counting approach: for each of the 32 bit positions, count
how many numbers have a 1 in that position. If the count is not divisible
by 3, the unique element has a 1 in that position. Reconstruct the answer
bit by bit.

Alternatively, use two bitmask accumulators: ones and twos. ones tracks bits
seen an odd number of times mod 3; twos tracks bits seen an even but non-zero
number of times mod 3. Update: new_ones = (ones XOR current) AND NOT twos;
new_twos = (twos XOR current) AND NOT new_ones. After all elements, ones
holds the unique value.

## Input format

```
Line 1: N (number of elements, N mod 3 == 1 since one unique + rest triples)
Line 2: N space-separated integers
```

## Oracle

- N=7, [2,2,3,2,4,4,4] → `3`
- N=4, [0,1,0,1] → invalid (only two distinct), use N=7 style
- N=7, [5,5,5,9,1,1,1] → `9`
- N=1, [42] → `42`

## Approach hint

Use the 32-bit counting approach: allocate a bit_counts array of 32 integers.
For each number, for each bit position b, check if (number / 2^b) mod 2 == 1
and increment bit_counts[b]. Reconstruct answer: for each bit b where
bit_counts[b] mod 3 != 0, add 2^b to the result.
