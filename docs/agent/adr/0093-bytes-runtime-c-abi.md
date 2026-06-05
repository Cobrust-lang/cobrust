---
doc_kind: adr
adr_id: 0093
title: "`bytes` runtime + C-ABI — the `__cobrust_bytes_*` family (Str-mirror immutable byte buffer)"
status: accepted
date: 2026-06-06
last_verified_commit: d171ade
supersedes: []
superseded_by: []
---

# ADR-0093: `bytes` runtime + C-ABI — the `__cobrust_bytes_*` family

## Context

`bytes` was a **type-system-only** type. The full surface existed at the
front of the pipeline but bottomed out with no runtime:

- `Ty::Bytes` exists (`cobrust-types/src/ty.rs:172`, `Display` → `"bytes"`).
- The parser accepts `bytes` as a type name
  (`cobrust-frontend/src/parser.rs:2463`) and lexes `b"..."` byte-string
  literals (`lexer.rs:467` prefix path → `ast::Literal::Bytes(Vec<u8>)`).
- HIR carries `Lit::Bytes(Vec<u8>)` (`cobrust-hir/src/tree.rs:395`,
  desugared at `desugar.rs:104`).
- MIR carries `Constant::Bytes(Vec<u8>)` (`cobrust-mir/src/tree.rs:329`,
  lowered at `lower.rs:3451`); a bare `b"..."` synthesises `Ty::Bytes`
  (`lower.rs:3724`).
- The type checker already types `len(bytes)`'s sibling for the SIZED
  set, `bytes[i] -> Int` (`check.rs:2259`), and the literal `Ty::Bytes`
  (`check.rs:4360`).

But **codegen left `Ty::Bytes` unmodeled**. The `b"..."` literal routed
through `materialize_str_buffer` under lossy UTF-8
(`llvm_backend.rs:5369`, "Wave-3 may introduce a dedicated
`__cobrust_bytes_*` family"), so a `b"\xff"` byte was silently dropped,
`len(b)` was unreachable (the sized set rejected `Ty::Bytes`), and a
`bytes` local had no drop symbol — the drop pass enumerated it as
drop-eligible (`drop.rs:142` `is_copy` excludes `Ty::Bytes`) but
`emit_drop_for_ty` (`llvm_backend.rs:5235`) had no `Ty::Bytes` arm, so a
`bytes` value would **leak** at scope exit. Net: a `.cb` program that
binds, measures, or indexes a `bytes` value could not compile + run
correctly.

`bytes` is **"Str without UTF-8"**: an immutable, heap-allocated byte
buffer behind an opaque `*mut u8` handle, with `clone-on-read` reuse and
exactly-once `drop` at scope exit — the SAME ownership discipline Str
already runs (ADR-0050c). This ADR mints the runtime so `bytes` becomes a
first-class value.

## Design principle (CLAUDE.md §2.5 — LLM-first surface)

An LLM writes `b"..."`, `len(b)`, and `b[i]` from its Python priors
without thinking. The three surface forms map 1:1 to Python:

| `.cb` source | type | runtime symbol |
|---|---|---|
| `b"abc"` literal | `bytes` | `__cobrust_bytes_from_raw(ptr, len)` (mint from `.rodata`) |
| `len(b)` | `int` | `__cobrust_bytes_len(b)` |
| `b[i]` | `int` (0..255) | `__cobrust_bytes_get(b, i)` |

- **§2.5-A compile-time-catch**: `b[i]` types to `int` (a byte value
  0..255), NOT `bytes` — a single index is a scalar, matching Python's
  `b"abc"[0] == 97`. A non-`int` index is a `TypeError::NotIndexable` at
  `cobrust check` time.
- **§2.5 (maximize-overlap-with-training-data)**: `len(b)` is the
  free-function form an LLM writes; it joins `str` / `list` / `dict` in
  the ADR-0088 sized set. `b[i] -> int` matches CPython exactly (an
  `int`, not a 1-byte `bytes`, which is the Python 3 `bytes.__getitem__`
  semantic).

## Decision

### 1. The `__cobrust_bytes_*` C-ABI family (`cobrust-stdlib/src/bytes.rs`)

A new module mirroring the `__cobrust_str_*` family
(`string.rs` + `fmt.rs`). The opaque handle is `*mut u8` pointing at a
`#[repr(C)] struct BytesBuffer { bytes: Vec<u8> }` — the exact shape of
`fmt.rs`'s `StringBuffer`, minus the UTF-8 invariant.

| symbol | signature | role |
|---|---|---|
| `__cobrust_bytes_from_raw` | `(ptr: *const u8, len: i64) -> *mut u8` | mint a heap `bytes` from a static/raw slice (the `b"..."` literal path); `null`/`len<=0` → empty buffer |
| `__cobrust_bytes_len` | `(b: *mut u8) -> i64` | byte length; `null` → 0 |
| `__cobrust_bytes_get` | `(b: *mut u8, i: i64) -> i64` | the `i`-th byte as a `0..255` int; out-of-range / `null` → `-1` (the bounds sentinel, sibling of `__cobrust_str_find`'s `-1`) |
| `__cobrust_bytes_drop` | `(b: *mut u8)` | free exactly once; `null` → no-op |
| `__cobrust_bytes_clone` | `(b: *mut u8) -> *mut u8` | deep-copy for the clone-on-read reuse path; `null` → `null` |

**Ownership convention** (ADR-0050c-mirror): a `bytes` value is
`.cb`-owned and freed EXACTLY ONCE via `__cobrust_bytes_drop` at scope
exit. A `bytes` value is `Move`-only (the operand-level `Move` discipline,
see §3 — `is_copy_type` excludes `Ty::Bytes`): a rebind transfers
ownership, and aliasing-then-reuse is a compile-time `use of moved value`
error in THIS phase. `__cobrust_bytes_clone` is the deep-copy shim
RESERVED for the Phase-2 aliasing surface (slice / concat) — it is
unit-tested but no `.cb`-lowering emits a call to it yet. `from_raw` mints
a fresh owned buffer (the literal is not shared with `.rodata`). `get` /
`len` BORROW (read-only); they never consume the handle.

### 2. Codegen — `Ty::Bytes` lowers to an opaque pointer handle (exactly like Str)

- **Type lowering**: `Ty::Bytes` falls through `lower_ty`'s `_ =>
  opaque_ptr_ty` arm (`llvm_backend.rs:4453`) — the same opaque `*mut u8`
  Str / List / Dict use. No new LLVM type. DI maps to the existing
  `cobrust::Bytes`-named basic type (added beside `cobrust::Str`) so an
  lldb pretty-printer can dispatch later.
- **`b"..."` literal codegen**: `Constant::Bytes(payload)`
  (`llvm_backend.rs:5369`) STOPS routing through `materialize_str_buffer`
  and instead mints via a new `materialize_bytes_buffer(payload)` helper:
  materialise the byte array into `.rodata` (reuse `materialize_str_data`
  — it is byte-exact, no UTF-8 assumption on the way out), then call
  `__cobrust_bytes_from_raw(ptr, len)`. This preserves non-UTF-8 bytes
  (`b"\xff"`) that the old str-buffer path corrupted.
- **The 5 externs** are declared in `declare_runtime_helpers` beside the
  `__cobrust_str_*` block, with `runtime_helper_param_counts` entries.
- **Drop schedule**: `emit_drop_for_ty` (`llvm_backend.rs:5235`) gains
  `Ty::Bytes => Some("__cobrust_bytes_drop")`. The drop PASS already
  enumerates a `Ty::Bytes` local (drop.rs `is_copy` excludes it); this
  arm is the ONLY edit needed to close the would-be leak. The drop fires
  exactly once at scope exit, identical to Str.

### 3. MIR — `len(bytes)` + `bytes[i]` retarget to the runtime symbols

- **`bytes[i]`**: a new arm in the `ExprKind::Index` lowering
  (`lower.rs:1706`), placed beside the `coil.Buffer` getitem arm
  (`lower.rs:1858`) it mirrors structurally. The base handle is BORROWED
  (`upgrade_move_to_copy_handle`) so the source `bytes` local survives
  and drops once. Emits `__cobrust_bytes_get(b, i) -> i64` into an `Int`
  dest. The result is a Copy scalar (`Operand::Copy`), so no clone is
  needed (unlike `list[str][i]` which clones a Str).
- **`len(bytes)`**: the type checker's `try_synth_len_builtin`
  (`check.rs:3561`) gains `Ty::Bytes` in the sized set. The CLI
  intrinsic-rewrite `Kind::LenPoly` (`intrinsics.rs:1999`) gains
  `Some(Ty::Bytes) => __cobrust_bytes_len` plus a
  `Constant::Bytes(_) => Some(Ty::Bytes)` effective-type case (so
  `len(b"abc")` on a literal routes correctly).
- **Operand-level Move** for `Ty::Bytes` is already correct: `is_copy_type`
  (`lower.rs:3621`) excludes `Ty::Bytes` (it lists only the scalars +
  List/Dict/Ref), so a `bytes` read emits `Operand::Move` — ownership
  transfers exactly like Str. No edit needed there; the `[i]` arm's
  Move→Copy upgrade handles the index-base borrow.

### 4. Cross-crate concern: the symbols live in `libcobrust_stdlib.a`

The `__cobrust_bytes_*` shims live in `cobrust-stdlib` — the shared
runtime archive ALL `.cb` programs already link. They are NOT a
cross-crate cabi dependency, so the F-cabi-feature duplicate-symbol issue
(coil/dora style) does NOT apply. A future `cobrust-dora` accessor that
needs the symbols gets them from the already-linked archive (no
`features = [...]` dance).

## §2.5 surface decision (the one chosen, vs the rejected alternatives)

- **`b[i] -> int`, NOT `b[i] -> bytes`** (chosen). CPython 3:
  `b"abc"[0] == 97` (an `int`). A 1-byte `bytes` is `b"abc"[0:1]`. The int
  form matches the LLM's Python prior AND is the §2.5-A compile-time-catch
  win (a byte is a scalar; arithmetic on it is an `int` op the checker
  proves). Rejected: returning `bytes` (a Rust-ism, mismatches Python).
- **`-1` out-of-range sentinel on `get`** (chosen), vs a panic. The
  minimal increment keeps `get` total (no abort helper); an explicit
  bounds-panic is a Phase-2 deferral (sibling of the dict
  panic-on-missing follow-up, ADR-0050d Decision 2A). `-1` is
  unambiguous (a real byte is 0..255).
- **No `bytes` literal interning / `.rodata` sharing** (chosen): `from_raw`
  always mints a fresh owned buffer, so the drop discipline is uniform
  (every `bytes` value is heap-owned + dropped once). Interning the
  literal would split the drop schedule (a `.rodata`-backed `bytes` must
  NOT be freed) — rejected for the minimal increment.

## Phasing

**This increment (Phase 1 — runtime foundation)**:
`__cobrust_bytes_*` C-ABI (`from_raw` / `len` / `get` / `drop` / `clone`)
+ `Ty::Bytes` codegen (opaque ptr handle) + `b"..."` literal codegen
(byte-exact mint) + the 5 externs + `len(bytes) -> int` + `bytes[i] -> int`
+ the exactly-once drop schedule. A `.cb` program may now bind, measure,
index, and pass a `bytes` value.

**Deferred (Phase 2 — the byte-buffer surface)**, each with an
`#[ignore = "ADR-0093 Phase 2: <op>"]` placeholder where a test exists:

- **Slicing** `b[lo:hi] -> bytes` (`__cobrust_bytes_slice`, the
  coil-buffer-slice mirror).
- **Concat** `b1 + b2 -> bytes` (`__cobrust_bytes_concat`, the
  `__cobrust_str_concat` mirror) + equality `b1 == b2`.
- **Methods** `.hex() -> str`, `.decode() -> str` (UTF-8, with an error
  path), and the `str.encode() -> bytes` inverse.
- **The dora accessor** `event.data_bytes() -> bytes` (Arrow
  Binary/UInt8 → `bytes`) + `event.send_output_bytes(id, b)`, the named
  ADR-0076c (D)-B-1b deferral. It mirrors the `data_buffer` /
  `send_output_buffer` pattern (`cobrust-dora/src/cabi.rs:813`,
  `ecosystem.rs:2761`) + extends `check_dora_send_output_id`
  (`check.rs:3308`) to fire `DoraUnknownOutputId` for `send_output_bytes`
  too. **Deferred to a follow-up** because the bytes runtime foundation
  is itself a coherent, hammer-loop-verified increment, and the dora
  accessor adds a substantial dual-build (`dora-real`) Arrow-decode +
  ecosystem-row + check-extension surface that wants its own sound slice
  rather than riding a foundation commit (F37 honest-phasing).

## Consequences

- **Positive**: `bytes` is a real value; the §2.5 surface (`b"..."`,
  `len(b)`, `b[i]`) compiles + runs byte-exact, including non-UTF-8 bytes
  the old lossy path corrupted. The drop schedule is leak-free +
  double-free-free (the Str discipline, verified by a 1000-iter
  hammer-loop). The Str machinery is UNTOUCHED — `bytes` is a sibling
  module, not an edit to `string.rs` / `fmt.rs`.
- **Negative / accepted debt**: the byte-buffer surface (slice / concat /
  methods / decode) + the dora accessor are NOT in this increment — a
  `.cb` program can hold + measure + index `bytes` but not yet slice or
  concat it. This is the honest minimal slice; Phase 2 lands the rest.
- **Evidence**: `crates/cobrust-cli/tests/bytes_primitive_e2e.rs` —
  `let b: bytes = b"abc"; print(len(b))` → `3`; `print(b[0])` → `97`; a
  1000-iter drop hammer-loop (no leak / crash); +
  `crates/cobrust-stdlib/src/bytes.rs` unit tests (round-trip,
  null-safety, clone-independence, out-of-range sentinel).

## References

- Str template: `cobrust-stdlib/src/{string,fmt}.rs` (the
  `__cobrust_str_*` family), `llvm_backend.rs` (`materialize_str_buffer`,
  `emit_drop_for_ty`, `lower_ty`), ADR-0050c (the Str/List drop
  discipline), ADR-0058f (the str-buffer literal codegen).
- Sized set: ADR-0088 (`len` polymorphism) — `bytes` joins
  `str`/`list`/`dict`.
- Index dispatch mirror: ADR-0077 (`coil.Buffer` operator-index
  dispatch) — the `bytes[i]` arm mirrors the buffer-getitem arm.
- Deferred dora accessor: ADR-0076c (D)-B-1b, ADR-0092 (the
  `DoraUnknownOutputId` compile-time check `send_output_bytes` will
  extend).
