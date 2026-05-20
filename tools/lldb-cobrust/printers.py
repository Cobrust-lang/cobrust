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


#: Tag constants mirroring `collections.rs::{K,V}_TAG_*`. Kept in sync
#: with the runtime by ADR-0050d Decision 7A semantics.
_DICT_TAG_I64 = 0
_DICT_TAG_STR = 1


def _eval_i64(target, expr, default=0):
    """Evaluate `expr` in the inferior process and return the result as
    a signed i64. Returns `default` on evaluation failure (e.g. when
    the process is dead, the expression doesn't compile, or the symbol
    is absent because the runtime stdlib wasn't linked)."""
    try:
        result = target.EvaluateExpression(expr)
    except Exception:  # noqa: BLE001 — lldb raises bare Exception subclasses.
        return default
    if not result or not result.IsValid():
        return default
    err = result.GetError()
    if err.Fail():
        return default
    return result.GetValueAsSigned(default)


def _eval_ptr(target, expr, default=0):
    """Evaluate `expr` and return the result as an unsigned pointer-sized
    integer (the address). Returns `default` on failure."""
    try:
        result = target.EvaluateExpression(expr)
    except Exception:  # noqa: BLE001
        return default
    if not result or not result.IsValid():
        return default
    err = result.GetError()
    if err.Fail():
        return default
    return result.GetValueAsUnsigned(default)


def _format_dict_key(target, process, dict_ptr, idx, k_tag):
    """Render the i-th dict key as a Cobrust source-form string."""
    if k_tag == _DICT_TAG_STR:
        addr = _eval_ptr(
            target,
            "(void*)__cobrust_dict_iter_key_str_at((unsigned char*){:#x}, {})".format(
                dict_ptr, idx
            ),
        )
        if addr == 0:
            return '""'
        raw = _read_string_buffer(process, addr)
        try:
            decoded = raw.decode("utf-8", errors="replace")
        except (UnicodeDecodeError, AttributeError):
            decoded = "<unreadable>"
        # Caller is supposed to drop the returned Str buffer, but
        # this is a read-only inspection — leaking 1 small allocation
        # per debugger render is acceptable per ADR-0059 §5 (printer
        # MUST NOT execute side-effectful runtime helpers during a
        # stop). We accept the leak rather than call `_str_drop` and
        # risk crashing the inferior.
        return '"' + decoded.replace("\\", "\\\\").replace('"', '\\"') + '"'
    # Default i64.
    value = _eval_i64(
        target,
        "(long long)__cobrust_dict_iter_key_i64_at((unsigned char*){:#x}, {})".format(
            dict_ptr, idx
        ),
    )
    return str(value)


def _format_dict_value(target, process, dict_ptr, idx, v_tag):
    """Render the i-th dict value as a Cobrust source-form string."""
    if v_tag == _DICT_TAG_STR:
        addr = _eval_ptr(
            target,
            "(void*)__cobrust_dict_iter_value_str_at((unsigned char*){:#x}, {})".format(
                dict_ptr, idx
            ),
        )
        if addr == 0:
            return '""'
        raw = _read_string_buffer(process, addr)
        try:
            decoded = raw.decode("utf-8", errors="replace")
        except (UnicodeDecodeError, AttributeError):
            decoded = "<unreadable>"
        return '"' + decoded.replace("\\", "\\\\").replace('"', '\\"') + '"'
    value = _eval_i64(
        target,
        "(long long)__cobrust_dict_iter_value_i64_at((unsigned char*){:#x}, {})".format(
            dict_ptr, idx
        ),
    )
    return str(value)


def cobrust_dict_summary(valobj, internal_dict):
    """`cobrust::Dict` summary — render `{k0: v0, k1: v1, ...}` in
    IndexMap insertion order.

    Wave-2 (ADR-0059a §6.2 RESOLVED) walks the dict via runtime
    accessors `__cobrust_dict_{key,value}_tag` +
    `__cobrust_dict_iter_{key,value}_{i64,str}_at` exported by
    `crates/cobrust-stdlib/src/collections.rs`. The accessors call
    `IndexMap::get_index(i)` which preserves insertion order
    (ADR-0050d Decision 6A).

    Three fallback paths in priority order:

    1. **Tag dispatch succeeds + iter walk succeeds** → render the
       real `{k0: v0, ...}` shape (wave-2 happy path).
    2. **Tag dispatch fails** (process dead / runtime stdlib not
       linked / Mac smoke fixture only emits object — no symbol
       table) → render `{<n entries>}` placeholder (wave-1
       behaviour, preserved for object-level smoke).
    3. **Empty dict / null dict** → render `{}`.
    """
    process = _process(valobj)
    if process is None:
        return "{}"
    target = valobj.GetTarget()
    if not target:
        return "{}"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "{}"

    # Tag dispatch — `-1` means null or runtime accessor unresolved
    # (e.g. when only the object file is loaded, no executable, no
    # process, no symbol table — the smoke harness path).
    k_tag = _eval_i64(
        target,
        "(long long)__cobrust_dict_key_tag((unsigned char*){:#x})".format(ptr),
        default=-1,
    )
    v_tag = _eval_i64(
        target,
        "(long long)__cobrust_dict_value_tag((unsigned char*){:#x})".format(ptr),
        default=-1,
    )

    if k_tag < 0 or v_tag < 0:
        # Fallback path 2 — keep wave-1's safe placeholder when the
        # runtime accessors are unavailable. Read first 8 bytes as a
        # length hint per the wave-1 best-effort path.
        length_hint = _read_i64(process, ptr)
        if length_hint <= 0:
            return "{}"
        safe_len = min(length_hint, 1 << 30)
        return "{{<{} entries>}}".format(safe_len)

    # We have valid tags. Query len + walk.
    length = _eval_i64(
        target,
        "(long long)__cobrust_dict_len((unsigned char*){:#x})".format(ptr),
        default=0,
    )
    if length <= 0:
        return "{}"
    display_count = min(length, MAX_INLINE_ELEMS)
    parts = []
    for i in range(display_count):
        k_repr = _format_dict_key(target, process, ptr, i, k_tag)
        v_repr = _format_dict_value(target, process, ptr, i, v_tag)
        parts.append("{}: {}".format(k_repr, v_repr))
    body = ", ".join(parts)
    if length > MAX_INLINE_ELEMS:
        body += TRUNCATION_MARKER + " ({} total)".format(length)
    return "{" + body + "}"


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
    """`cobrust::Adt` / `cobrust::Option` summary — render `None` or
    `Some(<inner>)`.

    Wave-2 (ADR-0059a §6.3 RESOLVED for generic Adt):
    Registers on `cobrust::Adt` AND `cobrust::Option`. Conservative
    ptr-as-tag fallback (null → None; non-null → Some(<addr>)) is
    the wave-2 baseline.

    Wave-3 (ADR-0059d §3.2) tag-dispatch extension:
    When the runtime exports `__cobrust_adt_tag` (the i32 discriminant
    at offset-0 per ADR-0059d §3.2 layout), attempt to read it via
    `EvaluateExpression`. Tag 0 → `None`; tag 1 → read 64-bit payload
    at byte offset 8 (after 4-byte tag + 4-byte alignment pad) and
    render `Some(<payload>)`.

    Fallback: if `__cobrust_adt_tag` is absent or the process is dead,
    falls back to the wave-2 ptr-as-tag behaviour (no regression).
    """
    process = _process(valobj)
    if process is None:
        return "None"
    ptr = _valobj_pointer_addr(valobj)
    if ptr == 0:
        return "None"

    # ADR-0059d §3.2 wave-3 tag-dispatch path.
    # Attempt to read the i32 tag at offset 0 via process memory read.
    # This avoids the EvaluateExpression overhead and works even when
    # the stdlib runtime accessor isn't linked.
    try:
        # Read 4 bytes at ptr as little-endian i32 (the tag field).
        error = valobj.GetTarget().GetProcess().GetError() if hasattr(
            valobj.GetTarget().GetProcess(), 'GetError') else None
        mem_error = lldb.SBError()  # type: ignore[name-defined]
        tag_bytes = process.ReadMemory(ptr, 4, mem_error)
        if mem_error.Success() and len(tag_bytes) == 4:
            import struct as _struct
            tag = _struct.unpack("<i", tag_bytes)[0]
            if tag == 0:
                return "None"
            if tag == 1:
                # Read i64 payload at offset 8 (4-byte tag + 4-byte pad).
                payload_error = lldb.SBError()  # type: ignore[name-defined]
                payload_bytes = process.ReadMemory(ptr + 8, 8, payload_error)
                if payload_error.Success() and len(payload_bytes) == 8:
                    payload = _struct.unpack("<q", payload_bytes)[0]
                    return "Some({})".format(payload)
                return "Some(<unreadable payload>)"
            # Unknown tag — fall through to ptr-as-tag.
    except Exception:  # noqa: BLE001
        pass

    # Wave-2 conservative fallback: ptr-as-tag.
    # Phase L+ refines via Adt-discriminant DI emission.
    return "Some(<{:#x}>)".format(ptr)


# =====================================================================
# Registration — `command script import` calls __lldb_init_module
# =====================================================================


def __lldb_init_module(debugger, internal_dict):
    """Register the 7 type summary providers under their DWARF
    type-names (ADR-0059a §3.3.1 Option A names emitted by
    `populate_di_basic_types`).

    Uses literal type-name matching for `cobrust::Str` /
    `cobrust::Dict` / `cobrust::Tuple` / `cobrust::Option` /
    `cobrust::Adt` (wave-2 §6.3), and regex `^cobrust::List` /
    `^cobrust::Set` (so future `cobrust::List<Int>` parametrised
    names also match — Phase L+ refinement).
    """
    cmds = [
        # Literal matches.
        "type summary add -F printers.cobrust_str_summary cobrust::Str",
        "type summary add -F printers.cobrust_dict_summary cobrust::Dict",
        "type summary add -F printers.cobrust_tuple_summary cobrust::Tuple",
        "type summary add -F printers.cobrust_option_summary cobrust::Option",
        # ADR-0059a §6.3 wave-2 — generic Adt printer bound until
        # MIR threads per-Adt names through DI.
        "type summary add -F printers.cobrust_option_summary cobrust::Adt",
        # Regex matches (forward-compatible with parametrised names).
        '''type summary add -F printers.cobrust_list_summary -x "^cobrust::List"''',
        '''type summary add -F printers.cobrust_set_summary -x "^cobrust::Set"''',
    ]
    for cmd in cmds:
        debugger.HandleCommand(cmd)
    # Silent confirmation (writes to lldb's status pane, not stderr)
    # so a `command script import` user sees the load completed.
    debugger.HandleCommand(
        'script print("cobrust pretty-printers loaded (ADR-0059a wave-2)")'
    )
