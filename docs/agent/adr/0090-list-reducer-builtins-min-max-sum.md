---
doc_kind: adr
adr_id: 0090
title: List-consume (borrow-read) mechanism + `min` / `max` / `sum` list-reducer builtins
status: accepted
date: 2026-06-05
last_verified_commit: b796fa4
supersedes: []
superseded_by: []
---

# ADR-0090: List-consume (borrow-read) mechanism + `min` / `max` / `sum` list-reducer builtins

## Context

Three of the most-used Python builtins a Python-trained LLM reaches for
were `unknown name` in Cobrust — every §2.5 (LLM-first) first-try miss,
in the spirit of ADR-0088 (`len(x)`) and ADR-0089 (`abs(x)` /
`range(stop)`):

- **`min(xs)` / `max(xs)`** — the smallest / largest ELEMENT of a list.
  `min([3, 1, 2]) == 1`, `max([3, 1, 2]) == 3`.
- **`sum(xs)`** — the sum. `sum([1, 2, 3]) == 6`, `sum([]) == 0`.

These are the first builtins that **CONSUME** (read-reduce) a whole
`list[T]` *argument*. Every prior list runtime shim
(`__cobrust_list_get` / `_set` / `_len` in
`crates/cobrust-stdlib/src/collections.rs`) is a per-element accessor;
`min`/`max`/`sum` reduce the entire list to a scalar. This ADR
establishes the **list-consume (borrow-read) mechanism** — the ABI by
which a builtin reads a list argument without owning it — which the
deferred `sorted(xs)` and `coil.array` (`np.array`) ops will reuse.

### The list-consume mechanism (the unlock)

A list is ALREADY passed to a callee by POINTER: `is_copy_type` returns
`true` for `Ty::List(_)` (`crates/cobrust-mir/src/lower.rs:495`), so a
list operand is **Copy-at-call** and the `.cb` scope retains ownership +
drops it once at scope exit (the same discipline that lets
`list_set(xs, i, v)` mutate `xs` without consuming it). Therefore
`min(xs)` passes the SAME `*mut u8` the `.cb` local holds to a runtime
shim `__cobrust_min_int(list: *mut u8) -> i64`.

The shim **BORROWS** (reads) the list — exactly like
`__cobrust_list_len` / `__cobrust_list_get`, which dereference
`&*list.cast::<ListI64Layout>()` via a SHARED reference and never
`Box::from_raw`. The reducer:

1. reads the length with `__cobrust_list_len(list)`;
2. reads each slot with `__cobrust_list_get(list, i) -> i64`;
3. (float family) reinterprets each `i64` slot as the stored `f64`
   bit-pattern — a `list[f64]`'s elements are materialised as
   `to_bits()` i64 slots (`lower.rs` lowers a `Constant::Float` to
   `Constant::Float(v.to_bits())`), and the list index-read path
   bitcasts them back.

The shim does **NOT** free the list (no `Box::from_raw`, no drop). A
double-free (shim frees + `.cb` scope drops) would be a use-after-free;
the borrow discipline is locked by a `min(xs)` … `len(xs)` …
`xs[i]` … `sum(xs)`-again program that exits 0 (the list dropped exactly
once).

### Element-type dispatch

Python's `min`/`max` return the smallest/largest ELEMENT, and `sum` the
sum — all of the **element type**. So `min(list[int]) -> int`,
`min(list[float]) -> float`. The type-checker special-case reads the
arg's `Ty::List(elem)` and returns `elem`; the lowering picks the int vs
float runtime shim per the element type.

**The ADR-0089 abs-miscompile lesson applies.** ADR-0089 found that a
builtin dispatching on an *arg's MIR temp type* silently miscompiles for
COMPUTED args (a `Ty::None` binary-op / call-return temp). Here the
dispatch keys on the **call's DEST/return type** (the type-checker's
resolved element type), NOT the list operand's MIR local — so a list
BUILT then passed (`sum(make_floats())`) routes to the float shim, not
the int shim.

## Options considered

1. **Pass the list by value / move it into the reducer.** Wrong ABI —
   Cobrust lists are Copy-at-call (borrow), and a moved list would
   double-free against the `.cb` scope drop. Rejected; the whole point
   is the borrow-read mechanism.

2. **Special-case `min`/`max`/`sum` in the type-checker (like ADR-0088
   `len` / ADR-0089 `abs`), dispatch the int/float shim on the DEST type
   at MIR-rewrite time, borrow-read in the runtime shim.** Mirrors the
   proven `len` / `abs` per-shape pattern. The element type is the
   return type; the empty-list policy is a runtime guard in the shim.
   **Chosen.**

3. **Require a method form `xs.min()` / fold via a comprehension.**
   Violates §2.5 (maximize-overlap-with-training-data) — Python writes
   `min(xs)` / `sum(xs)`. Rejected.

## Decision

Adopt option 2. Five layers:

### 1. Runtime shims (`crates/cobrust-stdlib/src/reduce.rs`, NEW)

Six `extern "C"` symbols, two ABI shapes:

- `__cobrust_min_int` / `_max_int` / `_sum_int` — `(list: *mut u8) -> i64`.
- `__cobrust_min_float` / `_max_float` / `_sum_float` — `(list: *mut u8) -> f64`.

Each reads `__cobrust_list_len` + `__cobrust_list_get` per element
(BORROW — no free). The float family `f64::from_bits(slot as u64)` each
i64 slot. **Empty-list policy:** `min`/`max` of an empty list TRAP via
`crate::panic::panic` (CPython `ValueError: min() arg is an empty
sequence` parity; §2.2 forbids exceptions → a clean `INTERNAL_PANIC`
non-zero exit); `sum([]) == 0` / `0.0` (CPython returns int `0`).
Wired into `crates/cobrust-stdlib/src/lib.rs` as `pub mod reduce`.

### 2. The shims ARE the cabi

No separate ABI shim layer — the `extern "C"` reducers are the C-ABI the
codegen-emitted calls land on directly.

### 3. Type-checker (`crates/cobrust-types/src/check.rs`)

`try_synth_reduce_builtin` fires for the bare PRELUDE `min`/`max`/`sum`
(their `DefId`s registered in a DEDICATED `reduce_defs: HashSet<DefId>`
slot at `prebind_item` — NOT `poly_intrinsic_defs`, which would widen
the narrow PRELUDE `list[i64]` arg to a fresh var and let a `list[float]`
slip past unification; a user-defined `min`/`max`/`sum` shadows the
def_id) with exactly one positional argument. It resolves the arg
(unwrapping one `Ref` so `sum(&xs)` works) and returns:

- **`list[int]`** → `Ty::Int`.
- **`list[float]`** → `Ty::Float`.
- **unresolved elem var** (un-annotated `min([])`) → unify the elem
  against `i64`, return `Ty::Int` (the int path; the runtime empty-list
  trap still fires).
- **`list[other]`** (`sum(["a"])`) → unify the elem against `i64`,
  raising the canonical `TypeError::TypeMismatch` (NO new variant).
- **non-list arg** (`min(5)`) → `TypeError::NotIterable { actual, span,
  suggestion }` — an EXISTING variant (no error-renderer cascade), with
  a §2.5-B fix suggestion (`"min / max / sum take a single list
  argument"`).

Runs in `synth_call` after the method-call + `len` + `abs` + `range`
dispatch and BEFORE the generic PRELUDE-stub-unify. The DEFERRED
multi-scalar-arg form (`min(1, 2, 3)`) and the `key=` / `default=`
kwargs return `Ok(None)` and hit the canonical `ArityMismatch` /
keyword diagnostic.

### 4. MIR (`crates/cobrust-mir/src/lower.rs`)

The list arg passes UNCHANGED (Copy-at-call, `is_copy_type`). The
PRELUDE stubs declare `-> i64`, but `lower_call` overrides the
`_callret` destination type to `Ty::Float` when the single list arg's
element type (read via `synth_expr_ty`, NOT the MIR temp — the ADR-0089
lesson) is `Float`; otherwise `Ty::Int`. WITHOUT this override the
`_callret` alloca would be an i64 while the float arm rewrites the call
to `__cobrust_*_float` (-> f64), corrupting `print(sum([1.5, 2.5]))`.

### 5. Codegen + intrinsic-rewrite

- **Externs** (`crates/cobrust-codegen/src/llvm_backend.rs`) — the 6
  shims declared alongside `__cobrust_int_abs`: `(ptr) -> i64` ×3,
  `(ptr) -> f64` ×3.
- **Rewrite** (`crates/cobrust-cli/src/build/intrinsics.rs`) — new
  `Kind::{Min, Max, Sum}` arm picks the runtime symbol from the call's
  DEST type (`local_ty[destination.local]` → `Float` ⇒ the `_float`
  shim, else the `_int` shim) — the ONE source of truth, mirroring
  `Kind::MathAbs`'s dest-type dispatch. The PRELUDE stubs are added to
  `crates/cobrust-frontend/src/prelude.rs` (`fn min(xs: list[i64]) ->
  i64: return 0`, etc. — narrow placeholder shapes; the special-case
  fully resolves the type) so the type-checker has a `DefId` to register
  and the def-id collection (first-body-wins guard, like the math
  family) picks them up.

### Deferred (documented, out of scope)

- **`min`/`max`/`sum` of MULTIPLE scalar args** — `min(1, 2, 3)`. Falls
  through to the canonical `ArityMismatch` (one PRELUDE param).
- **The `key=` and `default=` kwargs** — `min(xs, key=f)`,
  `min(xs, default=0)`. Keyword args defer to the generic path.
- **Reducing a `str` / `dict` / generator** — only `list[int]` and
  `list[float]` are wired (the element types with i64-slot storage).
- **`min`/`max` NaN ordering for floats** — a simple `<`/`>` reduction;
  the differential corpus avoids NaN inputs.

## Consequences

- **Positive**
  - `min(xs)` / `max(xs)` / `sum(xs)` type-check + lower + run for
    `list[int]` and `list[float]` (§2.5 first-try wins); the int result
    is usable in int arithmetic (`sum(xs) + 1`).
  - The **list-consume (borrow-read) mechanism** is established + locked
    by a list-reused-after-reduce test — `sorted` / `coil.array` reuse
    this ABI.
  - The element-type dispatch keys on the DEST type, immune to the
    ADR-0089 computed-arg miscompile (locked by `sum(make_floats())` +
    `list_new`-built int-list e2es).
  - NO new error variant — `min(5)` reuses `NotIterable`, `sum(["a"])`
    reuses `TypeMismatch`, `min(1, 2, 3)` reuses `ArityMismatch`, so the
    ADR-0088 error-renderer cascade (`error.rs` / `fix_safety` /
    `error_ux` / `cobrust-lsp` / `cobrust-types-cb` / `types-parity`)
    needed NO changes.
- **Negative**
  - The PRELUDE grows three placeholder stubs (`min`/`max`/`sum`); a
    user-defined same-named fn is correctly shadowed (first-body-wins
    guard in both the type-checker and the intrinsic-rewrite).
- **Neutral / unknown**
  - cobrust prints whole floats without the trailing `.0` (`print(3.0)`
    → `3`), so `sum([1.5, 2.5])` (value 4.0) prints `4` (Python prints
    `4.0`). A pre-existing float-format difference (ADR-0089), unrelated
    to this ADR.

## Evidence

- Repro (pre-fix): `min([3, 1, 2])` / `max(...)` / `sum(...)` →
  `unknown name: min/max/sum`.
- Post-fix e2e (`crates/cobrust-cli/tests/list_reduce_e2e.rs`, REAL
  compile → link → spawn, 14/14 green): int var/literal min/max/sum,
  negatives + `sum(xs)+1`, singleton, `sum([])==0`, float var/literal
  (incl. fractional `sum([1.5,2.5,0.25])==4.25`), the COMPUTED-arg
  cases (`sum(make_floats())`, `list_new`+`list_set` built int list),
  the BORROW lock (list reused + re-summed after all three reducers,
  exit 0), empty `min([])`/`max([])` traps (non-zero exit), `sum`/`min`/
  `max` of `range(...)`, a mixed int+float program.
- stdlib unit tests (`reduce.rs`, 6/6 green): int/float basic,
  negatives + singleton, `sum([])==0`/`0.0`, null-list, and a
  reused-after-reduce (borrow-not-free under the test allocator).
- Type-checker corpus: `well_typed.rs` w230–w236 (sum/min/max int +
  float, int-literal, borrowed-list `sum(&xs)`, reused-after-borrow) +
  `ill_typed.rs` i175–i178 (`min(5)`/`sum("abc")` → `NotIterable`,
  `min(1,2,3)` → `ArityMismatch`, `sum(["a","b"])` → `TypeMismatch`).
  cobrust-types: lib 155, well_typed 295 (+7), ill_typed 194 (+4).
- Differential oracle: python3.11 `min([3,1,2])`/`max`/`sum` =
  `1`/`3`/`6`; `min([1.5,2.5,3.0])`/`max`/`sum` = `1.5`/`3.0`/`7.0`;
  `sum([])` = `0`; `min([])` raises `ValueError`.
- Regression (all green): LC-100 `intrinsics_input`,
  `lc100_stress_e2e_b1`, `len_polymorphic_e2e`, `builtins_abs_range_e2e`,
  `list_str_e2e`, `well_typed`, `ill_typed`.
