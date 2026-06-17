---
doc_kind: adr
adr_id: 0109
title: '`list.append(v)` / `list.pop()` mutable methods — in-place grow/shrink, Copy-scalar element types (int/float), owned-element deferred'
status: accepted
date: 2026-06-18
last_verified_commit: a8836f75
supersedes: []
superseded_by: []
---

# ADR-0109: `xs.append(v)` / `xs.pop()` — in-place list mutation

## Context

Finding **F96** (§2.5 LLM-first): `xs.append(v)` and `xs.pop()` both
REJECTED at build (exit 2, `method `append`/`pop` not found on `list``).
Mutable lists are CORE Python — `xs.append(x)` appears in almost every
Python program (accumulate-in-a-loop, build-then-return, stack/queue),
and `xs.pop()` is the canonical "remove + use the last element". Their
absence was a constant first-try failure, directly against §2.5's
*Maximize-overlap-with-training-data*. This was an ADDITIVE gap (a clean
reject, NOT a silent miscompile), so the fix adds the two methods.

Python semantics (the CPython oracle this differential-tests against):

| op | behaviour |
|---|---|
| `xs.append(v)` | mutates `xs` IN PLACE (grows by 1); returns `None` |
| `xs.pop()` | removes + RETURNS the LAST element; mutates `xs` (shrinks by 1) |
| `[].pop()` | raises `IndexError` → Cobrust TRAPS (exit 3, §2.2 — not a sentinel) |

`pop(i)` (indexed), `insert`, `remove`, `extend`, and the in-place
`list.sort()` are OUT OF SCOPE (deferred follow-ups, see below).

The list C-ABI is **i64-SLOT**: every element occupies one `i64` slot —
an `int` is the raw value, a `float` is its IEEE-754 bit pattern
(`f64::to_bits`), a `str`/`list` is a host pointer cast to `i64`. This is
the same encoding `__cobrust_list_set` / `_get` / the `min`/`max`/`sum`/
`sorted` reducers already use.

## Options considered

1. **Add `append`/`pop` for Copy-scalar element lists (`int`/`float`/
   `bool`) only; REJECT owned-element lists (`str`/`list`/`dict`/`set`/
   `bytes`) at type-check with a §2.5-B fix-printing message; defer the
   ownership-transfer follow-up.** — CHOSEN.
2. Add `append`/`pop` for ALL element types in one increment, including
   the owned-element ownership transfer (append MOVES the operand INTO the
   list's `drop_elems`; pop transfers ownership OUT to the receiving
   binding). — Rejected for THIS increment: the double-free / leak surface
   is subtle (the list's `drop_elems` must see exactly the live elements;
   a popped owned element must be excluded from the list's drop yet owned
   by the binding). Splitting it out keeps F96 a clean, fully-verified
   increment and isolates the ownership work for its own audit.
3. Reuse the existing (codegen-orphaned) `push` method name instead of the
   Python-canonical `append`. — Rejected: §2.5 *training-data-overlap* —
   an LLM writes `xs.append(x)`, not `xs.push(x)`. `push` stays as a Rust
   alias.

## Decision

Add the two Python-canonical methods on `list[T]`:

- `xs.append(v: T) -> None` — `v` unifies with the element type `T` (§2.2:
  no appending a wrong-typed element). In-place grow.
- `xs.pop() -> T` — no-arg form only; removes + returns the last element.

**Element-type scope**: `T ∈ {int, float, bool}` (Copy scalars — no
ownership transfer). `T ∈ {str, list, dict, set, bytes}` is REJECTED at
type-check via the new `TypeError::UnsupportedListMutate` (§2.5-B: the
message PRINTS THE FIX — use a Copy-scalar element list, or rebuild via a
comprehension). This is a clean exit-2 reject, NOT a miscompile.

### Runtime (`cobrust-stdlib/src/collections.rs`)

- `__cobrust_list_append(list, v: i64)` — ALREADY existed (comprehension
  desugaring). In-place grow with doubling capacity.
- `__cobrust_list_append_float(list, v: f64)` — NEW; re-encodes the f64 to
  the i64 slot (`to_ne_bytes`) then delegates to `_append`. The `_float`
  twin mirrors the `min`/`sum`/`sort` `_int`/`_float` ABI so a NATIVE f64
  operand passes through a real `f64` register (no codegen-side bitcast).
- `__cobrust_list_pop(list) -> i64` — NEW; reads the last slot, shrinks
  `len` by 1 (capacity retained), returns the slot. Empty/null TRAPS via
  `crate::panic::panic("pop from empty list")` → exit 3 (NOT a raw
  `assert!` — the F79B/F92 lesson: a raw panic aborts SIGABRT with a
  path-leaking backtrace across the `extern "C"` boundary).
- `__cobrust_list_pop_float(list) -> f64` — NEW; decodes the popped slot
  back to f64 (`from_ne_bytes`).

### Typing (`cobrust-types/src/check.rs::try_synth_list_method`)

`append`/`pop` arms beside the existing `push`/`get`/`set`/`len`/
`is_empty`. `append` arity 1 + unify arg with `elem` → `Ty::None`; `pop`
arity 0 → `elem`. Both call `reject_owned_elem_mutate` (the
owned-element §2.5-B reject helper).

### Lowering + intrinsics

- `method_form_rewrite_name` (MIR `lower.rs`): `append → list_append`,
  `pop → list_pop` (NEW PRELUDE stubs `list_append`/`list_pop`).
- `lower_rewritten_method_call`: `list_pop`'s `_callret` return-type
  override re-pins the dest to the receiver's ELEMENT type (the monomorphic
  PRELUDE stub is `-> i64`; `list[float].pop()` must produce an f64 dest so
  the f64 runtime symbol is picked AND the dest alloca is an f64 register).
  The parallel of the `min`/`max`/`sum` reducer return-type override.
- The intrinsic-rewrite pass (`cobrust-cli/build/intrinsics.rs`,
  `Kind::ListAppend` / `Kind::ListPop`): picks the `_int` / `_float`
  runtime symbol — append from the VALUE arg's element type, pop from the
  call's DEST (element) type (the ADR-0089 abs-miscompile-proof source of
  truth).
- Codegen (`llvm_backend.rs`): declares `__cobrust_list_append_float`
  (`(ptr, f64) -> void`), `__cobrust_list_pop` (`(ptr) -> i64`),
  `__cobrust_list_pop_float` (`(ptr) -> f64`).

### Ownership / mutation persistence

Lists are operand-`is_copy_type` (passed by pointer; drop-eligible at the
DROP level). The receiver `xs` is read as `Operand::Copy(handle)` — NOT
moved — so append/pop mutate through the SAME live handle and `xs` stays
usable after the call, dropping exactly ONCE at scope exit. For the
Copy-scalar element types this increment supports there is NO
element-ownership transfer to get wrong.

## Consequences

- `xs.append(v)` / `xs.pop()` work end-to-end for `list[int]` / `list[float]`.
- `list[str]` / owned-element append/pop is a clean exit-2 reject (the
  follow-up is scoped + audited separately).
- New `TypeError::UnsupportedListMutate` variant. Full match-site set:
  `error.rs`, `fix_safety.rs` ×2, `lsp/diagnostic.rs`, `error_ux.rs`,
  `types-parity/lib.rs` ×2, AND the `.cb`-mirror crate `cobrust-types-cb`
  (`error_cb.rs` variant + `from_rust` + `Display` + `canonicalize` +
  variant-name; `fix_safety_cb.rs`; the `error_display_parity` byte-parity
  test). The initial F96 commit `6a5c20ad` MISSED the `cobrust-types-cb`
  mirror, so its exhaustive `type_error_cb_from_rust` match failed to
  compile (`error[E0004]`) and broke the WHOLE-workspace build/test; the
  cascade-completion repair (see finding F96) closes it. Lesson
  (sibling F83/F92): a new `TypeError` variant MUST be mirrored in
  `cobrust-types-cb`, and only `cargo build/clippy --workspace` catches a
  missed mirror — `-p cobrust-cli` is green-blind.
- `pop` on an empty list is a clean exit-3 TRAP (§2.2 — CPython
  `IndexError` parity, never a silent sentinel `0`).

## Evidence

- `cobrust-cli/tests/list_mutate_e2e.rs` — 12 e2es (CPython oracle): int
  append-grows, pop-returns-last, append-in-loop, pop-to-empty,
  empty-pop-traps (exit 3), pop-after-emptying-traps, float append/pop,
  pop-result-in-arithmetic, interleaved-coherent, str append/pop reject
  (exit 2), wrong-type append reject.
- `cobrust-stdlib/src/collections.rs` — `cabi_list_append_pop_int` +
  `cabi_list_append_pop_float` unit tests.
- Regression: `list_reduce_e2e` (14) / `list_slice_e2e` (31) / `list_str_e2e` /
  `leetcode_corpus_e2e` (12) green; stdlib (333) + types/mir/codegen/parity
  libs green; touched-crate clippy `-D warnings` clean.

## Follow-ups

- Owned-element `list[str]` / `list[list]` append/pop (ownership transfer).
- `pop(i)` (indexed pop, Python negative-index normalized).
- `insert(i, v)`, `remove(v)`, `extend(other)`, in-place `list.sort()`.
