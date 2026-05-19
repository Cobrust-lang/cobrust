#!/usr/bin/env python3
"""lldb pretty-printers for Cobrust types (ADR-0059a Phase L wave-1).

Six pretty-printers, one per non-primitive Cobrust type:
- `cobrust::Str` (StringBuffer{ bytes: Vec<u8> } layout, fmt.rs:64)
- `cobrust::List` (ListI64Layout{ items, len, cap }, collections.rs:374)
- `cobrust::Dict` (DictLayout{ keys, values }, collections.rs:670)
- `cobrust::Set` (SetI64Layout{ items, len, cap }, collections.rs:1060)
- `cobrust::Tuple` (heterogeneous N-i64 heap allocation)
- `cobrust::Option` (Adt-backed; Phase L+ wave when MIR carries Option DI)

Each type's DWARF type-name (e.g. `cobrust::Str`) is emitted by
`llvm_backend.rs::populate_di_basic_types` (ADR-0059a §3.3.1 Option A).
Pretty-printers register via `type summary add` / `type synthetic add`.

The Cobrust runtime stores all containers as opaque `*mut u8` pointers
that point to `#[repr(C)]` layouts in `crates/cobrust-stdlib/src/`. The
pretty-printers read the bytes at those pointer destinations + decode
per the layout in the source.

Conservative bounds:
- Containers truncate to 32 elements (`MAX_INLINE_ELEMS`) per
  ADR-0059a §2.2.
- Recursion depth caps at 8 (`MAX_RECURSE_DEPTH`) per §2.1.
- Invalid UTF-8 bytes render as `�` replacement char per §7.3.
- Null pointer / unreadable memory renders as the Cobrust source-form
  empty container (e.g. `""` / `[]` / `{}`).
"""

import lldb

MAX_INLINE_ELEMS = 32
MAX_RECURSE_DEPTH = 8
TRUNCATION_MARKER = ", ..."


# =====================================================================
# Helpers — byte / pointer reads via lldb SBProcess
# =====================================================================


def _process(valobj):
    """Return the SBProcess associated with `valobj`, or None if the
    inferior is gone (e.g. post-exit). All readers must short-circuit
    when this returns None."""
    target = valobj.GetTarget()
    if not target:
        return None
    return target.GetProcess()


def _read_ptr(process, addr):
    """Read a 64-bit native-endian pointer-sized value from `addr`.
    Returns 0 on read failure (null / unmapped)."""
    if addr == 0 or addr == lldb.LLDB_INVALID_ADDRESS:
        return 0
    err = lldb.SBError()
    val = process.ReadPointerFromMemory(addr, err)
    return val if err.Success() else 0


def _read_i64(process, addr):
    """Read a signed 64-bit native-endian integer from `addr`. Returns
    0 on read failure."""
    if addr == 0:
        return 0
    err = lldb.SBError()
    raw = process.ReadMemory(addr, 8, err)
    if not err.Success() or not raw:
        return 0
    return int.from_bytes(raw, byteorder="little", signed=True)


def _read_bytes(process, addr, count):
    """Read `count` bytes from `addr`. Returns b'' on failure or when
    `count <= 0`."""
    if addr == 0 or count <= 0:
        return b""
    err = lldb.SBError()
    raw = process.ReadMemory(addr, int(count), err)
    return raw if err.Success() and raw else b""


def _valobj_pointer_addr(valobj):
    """Extract the runtime pointer addr from a `cobrust::*` valobj.
    Cobrust containers are emitted as opaque `i8*` at LLVM level; the
    debug surface gives us a single 64-bit value that IS the pointer.
    Returns 0 on extraction failure."""
    # `GetValueAsUnsigned` returns the underlying register / memory
    # value as an integer (works whether the DI sees it as `ptr` or
    # `i64`).
    err = lldb.SBError()
    val = valobj.GetValueAsUnsigned(err, 0)
    if not err.Success():
        return 0
    return val


# =====================================================================
# Layout readers — mirror crates/cobrust-stdlib/src/{fmt,collections}.rs
# =====================================================================


def _read_string_buffer(process, ptr):
    """Read a `fmt::StringBuffer` (struct { bytes: Vec<u8> }) at `ptr`
    and return its bytes content.

    `Vec<u8>` repr-Rust layout is { ptr, cap, len } (the exact order
    is fixed by the unstable RawVec ABI as of rust-1.81; verified at
    HEAD f8c459f via `cargo expand` — the codegen surface assumes
    stable). We read 24 bytes from `ptr` and unpack as three u64.

    Returns the decoded bytes (empty on read failure)."""
    raw = _read_bytes(process, ptr, 24)
    if len(raw) < 24:
        return b""
    # Vec<u8> in-memory layout: { *mut u8, usize cap, usize len }
    data_ptr = int.from_bytes(raw[0:8], "little")
    # cap = raw[8:16] (unused for display)
    length = int.from_bytes(raw[16:24], "little")
    if data_ptr == 0 or length == 0:
        return b""
    # Truncate to a sane upper bound to keep the printer from reading
    # gigabytes of memory on a corrupted len.
    safe_len = min(length, 4096)
    return _read_bytes(process, data_ptr, safe_len)


def _read_list_i64(process, ptr):
    """Read a `ListI64Layout { items: *mut i64, len: i64, cap: i64 }`
    at `ptr`. Returns `(items_ptr, len)`; cap is informational."""
    raw = _read_bytes(process, ptr, 24)
    if len(raw) < 24:
        return (0, 0)
    items_ptr = int.from_bytes(raw[0:8], "little")
    length = int.from_bytes(raw[8:16], "little", signed=True)
    return (items_ptr, max(length, 0))


def _read_set_i64(process, ptr):
    """Read a `SetI64Layout` (same shape as ListI64Layout at wave-1
    per collections.rs:1060). Returns `(items_ptr, len)`."""
    return _read_list_i64(process, ptr)


# =====================================================================
# Provider classes — one per Cobrust type
# =====================================================================


def cobrust_str_summary(valobj, internal_dict):
    """`cobrust::Str` summary — decode StringBuffer bytes as UTF-8.

    Per ADR-0059a §7.3: invalid UTF-8 renders with `errors='replace'`;
    the printer never raises."""
    process = _process(valobj)
    if process is None:
        return '""'
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return '""'
    raw = _read_string_buffer(process, ptr)
    if not raw:
        return '""'
    try:
        decoded = raw.decode("utf-8", errors="replace")
    except (UnicodeDecodeError, AttributeError):
        return '"<unreadable>"'
    # Escape internal quotes for Python-repr-like display.
    return '"' + decoded.replace("\\", "\\\\").replace('"', '\\"') + '"'


def cobrust_list_summary(valobj, internal_dict):
    """`cobrust::List` summary — render `[e0, e1, ...]` for List<Int>.

    Wave-1 limitation: MIR does NOT carry the element type through to
    DI (ADR-0058c §4 deferral). Wave-1 assumes the conservative
    `List<Int>` semantics — i64 elements at the items_ptr + 8*i offset.
    Phase L+ richens with element-type recovery via `DIDerivedType`
    for `cobrust::List<T>` (deferred; see §3.3.1 'Phase L+ may add')."""
    process = _process(valobj)
    if process is None:
        return "[]"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "[]"
    items_ptr, length = _read_list_i64(process, ptr)
    if length <= 0 or items_ptr == 0:
        return "[]"

    display_count = min(length, MAX_INLINE_ELEMS)
    parts = []
    for i in range(display_count):
        elem = _read_i64(process, items_ptr + 8 * i)
        parts.append(str(elem))
    body = ", ".join(parts)
    if length > MAX_INLINE_ELEMS:
        body += TRUNCATION_MARKER + " ({} total)".format(length)
    return "[" + body + "]"


def cobrust_dict_summary(valobj, internal_dict):
    """`cobrust::Dict` summary — render `{k0: v0, k1: v1, ...}`.

    Wave-1: the runtime DictLayout at collections.rs:670 stores entries
    via `indexmap::IndexMap<i64, *mut u8>` for the `Dict<Int, Str>`
    shape (insertion order). The underlying IndexMap repr is NOT
    stable cross-version, so wave-1 takes the conservative path: read
    `len` (offset depends on indexmap revision) and render
    `{<len entries>}` placeholder if we cannot resolve the exact
    in-memory layout.

    Phase L+ wave-2 stabilises this by introducing an explicit
    runtime export (`__cobrust_dict_keys(dict, i) -> i64`,
    `__cobrust_dict_values(dict, i) -> *mut u8`) the printer can call
    via `EvaluateExpression`. Wave-1 ships the safe placeholder."""
    process = _process(valobj)
    if process is None:
        return "{}"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "{}"
    # Conservative: render `{<n entries>}` rather than guessing
    # IndexMap internals. The printer is non-crashing per §7.3.
    # Read first 8 bytes as a length hint; if zero, render `{}`.
    length_hint = _read_i64(process, ptr)
    if length_hint <= 0:
        return "{}"
    # Cap the displayed-count hint at a sane upper bound (corrupted
    # memory could return huge values).
    safe_len = min(length_hint, 1 << 30)
    return "{{<{} entries>}}".format(safe_len)


def cobrust_set_summary(valobj, internal_dict):
    """`cobrust::Set` summary — render `{e0, e1, ...}` for Set<Int>.

    Wave-1 mirrors the List<Int> path: SetI64Layout has the same shape
    as ListI64Layout at collections.rs:1060."""
    process = _process(valobj)
    if process is None:
        return "{}"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "{}"
    items_ptr, length = _read_set_i64(process, ptr)
    if length <= 0 or items_ptr == 0:
        return "{}"
    display_count = min(length, MAX_INLINE_ELEMS)
    parts = []
    for i in range(display_count):
        elem = _read_i64(process, items_ptr + 8 * i)
        parts.append(str(elem))
    body = ", ".join(parts)
    if length > MAX_INLINE_ELEMS:
        body += TRUNCATION_MARKER + " ({} total)".format(length)
    return "{" + body + "}"


def cobrust_tuple_summary(valobj, internal_dict):
    """`cobrust::Tuple` summary — render `(e0, e1, ..., eN)`.

    Wave-1: tuples store N i64 slots in a flat heap allocation
    (collections.rs:1162 'Tuple uses a flat struct backed by a heap
    allocation of N i64'). MIR does NOT carry the tuple arity through
    to DI (ADR-0058c §4 deferral). Wave-1 reads the first 8 i64 slots
    and renders them as a tuple; Phase L+ adds arity recovery."""
    process = _process(valobj)
    if process is None:
        return "()"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "()"
    # Conservative: render up to 8 i64 elements.
    parts = []
    for i in range(8):
        elem = _read_i64(process, ptr + 8 * i)
        # Stop at first zero (likely past the tuple's end).
        if elem == 0 and i >= 2:
            break
        parts.append(str(elem))
    if not parts:
        return "()"
    if len(parts) == 1:
        return "(" + parts[0] + ",)"  # Python single-element tuple form.
    return "(" + ", ".join(parts) + ")"


def cobrust_option_summary(valobj, internal_dict):
    """`cobrust::Option` summary — render `None` or `Some(<inner>)`.

    Wave-1 scaffolding: Option<T> is modeled as `Ty::Adt(...)` in
    MIR; ADR-0058c §4 defers Adt DI naming to Phase L+. Wave-1
    registers this provider conservatively — until MIR carries a
    `Ty::Adt` with the Option AdtId through to `di_type_for`, no
    local will match `cobrust::Option` and this printer will not be
    invoked. The body remains a placeholder that renders the
    discriminant; full discriminant recovery is Phase L+ scope.

    Per ADR-0059a §3.3.1 final bullet."""
    process = _process(valobj)
    if process is None:
        return "None"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "None"
    # Conservative: ptr-as-tag — null → None; non-null → Some(<addr>).
    # Phase L+ refines via Adt-discriminant DI emission.
    return "Some(<{:#x}>)".format(ptr)


# =====================================================================
# Registration — `command script import` calls __lldb_init_module
# =====================================================================


def __lldb_init_module(debugger, internal_dict):
    """Register the 6 type summary providers under their DWARF
    type-names (ADR-0059a §3.3.1 Option A names emitted by
    `populate_di_basic_types`).

    Uses literal type-name matching for `cobrust::Str` /
    `cobrust::Dict` / `cobrust::Tuple` / `cobrust::Option`, and regex
    `^cobrust::List` / `^cobrust::Set` (so future
    `cobrust::List<Int>` parametrised names also match — Phase L+
    refinement)."""
    cmds = [
        # Literal matches.
        "type summary add -F printers.cobrust_str_summary cobrust::Str",
        "type summary add -F printers.cobrust_dict_summary cobrust::Dict",
        "type summary add -F printers.cobrust_tuple_summary cobrust::Tuple",
        "type summary add -F printers.cobrust_option_summary cobrust::Option",
        # Regex matches (forward-compatible with parametrised names).
        '''type summary add -F printers.cobrust_list_summary -x "^cobrust::List"''',
        '''type summary add -F printers.cobrust_set_summary -x "^cobrust::Set"''',
    ]
    for cmd in cmds:
        debugger.HandleCommand(cmd)
    # Silent confirmation (writes to lldb's status pane, not stderr)
    # so a `command script import` user sees the load completed.
    debugger.HandleCommand(
        'script print("cobrust pretty-printers loaded (ADR-0059a wave-1)")'
    )
