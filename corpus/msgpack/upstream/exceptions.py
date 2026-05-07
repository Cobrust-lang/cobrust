"""msgpack-python exceptions.py — error types.

Vendored subset of msgpack-python 1.0.8 (Apache-2.0). M6 ships only
the two types fallback.py and the Cython sources raise.
"""


class PackException(Exception):
    """Raised when pack() receives a value outside the M6 scope."""


class UnpackException(Exception):
    """Raised when unpack() encounters malformed bytes."""
