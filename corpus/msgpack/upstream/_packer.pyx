# msgpack-python _packer.pyx — Cython packer.
#
# Vendored subset of msgpack-python 1.0.8 (Apache-2.0). M6 in-scope
# value types only (see corpus/msgpack/README.md). The translator's
# Cython lexical shim (crates/cobrust-translator/src/cython.rs) maps
# `cdef <type>` declarations to Rust types per ADR-0010 §2.

cimport cython


cdef inline int pack_byte(bytes_out, unsigned int value):
    """Append one byte to the output buffer."""
    bytes_out.append(value & 0xff)


cdef int pack_uint_cython(bytes_out, unsigned long value):
    """Pack a non-negative integer; mirrors fallback.pack_uint.

    The Cython form gains `unsigned long` typing so the emitted Rust
    signature can use `u64` directly instead of `serde_json::Value`.
    """
    if value <= 0x7f:
        bytes_out.append(value)
        return 0
    elif value <= 0xff:
        bytes_out.append(0xcc)
        bytes_out.append(value)
        return 0
    elif value <= 0xffff:
        bytes_out.append(0xcd)
        bytes_out.append((value >> 8) & 0xff)
        bytes_out.append(value & 0xff)
        return 0
    elif value <= 0xffffffff:
        bytes_out.append(0xce)
        bytes_out.append((value >> 24) & 0xff)
        bytes_out.append((value >> 16) & 0xff)
        bytes_out.append((value >> 8) & 0xff)
        bytes_out.append(value & 0xff)
        return 0
    else:
        bytes_out.append(0xcf)
        bytes_out.append((value >> 56) & 0xff)
        bytes_out.append((value >> 48) & 0xff)
        bytes_out.append((value >> 40) & 0xff)
        bytes_out.append((value >> 32) & 0xff)
        bytes_out.append((value >> 24) & 0xff)
        bytes_out.append((value >> 16) & 0xff)
        bytes_out.append((value >> 8) & 0xff)
        bytes_out.append(value & 0xff)
        return 0


cpdef bytes pack_obj_cython(object value):
    """Top-level pack entrypoint; returns bytes.

    `cpdef` exposes a Python-callable form. The translator emits this
    as `pub fn pack_obj_cython(value: serde_json::Value) -> Vec<u8>`.
    """
    cdef object out = bytearray()
    _pack_into(value, out)
    return bytes(out)


cdef int _pack_into(object value, object out):
    """Inner pack dispatcher; mirrors fallback.pack."""
    if value is None:
        out.append(0xc0)
        return 0
    if value is True:
        out.append(0xc3)
        return 0
    if value is False:
        out.append(0xc2)
        return 0
    if isinstance(value, int):
        if value >= 0:
            return pack_uint_cython(out, value)
        # Negative path uses fallback.pack_int via Python call.
        from .fallback import pack_int as _pi
        _pi(value, out)
        return 0
    if isinstance(value, float):
        from .fallback import pack_float as _pf
        _pf(value, out)
        return 0
    if isinstance(value, str):
        from .fallback import pack_str as _ps
        _ps(value, out)
        return 0
    if isinstance(value, (bytes, bytearray)):
        from .fallback import pack_bin as _pb
        _pb(value, out)
        return 0
    if isinstance(value, (list, tuple)):
        from .fallback import pack_array as _pa
        _pa(value, out)
        return 0
    if isinstance(value, dict):
        from .fallback import pack_map as _pm
        _pm(value, out)
        return 0
    raise TypeError("M6 scope: unsupported type")
