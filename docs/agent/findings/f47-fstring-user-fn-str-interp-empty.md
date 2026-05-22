---
name: f47
status: RATIFIED
family: F-language
date: 2026-05-22
last_verified_commit: e23d66c
---

# F47 — F-string: user-fn-returned `str` interpolates as empty

## §1 Context

Cobrust v0.5.1+ (possibly earlier). When a `str` value returned by a user-defined
function is interpolated inside an f-string, the interpolated slot prints empty instead
of the function's return value.

String literals and integer values interpolate correctly in the same f-string context;
the defect is specific to the `str` type when its source is a user-function return value
(not a string literal, not a variable bound to a literal at call site).

Repro confirmed via user-reported smoke test on 2026-05-22 using
`docs/agent/outreach/99-bottles.cb`.

## §2 Minimal Repro

```cobrust
fn line_count(n: i64) -> str:
    if n == 0: return "no more bottles"
    if n == 1: return "1 bottle"
    return f"{n} bottles"

fn main() -> i64:
    let c: str = line_count(99)
    print(f"{c} of beer on the wall")
    # actual:   " of beer on the wall"   ← c slot EMPTY
    # expected: "99 bottles of beer on the wall"
    return 0
```

The six-line excerpt above is the complete repro. Source file staged (not committed
pending fix) at `docs/agent/outreach/99-bottles.cb`.

## §3 Expected vs Actual Stdout

| | Value |
|---|---|
| Expected | `99 bottles of beer on the wall` |
| Actual | ` of beer on the wall` |

The leading space is the literal space before "of" in the f-string template; the `{c}`
slot contributes zero characters.

## §4 Suspected Root Cause

F-string lowering in the codegen layer (Cranelift + LLVM backends). When an f-string
slot value has type `Ty::Str` and its source is a function return value (rather than a
string literal constant), `lower_fstring_slot` likely calls `lower_constant(Str)` and
receives a `Ty::None` pointer or zero-length str buffer instead of the populated heap
string returned by the callee.

Candidate code path: `cobrust-codegen/src/lower/fstring.rs` — the slot lowering branch
that handles `Expr::Var` bound to a `Ty::Str` may not dereference the alloca correctly
after a call instruction, depending on how the ABI returns `str` (fat pointer vs. single
pointer vs. length + ptr pair).

## §5 Detection Rule

Add corpus fixture `codegen_diff_corpus::fstring_user_fn_str_interp` covering both
Cranelift and LLVM backends:

```rust
// tests/codegen_diff_corpus.rs
#[test]
fn fstring_user_fn_str_interp() {
    // compile + run 99-bottles.cb repro snippet
    // assert stdout == "99 bottles of beer on the wall\n"
}
```

CI gate candidate: any f-string integration test that passes a user-fn `str` return
value as a slot argument.

## §6 Status

RATIFIED 2026-05-22 by user-reported smoke. Repro file:
`docs/agent/outreach/99-bottles.cb` (staged, not committed until fix lands).

## §7 Resolution

Queued for v0.6.x or v0.7.0. Family priority: F-language (codegen/runtime defect),
distinct from packaging-discipline findings (F35-sibling family). Fix must include
regression test in `codegen_diff_corpus` before closing.

## §8 Cross-References

- ADR-0050a — f-string grammar design
- ADR-0050b — f-string runtime buffer semantics
- ADR-0058f — string runtime maturity (wave-6 scope)
