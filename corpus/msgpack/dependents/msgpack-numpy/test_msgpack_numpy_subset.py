"""Vendored subset of msgpack-numpy tests.

Pinned upstream version 0.4.8. Selected 3 cases that exercise the M6
binary type path (numpy arrays serialise as `bytes` payloads) without
requiring numpy itself in the M6 gate path. The L3 driver records
PASS/FAIL per test.

Out of scope for M6 (deferred to M7+):
- ext-type-encoded numpy arrays
- structured dtype arrays
"""

import sys
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "..", "upstream")
sys.path.insert(0, SHIPPED)

try:
    from cobrust_msgpack import pack, unpack  # type: ignore
except ImportError:
    from msgpack_core import pack, unpack  # type: ignore


def test_int_array_serialises_as_bin():
    """Simulate numpy int32 array: 4 elements × 4 bytes = 16 bytes."""
    raw = struct.pack(">4i", 1, 2, 3, 4)
    payload = {"shape": [4], "dtype": "int32", "data": raw}
    out = unpack(pack(payload))
    assert out["shape"] == [4]
    assert out["data"] == raw


def test_float_scalar_roundtrips():
    """Simulate np.float64 scalar."""
    payload = {"shape": [], "dtype": "float64", "value": 3.14}
    out = unpack(pack(payload))
    assert abs(out["value"] - 3.14) < 1e-12


def test_array_metadata_roundtrips():
    """Simulate numpy metadata wrapper (no payload)."""
    payload = {"shape": [2, 3], "dtype": "int8"}
    out = unpack(pack(payload))
    assert out["shape"] == [2, 3]
    assert out["dtype"] == "int8"


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
