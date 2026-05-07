# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.1 (per ADR-0014).
#
# Reads a JSON request from stdin describing a ufunc call:
#   {"op": "add|sub|mul|div|pow|eq|ne|lt|le|gt|ge|sin|cos|exp|log|sqrt",
#    "a": {"dtype": "...", "shape": [...], "data": [...]},
#    "b": {"dtype": "...", "shape": [...], "data": [...]} (optional, only
#         for binary ops)}
#
# Runs the upstream `numpy` package, serialises the resulting array as
#   {"dtype": "Int32|...", "shape": [...], "data": [...]}
# and writes that JSON to stdout. The Rust differential test
# (`crates/cobrust-numpy/tests/ufunc_differential.rs`) drives this
# script as a subprocess and bytewise-compares the output against
# `cobrust_numpy::Array::add(...).to_json()` for the same inputs.

from __future__ import annotations

import json
import sys


PY_TO_RUST = {
    "int32": "Int32",
    "int64": "Int64",
    "float32": "Float32",
    "float64": "Float64",
    "bool": "Bool",
}


RUST_TO_NUMPY = {v: k for k, v in PY_TO_RUST.items()}


def make_array(req):
    """Materialise a numpy array from {dtype, shape, data}."""
    import numpy as np

    dtype_str = RUST_TO_NUMPY[req["dtype"]]
    shape = req["shape"]
    data = req["data"]
    arr = np.array(data, dtype=dtype_str).reshape(shape) if shape else np.array(data[0], dtype=dtype_str)
    return arr


def to_payload(arr):
    """Serialise a numpy array to the cobrust-numpy `to_json()` shape."""
    import numpy as np

    py_dtype_name = arr.dtype.name
    if py_dtype_name not in PY_TO_RUST:
        raise ValueError(f"unsupported dtype: {py_dtype_name}")
    rust_variant = PY_TO_RUST[py_dtype_name]
    flat = arr.flatten().tolist()
    if py_dtype_name == "bool":
        flat = [bool(x) for x in flat]
    return {
        "dtype": rust_variant,
        "shape": list(arr.shape),
        "data": flat,
    }


def run_one(req: dict) -> dict:
    import numpy as np

    op = req["op"]
    a = make_array(req["a"])
    BINARY_OPS = {
        "add": np.add, "sub": np.subtract, "mul": np.multiply,
        "div": np.divide, "pow": np.power,
        "eq": np.equal, "ne": np.not_equal,
        "lt": np.less, "le": np.less_equal,
        "gt": np.greater, "ge": np.greater_equal,
    }
    UNARY_OPS = {
        "sin": np.sin, "cos": np.cos, "exp": np.exp,
        "log": np.log, "sqrt": np.sqrt,
    }
    if op in BINARY_OPS:
        b = make_array(req["b"])
        with np.errstate(all="ignore"):
            try:
                out = BINARY_OPS[op](a, b)
            except ZeroDivisionError:
                return {"error": "integer_division_by_zero"}
        return to_payload(out)
    elif op in UNARY_OPS:
        with np.errstate(all="ignore"):
            out = UNARY_OPS[op](a)
        return to_payload(out)
    else:
        return {"error": f"unknown op: {op}"}


def main() -> int:
    src = sys.stdin.read()
    if not src.strip():
        print(json.dumps({"error": "empty input"}))
        return 1
    try:
        req = json.loads(src)
    except json.JSONDecodeError as e:
        print(json.dumps({"error": f"json decode: {e}"}))
        return 1
    try:
        out = run_one(req)
    except Exception as e:
        out = {"error": str(e)}
    print(json.dumps(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
