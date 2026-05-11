# failure.md — LC-053 symmetric-tree

## Status

RUNTIME-FAIL

## Failing command

```
cargo run -p cobrust-cli --quiet -- build examples/leetcode-stress/053-symmetric-tree/solution.cb -o /tmp/lc100-053
printf "5\n1 1 2\n2 3 -1\n2 -1 4\n3 -1 -1\n3 -1 -1\n" | /tmp/lc100-053
```

## Raw stderr

(none — exit code 0, stdout mismatch on case 2 only)

## Actual vs expected

Case 2 input: 5-node tree where root(val=1) has left=node1(val=2,left=node3)
and right=node2(val=2,right=node4). Both node3 and node4 have val=3.
Expected stdout: "false\n"
Actual stdout: "true\n"

## Suspected root cause

Test corpus defect. The tree represents:
      1
     / \
    2   2
   /     \
  3       3

By the definition in README.md (is_mirror(a,b): both null→true; one null→false;
val equal and is_mirror(left[a],right[b]) and is_mirror(right[a],left[b])):
  - is_mirror(node1, node2): val[1]=val[2]=2 ✓
    - is_mirror(left[1],right[2]) = is_mirror(3,4): val[3]=val[4]=3 ✓
      - is_mirror(-1,-1)=true ✓, is_mirror(-1,-1)=true ✓
    - is_mirror(right[1],left[2]) = is_mirror(-1,-1) = true ✓
  → is_mirror(1,2) = true → tree IS symmetric.

The expected "false" is incorrect. The tree with left subtree having a left
child and right subtree having a right child is symmetric by the mirror definition.

## Candidate fix tier

source-level gap (test corpus)

## Notes

Solution implements the correct is_mirror algorithm per README.md. Cases 1,3,4
all produce the correct output. Case 2's expected value appears to have been
set incorrectly in test.toml.
