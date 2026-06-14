---
doc_kind: finding
finding_id: f92-str-ordering-comparison-codegen-panic
last_verified_commit: TBD
discovered_by: §5.1 no-panic-on-type-checked-input + §2.5 LLM-first review (str comparison)
severity: P1
related: f85-codegen-panic-on-typechecked-input-class, f87-codegen-panic-on-typechecked-input-class, ADR-0078 (str == str / str + str retarget — direct pattern sibling), ADR-0093 (bytes comparison deferral / clean reject), ADR-0094 + ADR-0101 + ADR-0103 (str codepoint arc)
status: closed_by_F92
---

# Finding: `str` ORDERING comparison (`<` `<=` `>` `>=`) CRASHED the compiler

## Hypothesis

`"abc" < "abd"` (and `>`, `<=`, `>=`) CRASHED the `cobrust build` compiler
with a codegen panic — build exit 101. `==` / `!=` on `str` already WORKED
(ADR-0078); only the four ORDERING ops crashed.

CPython oracle (`/opt/homebrew/bin/python3.11`):

| expression       | CPython | pre-F92 Cobrust |
|------------------|---------|-----------------|
| `"abc" < "abd"`  | `True`  | build exit 101  |
| `"abc" > "abd"`  | `False` | build exit 101  |
| `"abc" <= "abd"` | `True`  | build exit 101  |
| `"abc" >= "abd"` | `False` | build exit 101  |
| `"abc" == "abc"` | `True`  | `True` (worked)  |

## Root cause — the F85/F87 codegen-panic class

The type checker ACCEPTS `str < str`, so the program type-checks and then
crashes in codegen:

- `synth_bin`'s comparison arm (`crates/cobrust-types/src/check.rs`,
  `BinOp::Eq | NotEq | Lt | LtEq | Gt | GtEq`) does `unify(Str, Str)` →
  succeeds → returns `Ty::Bool`, IDENTICALLY for `==` and for `<`/`>`/etc.
- Codegen's `lower_binop` (`crates/cobrust-codegen/src/llvm_backend.rs`)
  `Lt/LtEq/Gt/GtEq` arms call `into_int_value()` / `into_float_value()`.
  A `str` is an OPAQUE POINTER, so inkwell panics with `expected the
  IntValue variant` — a raw ICE, NOT a Cobrust diagnostic.

`str == str` did NOT crash only because ADR-0078 had already RETARGETED it
in MIR lowering (`lower_bin`) to `__cobrust_str_eq` before codegen — the
ordering ops simply had no equivalent retarget, so they fell through to the
int-assuming codegen arms.

This violates §5.1 ("the compiler must not panic on type-checked input")
and §2.5 (an LLM agent writes `s1 < s2` constantly — sorting, ordering,
binary search; Python performs lexicographic str comparison, so the LLM-
first fix is to IMPLEMENT it, not reject it).

## Fix (F92 / ADR-0104)

IMPLEMENT lexicographic `str` ordering — Python supports it.

- **runtime** (`crates/cobrust-stdlib/src/io.rs`):
  `__cobrust_str_cmp(a, b) -> i64` returns the sign of Rust `a.cmp(b)`
  (`Ordering::{Less,Equal,Greater}` → -1/0/+1), beside `__cobrust_str_eq`.
- **MIR** (`crates/cobrust-mir/src/lower.rs`): a `str` ordering arm in
  `lower_bin` (immediately below the `str == str` arm) retargets the four
  ops to `__cobrust_str_cmp`, then materialises the bool as `cmp OP 0`
  (reusing the SAME `bin_to_mir(op)` — `a < b` ⇔ cmp < 0, etc.). Operands
  are BORROWED (Move→Copy upgrade; `__cobrust_str_cmp` reads, does not
  consume — the source str locals survive + drop once).
- **codegen** (`crates/cobrust-codegen/src/llvm_backend.rs`): declare the
  `__cobrust_str_cmp` extern beside `__cobrust_str_eq`. The integer
  `lower_binop` `Lt/LtEq/Gt/GtEq` arms then handle `cmp OP 0` unchanged.
- **check.rs**: comparison typing UNCHANGED — `unify(Str, Str)` already
  accepts ordering (the very thing that made it type-check); mixed
  `str < int` still `unify`-rejects cleanly (exit 2, no panic).

### Codepoint vs byte order

Python compares `str` by CODEPOINT; Rust `str` `Ord` is BYTE-lexicographic
over UTF-8. UTF-8 is ORDER-PRESERVING, so for valid UTF-8 (which every
Cobrust `str` is) byte order == codepoint order — `a.cmp(b)` matches
CPython. Confirmed by `str_cmp_e2e_04` (`"é"`(U+00E9) > `"f"`(U+0066)).

### `bytes` ordering — left as a clean reject

`bytes < bytes` (all `bytes` comparison ops) remain an ADR-0093 deferral:
the `check.rs` comparison arm REJECTS them at type-check with a fix-
printing diagnostic (exit 2), NOT a codegen panic. F92 confirms the reject
is clean (`str_cmp_e2e_09`) and scopes the `__cobrust_bytes_cmp` impl to a
future follow-up.

## Evidence / repro

```
fn main() -> None:
    print("abc" < "abd")   # pre-F92: build exit 101 (codegen panic); now: True
```

- `crates/cobrust-cli/tests/str_cmp_e2e.rs` — `str_cmp_e2e_01..09`
  (four ops vs CPython, prefix/empty, equal inclusive-vs-strict, unicode
  codepoint, str variables in `if`, numeric-`<`/`>`-unchanged regression,
  str-`==`/`!=`-unchanged regression, mixed-`str<int`-reject,
  bytes-ordering-reject).
- ADR-0104 (decision + the codepoint/byte order-preservation note).
