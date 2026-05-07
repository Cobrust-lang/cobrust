# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored numpy 2.0.2 reference subset for cobrust M7.1 (per ADR-0014).
# This is the pipeline-time reference: the L0 differential harness
# imports this module, NOT the upstream numpy package, so the synthetic
# pipeline can run without network or wheel installation.
#
# Scope (M7.1 per ADR-0014):
#   - Universal functions: add / sub / mul / div / pow.
#   - Comparison ufuncs: eq / ne / lt / le / gt / ge.
#   - Element-wise math: sin / cos / exp / log / sqrt.
#   - Broadcasting (right-aligned, size-1-expand, equal-or-mismatch).
#   - Type promotion (NEP 50, 5-dtype tier).
#
# The byte-for-byte / rtol-bounded contract against upstream `numpy`
# is exercised at the test layer (`tests/ufunc_differential.rs`); this
# file is the reference oracle the L0 harness uses to check the
# **translator pipeline** itself emits the right bytes.

from __future__ import annotations

import math
from typing import List, Sequence

# Dtype mapping table — must match Rust Dtype enum in
# crates/cobrust-numpy/src/dtype.rs.
DTYPES = ("Int32", "Int64", "Float32", "Float64", "Bool")


def result_type(a: str, b: str) -> str:
    """NumPy 2.x NEP 50 promotion table for the M7.0 5-dtype tier.

    Mirrors crates/cobrust-numpy/src/promote.rs::result_type.
    """
    pair = (a, b)
    if pair == (a, a):
        # Same-dtype path collapses below; fall through to the table.
        pass
    table = {
        ("Bool", "Bool"): "Bool",
        ("Bool", "Int32"): "Int32", ("Int32", "Bool"): "Int32",
        ("Bool", "Int64"): "Int64", ("Int64", "Bool"): "Int64",
        ("Bool", "Float32"): "Float32", ("Float32", "Bool"): "Float32",
        ("Bool", "Float64"): "Float64", ("Float64", "Bool"): "Float64",
        ("Int32", "Int32"): "Int32",
        ("Int32", "Int64"): "Int64", ("Int64", "Int32"): "Int64",
        ("Int32", "Float32"): "Float64", ("Float32", "Int32"): "Float64",
        ("Int32", "Float64"): "Float64", ("Float64", "Int32"): "Float64",
        ("Int64", "Int64"): "Int64",
        ("Int64", "Float32"): "Float64", ("Float32", "Int64"): "Float64",
        ("Int64", "Float64"): "Float64", ("Float64", "Int64"): "Float64",
        ("Float32", "Float32"): "Float32",
        ("Float32", "Float64"): "Float64", ("Float64", "Float32"): "Float64",
        ("Float64", "Float64"): "Float64",
    }
    if pair not in table:
        raise ValueError(f"unknown dtype pair: {pair}")
    return table[pair]


def unary_math_dtype(a: str) -> str:
    """Promote integer dtypes to Float64 for unary math; preserve floats."""
    return {"Int32": "Float64", "Int64": "Float64", "Bool": "Float64",
            "Float32": "Float32", "Float64": "Float64"}[a]


def broadcast_shape(a: Sequence[int], b: Sequence[int]) -> List[int]:
    """Compute the numpy-exact broadcast shape of two input shapes.

    Right-align, pad shorter on the LEFT with 1s, expand size-1 axes,
    raise on mismatch.
    """
    n = max(len(a), len(b))
    out = []
    for k in range(n):
        a_dim = a[len(a) - 1 - k] if k < len(a) else 1
        b_dim = b[len(b) - 1 - k] if k < len(b) else 1
        if a_dim == b_dim:
            dim = a_dim
        elif a_dim == 1:
            dim = b_dim
        elif b_dim == 1:
            dim = a_dim
        else:
            raise ValueError(
                f"operands could not be broadcast together with shapes {list(a)} {list(b)}"
            )
        out.append(dim)
    out.reverse()
    return out


def _broadcast_index(target_shape, input_shape, target_idx):
    """Map a target multi-index to the input multi-index per broadcast rules."""
    pad = len(target_shape) - len(input_shape)
    return tuple(
        (0 if input_shape[k - pad] == 1 else target_idx[k])
        for k in range(pad, len(target_shape))
    )


def _flat_index(shape, multi):
    """Row-major flat index for a multi-index in a given shape."""
    idx = 0
    stride = 1
    for k in range(len(shape) - 1, -1, -1):
        idx += multi[k] * stride
        stride *= shape[k]
    return idx


def _multi_indices(shape):
    """Yield every multi-index tuple in row-major order."""
    if not shape:
        yield ()
        return
    counters = [0] * len(shape)
    while True:
        yield tuple(counters)
        for k in range(len(shape) - 1, -1, -1):
            counters[k] += 1
            if counters[k] < shape[k]:
                break
            counters[k] = 0
        else:
            return


def binary_op(op: str, a_data, a_shape, b_data, b_shape):
    """Apply a binary ufunc element-wise after broadcasting.

    `op` is one of: add / sub / mul / div / pow / eq / ne / lt / le / gt / ge.
    Returns (result_data, result_shape, result_dtype).
    """
    out_shape = broadcast_shape(a_shape, b_shape)
    out = []
    for multi in _multi_indices(out_shape):
        a_multi = _broadcast_index(out_shape, a_shape if a_shape else (1,), multi)
        b_multi = _broadcast_index(out_shape, b_shape if b_shape else (1,), multi)
        a_idx = _flat_index(a_shape if a_shape else (1,), a_multi)
        b_idx = _flat_index(b_shape if b_shape else (1,), b_multi)
        x = a_data[a_idx]
        y = b_data[b_idx]
        if op == "add":
            out.append(x + y)
        elif op == "sub":
            out.append(x - y)
        elif op == "mul":
            out.append(x * y)
        elif op == "div":
            if y == 0 and isinstance(y, int):
                raise ZeroDivisionError("integer division by zero")
            if isinstance(x, float) or isinstance(y, float):
                if y == 0.0:
                    if x == 0.0:
                        out.append(float("nan"))
                    elif x > 0.0:
                        out.append(float("inf"))
                    else:
                        out.append(float("-inf"))
                    continue
            out.append(x / y)
        elif op == "pow":
            if isinstance(x, int) and isinstance(y, int) and y < 0:
                out.append(0)
            else:
                out.append(x ** y)
        elif op == "eq":
            out.append(x == y)
        elif op == "ne":
            out.append(x != y)
        elif op == "lt":
            out.append(x < y)
        elif op == "le":
            out.append(x <= y)
        elif op == "gt":
            out.append(x > y)
        elif op == "ge":
            out.append(x >= y)
        else:
            raise ValueError(f"unknown op: {op}")
    return out, out_shape


def unary_math(op: str, a_data):
    """Element-wise unary math: sin / cos / exp / log / sqrt.

    Returns the data list (caller knows the shape).
    """
    fn = {"sin": math.sin, "cos": math.cos, "exp": math.exp,
          "log": math.log, "sqrt": math.sqrt}[op]
    out = []
    for x in a_data:
        try:
            out.append(fn(float(x)))
        except (ValueError, OverflowError):
            # log(0) raises ValueError in math; produces -inf in numpy.
            if op == "log" and float(x) == 0.0:
                out.append(float("-inf"))
            elif op == "log" and float(x) < 0.0:
                out.append(float("nan"))
            elif op == "sqrt" and float(x) < 0.0:
                out.append(float("nan"))
            else:
                out.append(float("nan"))
    return out


# ---- Public surface (M7.1) ----

def add(a_data, a_shape, b_data, b_shape):
    return binary_op("add", a_data, a_shape, b_data, b_shape)


def subtract(a_data, a_shape, b_data, b_shape):
    return binary_op("sub", a_data, a_shape, b_data, b_shape)


def multiply(a_data, a_shape, b_data, b_shape):
    return binary_op("mul", a_data, a_shape, b_data, b_shape)


def divide(a_data, a_shape, b_data, b_shape):
    return binary_op("div", a_data, a_shape, b_data, b_shape)


def power(a_data, a_shape, b_data, b_shape):
    return binary_op("pow", a_data, a_shape, b_data, b_shape)


def sin(a_data):
    return unary_math("sin", a_data)


def cos(a_data):
    return unary_math("cos", a_data)


def exp(a_data):
    return unary_math("exp", a_data)


def log(a_data):
    return unary_math("log", a_data)


def sqrt(a_data):
    return unary_math("sqrt", a_data)
