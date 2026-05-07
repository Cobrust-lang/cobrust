"""Vendored upstream subset of msgpack-python's test_pack tests.

Pinned to msgpack-python 1.0.8. Tests a representative slice of the
M6 value scope (nil/bool/int/float/str/bytes/array/map). The full
upstream test bank is at
https://github.com/msgpack/msgpack-python/blob/v1.0.8/test/test_pack.py.

These are run by the M6 gate via CPython subprocess; failures emit
"FAIL <name>" and the L3 driver counts them.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
UPSTREAM = os.path.join(os.path.dirname(HERE), "upstream")
sys.path.insert(0, UPSTREAM)

from msgpack_core import pack, unpack  # noqa: E402


def test_nil_roundtrip():
    assert unpack(pack(None)) is None


def test_bool_roundtrip():
    assert unpack(pack(True)) is True
    assert unpack(pack(False)) is False


def test_positive_int_roundtrip():
    for v in (0, 1, 0x7f, 0x80, 0xff, 0x100, 0xffff, 0x10000):
        assert unpack(pack(v)) == v, "positive int %d failed" % v


def test_negative_int_roundtrip():
    for v in (-1, -0x20, -0x21, -0x80, -0x81, -0x8000):
        assert unpack(pack(v)) == v, "negative int %d failed" % v


def test_float_roundtrip():
    for v in (0.0, 3.141592653589793, -1.5):
        assert abs(unpack(pack(v)) - v) < 1e-12, "float %r failed" % v


def test_str_roundtrip():
    for v in ("", "x", "hello", "x" * 31, "x" * 32, "y" * 256):
        assert unpack(pack(v)) == v, "str of len %d failed" % len(v)


def test_bin_roundtrip():
    for v in (b"", b"x", b"x" * 256, b"y" * 65536):
        assert unpack(pack(v)) == v, "bin of len %d failed" % len(v)


def test_array_roundtrip():
    for v in ([], [1], [1, 2, 3], [[1, 2], [3, 4]]):
        assert unpack(pack(v)) == v, "array %r failed" % v


def test_map_roundtrip():
    # Note: M6 sorts keys for determinism, so dict equality holds.
    for v in ({}, {"a": 1}, {"x": 1, "y": 2}, {"k": [1, 2, 3]}):
        assert unpack(pack(v)) == v, "map %r failed" % v


if __name__ == "__main__":
    failures = []
    for name, fn in list(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print("PASS", name)
            except Exception as e:
                failures.append((name, str(e)))
                print("FAIL", name, e)
    sys.exit(1 if failures else 0)
