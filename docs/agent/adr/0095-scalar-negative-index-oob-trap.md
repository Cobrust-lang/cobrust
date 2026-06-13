---
doc_kind: adr
adr_id: 0095
title: "scalar `s[i]` / `b[i]` — Python from-end negative indexing + OOB-TRAP (F79 Option B, the A→B maturation; kills the silent `\"\"`/`-1` sentinel)"
status: accepted
date: 2026-06-13
last_verified_commit: HEAD
supersedes: []
superseded_by: []
relates_to: [adr:0093, adr:0094, finding:f78, finding:f79, "claude.md:§2.2", "claude.md:§2.5"]
---

# ADR-0095: scalar `s[i]` / `b[i]` — from-end negative indexing + OOB-TRAP

## Context

The SCALAR single-index operator on `str` (`s[i] -> str`, ADR-0094) and
`bytes` (`b[i] -> int`, ADR-0093) had two §2.2 holes, both tracked by
finding **F79**:

1. **Negative index** — `"hello"[-1]` (the #1 Python indexing idiom — an
   LLM writes it constantly, §2.5 maximize-training-data-overlap) silently
   returned the sentinel `""` (str) / `-1` (bytes), NOT the last element.
   CPython: `"hello"[-1] == "o"`, `b"abc"[-1] == 99`.
2. **Out-of-range** — `"hello"[100]` / `b"abc"[100]` ALSO silently returned
   the SAME sentinel (`""` / `-1`). An in-band wrong value §2.2 forbids;
   CPython raises `IndexError`.

ADR-0093/0094 §Phasing shipped **Option A** (the interim, §2.5-A compile-
time-catch): a NEGATIVE-LITERAL scalar index was REJECTED at `cobrust
check` (`TypeError::UnsupportedSliceShape`, reused), with the §2.5-B fix
`s[len(s) - 1]`. Option A was the *safe* choice while the runtime still
clamped `i < 0` to a sentinel: rejecting loudly beats a silent wrong value.
But Option A:

- did NOT support `s[-1]` (it rejected the #1 idiom — a §2.5 first-try
  trap: the LLM writes `s[-1]`, gets a reject, must rewrite to
  `s[len(s) - 1]`);
- did NOT catch a NON-LITERAL runtime-negative `s[i]` (a variable holding a
  negative value still hit the sentinel);
- left the SILENT-POSITIVE-OOB hole (`s[100]`) entirely open — Option A
  only touched negatives.

## Decision (Option B — the planned maturation)

Now that the runtime is codepoint-correct (ADR-0094), implement FULL
Python scalar-index semantics in the runtime + REMOVE the Option-A compile-
time reject:

### 1. Runtime (`cobrust-stdlib/src/string.rs` + `bytes.rs`)

`__cobrust_str_char_at(s, i)` and `__cobrust_bytes_get(b, i)`:

```
len = <codepoint count for str  |  byte count for bytes>
idx = if i < 0 { len + i } else { i }   # Python from-end normalization
if idx < 0 || idx >= len { crate::panic::panic("<kind> index out of range: i={i} len={len}") }
return element at idx
```

- **Negative index** reads from the end: `s[-1] == s[len-1]`. For `str`
  this is CODEPOINT-addressed (`len = chars().count()`, the walk uses
  `chars().nth(idx)`), so `"héllo"[-1] == "o"` and `"héllo"[-4] == "é"`
  (a byte-indexed impl would cut the 2-byte `é` or land off-by-one).
- **True OOB** (positive `idx >= len` OR too-negative `i < -len`) TRAPS via
  `crate::panic::panic` — the SAME project trap convention
  `__cobrust_bytes_decode` uses, NOT a raw `assert!`. The runtime surfaces
  it as **exit 3 (INTERNAL_PANIC)** with a single clean
  `cobrust panic: <kind> index out of range: i=.. len=..` line — no Rust
  backtrace, no leaked internal source paths. (A raw `assert!` here would
  instead abort the `extern "C"` frame as a non-unwinding SIGABRT → exit 134
  + a ~20-line path-leaking backtrace; the B-1b-style audit caught + fixed
  that drift.) Mirrors Rust's own `s[i]` slice-OOB trap. The diagnostic
  NAMES the bad index AND the length (§2.5-B). A NULL handle is treated as
  empty (`len == 0`), so any index traps.
- **NO sentinel.** The `if i < 0 { return ""/-1 }` guard AND the
  `None => ""/-1` positive-OOB arm are both DELETED. This is the §2.2 fix:
  an out-of-range scalar read is unrecoverable, not an in-band value.

### 2. Type checker (`cobrust-types/src/check.rs`)

The `(Ty::Str, IndexKind::Expr)` and `(Ty::Bytes, IndexKind::Expr)` arms
now ACCEPT every integer index (unify with `Int`, return `Str` / `Int`).
The Option-A `literal_int_value(e).is_some_and(|v| v < 0)` reject is
REMOVED from BOTH arms — `s[-1]` is a VALID expression. No new error
variant, no error-cascade change (we removed a reject, added none), so the
`error_cb` / `error_ux` / `types-parity` byte-parity tripwire stays GREEN.

### Scope — SCALAR ONLY

This increment is SCALAR-index only. The SLICE-shape rejects
(`UnsupportedSliceShape` for open-ended `s[1:]`, stepped `s[0:4:2]`, and
negative-bound `s[1:-1]` slices) are UNCHANGED — a negative SLICE bound
stays rejected at `check`. Only the scalar `s[i]` / `b[i]` negative index
becomes valid + from-end. Full negative-bound SLICE support is a separate
follow-up (it needs a `len`-relative slice lowering).

## Consequences

- `s[-1]` / `b[-1]` (the #1 idiom) is correct on the first try — §2.5 win.
- A true OOB scalar read TRAPS loudly instead of silently miscompiling —
  the §2.2 hole (BOTH the negative AND the pre-existing positive-OOB
  sentinel) is closed for the scalar path.
- The MIR / codegen are UNCHANGED — the scalar index already lowered to a
  `__cobrust_str_char_at` / `__cobrust_bytes_get` call; only the runtime
  body + the check.rs gate changed.
- Unit-test note: the OOB TRAP can NOT be observed by an in-crate
  `#[should_panic]` test — `crate::panic::panic` terminates the process
  (exit 3) rather than unwinding a catchable Rust panic, so it would abort
  the test runner. The TRAP is therefore verified END-TO-END in the cli
  e2e suites (build a `.cb`, run the exe, assert `exit == 3` + the
  `… index out of range` stderr diagnostic — the `assert_build_run_traps`
  helper asserts the EXACT exit 3, so a regression back to a raw-abort
  exit 134 or a silent sentinel cannot pass). The runtime unit
  tests cover the POSITIVE behaviors (from-end negative read + codepoint
  addressing).

## Phasing (deferrals, named)

- Negative-bound + open-ended + stepped SLICES (`s[-2:]`, `s[1:]`,
  `s[0:4:2]`) stay rejected at `check` — a `len`-relative slice lowering is
  the next follow-up.
- An ASCII O(1) fast-path for `str` from-end indexing is deferred (the
  `chars().count()` + `chars().nth()` walk is O(n); no benchmark demands a
  fast-path yet — same posture as ADR-0094).

## Evidence

- e2e (CPython-3 oracle): `crates/cobrust-cli/tests/str_slice_e2e.rs`
  (`str_slice_e2e_06_negative_scalar_index_from_end` — `s[-1]`/`s[-2]`/
  `s[-5]` + the multi-byte `"héllo"[-1]`/`[-4]` codepoint case + a
  runtime-negative `s[j]`; `str_slice_e2e_06_positive_oob_traps` +
  `…_negative_oob_traps`) and `bytes_ops_e2e.rs` (the lockstep `bytes`
  twin: `bytes_ops_e2e_10_negative_scalar_index_from_end` —
  `b"\x01\x02\xff"[-1] == 255` + a runtime-negative `b[j]`;
  `…_positive_oob_traps` + `…_negative_oob_traps`).
- stdlib unit: `cobrust-stdlib/src/string.rs` (`str_char_at_codepoint`,
  `str_char_at_negative_index_codepoint`) + `bytes.rs`
  (`get_negative_index_from_end`).
- regression GREEN: full `cobrust-cli` sweep (the one flaky `coil_*`
  binary is the known libcoil parallel-link flake — passes in isolation),
  `cobrust-stdlib` lib, `cobrust-types` + `cobrust-types-cb` +
  `cobrust-types-parity` (byte-parity cascade tripwire), `leetcode_corpus_e2e`
  (LC-100 — no program relied on the silent-OOB sentinel).
