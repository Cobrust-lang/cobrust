---
name: f47
status: RESOLVED
family: F-language
date: 2026-05-22
resolved: 2026-05-25
last_verified_commit: HEAD
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

RESOLVED 2026-05-25. Fix shipped in two coordinated changes:

1. **MIR `lower_call` + `lower_rewritten_method_call`**
   (`crates/cobrust-mir/src/lower.rs`): the `_callret` destination local
   was declared as `Ty::None` regardless of the callee's actual return
   type. Downstream f-string `lower_aggregate_format_string` inspects
   `body.locals[op_local].ty` to decide between `is_str` (call
   `__cobrust_fmt_str`) and the fall-through `v_ty.is_int()` branch
   (call `__cobrust_fmt_int`). With `Ty::None`, `is_str = false` and the
   raw heap pointer was formatted as a decimal number. Fix: propagate
   the callee's declared `Ty::Fn(...).return_ty` to the `_callret`
   declaration so the downstream dispatch sees `Ty::Str`.

2. **Both Cranelift and LLVM backends `lower_statement`**
   (`crates/cobrust-codegen/src/cranelift_backend.rs` + `llvm_backend.rs`):
   the special-case materialise branch for `Use(Constant::Str(payload))`
   required the destination's declared type to be `Ty::Str`. The
   function's return slot `_return` is declared `Ty::None` per
   `BodyBuilder::new` convention; so `fn f() -> str: return "literal"`
   fell through to `lower_constant(Constant::Str(_))` which returned the
   M9 stub `iconst(I64, 0)`. Fix: also fire materialisation when
   `place.local == body.return_local`. Safe because `Use(Constant::Str)`
   to the return slot only arises when the type checker has validated
   the function returns `Ty::Str`.

Both changes ship together — the MIR change handles call-result
interpolations (`f"{user_fn()}"` shape) and the codegen change handles
the literal-return shape (`return "..."` inside the callee).

### §6.1 Resolution commits

- `cf0864c` — MIR + codegen fix + corpus + outreach file
- `dcb1714` — finding update (this commit)

### §6.2 Regression corpus

`crates/cobrust-cli/tests/fstring_user_fn_str_corpus.rs` — six fixtures
covering the F47 surface:

- `fstring_user_fn_str_simple` — minimal repro (`f"got {make_str()}"`)
- `fstring_user_fn_str_concat` — literal slots surrounding the hole
- `fstring_user_fn_str_multi` — multiple user-fn-returned str slots
- `fstring_literal_baseline` — control (was always correct pre-fix)
- `fstring_user_fn_str_branch_returns` — multi-branch return literals
  (mirrors 99-bottles `line_count` shape)
- `fstring_user_fn_str_with_int_mix` — interleaves Str + Int holes

All 6 pass on Cranelift backend post-fix. Pre-fix the simple case
prints `"got !\n"` against expected `"got hello!\n"`.

### §6.3 99-bottles outreach

`docs/agent/outreach/99-bottles.cb` now produces the correct 99-bottles
song output (`99 bottles of beer on the wall` ... `Take it down and
pass it around, no more bottles of beer on the wall.` ... `Go to the
store and buy some more, 99 bottles of beer on the wall.`).

## §7 Verification

Post-fix repro confirms:

```text
$ cobrust build /tmp/f47_repro.cb -o /tmp/f47_repro
$ /tmp/f47_repro
got hello!
```

`docs/agent/outreach/99-bottles.cb` renders all 100 lines correctly
including the singular `1 bottle` / `no more bottles` branches.

## §8 Cross-References

- ADR-0050a — f-string grammar design
- ADR-0050b — f-string runtime buffer semantics
- ADR-0058f — string runtime maturity (wave-6 scope)
- F49 — pre-flight smoke-check discipline (parallel surface)
