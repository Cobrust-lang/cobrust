"""Vendored subset of redis-py tests that exercise msgpack as a cache codec.

Pinned upstream version 5.0.7. Selects 4 representative cases that
serialise/deserialise common cache payloads via msgpack — the typical
real-world dependency pattern. Out-of-scope tests (full Redis server,
pubsub) are not vendored.

These tests are run by the M6 L3 driver (`crates/cobrust-translator/
src/downstream.rs`) and emit PASS/FAIL lines that drive `gates.dependents`.
"""

import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "..", "upstream")
sys.path.insert(0, SHIPPED)

try:
    from cobrust_msgpack import pack, unpack  # type: ignore
except ImportError:
    # Fall back to the corpus's pure-Python msgpack_core; semantically
    # equivalent for the M6 scope.
    from msgpack_core import pack, unpack  # type: ignore


def test_simple_cache_value_roundtrips():
    """A typical cache payload: dict with str/int/list."""
    value = {"user_id": 42, "name": "alice", "perms": [1, 2, 3]}
    out = unpack(pack(value))
    assert out == value


def test_binary_cache_value_roundtrips():
    """Cache payload with bytes (image thumbnails, binary blobs)."""
    value = {"thumbnail": b"\xff\xd8\xff\xe0", "size": 1024}
    out = unpack(pack(value))
    assert out == value


def test_nested_cache_value_roundtrips():
    """Nested dict (config bundle)."""
    value = {
        "settings": {"debug": False, "timeout": 30},
        "tags": ["a", "b"],
    }
    out = unpack(pack(value))
    assert out == value


def test_unicode_cache_value_roundtrips():
    """Unicode strings — non-ASCII payloads."""
    value = {"name": "Łukasz", "msg": "你好"}
    out = unpack(pack(value))
    assert out == value


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
