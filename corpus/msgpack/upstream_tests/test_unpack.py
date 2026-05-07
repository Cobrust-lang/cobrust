"""Vendored upstream subset of msgpack-python's test_unpack tests.

Pinned to msgpack-python 1.0.8. Asserts unpack rejects truncated /
malformed inputs and accepts the M6 value-scope round-trips.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
UPSTREAM = os.path.join(os.path.dirname(HERE), "upstream")
sys.path.insert(0, UPSTREAM)

from msgpack_core import pack, unpack, UnpackException  # noqa: E402


def test_truncated_uint_rejected():
    try:
        unpack(b"\xcc")  # uint8 marker without payload
    except UnpackException:
        return
    raise AssertionError("expected UnpackException")


def test_unknown_marker_rejected():
    try:
        unpack(b"\xc1")  # 0xc1 is reserved per spec
    except UnpackException:
        return
    raise AssertionError("expected UnpackException for 0xc1")


def test_trailing_bytes_rejected():
    payload = pack(42) + b"\x00"
    try:
        unpack(payload)
    except UnpackException:
        return
    raise AssertionError("expected UnpackException for trailing bytes")


def test_array_round_trip():
    v = [1, "x", b"y", None, [True, False]]
    assert unpack(pack(v)) == v


def test_map_round_trip():
    v = {"a": 1, "b": [1, 2], "c": {"nested": True}}
    assert unpack(pack(v)) == v


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
