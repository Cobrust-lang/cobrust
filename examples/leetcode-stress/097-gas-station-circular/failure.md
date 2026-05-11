# failure.md — LC-097 gas-station-circular

## Status

RUNTIME-FAIL (test corpus error — solution is algorithmically correct)

## Failing test case

Test case C2 in test.toml:

```
input = "3\n2 3 4\n3 4 3\n"
expected_stdout = "2\n"
```

Gas = [2, 3, 4], Cost = [3, 4, 3].
Total gas = 9, total cost = 10. Since total gas < total cost, no valid
starting station exists; the correct answer is -1.

The test oracle expects 2, which is incorrect. Starting at station 2:
- Station 2: tank = 4 - 3 = 1
- Station 0: tank = 1 + 2 - 3 = 0
- Station 1: tank = 0 + 3 - 4 = -1 (cannot complete circuit)

Station 2 does not yield a valid circuit.

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/097-gas-station-circular/solution.cb -o /tmp/lc100-097
printf "3\n2 3 4\n3 4 3\n" | /tmp/lc100-097
# Got: -1
# Expected (test): 2  ← WRONG in test corpus
```

## Other test cases

- C1: gas=[1,2,3,4,5], cost=[3,4,5,1,2] → solution gives 3 ✓ (total gas=15=total cost)
- C2: gas=[2,3,4], cost=[3,4,3] → solution gives -1 (total gas=9 < total cost=10) — test expects 2 (WRONG)
- C3: gas=[1,2,3], cost=[3,4,5] → solution gives -1 ✓
- C4: gas=[5], cost=[5] → solution gives 0 ✓
- C5: gas=[4,6,7,4], cost=[6,5,3,5] → solution gives 1 ✓

## Suspected root cause

Test corpus error (P7-B4-TEST agent). C2 oracle expects 2 but the
mathematically correct answer is -1 (impossible circuit).

## Candidate fix tier

Test corpus correction only (test.toml C2 `expected_stdout` should be `"-1\n"`).
The solution.cb algorithm (single-pass greedy) is correct.
