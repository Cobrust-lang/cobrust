# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.3 (per ADR-0016).
#
# Reads a JSON request from stdin describing a reduction call:
#   {"op": "sum|prod|mean|std|var|min|max|argmin|argmax",
#    "a": {"dtype": "...", "shape": [...], "data": [...]},
#    "axis": null | <int>,
#    "ddof": <int> (only for std/var)}
#
# Runs the upstream `numpy` package, serialises the resulting array as
#   {"dtype": "Int32|...", "shape": [...], "data": [...]}
# and writes that JSON to stdout. The Rust differential test
# (`crates/cobrust-numpy/tests/reduce_differential.rs`) drives this
# script as a subprocess and bytewise-compares the output against
# `cobrust_numpy::reduce::<op>(...).to_json()` for the same inputs.

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
    if shape:
        arr = np.array(data, dtype=dtype_str).reshape(shape)
    else:
        arr = np.array(data[0] if data else 0, dtype=dtype_str)
    return arr


def to_payload(arr):
    """Serialise a numpy array to the cobrust-numpy `to_json()` shape."""
    import numpy as np

    if not hasattr(arr, "dtype"):
        # Bare scalar — wrap in 0-d ndarray for stable shape.
        arr = np.asarray(arr)

    py_dtype_name = arr.dtype.name
    if py_dtype_name not in PY_TO_RUST:
        # numpy.argmin / argmax return intp (int64 on 64-bit hosts).
        if py_dtype_name in ("intp", "int64"):
            py_dtype_name = "int64"
        else:
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
    axis = req.get("axis")
    ddof = req.get("ddof", 0)

    try:
        with np.errstate(all="ignore"):
            if op == "sum":
                out = np.sum(a, axis=axis)
            elif op == "prod":
                out = np.prod(a, axis=axis)
            elif op == "mean":
                out = np.mean(a, axis=axis)
            elif op == "std":
                out = np.std(a, axis=axis, ddof=ddof)
            elif op == "var":
                out = np.var(a, axis=axis, ddof=ddof)
            elif op == "min":
                out = np.min(a, axis=axis)
            elif op == "max":
                out = np.max(a, axis=axis)
            elif op == "argmin":
                out = np.argmin(a, axis=axis)
            elif op == "argmax":
                out = np.argmax(a, axis=axis)
            else:
                return {"error": f"unknown op: {op}"}
    except (ValueError, IndexError, TypeError) as e:
        return {"error": str(e)}

    out = np.asarray(out)
    return to_payload(out)


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
