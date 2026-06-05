---
doc_kind: adr
adr_id: 0091
title: "`coil.array([list])` — the `np.array` bridge (list-data → Buffer, element-dtype dispatch)"
status: accepted
date: 2026-06-05
last_verified_commit: 84d2529
supersedes: []
superseded_by: []
---

# ADR-0091: `coil.array([list])` — the `np.array` bridge (list-data → Buffer)

## Context

`coil.array([...])` is the FUNDAMENTAL numpy constructor `np.array([...])`
— the single most-reached-for numpy entry point a numpy-trained LLM writes
(§2.5 maximize-overlap-with-training-data). It is the BRIDGE from real
`.cb` list data to a coil `Buffer`: the `parse → list → array → stats`
pipeline's missing first hop. Until now coil could build a `Buffer` only
from scalar args (`coil.zeros(n)`, `coil.arange(n)` — ADR-0072, the
#numpy BATCH-20 row) or from another `Buffer`; there was no way to lift a
`.cb` `list[int]` / `list[float]` into a `Buffer` and then reduce it
(`coil.mean` / `coil.std`).

`coil.array` is the FIRST coil constructor that **CONSUMES** a Cobrust
`list[T]` argument. This directly REUSES the **list-consume (borrow-read)
mechanism** ADR-0090 established for `min` / `max` / `sum` (which read a
borrowed `list[T]` to a scalar). ADR-0090 §"The list-consume mechanism"
explicitly named `coil.array` as a planned reuse site.

### The op (numpy 2.x, oracle `python3.11` numpy)

- `coil.array([1, 2, 3])` → `array([1, 2, 3], dtype=int64)`
  (`np.array([1,2,3]).dtype == int64`).
- `coil.array([1.0, 2.5])` → a `float64` Buffer
  (`np.array([1.0,2.5]).dtype == float64`).
- `coil.array([])` → `array([], dtype=float64)` — numpy's empty-list
  default dtype is **float64** (`np.array([]).dtype == float64`); an
  annotated empty `list[int]` → an empty `int64` Buffer (matching the
  static element type). NOT a trap.
- CHAIN: `coil.mean(coil.array([1, 2, 3, 4])) == 2.5` — the produced
  Buffer flows into coil reductions (the parse → array → stats payoff).

The NESTED 2-D form `coil.array([[1, 2], [3, 4]])` (a `list[list[T]]`) is
a **documented DEFERRAL**: it needs a recursive list read (a
`list[list[i64]]` is a list of list pointers); this ADR ships the **1-D
form**.

### Two reuses (do NOT reinvent)

1. **The ADR-0090 list-consume (borrow-read).** A `.cb` list passes to a
   callee by POINTER: `is_copy_type(Ty::List(_))` returns `true`
   (`cobrust-mir/src/lower.rs`), so the list operand is **Copy-at-call**
   and the `.cb` scope retains ownership + drops it ONCE at scope exit.
   The two `coil.array` shims **BORROW** the list via the stdlib
   `__cobrust_list_len` / `__cobrust_list_get` SHARED-reference accessors
   (resolved from `libcobrust_stdlib.a` at `.cb`-link time) — they NEVER
   `Box::from_raw` / free, EXACTLY like `__cobrust_min_int`
   (`cobrust-stdlib/src/reduce.rs`). A shim-free + scope-drop double-free
   would be a use-after-free; the borrow is LOCKED by a `coil.array(xs)`
   … `len(xs)` program that exits 0 (the list dropped exactly once).

2. **The ADR-0089/0090 element-type dispatch.** `np.array(<int list>)` is
   `int64`, `np.array(<float list>)` is `float64`. The dispatch keys on
   the list arg's STATIC element type (the type-checker's resolved
   `Ty::List(elem)`, read at MIR via `synth_expr_ty` — NOT a fragile
   arg-temp read). This is the **ADR-0089 abs-miscompile-proof** dispatch:
   a list BUILT then passed (`coil.array(make_ints())`) has a fine
   `Ty::List` synth type, so it routes by its real element type even
   though its MIR-temp bookkeeping is incidental.

## Options considered

1. **Express `coil.array` as a normal manifest `EcoSig` with a concrete
   `EcoParam::Value(Ty::List(...))` arg.** Rejected — an `EcoParam::Value`
   carries ONE concrete arg type, so it cannot accept BOTH `list[int]` and
   `list[float]`, and the EcoSig's single `runtime_symbol` cannot dispatch
   int-vs-float. The element-dtype polymorphism needs a special-case.

2. **Special-case `("coil","array")` in the type-checker
   (`try_synth_ecosystem_call`, BEFORE `check_eco_sig`) + dispatch the
   int/float shim at MIR-lowering time on the list arg's resolved element
   type; borrow-read in the runtime shims.** Mirrors ADR-0090's
   `try_synth_reduce_builtin` (which reads `Ty::List(elem)` for
   `min`/`max`/`sum`). The return type is the UNIFORM `coil_buffer_ty()`
   (the dtype is a RUNTIME Buffer property, NOT a static type — SIMPLER
   than ADR-0090 where the scalar return differed Int vs Float); the
   int-vs-float choice lives ONLY in the MIR shim-symbol pick + the runtime
   shim. **Chosen.**

3. **Require a method form `xs.to_array()` / a `coil.array(xs, dtype=...)`
   kwarg.** Violates §2.5 (maximize-overlap-with-training-data) — numpy
   writes `np.array([...])`. The `dtype=` kwarg is a deferral. Rejected.

## Decision

Adopt option 2. Five layers.

### 1. Runtime shims (`crates/cobrust-coil/src/cabi.rs`)

Two `extern "C"` symbols, ONE ABI shape `(list: *mut u8) -> *mut u8` (a
borrowed list pointer in, a fresh Boxed `Buffer` out):

- `__cobrust_coil_array_int` — reads `__cobrust_list_len` + each
  `__cobrust_list_get` i64 slot DIRECTLY (int elements are raw i64 slots)
  into a `Vec<i64>`, builds `Array::Int64` via the EXISTING
  `constructors::array_i64(&data, &[len])` kernel.
- `__cobrust_coil_array_float` — reads each i64 slot and reinterprets it
  as the stored `f64` bit-pattern (`f64::from_bits(slot as u64)` — the
  ADR-0090 float-list slot convention) into a `Vec<f64>`, builds
  `Array::Float64` via `constructors::array_f64(&data, &[len])`.

Both BORROW the list (no `Box::from_raw`, no free). Both return
`Box::into_raw(Box::new(arr))` — the `.cb` scope drops the Buffer ONCE via
`__cobrust_coil_buffer_drop` (every coil constructor's ownership shape).
**Empty / null** list → an empty Buffer of the shim's dtype (NOT a trap);
a null list reads as length 0 (the stdlib `__cobrust_list_len` tolerates
null). The two stdlib accessors are declared in cabi's existing
`unsafe extern "C"` block (alongside `__cobrust_list_new` / `_set` /
`_panic` / `_str_*`).

### 2. The shims ARE the cabi

No separate ABI shim layer — the `extern "C"` array builders are the C-ABI
the codegen-emitted call lands on directly (same as ADR-0090's reducers).

### 3. Type-checker (`crates/cobrust-types/src/{ecosystem.rs,check.rs}`)

- `ecosystem.rs`: a `("coil","array")` `EcoSig` row exists ONLY so
  `lookup_module_fn` returns `Some` (the special-case reads the real arg).
  Its `runtime_symbol` is the float shim (the MIR override supplies the
  int shim); its `param` is a sentinel `list[float]` (unused — the
  special-case owns the arg-check); ret `coil_buffer_ty()`; tier
  `Semantic`.
- `check.rs`: `synth_coil_array` fires from `try_synth_ecosystem_call`
  Case 1 when `module == "coil" && name == "array"`, BEFORE
  `check_eco_sig`. It resolves the single positional arg (unwrapping one
  `Ref` so `coil.array(&xs)` works) and returns:
  - **`list[int]`** / **`list[float]`** → `coil_buffer_ty()`.
  - **unresolved elem var** (un-annotated `coil.array([])`) → unify the
    elem against `Float` (numpy's empty-list default dtype is float64),
    return `coil_buffer_ty()` (the float shim builds an empty Float64
    Buffer).
  - **`list[other]`** (`coil.array(["a"])`, OR the DEFERRED nested
    `list[list]`) → unify the elem against `Float`, raising the canonical
    `TypeMismatch` (NO new variant).
  - **non-list arg** (`coil.array(5)`) → the canonical `NotIterable`
    (an EXISTING variant — no error-renderer cascade), with a §2.5-B fix
    suggestion.
  - **wrong arity** (`coil.array()`, `coil.array(a, b)`, a `dtype=` kwarg)
    → the canonical `ArityMismatch` (the special-case owns the whole
    `coil.array` arg-check).

### 4. MIR (`crates/cobrust-mir/src/lower.rs`)

The list arg passes UNCHANGED (Copy-at-call, `is_copy_type`). At the
Case-1 ecosystem-call site, when `rn.name == "coil" && name == "array"`,
the runtime symbol is chosen from the SINGLE list arg's element type
(`synth_expr_ty` → `Ty::List(Ty::Int)` ⇒ `__cobrust_coil_array_int`, else
`__cobrust_coil_array_float`) — NOT the EcoSig's fixed `runtime_symbol`. An
empty / unresolved elem stays on the FLOAT shim (matching the
type-checker's `synth_coil_array` anchor + numpy's empty default). This is
the ADR-0089 lesson: dispatch on the resolved element type, NOT the arg's
MIR temp, so `coil.array(make_ints())` routes by its real int element type.

### 5. Codegen (`crates/cobrust-codegen/src/llvm_backend.rs`)

The two shims are declared alongside `__cobrust_coil_arange`, with a NEW
`coil_listconsume_ty = (ptr) -> ptr` fn-type (no prior coil free-function
in that block declares a `(ptr) -> ptr` shape). No `lower_*` type-switch —
the MIR retarget hands codegen a concrete `Constant::Str` symbol.

### Deferred (documented, out of scope)

- **The NESTED 2-D form** `coil.array([[1, 2], [3, 4]])` (`list[list[T]]`)
  — needs a recursive list read; a `Ty::List(Ty::List(_))` elem raises
  `TypeMismatch` here. The 1-D form ships.
- **The `dtype=` kwarg** `coil.array([1, 2], dtype="float64")` — defers to
  the canonical `ArityMismatch`; `coil.astype` (ADR-0072 BATCH-19) covers
  post-hoc dtype conversion.
- **Non-int/non-float element lists** (`list[str]`, `list[bool]`) — only
  `list[int]` and `list[float]` are wired (the i64-slot-storage element
  types).

## Consequences

- **Positive**
  - `coil.array([...])` type-checks + lowers + runs for `list[int]`
    (→ int64 Buffer) and `list[float]` (→ float64 Buffer) — the §2.5
    first-try win for the single most-reached-for numpy constructor.
  - The `parse → list → array → stats` pipeline is now closed: a `.cb`
    `list` lifts into a `Buffer` and flows into `coil.mean` / `coil.std`
    (the CHAIN e2e proves `coil.mean(coil.array([1,2,3,4])) == 2.5`).
  - REUSES the ADR-0090 list-consume (borrow-read) ABI VERBATIM — the same
    `__cobrust_list_len` / `_get` shared-ref accessors `__cobrust_min_int`
    uses; the borrow is locked by a list-reused-after-`array` e2e (exit 0).
  - The element-dtype dispatch keys on the resolved element type, immune to
    the ADR-0089 computed-arg miscompile (locked by a
    `coil.array(make_ints())` e2e).
  - NO new error variant — `coil.array(5)` reuses `NotIterable`,
    `coil.array(["a"])` reuses `TypeMismatch`, a wrong arity reuses
    `ArityMismatch`, so the ADR-0088 error-renderer cascade needed NO
    changes.
- **Negative**
  - The `coil.array` manifest row's `runtime_symbol` + `param` are a
    sentinel (the special-case + MIR override own the real behavior) — a
    small "the EcoSig row is not the source of truth for this one op"
    wrinkle, documented inline in both `ecosystem.rs` and `check.rs`.
- **Neutral / unknown**
  - cobrust prints whole floats without the trailing `.0`
    (`coil.array([1.0, 2.5])` → `array([1, 2.5], dtype=float64)`,
    Python prints `[1. , 2.5]`). A pre-existing float-format difference
    (ADR-0089), unrelated to this ADR — the dtype + values are
    numpy-exact.

## Evidence

- Repro (pre-fix): `coil.array([1, 2, 3])` →
  `UnknownName { name: "coil.array", ... }`.
- Post-fix e2e (`crates/cobrust-cli/tests/coil_array_e2e.rs`, REAL
  compile → link → spawn, 11/11 green): int-list → int64
  (`array([1, 2, 3], dtype=int64)`) via a binding AND an inline literal,
  float-list → float64 (`dtype=float64` + the fractional `2.5` preserved),
  the COMPUTED-list dispatch (`coil.array(make_ints())` → int64, the
  ADR-0089 proof), empty `coil.array([])` → `array([], dtype=float64)`,
  empty `list[i64]` → `array([], dtype=int64)`, the CHAIN
  `coil.mean(coil.array([1,2,3,4])) == 2.5` (int) +
  `coil.mean(coil.array([1.5,2.5,3.5])) == 2.5` (float), the BORROW lock
  (`len(xs)` after `coil.array(xs)`, exit 0), and the negatives
  (`coil.array(5)` → `NotIterable`, `coil.array(["a","b"])` →
  `TypeMismatch`).
- cabi unit tests (`cabi.rs`, 5/5 green, via a `FakeList` test stub for the
  stdlib list ABI): int → int64 + the BORROW-not-free check (list reused
  after the shim, only the result Buffer drops), float → float64
  (from_bits slots), empty int → empty int64, empty float → empty float64,
  null → empty.
- Differential oracle (`python3.11` numpy 2.x): `np.array([1,2,3])` =
  `[1,2,3]` dtype `int64`; `np.array([1.0,2.5])` = `[1. ,2.5]` dtype
  `float64`; `np.array([])` = `array([], dtype=float64)`;
  `np.array([], dtype=int64)` dtype `int64`; `np.mean(np.array([1,2,3,4]))`
  = `2.5` (an f64).
- Regression (all green): ADR-0090 `list_reduce_e2e` (14),
  `len_polymorphic_e2e` (7), `builtins_abs_range_e2e` (12), LC-100
  `intrinsics_input` (101), `coil_hello_e2e` (3) + `coil_construct_e2e`
  (10) + `coil_arange_e2e` (7) + `coil_stats_e2e` (4), `list_str_e2e` (31),
  `well_typed` (295), `ill_typed` (194). cobrust-coil lib 517, cobrust-
  types lib 155.
