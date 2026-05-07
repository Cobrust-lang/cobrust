"""L0 differential harness for msgpack.pack — M6.

Drives both `corpus/msgpack/upstream/msgpack_core.pack` (the L0 oracle)
and the canonical CPython `msgpack.packb` (the L3 oracle). The
harness emits `bytes(out)` from each side and reports a divergence
when they differ. Constitution §4.2's "differential testing as oracle"
binding for the M6 native-extension translation per ADR-0010.

Usage: `python3 h_pack.py` runs a pinned 16-input mini-suite and
prints PASS/FAIL per case to stdout. Designed to be invoked from the
M6 integration tests (`crates/cobrust-msgpack/tests/msgpack_pipeline.rs`).
"""

import os
import sys
import json

HERE = os.path.dirname(os.path.abspath(__file__))
UPSTREAM = os.path.join(os.path.dirname(HERE), "upstream")
sys.path.insert(0, UPSTREAM)

from msgpack_core import pack as oracle_pack  # noqa: E402

try:
    import msgpack as upstream_msgpack  # noqa: F401  type: ignore
    HAVE_UPSTREAM = True
except ImportError:
    HAVE_UPSTREAM = False


def _run(name, value):
    """Pack `value` via both oracles and emit PASS/FAIL."""
    ours = oracle_pack(value)
    if HAVE_UPSTREAM:
        upstream = upstream_msgpack.packb(value, use_bin_type=True)
        if bytes(ours) == bytes(upstream):
            print("PASS", name)
            return True
        print("FAIL", name, "ours=", bytes(ours).hex(), "upstream=", bytes(upstream).hex())
        return False
    # No upstream — at least confirm we don't blow up.
    _ = bytes(ours)
    print("PASS_NO_UPSTREAM", name)
    return True


CASES = [
    ("nil", None),
    ("true", True),
    ("false", False),
    ("zero", 0),
    ("positive_fixint", 0x42),
    ("uint8_boundary", 0xff),
    ("uint16_boundary", 0xffff),
    ("uint32_boundary", 0xffff_ffff),
    ("negative_fixint", -1),
    ("int8_boundary", -0x80),
    ("int16_boundary", -0x8000),
    ("zero_float", 0.0),
    ("pi_float", 3.141592653589793),
    ("empty_str", ""),
    ("hello_str", "hello world"),
    ("empty_bin", b""),
    ("hello_bin", b"hello"),
    ("empty_array", []),
    ("simple_array", [1, 2, 3]),
    ("nested_array", [[1, 2], ["a", "b"]]),
    ("empty_map", {}),
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
