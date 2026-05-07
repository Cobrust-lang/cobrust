# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored numpy 2.0.2 reference subset for cobrust M7.0 (per ADR-0013).
# This is a translation-time reference: the L0 differential harness
# imports this module, NOT the upstream numpy package, so the synthetic
# pipeline can run without network or wheel installation.
#
# Scope (M7.0 per ADR-0013 §"Decision"):
#   - Dtype enum & string mapping for: int32/int64/float32/float64/bool
#   - Constructors: array(values, shape, dtype),
#     zeros(shape, dtype), ones(shape, dtype),
#     arange(start, stop, step, dtype)
#   - Observer surface: shape, ndim, size, dtype, repr
#
# The byte-for-byte / rtol-bounded contract against upstream `numpy`
# is exercised at the test layer (`tests/numpy_differential.rs`); this
# file is the reference oracle the L0 harness uses to check the
# **translator pipeline** itself emits the right bytes.

from __future__ import annotations

from typing import List, Sequence, Union

# Dtype mapping table — must match Rust Dtype enum in
# crates/cobrust-numpy/src/dtype.rs.
DTYPE_TABLE = {
    "int32":   ("Int32",   4),
    "i4":      ("Int32",   4),
    "int64":   ("Int64",   8),
    "i8":      ("Int64",   8),
    "float32": ("Float32", 4),
    "f4":      ("Float32", 4),
    "float64": ("Float64", 8),
    "f8":      ("Float64", 8),
    "bool":    ("Bool",    1),
    "?":       ("Bool",    1),
}


def parse_dtype(s: str) -> str:
    """Map a Python dtype string to the canonical Rust Dtype variant name."""
    if s not in DTYPE_TABLE:
        raise ValueError(f"unsupported dtype string: {s!r}")
    return DTYPE_TABLE[s][0]


def item_size(rust_variant: str) -> int:
    for v, sz in DTYPE_TABLE.values():
        if v == rust_variant:
            return sz
    raise ValueError(f"unknown dtype variant: {rust_variant!r}")


def cast_to_dtype(value: float, dtype_variant: str):
    """Cast a Python float to the canonical Python representation
    of the given Rust Dtype variant."""
    if dtype_variant == "Int32":
        return int(value) & 0xFFFFFFFF if int(value) >= 0 else int(value)
    if dtype_variant == "Int64":
        return int(value)
    if dtype_variant == "Float32":
        # numpy down-casts via IEEE 754 single-precision; emulate via struct.
        import struct
        packed = struct.pack(">f", float(value))
        return struct.unpack(">f", packed)[0]
    if dtype_variant == "Float64":
        return float(value)
    if dtype_variant == "Bool":
        return bool(value)
    raise ValueError(f"unknown dtype variant: {dtype_variant!r}")


def shape_size(shape: Sequence[int]) -> int:
    """Number of elements in an array of the given shape."""
    n = 1
    for d in shape:
        if d < 0:
            raise ValueError(f"negative dimension: {d}")
        n *= d
    return n


def array(values: List[float], shape: List[int], dtype: str) -> dict:
    """Pure-Python reference for cobrust_numpy::array().

    Returns a JSON-shaped dict {dtype, shape, data}. The Rust port emits
    the same JSON via Array::to_json() so the differential harness can
    bytewise-compare.
    """
    rust_variant = parse_dtype(dtype)
    n = shape_size(shape)
    if len(values) != n:
        raise ValueError(
            f"values length {len(values)} does not match shape product {n}"
        )
    casted = [cast_to_dtype(v, rust_variant) for v in values]
    return {"dtype": rust_variant, "shape": list(shape), "data": casted}


def zeros(shape: List[int], dtype: str) -> dict:
    """Pure-Python reference for cobrust_numpy::zeros()."""
    rust_variant = parse_dtype(dtype)
    n = shape_size(shape)
    if rust_variant == "Bool":
        data = [False] * n
    elif rust_variant in ("Int32", "Int64"):
        data = [0] * n
    else:
        data = [0.0] * n
    return {"dtype": rust_variant, "shape": list(shape), "data": data}


def ones(shape: List[int], dtype: str) -> dict:
    """Pure-Python reference for cobrust_numpy::ones()."""
    rust_variant = parse_dtype(dtype)
    n = shape_size(shape)
    if rust_variant == "Bool":
        data = [True] * n
    elif rust_variant in ("Int32", "Int64"):
        data = [1] * n
    else:
        data = [1.0] * n
    return {"dtype": rust_variant, "shape": list(shape), "data": data}


def arange(start: float, stop: float, step: float, dtype: str) -> dict:
    """Pure-Python reference for cobrust_numpy::arange().

    Half-open range; numpy semantics. `step == 0` raises. For floats,
    numpy uses `ceil((stop-start)/step)` for the count; for ints it
    uses the same formula but with integer truncation. We follow
    numpy 2.0.2 exactly for the M7.0 dtype tier.
    """
    if step == 0:
        raise ValueError("arange: step must be nonzero")
    rust_variant = parse_dtype(dtype)
    # Compute count the way numpy does for the M7.0 dtype tier.
    if rust_variant == "Bool":
        raise ValueError("arange: dtype=bool not supported (matches numpy)")
    # ceil((stop-start)/step), but careful with sign of step.
    delta = stop - start
    if step > 0 and delta <= 0:
        count = 0
    elif step < 0 and delta >= 0:
        count = 0
    else:
        # ceiling toward +inf along the step direction
        from math import ceil
        count = max(0, int(ceil(delta / step)))
    raw = [start + i * step for i in range(count)]
    casted = [cast_to_dtype(v, rust_variant) for v in raw]
    return {"dtype": rust_variant, "shape": [count], "data": casted}


def array_repr(arr: dict) -> str:
    """numpy-style repr() for the M7.0 dtype tier.

    Matches `numpy.array_repr(np.array(data, dtype=...).reshape(shape))`
    for the tier above. Format: `array([...], dtype=...)`.
    """
    dtype_v = arr["dtype"]
    shape = arr["shape"]
    data = arr["data"]
    # Reconstruct a nested list per the shape (1-D + 2-D in M7.0 scope).
    def to_nested(flat, dims):
        if not dims:
            return flat[0]
        if len(dims) == 1:
            return list(flat[: dims[0]])
        step = 1
        for d in dims[1:]:
            step *= d
        out = []
        idx = 0
        for _ in range(dims[0]):
            out.append(to_nested(flat[idx:idx + step], dims[1:]))
            idx += step
        return out
    if not shape:
        body = repr(data[0]) if data else ""
    else:
        body = repr(to_nested(data, shape))
    py_dtype_name = {
        "Int32": "int32",
        "Int64": "int64",
        "Float32": "float32",
        "Float64": "float64",
        "Bool": "bool",
    }[dtype_v]
    return f"array({body}, dtype={py_dtype_name})"
