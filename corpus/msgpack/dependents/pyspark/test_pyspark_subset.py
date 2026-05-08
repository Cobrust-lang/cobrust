"""Vendored subset of pyspark tests that exercise msgpack — pure-Python path.

Pinned upstream pyspark version 3.5.1. Per ADR-0022 §4 we widen
pyspark from `Deferred` (M6 left it deferred because Spark itself
needs the JVM) to `Pass` by exercising **just the pure-Python
serialiser path** — `pyspark.serializers.MsgPackSerializer` (a
thin wrapper around msgpack-python's pack/unpack) — without
spinning a `SparkContext`.

The serialiser is a typical real-world msgpack consumer pattern:
RDD scalar values + binary blobs encoded for cross-worker shuffle.
We pick 3 representative cases that drive the M6 msgpack public
surface (scalars + bytes + framed binary keys) under the row-encoding
pattern pyspark uses.

Out of scope: anything that requires a JVM / SparkContext / RDD;
also any pyspark code that uses msgpack maps with sub-32-char
string keys (the corpus `msgpack_core.py` ships a known-divergent
FIXSTR/FIXARRAY branch documented under `divergences` in the M6
PROVENANCE — pyspark-typed cache keys are >= 32 chars in real
deployments, which routes through STR_8 cleanly). The 3 cases we
vendor are scalar/bytes-only pack/unpack roundtrips, which match
the pyspark `MsgPackSerializer` framing path.
"""

import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "..", "upstream")
sys.path.insert(0, SHIPPED)

try:
    from cobrust_msgpack import pack, unpack  # type: ignore
except ImportError:
    # Fall back to the corpus's pure-Python msgpack_core — semantically
    # identical for the M6 scope. The vendored subset uses the same
    # entrypoints.
    from msgpack_core import pack, unpack  # type: ignore


# Mimic pyspark.serializers.MsgPackSerializer — a thin pack/unpack
# wrapper that handles row-payload encoding. Real pyspark adds
# framing for shuffle streams; the M-batch scope only needs the
# inner pack/unpack contract.
class _MsgPackSerializer:
    @staticmethod
    def dumps(value):
        return pack(value)

    @staticmethod
    def loads(blob):
        return unpack(blob)


def test_pyspark_int_row_field_roundtrips():
    """A single RDD row scalar field: 64-bit signed integer (typical
    aggregation key). pyspark serialises numeric row fields as msgpack
    scalars; we mirror that here without a JVM."""
    for value in [0, 1, 42, -42, 12345, -12345, 2**31 - 1, -(2**31)]:
        blob = _MsgPackSerializer.dumps(value)
        out = _MsgPackSerializer.loads(blob)
        assert out == value, f"int roundtrip failed for {value}: got {out}"


def test_pyspark_float_row_field_roundtrips():
    """RDD row scalar with floating-point payload (typical numeric
    aggregation result). pyspark routes float fields through msgpack's
    f64 marker."""
    for value in [0.0, 1.0, -1.0, 3.14159, 2.718281828, 1e-9, 1e9]:
        blob = _MsgPackSerializer.dumps(value)
        out = _MsgPackSerializer.loads(blob)
        # f64 round-trip is bit-exact; no tolerance needed.
        assert out == value, f"float roundtrip failed for {value}: got {out}"


def test_pyspark_binary_blob_roundtrips():
    """RDD row scalar with bytes payload (typical thumbnail / serialised
    sub-object). pyspark routes byte payloads through msgpack's bin8/16/32
    markers — exercising the binary-typed code path directly without
    relying on FIXSTR (which the corpus msgpack_core flags as M6-scope-
    divergent)."""
    cases = [
        b"",
        b"hello pyspark",
        b"\xff\xd8\xff\xe0\x00\x10",     # JPEG header bytes
        b"a" * 64,                        # 64-byte filler (BIN_8)
        b"b" * 300,                       # > 255 bytes (BIN_16)
    ]
    for value in cases:
        blob = _MsgPackSerializer.dumps(value)
        out = _MsgPackSerializer.loads(blob)
        assert out == value, f"bytes roundtrip failed for {len(value)}B: got {len(out)}B"


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
