---
doc_kind: finding
finding_id: while-binop-eq-zero-condition-miscompile
last_verified_commit: 9ff481c
discovered_by: review-claude (LeetCode farm Round 1, LC 263 ugly-number)
severity: P0
related: m12-x-while-if-codegen-regression (closed by M11.1), codegen-i8-i64-mismatch-at-4-blocks (closed by ADR-0033 Option C)
status: open
---

# Finding: `while <BinOp_expr> == 0:` head miscompiles, body never entered

## Hypothesis

After M11.1 (while-leading-if codegen regression closed at `ea093ef`) and
ADR-0033 Option C (Ty::None / narrow-type unified at `3392eb5`), the `while`
codegen path was assumed clean for non-trivial conditions. This finding
records a **third** independent codegen bug surface in `while` head
lowering.

## Method

Discovered organically while writing LeetCode 263 (Ugly Number) in the
external `Cobrust-leetcode-farm/` test farm — a pure-stress test of
current Cobrust capability with no relation to corpus enumeration.

## Result

The same boolean expression `n % 2 == 0` evaluates **truthfully** when
used as an `if` head, but **falsely** when used as a `while` head, on
the very first iteration with the same value of `n`.

### Minimum reproducer

```cobrust
fn main() -> i64:
    let n: i64 = 6
    if n % 2 == 0:
        print("if-branch yes")     # ALWAYS prints
    while n % 2 == 0:
        print("while-iter")        # NEVER prints
        n = 9999                   # never executed
    print("final n =")
    print_int(n)                   # prints 6, not 9999 → while body never entered
    return 0
```

Build: PASS (no verifier error, no link error)
Run output:
```
if-branch yes
final n =
6
```

Expected:
```
if-branch yes
while-iter
final n =
9999
```

### Probe matrix (full repro at `/tmp/lc263_repro3/`)

| Probe | While-head shape | n=6 enters body? | Verdict |
|---|---|---|---|
| Original | `while n % 2 == 0` | NO | ❌ MISCOMPILE |
| Probe 1 | `while m == 0` (m precomputed `let m = n % 2` then `m = n % 2` updated in body) | YES | ✓ correct |
| Probe 2 | `while n != 1` | YES | ✓ correct |
| Probe 3 | `while n > 0` | YES | ✓ correct |

So the bug surface is **specifically** `while <expr_containing_BinOp> == 0`
where the LHS of `==` is a non-trivial expression (`%`, presumably `+`,
`-`, `*`, `/` too — to be confirmed). Pre-computing the BinOp into a
temp `let m = n % 2` and writing `while m == 0` works around it.

### Affected scope (estimate)

Any Cobrust source-level `while <expr> == 0` or `while <expr> != 0` where
`<expr>` is a non-trivial BinOp — i.e. effectively all "while there's
still factors of 2/3/5" or "while remainder zero" idioms. This pattern
is **extremely common** in number-theory / algorithm code:
- Ugly Number (LC 263)
- GCD-via-Euclid (`while b != 0`)
- Factor reduction
- Bit traversal (`while n & 1 == 0`, if bit ops land)

Pre-computing the mod into a temp is a reliable workaround; the
fizzbuzz-style `if n % 15 == 0` inside a `while n <= 15` does NOT trigger
the bug, because the BinOp+`==0` is in an `if` head, not the `while` head.

## Root-cause hypothesis (for fix sprint)

The `if` head and `while` head are likely lowered through different
codegen paths in `crates/cobrust-codegen/src/cranelift_backend.rs`. One
path (likely `if`) materialises the BinOp result as an SSA value then
compares against 0; the other path (likely `while`) may be dropping the
BinOp short-circuit / mistreating the condition's operand chain — for
example, treating the condition as a direct truthy check on the BinOp
result without honouring the `== 0` comparator.

Specifically suspect: in the `while` head, `<BinOp> == 0` may be
optimised into `not <BinOp>` (i.e. `BinOp == 0` ≡ `!BinOp` if BinOp
result is bool), but this optimisation is **wrong for i64 BinOps** —
`n % 2 == 0` is an arithmetic-then-equality check, not a bool inversion.

A `while` codegen path that converts `cond` into `if !cond goto exit`
might be trying to fuse the `==0` comparator into a single negation
without preserving the integer-equality semantic.

## Fix direction

1. **Localise**: search `crates/cobrust-codegen/src/cranelift_backend.rs`
   for the `while` lowering function (`lower_while` or similar), find
   how the head condition's IR is emitted.
2. **Compare** to `if` head lowering. The `if` path produces correct
   `icmp eq <BinOp>, 0` then `brif`; the `while` path may have a
   shortcut that misfolds.
3. **Fix**: ensure both heads route through the same `lower_condition`
   helper, with no `BinOp == 0` → `!BinOp` simplification at the IR
   layer.

## Workaround (until fix lands)

Pre-compute the BinOp result into a `let` temp before the while head:

```cobrust
# Instead of:
while n % 2 == 0:
    n = n / 2

# Write:
let m: i64 = n % 2
while m == 0:
    n = n / 2
    m = n % 2
```

This is what the LC 263 farm entry uses post-discovery.

## Cross-references

- ADR-0030 (M11.1 — fixed M12.x's while-leading-if regression); orthogonal
- ADR-0033 (codegen Ty::None / narrow-type Option C); orthogonal
- finding `m12-x-while-if-codegen-regression` (M11.1 closed); orthogonal
- finding `codegen-i8-i64-mismatch-at-4-blocks` (ADR-0033 closed); orthogonal
- LC 263 farm entry: `Cobrust-leetcode-farm/lc_263_ugly/`
- Minimum repro: `/tmp/lc263_repro3/`
- Probe matrix: `/tmp/lc263_repro3/src/main.cb`

This is the THIRD independent `while` codegen bug surface found in the
last 24 hours (M12.x → M11.1 → ADR-0033 → this). The methodology
finding `two-bugs-one-fix-option-c-pattern` is highly relevant — when
fixing this, the fix author should ALSO verify the `if` and `while`
heads share a single `lower_condition` primitive, in the spirit of
ADR-0033 Option C's "find the root primitive, don't patch the surface".
