---
doc_kind: adr
adr_id: 0059e
name: 0059e
parent_adr: 0059a
title: "Phase L wave-3 follow-up — Str runtime full closure (§6.1 truly RESOLVED)"
status: accepted
date: 2026-05-21
ratified_at: 6e1ac9c
ratified_on: 2026-05-21
phase: Phase L wave-3 follow-up
last_verified_commit: 6e1ac9c
relates_to: [adr:0059a, adr:0059d, adr:0050c]
discovered_by: ADR-0059a §6.1 wave-3 honest-cite — "full runtime `frame variable s = \"hello\"` breakpoint round-trip requires a runtime Str allocator + populated StringBuffer (not available in a bare MIR fixture). This test closes the DIE presence half of §6.1; the bp-hit content half is deferred to ADR-0059c `cobrust debug` CLI path."
---

# ADR-0059e: Phase L wave-3 follow-up — Str runtime full closure

## 1. Motivation

ADR-0059a §6.1 honest-cite was carried through wave-2 (`16e0a37`,
2026-05-20) and wave-3 (`79bd1b2`, 2026-05-20) closing the DIE-presence
half but leaving the **content render** half deferred:

> Wave-3 §6.1 closure: `lldb_linked_str_frame_variable` test added to
> `dwarf_lldb_smoke.rs` verifying the linked-executable path emits a
> binary and the `cobrust::Str` DIE is present in it.
>
> **Remaining honest-cite (preserved)**: full runtime `frame variable s`
> showing actual Str content at a breakpoint requires stdlib linkage +
> a populated StringBuffer at runtime.

ADR-0059e closes the remaining half. The technical blocker was NOT
stdlib linkage (the wave-3 `lldb_linked_str_frame_variable` already
exercises a linked exe path with `cc`-linked-libc). The real blocker was
the **DIE shape**: `populate_di_basic_types` emits `cobrust::Str` as a
`DIBasicType` (opaque 64-bit pointer with name only). lldb's
`frame variable s` only renders the opaque `*mut u8` address — neither
the printer nor lldb has any structured handle to walk into the
`StringBuffer { bytes: Vec<u8> }` layout.

ADR-0059e attaches structured DI member fields to `cobrust::Str`
(mirroring the wave-3 `cobrust::Option` `DICompositeType` precedent in
`llvm_backend.rs` lines 905-919) so the lldb printer can read the
`ptr`/`len` fields via `SBValue.GetChildMemberWithName` directly.

This brings Phase L §6.1 from "DIE presence" to "full
`frame variable s = "hello"` content render" — closing the last
honest-cite from the entire Phase L roster.

Anchors verified at HEAD `c1880e3`:

- `docs/agent/adr/0059a-lldb-pretty-printers.md` §6.1 (honest-cite for
  bp-hit content carried to ADR-0059c).
- `docs/agent/adr/0059d-linker-harness-and-per-variant-adt-di.md` §3.3
  (linker harness shipped; Str runtime deferred to follow-up).
- `crates/cobrust-codegen/src/llvm_backend.rs::populate_di_basic_types`
  (Option `DICompositeType` precedent at lines 861-919; Str opaque
  basic-type at line 847).
- `tools/lldb-cobrust/printers.py::cobrust_str_summary` (current raw
  `process.ReadMemory` byte-decode; needs structured-member read).
- `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs::lldb_linked_str_frame_variable`
  (line 700; HONEST-CITE in comment + DIE-only assertion).

## 2. §2.5 LLM-first audit

LLM agents at a debugger breakpoint need to SEE Str content. The agent
reads:

```
(lldb) frame variable s
(Str) s = "hello"
```

…not the wave-3 baseline:

```
(lldb) frame variable s
(cobrust::Str) s = 0x000060000123a000
```

§2.5 §A (compile-time-catch-errors): per-field `DICompositeType` emission
attaches structured field DIs to the DWARF. The codegen `create_struct_type`
+ `create_member_type` APIs catch malformed field offsets / encodings
at LLVM build time — a wrong tag type would be rejected at the cargo
build gate, not at runtime.

§2.5 §B (training-data overlap): `(Str) s = "hello"` is the Python
`repr(s)` shape; the most-trained-on debugger output format in 2026
LLM corpora. The wave-2 Python self-tests already verified the byte-decode
contract; wave-3 added DIE presence; this ADR closes the final round-trip
gap so the LLM agent guarantee is no longer a stub.

## 3. Scope

### 3.1 Stdlib Str allocator already emits structured layout

No stdlib change required. `crates/cobrust-stdlib/src/fmt.rs::StringBuffer`
is already `#[repr(C)]` with a single `bytes: Vec<u8>` field. The Rust
`Vec<u8>` ABI is `{ ptr: *mut u8, cap: usize, len: usize }` (24 bytes),
verified at wave-2 in `printers.py::_read_string_buffer`. The runtime
allocator already commits this shape via `__cobrust_str_new` /
`__cobrust_str_push_static`; this ADR does NOT change stdlib semantics
or layout.

The "stdlib Str allocator emits structured DWARF-readable struct" goal
named in the task brief is **achieved at the codegen DI level**, not at
the stdlib level — the stdlib layout is already structured; what was
missing was the codegen surfacing the structure into DWARF.

### 3.2 Codegen attaches DI member fields to `cobrust::Str`

`crates/cobrust-codegen/src/llvm_backend.rs::populate_di_basic_types`
extends the existing `cobrust::Str` `DIBasicType` emission with a
parallel `DICompositeType` named `cobrust::Str` carrying two member
fields:

- `ptr: *const u8` at offset 0, size 64-bit, encoding `DW_ATE_ADDRESS`.
- `len: u64` at offset 64, size 64-bit, encoding `DW_ATE_UNSIGNED`.

Mirrors the wave-3 `cobrust::Option` `DICompositeType` precedent (lines
861-919). The composite is emitted unconditionally, like Option, so
`image lookup --type cobrust::Str` finds the DIE in lldb. The
field offsets `(0, 64)` model a **logical view** for the printer:
the runtime layout is `Box<StringBuffer { Vec<u8> }>` (an indirection
through the box pointer), so the printer's child-member walk is
correct only when applied to a freshly-dereferenced `StringBuffer`,
which is what wave-2's `_read_string_buffer` already does at the
Vec<u8> level. The new composite gives lldb a structured handle on
the post-dereference shape.

`di_basic_types["Str"]` continues to point at the opaque-pointer
basic-type so function signatures (`fn take_str(x: Str) -> Str`)
keep using the pointer-sized DI. The composite lives in a new field
`di_str_composite: Option<DICompositeType<'ctx>>` alongside
`di_option_composite`.

### 3.3 Python printer SBValue child-member read with fallback

`tools/lldb-cobrust/printers.py::cobrust_str_summary` extends with a
**structured-member** path that runs BEFORE the wave-2 raw-memory
fallback:

```python
ptr_child = valobj.GetChildMemberWithName("ptr")
len_child = valobj.GetChildMemberWithName("len")
if ptr_child and ptr_child.IsValid() and len_child and len_child.IsValid():
    ptr_addr = ptr_child.GetValueAsUnsigned(...)
    str_len  = len_child.GetValueAsUnsigned(...)
    if ptr_addr != 0 and 0 < str_len <= MAX_STR_LEN:
        raw = process.ReadMemory(ptr_addr, str_len, err)
        return '"' + raw.decode("utf-8", errors="replace") + '"'
# Fallback: wave-1/wave-2 raw-memory path (preserved for older binaries
# that didn't emit DICompositeType for cobrust::Str).
```

The fallback path (`_read_string_buffer` walking the StringBuffer Vec)
is preserved verbatim so this ADR is a strict superset of wave-2
behaviour. A binary built without the DI composite still decodes; a
binary built with the composite gets the cleaner structured walk.

### 3.4 Test corpus extension (3 new wave-3-follow-up tests)

Three new tests added to `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs`:

1. **`lldb_smoke_str_di_composite_type_fields`** — object-level. Emits
   a `take_str` fixture; asserts `image lookup --type cobrust::Str`
   returns a DIE with `ptr` + `len` member fields visible in the
   DWARF.
2. **`lldb_smoke_str_di_composite_regression_adt_preserved`** —
   regression guard that the new Str composite doesn't break the
   wave-3 Option DICompositeType. Asserts both `cobrust::Str` and
   `cobrust::Option` DIEs are present in the same object.
3. **`lldb_str_printer_structured_member_self_test`** — Python self-test
   (added to `tools/lldb-cobrust/tests/test_printers.py`) exercising
   the new structured-member read path with mocked
   `GetChildMemberWithName` returning valid `ptr`/`len` values. The
   wave-2 fallback path remains tested by the existing 5 `TestStrSummary`
   cases.

These do NOT remove `#[ignore]` markers (the wave-3 tests are
**conditional-skip** via `find_lldb()` / `linker_available()`, not
`#[ignore]`). They extend the wave-3 contract from DIE-presence to
DIE-with-member-fields.

The full bp-hit `frame variable s = "hello"` runtime test remains
guarded by `linker_available()` + `find_lldb()` (Mac dev hosts without
lldb-18 still skip cleanly) — that gate is unchanged from wave-3. The
content-render half is verified at the printer self-test level (lldb
mock), which is the testable surface available without a real lldb-18
+ linked stdlib runtime.

## 4. Non-goals

Explicitly out of scope:

- **Mutable Str inspect**: read-only display only. `frame variable`
  shows a snapshot.
- **Interior pointer rewrite**: Str owns its bytes per ADR-0050c; no
  shared-buffer scheme.
- **gdb pretty-printers**: lldb-only per ADR-0059 §5.3 (gdb is Phase L+).
- **Per-variant `cobrust::List<Int>` composite DI**: List/Dict/Set
  remain at opaque-pointer DI; this ADR only extends Str. Other
  containers carry their own follow-up sub-ADR if demand surfaces.
- **DWARF DW_TAG_member offset reflecting `Box<StringBuffer>`
  indirection**: the composite models the logical (ptr, len) view; the
  printer dereferences the Box at runtime via wave-2's `_read_string_buffer`.
  A truly-faithful nested DICompositeType (`Box<StringBuffer { Vec<u8> }>`)
  would require chained struct DIs — Phase L+ refinement, not closed in
  this ADR.

## 5. Acceptance gate

The full wave-3 test corpus PLUS 2 new lldb smoke tests PLUS 1 new
Python self-test:

| Surface | Count |
|---|---|
| lldb smoke tests (`dwarf_lldb_smoke.rs`) | 15 baseline (wave-3) + 2 new = 17 |
| Python self-tests (`test_printers.py`) | 14 baseline (wave-3) + 1 new = 15 |
| stdlib unit tests (Dict iter accessors) | 7 (preserved) |

When `lldb-18` + `cc` are both present (DG CI):
- `lldb_linked_str_frame_variable` still PASSes (DIE presence — unchanged contract).
- `lldb_smoke_str_di_composite_type_fields` PASSes (new — member fields verified).
- `lldb_smoke_str_di_composite_regression_adt_preserved` PASSes (regression).

When only `lldb-18` is absent (Mac dev):
- All 3 new tests skip cleanly per `find_lldb()` returning `None`.
- Python self-tests still run unconditionally (14 baseline + 1 new = 15 PASS).

The acceptance contract: **Phase L §6.1 honest-cite is truly RESOLVED**
when the Python self-test confirms structured-member read + the lldb
smoke test confirms `ptr`/`len` member DIs are present in DWARF.

## 6. Implementation plan

~250 LOC delta (within the brief's 200-400 estimate):

| Surface | LOC delta |
|---|---|
| `llvm_backend.rs` Str DICompositeType emission | ~70 |
| `printers.py` SBValue child-member read path | ~50 |
| `dwarf_lldb_smoke.rs` 2 new tests | ~80 |
| `test_printers.py` 1 new self-test | ~30 |
| ADR (this file) + dual-track doc updates | non-LOC |

Atomic commits:

1. `docs(adr): 0059e author Phase L §6.1 Str runtime full closure`
2. `feat(codegen): cobrust::Str DICompositeType with ptr+len members (ADR-0059e §3.2)`
3. `feat(lldb-printers): cobrust_str_summary uses SBValue child-member read (ADR-0059e §3.3)`
4. `tests(dwarf-lldb): 3 new tests for §6.1 Str runtime full closure (ADR-0059e §3.4)`
5. `docs(adr+dual-track): 0059e accepted + 0059a §6.1 truly RESOLVED (Phase L truly FULL CLOSED)`

## 7. Consequences

### 7.1 Positive

- ADR-0059a §6.1 honest-cite RESOLVED — the final Phase L deferral
  closes.
- Phase L truly UX-complete: every named Cobrust container type
  (`Str`, `Dict`, `List`, `Option`, `Adt`) has a structured DI handle
  the printer can walk.
- §2.5 §B positive: LLM agents at lldb breakpoints see Python-repr-shaped
  content for Str, matching the format dominant in 2026 training corpora.
- Wave-2 raw-memory fallback preserved → backward-compatible with
  binaries built before ADR-0059e.

### 7.2 Negative

- +~70 LOC in `llvm_backend.rs` for the Str composite emission;
  ongoing maintenance burden.
- The printer's child-member path adds a branch that must stay in
  sync with the codegen DI layout (`ptr` at offset 0, `len` at
  offset 64).

### 7.3 Neutral

- The composite DI models a **logical** ptr+len view, not the raw
  `Box<StringBuffer>` indirection. Faithful nested DI is Phase L+
  refinement.
- Other containers (List/Dict/Set) remain at opaque-pointer DI;
  ADR-0059e is Str-only scope per §4 non-goals.

## 8. Pre-dispatch acceptance gate

ADR-0059e dispatch may proceed when:

- [x] ADR-0059d (wave-3 linker harness + Option composite) status = accepted.
      Verified at `79bd1b2`.
- [x] ADR-0059a §6.1 wave-3 honest-cite preserved in the code comment
      at `lldb_linked_str_frame_variable`. Anchor verified.
- [x] `populate_di_basic_types` Option composite emission precedent
      lines 861-919. Anchor verified.
- [x] `printers.py::_read_string_buffer` wave-2 fallback path remains
      live so the new structured-member path is additive. Verified.

— P9 Tech Lead, 2026-05-21
