# msgpack-python _unpacker.pyx — Cython unpacker.
#
# Vendored subset of msgpack-python 1.0.8 (Apache-2.0). The translator's
# Cython lexical shim (crates/cobrust-translator/src/cython.rs) maps
# `cdef <type>` declarations to Rust types per ADR-0010 §2.

cimport cython


cdef inline unsigned int read_byte(bytes data, Py_ssize_t pos):
    """Read one byte at position `pos`."""
    return data[pos]


cdef long unpack_uint_cython(bytes data, Py_ssize_t pos, int n_bytes):
    """Read n_bytes of big-endian unsigned int.

    The Cython form gains explicit Py_ssize_t for pos so the emitted
    Rust signature uses `usize` directly.
    """
    cdef long value = 0
    cdef int i
    if pos + n_bytes > len(data):
        raise ValueError("truncated uint")
    for i in range(n_bytes):
        value = (value << 8) | data[pos + i]
    return value


cpdef object unpack_obj_cython(bytes data):
    """Top-level unpack entrypoint; returns the decoded Python object.

    `cpdef` exposes a Python-callable form. The translator emits this
    as `pub fn unpack_obj_cython(data: &[u8]) -> serde_json::Value`.
    """
    cdef object out
    cdef Py_ssize_t pos = 0
    out, pos = _unpack_dispatch(data, pos)
    if pos != len(data):
        raise ValueError("trailing bytes after value")
    return out


cdef tuple _unpack_dispatch(bytes data, Py_ssize_t pos):
    """Dispatch one value starting at pos. Returns (value, new_pos)."""
    cdef unsigned int marker
    if pos >= len(data):
        raise ValueError("EOF before value")
    marker = data[pos]
    # Defer to the fallback.py implementation for the body — the
    # Cython shim's value lies in typed entrypoints (`pack_uint_cython`,
    # `unpack_uint_cython`), not in re-implementing the dispatcher.
    from .fallback import _unpack_one as _u1
    return _u1(data, pos)
