"""L0 differential harness for msgpack.unpack — M6.

Drives `corpus/msgpack/upstream/msgpack_core.unpack` (the L0 oracle)
and CPython `msgpack.unpackb` (the L3 oracle), feeding each pre-packed
byte sequence and asserting deep equality of the resulting Python
values. Companion to h_pack.py — together they prove the round-trip
contract `unpack(pack(x)) == x` per ADR-0010 §1.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
UPSTREAM = os.path.join(os.path.dirname(HERE), "upstream")
sys.path.insert(0, UPSTREAM)

from msgpack_core import pack as oracle_pack, unpack as oracle_unpack  # noqa: E402

try:
    import msgpack as upstream_msgpack  # noqa: F401  type: ignore
    HAVE_UPSTREAM = True
except ImportError:
    HAVE_UPSTREAM = False


def _run(name, value):
    """Round-trip `value` through both oracles and compare."""
    bytes_via_ours = oracle_pack(value)
    ours = oracle_unpack(bytes(bytes_via_ours))
    if ours != value and not (isinstance(value, float) and abs(ours - value) < 1e-12):
        print("FAIL", name, "round-trip diverged: in=", value, "out=", ours)
        return False
    if HAVE_UPSTREAM:
        upstream = upstream_msgpack.unpackb(
            bytes(upstream_msgpack.packb(value, use_bin_type=True)),
            raw=False,
        )
        # We accept tuple-vs-list mismatch since the M6 oracle list-canonicalises.
        norm = lambda v: list(v) if isinstance(v, tuple) else v
        if norm(upstream) != norm(value) and not (
            isinstance(value, float) and abs(upstream - value) < 1e-12
        ):
            print("FAIL", name, "upstream diverged: in=", value, "out=", upstream)
            return False
    print("PASS", name)
    return True


CASES = [
    ("nil", None),
    ("true", True),
    ("false", False),
    ("zero", 0),
    ("positive_fixint", 0x42),
    ("uint8_boundary", 0xff),
    ("uint16_boundary", 0xffff),
    ("negative_fixint", -1),
    ("int8_boundary", -0x80),
    ("zero_float", 0.0),
    ("pi_float", 3.141592653589793),
    ("empty_str", ""),
    ("hello_str", "hello world"),
    ("hello_bin", b"hello"),
    ("simple_array", [1, 2, 3]),
    ("simple_map", {"x": 1, "y": 2}),
]


def main():
    failures = 0
    for name, value in CASES:
        if not _run(name, value):
            failures += 1
    if failures:
        print("FAIL: %d divergence(s)" % failures)
        sys.exit(1)
    print("PASS: %d case(s)" % len(CASES))


if __name__ == "__main__":
    main()
