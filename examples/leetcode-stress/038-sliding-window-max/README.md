# LC-038 Sliding Window Maximum

**Category**: Stack / Queue
**Difficulty**: Medium

## Algorithm

Given an array of N integers and a window size K, compute the maximum value
in every contiguous window of size K as it slides from left to right. There
are N-K+1 such windows.

The efficient approach uses a monotone-decreasing deque (double-ended queue).
The deque stores indices. Before processing element i:
1. Remove from the front any index that is no longer within the current window
   (index <= i - K).
2. Remove from the back any index whose value is less than or equal to the
   current element (those indices can never be the window maximum).
3. Push the current index to the back.
4. Once i >= K-1, the front of the deque is the maximum index for this window.

This runs in O(N) overall.

## Input format

```
Line 1: N K
Line 2: N space-separated integers
```

## Oracle

N=8 K=3, [1, 3, -1, -3, 5, 3, 6, 7] → `3 3 5 5 6 7`

## Approach hint

Emulate the deque with a circular buffer or two-pointer list. Track front and
back cursors. For the window expiry check, compare `deque_front_index <= i - k`.
