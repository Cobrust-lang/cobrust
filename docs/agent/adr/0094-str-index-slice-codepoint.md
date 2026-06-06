---
doc_kind: adr
adr_id: 0094
title: "`str` index OPERATOR — `s[i]` / `s[lo:hi]` codepoint-addressed (F78 silent-miscompile closure, the `bytes` Phase-2 slice mirror)"
status: accepted
date: 2026-06-06
last_verified_commit: 8b33946
supersedes: []
superseded_by: []
relates_to: [adr:0093, finding:f78, "claude.md:§2.2", "claude.md:§2.5"]
---

# ADR-0094: `str` index OPERATOR — `s[i]` / `s[lo:hi]` codepoint-addressed

## Context

The `str` **index operator** silently miscompiled to the WHOLE base
string — a CLAUDE.md §2.2 silent-miscompile in one of the most common
ops (finding F78, verified at HEAD `5248d8f`, re-verified `8b33946`):

```
print("hello"[1:4])      # built + ran exit 0, printed  hello  (CPython "ell")
print("hello"[1])        # built + ran exit 0, printed  hello  (CPython "e")
len("hello"[1:4])        # use-of-moved-value COMPILE ERROR
```

The slice form returned the wrong value at exit 0 with NO diagnostic (the
dangerous case); the value-move contexts hit an unrelated borrow error.

**Root cause.** The generic `ExprKind::Index` lowering fell through to the
`Projection::Index` codegen no-op for `Ty::Str`. There was no
`__cobrust_str_slice` / `__cobrust_str_char_at` runtime at all — the
`IndexKind::Slice` collapsed to `Constant::Int(0)` (the generic
`lower_index` scalar-only contract) and the projection was a codegen
pass-through, so the expression evaluated to the base operand. The type
checker's catch-all `(other, IndexKind::Slice { .. }) => Ok(other.clone())`
typed `str[1:4]` as `str` and waved it through.

This is the SAME generic-slice fall-through ADR-0093 §2 just fixed for
`bytes` (`__cobrust_bytes_slice` + a dedicated MIR `Slice` arm +
`TypeError::UnsupportedSliceShape` for the non-`lo:hi` shapes). `str`
never got that treatment. F78 is orthogonal to ADR-0093 — it PREDATES it;
the `bytes` work merely surfaced it during the adversarial audit.

### Pre-existing surface inventory (what already worked)

- `Ty::Str` exists; the `b"..."`/`"..."` literals, `len(s)`, `str + str`,
  `str == str`, and the `str.method()` family all have runtime.
- The type checker ALREADY declares the index TYPE contract:
  `str[i] -> str` (`check.rs:2254`, a 1-codepoint string).
- A legacy `__cobrust_str_at(s, i) -> str` shim exists (`io.rs:541`) for
  the source-level `str_at(s, i)` PRELUDE FUNCTION — but it is **byte-
  based** (`s[idx..=idx]` where `idx = i as usize` is a byte offset) and
  is NOT wired into the `s[i]` index OPERATOR. It would PANIC on a
  multi-byte UTF-8 boundary; the LC-100 corpus uses it on ASCII input
  where byte == codepoint, so the wart was latent.

## The load-bearing decision: CODEPOINT, not byte

**`str` slicing + scalar indexing is CODEPOINT-based (Unicode scalar
values), not byte-based.** This is the one design decision that differs
from `bytes` (where every byte is independent and byte-addressing is the
only sensible unit).

### Why codepoint

1. **Python parity + §2.5 maximize-overlap-with-training-data.** CPython
   `str[i]` / `str[i:j]` index by Unicode scalar; `"héllo"[1] == "é"` and
   `"héllo"[1:3] == "él"`. An LLM writes `s[i]` from its Python priors
   expecting codepoint semantics. Byte semantics would be a silent
   divergence on every non-ASCII string — exactly the §2.5 surprise the
   constitution forbids.
2. **The TYPE contract already says codepoint.** `check.rs` types
   `str[i] -> str` (a 1-codepoint string), NOT `-> int` (a byte). A
   byte-based implementation would contradict the declared surface.
3. **§2.2 no-silent-corruption.** A byte-based slice can cut the middle of
   a multi-byte UTF-8 codepoint, yielding INVALID UTF-8 — a §2.2
   violation (the `StringBuffer` invariant is "always valid UTF-8"). A
   codepoint-based slice CANNOT: a boundary always lands on a `char`
   boundary by construction, so the result is ALWAYS valid UTF-8. **No
   mid-codepoint cut is representable, so no snap-to-boundary and no trap
   is needed** for the supported `lo:hi` form. (Had we chosen byte
   semantics, §2.2 would force snap-or-trap on a mid-codepoint cut;
   codepoint semantics make the question moot.)
4. **Consistency mandate.** `s[i]` and `s[lo:hi]` MUST use the same unit.
   Since the type contract fixes the scalar at codepoint (`-> str`,
   1-codepoint), the slice must match.

### Consequence: the scalar `s[i]` operator is fixed too

F78 is scoped to the slice, but `s[i]` scalar was the SAME silent
whole-string miscompile (it also fell through the generic no-op). Leaving
`s[i]` returning the whole string while `s[lo:hi]` is correct would be a
fresh §2.2 hole + violate the consistency mandate. So this ADR fixes BOTH
in one MIR arm (mirroring the `bytes` arm, which handles both scalar and
slice). The new `s[i]` operator routes to a CODEPOINT runtime
(`__cobrust_str_char_at`), NOT the legacy byte-based `str_at()` function
(left untouched so its LC-100 ASCII callers are unaffected).

### Cost note

Codepoint addressing is O(n) per index (a `char_indices()` walk), like
CPython's own non-O(1) handling of variable-width encodings at the C
level for non-ASCII. This is the Python-faithful cost; an O(1) ASCII
fast-path or a grapheme tier is a §Phasing follow-up if a benchmark
demands it. Token cost / micro-perf is not a constraint here
(constitution §1.1); correctness + Python parity is.

## Decision

Mint the `str` index OPERATOR runtime + wire it through MIR + the type
checker, MIRRORING the ADR-0093 §2 `bytes` slice machinery but
codepoint-addressed.

| `.cb` source | type | runtime symbol | unit |
|---|---|---|---|
| `s[i]` | `str` (1 codepoint) | `__cobrust_str_char_at(s, i)` | CODEPOINT |
| `s[lo:hi]` | `str` | `__cobrust_str_slice(s, lo, hi)` | CODEPOINT range `[lo, hi)` |
| `s[1:]` / `s[:3]` / `s[0:4:2]` / `s[1:-1]` | — | REJECT | `TypeError::UnsupportedSliceShape` |

### 1. Runtime (`cobrust-stdlib/src/string.rs`)

- `__cobrust_str_char_at(s, i) -> *mut Str` — the i-th codepoint
  (`s.chars().nth(i)`) as a fresh 1-codepoint `str`. OOB (`i < 0` or
  `i >= n_chars`) / NULL → fresh empty `str` (sentinel).
- `__cobrust_str_slice(s, lo, hi) -> *mut Str` — the codepoint range
  `[lo, hi)`, mapped to byte offsets via `char_indices()`, as a fresh
  `str`. **Python clamp** on OOB (`"hello"[1:99] == "ello"`,
  `"hello"[3:1] == ""`); negative bounds are rejected upstream (never
  reach the runtime on the accepted-program path). NULL → fresh empty.

Both MINT a fresh owned `str` the `.cb` scope drops EXACTLY ONCE via
`__cobrust_str_drop`; the input `s` is BORROWED (read-only via
`str_buf_as_str_local`) — the SAME mint-fresh / borrow-input discipline
`__cobrust_str_concat` / `__cobrust_bytes_slice` run (ADR-0050c).

### 2. MIR (`cobrust-mir/src/lower.rs`)

A `Ty::Str` arm in the `ExprKind::Index` lowering, beside the
`Dict`/`List`/`Bytes` arms (the `bytes` split, mirrored):

- SLICE `IndexKind::Slice { start: Some, stop: Some, step: None, non-neg }`
  → `__cobrust_str_slice`, base BORROWED (Move→Copy upgrade), result a
  fresh `str` MOVED out (dropped once by the consuming binding — a Copy
  would double-free).
- SCALAR `IndexKind::Expr` → `__cobrust_str_char_at`, base BORROWED,
  result a fresh 1-codepoint `str` MOVED out (UNLIKE the `bytes` scalar
  `b[i] -> int`, a Copy scalar — `str`'s scalar is an owned heap value).
- An unsupported SLICE shape reaching MIR is a hard `MirError`
  (defense-in-depth, constitution §6) — it MUST have been rejected
  upstream; NEVER the silent whole-string fall-through F78 documents.

### 3. Type checker (`cobrust-types/src/check.rs`)

- The scalar arm `(Ty::Str, IndexKind::Expr) -> Ty::Str` already existed.
- A new `(Ty::Str, IndexKind::Slice { .. })` arm, a byte-for-byte mirror
  of the ADR-0093 `bytes` slice arm: type-check each present bound as
  `Int`, then gate on shape. The contiguous `lo:hi` with both
  non-negative bounds + default step → `Ty::Str`; EVERY other shape →
  `TypeError::UnsupportedSliceShape` (§2.5-A compile-time-catch) instead
  of the old silent `Ok(other.clone())` catch-all.

### 4. `UnsupportedSliceShape` extended to `Ty::Str` — no new cascade

`TypeError::UnsupportedSliceShape { span, suggestion }` (ADR-0093) is
payload-free (Span + suggestion, no `Ty`), so extending its TRIGGER to
`Ty::Str` needs NO new variant + NO new cascade wiring — the full
cascade (`error.rs` Display / `error_cb.rs` / `fix_safety{,_cb}` /
`error_ux` / `lsp/diagnostic` / `types-parity`) already routes it. The
per-site `suggestion` carries the TYPE-SPECIFIC §2.5-B fix
(`s[1:len(s)]` for `str`, `b[1:len(b)]` for `bytes`).

The ONE cascade touch: `error_ux.rs`'s CLI `msg` was bytes-specific
("unsupported `bytes` slice shape …"). It is generalized to a
shape-agnostic line; the per-site `suggestion` (`hint`) now carries the
type-specific form. This is SAFE for the byte-parity tripwire because:

- The `#[error(...)]` Display in `error.rs` + `error_cb.rs` is UNCHANGED
  (the types-parity `error_display_parity` test diff-tests rendered
  Display text — it stays byte-identical, still asserts
  `"unsupported \`bytes\` slice shape"`).
- The `bytes` e2e (`bytes_ops_e2e_08`) asserts `stderr.contains(
  "b[1:len(b)]")`, which flows through the unchanged `bytes` suggestion.

(error_ux's CLI `msg` is the CLI's own rendering and is NOT parity-tested;
generalizing it is the §2.5-B-honest path so a `str` rejection does not
print "bytes".)

### 5. Codegen (`cobrust-codegen/src/llvm_backend.rs`)

Two externs registered beside `__cobrust_str_at`:
`__cobrust_str_char_at(*mut Str, i64) -> *mut Str` and
`__cobrust_str_slice(*mut Str, i64, i64) -> *mut Str`. Symbols live in
`libcobrust_stdlib.a` (already-linked).

## Consequences

- `"hello"[1:4] == "ell"`, `"hello"[1] == "e"`, `len("hello"[1:4]) == 3`,
  `"héllo"[1:3] == "él"` — all match CPython 3 (verified e2e +
  differential). The F78 §2.2 silent-miscompile is closed for BOTH index
  shapes.
- A `str` slice NEVER produces invalid UTF-8 (codepoint boundaries are
  total) and NEVER silently miscompiles ANY shape: `lo:hi` is correct;
  open/stepped/negative REJECT at `cobrust check`.
- The minted slice/scalar `str` drops EXACTLY ONCE; the base is BORROWED
  (a 1000-iteration drop-hammer + a base-survives-N-reads e2e prove no
  double-free / leak).
- The legacy byte-based `str_at()` FUNCTION is untouched (its LC-100 ASCII
  callers unaffected); the `s[i]` OPERATOR supersedes it with codepoint
  semantics for new code.

## Phasing (deferrals, named)

- Open-ended (`s[1:]` / `s[:3]` / `s[:]`), non-unit step (`s[0:4:2]`),
  and negative-bound (`s[-1]` / `s[1:-1]`) slices REJECT today. Wiring
  them is a follow-up (the open-end + negative needs a `len`-relative
  lowering; step needs a strided minter).
- An explicit OOB-PANIC for the scalar `s[i]` (vs. today's empty-string
  sentinel, matching `str_at` / `bytes_get`) is deferred.
- An ASCII O(1) fast-path / grapheme-cluster tier is deferred (no
  benchmark demands it yet).

## Evidence

- e2e: `crates/cobrust-cli/tests/str_slice_e2e.rs` (6 tests: F78 slice
  fix, scalar index, the codepoint case, the 4 unsupported-shape
  rejects, a 200-iter drop hammer, base-borrowed-survives). All assert
  against the CPython-3 oracle.
- stdlib unit: `cobrust-stdlib/src/string.rs` `#[cfg(test)]` (6 tests:
  ascii clamp, codepoint-not-byte, never-invalid-UTF-8 over a
  4-byte-codepoint string, char_at, null/empty, drop-hammer).
- regression GREEN: full `str` corpus (`list_str_e2e` /
  `method_call_e2e` / `str_methods_py_e2e` / `string_stdlib_e2e`),
  `bytes_ops_e2e` + `bytes_primitive_e2e` (the `bytes` slice unbroken),
  `cobrust-types`, `cobrust-types-cb` + `cobrust-types-parity` (the
  byte-parity cascade tripwire), `cobrust-stdlib` lib,
  `intrinsics_input` (LC-100), `lc100_stress_e2e_b1`,
  `borrow_phase_g_e2e` (the `str_at` consumers).
