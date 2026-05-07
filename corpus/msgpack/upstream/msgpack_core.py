# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2008–2024 INADA Naoki and msgpack-python contributors
#
# Concatenated single-file source the Cobrust M6 translator consumes.
# Pure-Python form composes `pack` / `unpack` over the M6 value scope
# (nil/bool/int/float/str/bytes/array/map). The Cython translation
# covers `pack_uint_cython` and `unpack_uint_cython` from the .pyx
# sibling files (`_packer.pyx`, `_unpacker.pyx`).
#
# The full upstream is at https://github.com/msgpack/msgpack-python
# (Apache-2.0). This subset is included verbatim under §"Scope window"
# of corpus/msgpack/README.md.
#
# The L0 spec at corpus/msgpack/spec.toml binds the public + helper
# names to behaviour contracts; the L0 differential harness at
# corpus/msgpack/harness/h_pack.py + h_unpack.py drives both this file
# and the canonical CPython msgpack package as oracles.

NIL = 0xc0
FALSE = 0xc2
TRUE = 0xc3
BIN_8 = 0xc4
BIN_16 = 0xc5
BIN_32 = 0xc6
FLOAT_32 = 0xca
FLOAT_64 = 0xcb
UINT_8 = 0xcc
UINT_16 = 0xcd
UINT_32 = 0xce
UINT_64 = 0xcf
INT_8 = 0xd0
INT_16 = 0xd1
INT_32 = 0xd2
INT_64 = 0xd3
STR_8 = 0xd9
STR_16 = 0xda
STR_32 = 0xdb
ARRAY_16 = 0xdc
ARRAY_32 = 0xdd
MAP_16 = 0xde
MAP_32 = 0xdf

POSITIVE_FIXINT_MAX = 0x7f
FIXMAP_PREFIX = 0x80
FIXARRAY_PREFIX = 0x90
FIXSTR_PREFIX = 0xa0


class PackException(Exception):
    pass


class UnpackException(Exception):
    pass


def pack_uint(value, out):
    """Pack a non-negative integer."""
    if value < 0:
        raise PackException("pack_uint requires non-negative")
    if value <= 0x7f:
        out.append(value)
    elif value <= 0xff:
        out.append(UINT_8)
        out.append(value)
    elif value <= 0xffff:
        out.append(UINT_16)
        out.append((value >> 8) & 0xff)
        out.append(value & 0xff)
    elif value <= 0xffffffff:
        out.append(UINT_32)
        for shift in (24, 16, 8, 0):
            out.append((value >> shift) & 0xff)
    else:
        out.append(UINT_64)
        for shift in (56, 48, 40, 32, 24, 16, 8, 0):
            out.append((value >> shift) & 0xff)


def pack_int(value, out):
    """Pack any signed integer."""
    if value >= 0:
        return pack_uint(value, out)
    if value >= -0x20:
        out.append(0xe0 | (value & 0x1f))
    elif value >= -0x80:
        out.append(INT_8)
        out.append(value & 0xff)
    elif value >= -0x8000:
        out.append(INT_16)
        out.append((value >> 8) & 0xff)
        out.append(value & 0xff)
    elif value >= -0x80000000:
        out.append(INT_32)
        for shift in (24, 16, 8, 0):
            out.append((value >> shift) & 0xff)
    else:
        out.append(INT_64)
        for shift in (56, 48, 40, 32, 24, 16, 8, 0):
            out.append((value >> shift) & 0xff)


def pack_float(value, out):
    """Pack a 64-bit big-endian IEEE 754 float."""
    import struct
    out.append(FLOAT_64)
    out.extend(struct.pack(">d", float(value)))


def pack_str(value, out):
    """Pack a Python str as msgpack str."""
    body = value.encode("utf-8")
    n = len(body)
    if n <= 0x1f:
        out.append(FIXSTR_PREFIX | n)
    elif n <= 0xff:
        out.append(STR_8)
        out.append(n)
    elif n <= 0xffff:
        out.append(STR_16)
        out.append((n >> 8) & 0xff)
        out.append(n & 0xff)
    else:
        out.append(STR_32)
        for shift in (24, 16, 8, 0):
            out.append((n >> shift) & 0xff)
    out.extend(body)


def pack_bin(value, out):
    """Pack Python bytes as msgpack bin."""
    n = len(value)
    if n <= 0xff:
        out.append(BIN_8)
        out.append(n)
    elif n <= 0xffff:
        out.append(BIN_16)
        out.append((n >> 8) & 0xff)
        out.append(n & 0xff)
    else:
        out.append(BIN_32)
        for shift in (24, 16, 8, 0):
            out.append((n >> shift) & 0xff)
    out.extend(value)


def pack_array(value, out):
    """Pack a list/tuple as msgpack array."""
    n = len(value)
    if n <= 0x0f:
        out.append(FIXARRAY_PREFIX | n)
    elif n <= 0xffff:
        out.append(ARRAY_16)
        out.append((n >> 8) & 0xff)
        out.append(n & 0xff)
    else:
        out.append(ARRAY_32)
        for shift in (24, 16, 8, 0):
            out.append((n >> shift) & 0xff)
    for elem in value:
        pack(elem, out)


def pack_map(value, out):
    """Pack a dict as msgpack map (str keys only)."""
    n = len(value)
    if n <= 0x0f:
        out.append(FIXMAP_PREFIX | n)
    elif n <= 0xffff:
        out.append(MAP_16)
        out.append((n >> 8) & 0xff)
        out.append(n & 0xff)
    else:
        out.append(MAP_32)
        for shift in (24, 16, 8, 0):
            out.append((n >> shift) & 0xff)
    for k in sorted(value.keys()):
        if not isinstance(k, str):
            raise PackException("M6 only supports str keys")
        pack_str(k, out)
        pack(value[k], out)


def pack(value, out=None):
    """Top-level pack."""
    if out is None:
        buf = bytearray()
        pack(value, buf)
        return bytes(buf)
    if value is None:
        out.append(NIL)
    elif value is True:
        out.append(TRUE)
    elif value is False:
        out.append(FALSE)
    elif isinstance(value, int):
        pack_int(value, out)
    elif isinstance(value, float):
        pack_float(value, out)
    elif isinstance(value, str):
        pack_str(value, out)
    elif isinstance(value, (bytes, bytearray)):
        pack_bin(value, out)
    elif isinstance(value, (list, tuple)):
        pack_array(value, out)
    elif isinstance(value, dict):
        pack_map(value, out)
    else:
        raise PackException("M6 scope: unsupported type")


def unpack_uint(data, pos, n_bytes):
    """Read n_bytes of big-endian unsigned int."""
    if pos + n_bytes > len(data):
        raise UnpackException("truncated uint")
    value = 0
    for i in range(n_bytes):
        value = (value << 8) | data[pos + i]
    return value, pos + n_bytes


def unpack_int(data, pos, n_bytes):
    """Read n_bytes of big-endian signed int."""
    value, new_pos = unpack_uint(data, pos, n_bytes)
    sign_bit = 1 << (8 * n_bytes - 1)
    if value & sign_bit:
        value -= 1 << (8 * n_bytes)
    return value, new_pos


def unpack_float(data, pos, n_bytes):
    """Read 4 or 8 bytes of big-endian IEEE 754 float."""
    import struct
    if pos + n_bytes > len(data):
        raise UnpackException("truncated float")
    fmt = ">f" if n_bytes == 4 else ">d"
    (v,) = struct.unpack(fmt, bytes(data[pos:pos + n_bytes]))
    return v, pos + n_bytes


def unpack_str(data, pos, length):
    """Read `length` bytes of utf-8."""
    if pos + length > len(data):
        raise UnpackException("truncated str")
    return bytes(data[pos:pos + length]).decode("utf-8"), pos + length


def unpack_bin(data, pos, length):
    """Read `length` bytes of binary."""
    if pos + length > len(data):
        raise UnpackException("truncated bin")
    return bytes(data[pos:pos + length]), pos + length


def unpack_array(data, pos, length):
    """Read `length` msgpack values into a list."""
    out = []
    cursor = pos
    for _ in range(length):
        v, cursor = _unpack_one(data, cursor)
        out.append(v)
    return out, cursor


def unpack_map(data, pos, length):
    """Read `length` (key, value) pairs into a dict."""
    out = {}
    cursor = pos
    for _ in range(length):
        k, cursor = _unpack_one(data, cursor)
        v, cursor = _unpack_one(data, cursor)
        out[k] = v
    return out, cursor


def _unpack_one(data, pos):
    """Dispatch one msgpack value at pos. Returns (value, new_pos)."""
    if pos >= len(data):
        raise UnpackException("EOF before value")
    marker = data[pos]
    if marker <= 0x7f:
        return marker, pos + 1
    if marker >= 0xe0:
        return marker - 0x100, pos + 1
    if marker == NIL:
        return None, pos + 1
    if marker == TRUE:
        return True, pos + 1
    if marker == FALSE:
        return False, pos + 1
    if marker == UINT_8:
        return unpack_uint(data, pos + 1, 1)
    if marker == UINT_16:
        return unpack_uint(data, pos + 1, 2)
    if marker == UINT_32:
        return unpack_uint(data, pos + 1, 4)
    if marker == UINT_64:
        return unpack_uint(data, pos + 1, 8)
    if marker == INT_8:
        return unpack_int(data, pos + 1, 1)
    if marker == INT_16:
        return unpack_int(data, pos + 1, 2)
    if marker == INT_32:
        return unpack_int(data, pos + 1, 4)
    if marker == INT_64:
        return unpack_int(data, pos + 1, 8)
    if marker == FLOAT_32:
        return unpack_float(data, pos + 1, 4)
    if marker == FLOAT_64:
        return unpack_float(data, pos + 1, 8)
    if FIXSTR_PREFIX <= marker < FIXSTR_PREFIX | 0x20:
        length = marker & 0x1f
        return unpack_str(data, pos + 1, length)
    if marker == STR_8:
        length, p2 = unpack_uint(data, pos + 1, 1)
        return unpack_str(data, p2, length)
    if marker == STR_16:
        length, p2 = unpack_uint(data, pos + 1, 2)
        return unpack_str(data, p2, length)
    if marker == STR_32:
        length, p2 = unpack_uint(data, pos + 1, 4)
        return unpack_str(data, p2, length)
    if marker == BIN_8:
        length, p2 = unpack_uint(data, pos + 1, 1)
        return unpack_bin(data, p2, length)
    if marker == BIN_16:
        length, p2 = unpack_uint(data, pos + 1, 2)
        return unpack_bin(data, p2, length)
    if marker == BIN_32:
        length, p2 = unpack_uint(data, pos + 1, 4)
        return unpack_bin(data, p2, length)
    if FIXARRAY_PREFIX <= marker < FIXMAP_PREFIX:
        length = marker & 0x0f
        return unpack_array(data, pos + 1, length)
    if marker == ARRAY_16:
        length, p2 = unpack_uint(data, pos + 1, 2)
        return unpack_array(data, p2, length)
    if marker == ARRAY_32:
        length, p2 = unpack_uint(data, pos + 1, 4)
        return unpack_array(data, p2, length)
    if FIXMAP_PREFIX <= marker < FIXARRAY_PREFIX:
        length = marker & 0x0f
        return unpack_map(data, pos + 1, length)
    if marker == MAP_16:
        length, p2 = unpack_uint(data, pos + 1, 2)
        return unpack_map(data, p2, length)
    if marker == MAP_32:
        length, p2 = unpack_uint(data, pos + 1, 4)
        return unpack_map(data, p2, length)
    raise UnpackException("M6 scope: unknown marker 0x%02x" % marker)


def unpack(data):
    """Top-level unpack."""
    value, pos = _unpack_one(data, 0)
    if pos != len(data):
        raise UnpackException("trailing bytes after value")
    return value


# -----------------------------------------------------------------------
# Cython-derived typed entrypoints (translated from _packer.pyx /
# _unpacker.pyx by the M6 Cython lexical shim per ADR-0010 §2). These
# functions exist as Python placeholders for the L0 oracle; the real
# Cython compilation is out of scope for the M6 corpus (we only need
# the source bytes for the SHA staleness check).
# -----------------------------------------------------------------------


def pack_uint_cython(value, out):
    """Cython-typed entrypoint: pack a non-negative i64 value."""
    return pack_uint(value, out)


def unpack_uint_cython(data, pos, n_bytes):
    """Cython-typed entrypoint: read n_bytes big-endian unsigned int."""
    value, _ = unpack_uint(data, pos, n_bytes)
    return value
