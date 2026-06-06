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

**F79 closure (negative-LITERAL scalar reject — Option A, lockstep with
ADR-0094)**: a NEGATIVE-INTEGER-LITERAL `bytes` scalar index (`b"abc"[-1]`,
`b[-2]`) now REJECTS at `cobrust check` (`TypeError::UnsupportedSliceShape`,
REUSED — no new cascade), MIRRORING the slice path's negative-bound reject
(`literal_int_value(..) < 0`). The diagnostic prints the §2.5-B fix
(`b[len(b) - 1]`). This closes the F79 §2.2 silent-miscompile for the
literal `b[-1]` (was a silent sentinel `-1`; CPython `b"abc"[-1] == 99`,
the last byte). `bytes_ops_e2e_10` pins it (lockstep twin of
`str_slice_e2e_06`). **Residual (Option B — still deferred)**: a
NON-LITERAL runtime-negative index still hits the `__cobrust_bytes_get`
`i < 0` sentinel; full from-end negative indexing + a scalar OOB-PANIC are
the larger follow-up (a non-literal `b[i]` STILL type-checks —
`bytes_ops_e2e_10c` asserts the deferred path is unbroken).

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

---

# ADR-0093 Phase 2 — the byte-buffer surface (slice / concat / encode / decode / hex)

> Phase 2 lands the byte-buffer OPERATIONS the Phase-1 foundation
> deferred. Status: **accepted** (date 2026-06-06; builds on Phase-1
> commit `5eeb4b9`). The dora accessor (`event.data_bytes()`) remains a
> SEPARATE follow-up (ADR-0076c B-1b) — it is NOT in this increment.

## Context

Phase 1 minted `bytes` as a runtime value (`b"..."`, `len(b)`, `b[i]`,
exactly-once drop) but a `.cb` program could not yet **transform** a
`bytes`: no slice, no concat, no `str`↔`bytes` bridge. Phase 2 adds five
operations, each of which MINTS a fresh heap value (a fresh `bytes` or a
fresh `str`) the `.cb` scope owns + drops EXACTLY ONCE while BORROWING
(never freeing) its inputs — the SAME mint-fresh / borrow-inputs
discipline `__cobrust_str_concat` / `__cobrust_str_lower` already run.

## Decision

### 1. Five new `__cobrust_bytes_*` shims (`cobrust-stdlib/src/bytes.rs`)

| `.cb` source | type | runtime symbol | mints | borrows |
|---|---|---|---|---|
| `b[lo:hi]` | `bytes` | `__cobrust_bytes_slice(b, lo, hi)` | fresh `bytes` | `b` |
| `b1 + b2` | `bytes` | `__cobrust_bytes_concat(a, b)` | fresh `bytes` | `a`, `b` |
| `s.encode()` | `bytes` | `__cobrust_bytes_from_str(s)` | fresh `bytes` | `s` (str) |
| `b.decode()` | `str` | `__cobrust_bytes_decode(b)` | fresh `str` | `b` |
| `b.hex()` | `str` | `__cobrust_bytes_hex(b)` | fresh `str` | `b` |

- **Slice** uses **Python clamp** semantics (NOT the `coil.Buffer`
  abort-on-OOB): `b"abcd"[1:99] == b"bcd"`, `b"abcd"[3:1] == b""`. Bounds
  clamp to `[0, len]`; `hi < lo` → empty. The ONLY supported slice shape is
  the contiguous `b[lo:hi]` with **both non-negative bounds present and the
  default step**. Negative / open-ended (`b[1:]`, `b[:3]`) / step (`b[::2]`)
  bounds are a §Phasing deferral and are **REJECTED at compile time**
  (`TypeError::UnsupportedSliceShape`, §2.5-A) — see §"Slice-shape
  soundness" below.

  **Correction (Phase-2 repair).** An earlier draft of this ADR claimed the
  unsupported shapes "fall through to the generic index path, a bounded
  gap, NOT a miscompile." That claim was **empirically FALSE and is
  withdrawn**: a non-`lo:hi` shape type-checked as `Ty::Bytes`, fell out of
  the MIR `bytes`-index guard to the generic `Projection::Index` path where
  the `Slice` collapsed to `Constant::Int(0)` and the index projection was a
  codegen no-op, so the expression **silently evaluated to the WHOLE base
  buffer** at exit 0 with no diagnostic (`b"hello"[1:]` printed `len 5`, not
  CPython's `4`; `b"hello"[0:4:2]` printed `5`, step dropped; `b"hello"[1:-1]`
  printed `len 0`). That is the exact §2.2 silent-coercion / §2.5
  compile-time-catch-miss the constitution most forbids — a wrong answer
  shipped as a "bounded gap." The repair lifts it to a hard compile reject
  (below). (The `coil.Buffer` slice arm carried the identical latent
  silent-fallthrough defect; its `__cobrust_coil_buffer_slice` guard is a
  tracked follow-up under ADR-0077 §12 — the bytes repair is the template.)
- **`from_str` / `decode`** bridge `bytes` ↔ `str`. Both `BytesBuffer` and
  `StringBuffer` are `#[repr(C)] struct { bytes: Vec<u8> }`, so the bridge
  reads one and mints the other via the public `__cobrust_str_*`
  accessors (`__cobrust_str_len` / `__cobrust_str_ptr` to READ a str;
  `__cobrust_str_new` / `__cobrust_str_push_static` to MINT a str) — NO
  edit to the private `StringBuffer` struct in `fmt.rs`.
- **`encode` is total** (a `str`'s stored bytes are always valid UTF-8, so
  there is no error path). **`decode` is fallible** — see §2 below.
- **`hex`** is CPython `bytes.hex()`: lowercase, two chars per byte, no
  separator (`b"\xff\x00".hex() == "ff00"`).

### 2. The §2.2 design point — `bytes.decode()` of INVALID UTF-8 TRAPS (never lossy)

**CLAUDE.md §2.2 forbids silent coercion.** Decoding an invalid byte
sequence (`b"\xff\xfe"`) is exactly that hazard: Python's `bytes.decode()`
defaults to `errors="strict"` (raises `UnicodeDecodeError`), but a
careless port could `from_utf8_lossy` (silent U+FFFD replacement) or
truncate at `valid_up_to()`. Both are silent coercion — **rejected**.

**Decision: invalid UTF-8 TRAPS** via `std.panic::panic` — it writes a
structured `bytes.decode: invalid utf-8 at byte N` diagnostic to stderr
(where `N == Utf8Error::valid_up_to()`, the first invalid byte) and exits
the process with code 3 (`INTERNAL_PANIC`, ADR-0024 §"Exit-code scheme").
This is the SAME trap every other Cobrust domain error surfaces through
(`__cobrust_panic`, the dict-missing-key follow-up, the `coil_panic`
out-of-bounds). On VALID UTF-8 it mints a fresh `str` byte-exact.

- **§2.5-B (errors print the FIX)**: the byte offset `N` lets the LLM/user
  locate the bad input in the source `bytes` without a debugger.
- **The `.cb` surface shape**: `b.decode() -> str` (NOT `Result[str,
  DecodeError]`). NO stdlib op today returns a surface `Result` (e.g.
  `read_line() -> str`, NOT `Result[str, IoError]` —
  `intrinsics_input.rs` pins this). A `Result`-returning decode is the
  named §Phasing follow-up **once stdlib-fallible-`Result` returns are
  wired language-wide**; until then the trap is the sound v1 — a decode
  failure is a precondition violation ("truly unrecoverable" per §2.2),
  and the trap is NEVER lossy. This is the load-bearing choice: an invalid
  byte is loud (process abort + diagnostic), never silently swallowed.

### 3. The 6-layer wiring (mirrors the Str method path)

- **Slice** (`b[lo:hi]`): typecheck `(Ty::Bytes, IndexKind::Slice) ->
  Ty::Bytes` (`check.rs`); MIR `lower_expr` Index arm gains a Slice branch
  beside the Phase-1 scalar `b[i]` branch (the `coil.Buffer` slice mirror)
  — base BORROWED (Move→Copy), result MOVED out (a Copy would double-free
  the fresh buffer); dest local `Ty::Bytes` → drop-scheduled.
- **Concat** (`b1 + b2`): typecheck `synth_bin` Add accept-set gains
  `Ty::Bytes if op == Add => Ty::Bytes` (so `bytes - bytes` still rejects,
  CPython-faithful); MIR `lower_bin` Add guard gains a `Ty::Bytes` arm
  beside the `str_concat` arm — both operands BORROWED, result MOVED out.
- **Methods** (`s.encode()` / `b.decode()` / `b.hex()`): the full 6-layer
  Str-method path —
  1. **PRELUDE** (`prelude.rs`): `bytes_from_str(s: str) -> bytes`,
     `bytes_decode(b: bytes) -> str`, `bytes_hex(b: bytes) -> str`.
  2. **typecheck** (`check.rs`): `encode` added to `try_synth_str_method`
     (-> `Ty::Bytes`); a new `try_synth_bytes_method` table (`decode` /
     `hex` -> `Ty::Str`; unknown -> `UnknownMethod` with a §2.5-B fix
     hint), wired into `try_synth_method_call`.
  3. **method-form rewrite** (`lower.rs::method_form_rewrite_name`):
     `Str::encode -> bytes_from_str`; a new `Ty::Bytes` arm (`decode ->
     bytes_decode`, `hex -> bytes_hex`).
  4. **receiver borrow** (`lower_rewritten_method_call`): the three names
     added to the borrow-not-move set; the receiver upgrade chains
     `upgrade_move_to_copy_for_str` THEN `..._for_bytes` (each a no-op on
     the wrong type) so a `Ty::Bytes` receiver is borrowed too (without
     it, `b.hex(); len(b)` would `UseAfterMove`).
  5. **intrinsic Kinds** (`build/intrinsics.rs`): `BytesFromStr` /
     `BytesDecode` / `BytesHex` join the single-arg ptr→ptr emit block
     beside `StrLower` / `StrClone`; def-id tracking + `kind_for_def_id`.
  6. **codegen externs** (`llvm_backend.rs::declare_runtime_helpers`): the
     5 new symbols declared beside the Phase-1 `__cobrust_bytes_*` block
     (`runtime_helper_decls` + `runtime_helper_param_counts`).
- **Chained method calls** (`s.encode().decode()`):
  `lower.rs::synth_expr_ty`'s `ExprKind::Call` Attr branch gains a
  method-form-rewrite return-type resolution so the inner `.encode()`
  synths to `Ty::Bytes`, driving the outer `.decode()` dispatch (without
  it the inner call synthed `Ty::None` → the outer rewrite returned
  `None` → empty/wrong output).

### 3a. Slice-shape soundness (the §2.2 / §2.5 compile-time reject)

The runtime slice shim `__cobrust_bytes_slice(b, lo, hi)` only models the
contiguous `lo:hi` form. Every OTHER slice shape is **REJECTED at compile
time** — it is NOT a silent fallthrough:

- **typecheck (primary catch, `check.rs`)**: the `(Ty::Bytes,
  IndexKind::Slice { start, stop, step })` arm type-checks each present
  bound (`unify Int`) then GATES the shape: it returns `Ty::Bytes` ONLY for
  `step.is_none() && start.is_some() && stop.is_some()` with neither bound a
  negative integer literal; every other shape (open-ended, stepped, or a
  syntactically-negative bound such as `b[1:-1]`) is
  `TypeError::UnsupportedSliceShape`, a §2.5-B fix-printing diagnostic that
  names the supported `b[lo:hi]` form (`write both explicit bounds, e.g.
  b[1:len(b)]`).
- **MIR (defense-in-depth, `lower.rs`)**: the `Ty::Bytes` slice branch now
  emits the `__cobrust_bytes_slice` call ONLY for the `(None step, Some
  start, Some stop)` shape; any other `IndexKind::Slice` reaching MIR (i.e.
  the type checker was bypassed) returns a hard `MirError::Internal` instead
  of falling through to the generic `Projection::Index` no-op. There is NO
  remaining path by which an unsupported bytes-slice shape evaluates to the
  whole buffer.
- **residual (runtime-negative bound)**: a bound that is a runtime *value*
  (not a literal) carrying a negative number cannot be caught statically; it
  clamps to `[0, len]` in the shim (`b[runtime_neg:hi]` treats the negative
  as `0`). This is a documented divergence from CPython's from-end negative
  indexing — NOT a silent whole-buffer miscompile (the shim still returns a
  correctly-bounded sub-slice, never the aliased base). Full negative-index
  support is the §Phasing follow-up.

### 3b. `bytes` comparison rejects (no codegen ICE)

`bytes cmp bytes` (`==` / `!=` / `<` / `<=` / `>` / `>=`) is **REJECTED at
compile time** with a §2.5-B fix-printing `TypeError::TypeMismatch`
(`synth_bin`'s comparison arm gains a `Ty::Bytes`-on-either-side guard that
runs BEFORE the `unify → Ok(Ty::Bool)` fall-through, unwrapping a `&`-borrow
handle like the `coil.Buffer` guard). Rationale: two `Ty::Bytes` DO unify,
so without the guard the arm returned `Ok(Ty::Bool)` and codegen's
comparison path (`llvm_backend.rs` `lower_binop → into_int_value()`) **ICE'd
on the opaque `bytes` POINTER operand** — a raw inkwell `Found PointerValue
… but expected the IntValue variant` panic, NOT a Cobrust diagnostic (a §2.5
+ §5.1 "no panic without rationale" violation). This ICE pre-existed Phase-2
(Phase-1 `b"a"` literals already made `b"a" == b"a"` constructible), but
Phase-2's slice/concat/encode all return fresh `bytes`, making "compare two
bytes" the single most obvious next operation an LLM/user writes — so the
reject is shipped here. Lexicographic `bytes` comparison (a
`__cobrust_bytes_eq` / `__cobrust_bytes_cmp` shim + the `lower_bin`
Eq/NotEq/ordering `Ty::Bytes` arm) is the named §Phasing follow-up; the
diagnostic prints the interim fix (compare `len(a)` with `len(b)`, or
`a.decode()` with `b.decode()` when both sides are known valid UTF-8).

## Drop discipline (the hammer-loop proof)

Each minting op declares its result with the minted `Ty` (slice/concat →
`Ty::Bytes`; decode/hex → `Ty::Str`; encode → `Ty::Bytes` via the PRELUDE
fn return type), and `drop.rs::is_copy` excludes BOTH `Ty::Bytes` and
`Ty::Str`, so the result is drop-scheduled (freed EXACTLY ONCE at scope
exit). The result is MOVED out (a Copy would leave two owners → a
double-free). Inputs are BORROWED (Move→Copy operand upgrade) — they
survive the op and drop once at their own scope exit. A 1000-iter
hammer-loop (each iteration minting a fresh slice + concat + decode + hex
and dropping all) exits 0 with the exact accumulator (no double-free /
leak / use-after-free).

## Consequences

- **Positive**: a `.cb` program can now slice, concat, and bridge `bytes`
  ↔ `str` — the full byte-buffer surface. The §2.2 no-silent-coercion law
  holds: invalid-UTF-8 decode is loud (trap + diagnostic + byte offset),
  NEVER lossy. The Str machinery is again UNTOUCHED — the bridge READS/
  MINTS Str via the public `__cobrust_str_*` C-ABI, not a `string.rs` /
  `fmt.rs` edit. The drop schedule is leak-free + double-free-free
  (hammer-loop verified, `.cb`-level + Rust-unit-level).
- **Negative / accepted debt** (honest §Phasing, each a documented gap,
  NOT a failing test, and — critically — each a COMPILE-TIME REJECT, never a
  silent miscompile):
  - **Negative / open-ended / step slice bounds** (`b[1:]`, `b[:3]`,
    `b[0:4:2]`, `b[1:-1]`) are **rejected at compile time**
    (`TypeError::UnsupportedSliceShape`, §3a) — the contiguous non-negative
    `lo:hi` form is the supported increment. (The `coil.Buffer` slice
    Phase-2a cap, ADR-0077 §12, carries the identical latent silent-
    fallthrough defect and is a tracked follow-up — the bytes repair is the
    template.)
  - **`bytes cmp bytes`** (`==` / `!=` / `<` / ordering) is **rejected at
    compile time** (`TypeError::TypeMismatch`, §3b) — NOT the raw inkwell
    ICE it was at committed HEAD, and NOT in this increment as a working op.
    A follow-up adds `__cobrust_bytes_eq` / `__cobrust_bytes_cmp` + the
    `lower_bin` Eq/NotEq/ordering `Ty::Bytes` arm (the `str ==` mirror).
  - **`Result[str, DecodeError]` ergonomic decode** awaits language-wide
    stdlib-fallible-`Result` returns; the trap is the sound v1.
  - **The dora `event.data_bytes()` accessor** remains the SEPARATE
    ADR-0076c B-1b follow-up (its Phase-1 `#[ignore]` placeholder stays
    ignored — it is NOT part of the byte-buffer-surface increment).
- **Evidence**: `crates/cobrust-cli/tests/bytes_ops_e2e.rs` (9 e2es: slice +
  clamp, concat, the `encode().decode()` round-trip incl. multi-byte UTF-8,
  the invalid-UTF-8 decode trap, hex incl. non-UTF-8 bytes, a 1000-iter drop
  hammer-loop, inputs-borrowed-not-consumed, the unsupported-slice-shape
  compile reject `_08`, and the `bytes`-comparison compile reject `_09`) +
  `crates/cobrust-stdlib/src/bytes.rs` Phase-2 unit tests (slice/concat/
  encode-decode round-trip/hex/null-safety + a Phase-2 hammer-loop) +
  `crates/cobrust-types-cb/tests/error_display_parity.rs::test_display_unsupported_slice_shape`
  (the new `TypeErrorCb` mirror byte-parity). The CPython-3 oracle:
  `b"hello"[1:4] == b"ell"`, `b"hello".hex() == "68656c6c6f"`,
  `"héllo".encode().decode() == "héllo"`, `b"\xff\xfe".decode()` raises;
  the soundness rejects: `b"hello"[1:]` / `b"hello"[0:4:2]` / `b"abc" ==
  b"abc"` all FAIL the build (where they previously printed a wrong answer
  or ICE'd).

## References (Phase 2)

- Str concat / method template: `lower.rs` (`lower_bin` `str_concat` arm,
  `method_form_rewrite_name`, `lower_rewritten_method_call`),
  `build/intrinsics.rs` (the `StrLower`/`StrClone` single-arg emit block),
  ADR-0085 (the Python-named str methods 6-layer path).
- Slice template: ADR-0077 (`coil.Buffer` slice, `lower_expr` Index Slice
  branch) — but Python-clamp, not abort-on-OOB.
- The trap path: `cobrust-stdlib/src/panic.rs` (`panic` → stderr +
  exit 3), ADR-0024 (exit-code scheme), ADR-0025 (the panic surface).
- §2.2: CLAUDE.md "Drop from Python — silent coercion → type error".
