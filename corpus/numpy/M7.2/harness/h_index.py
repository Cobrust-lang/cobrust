# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.2 (per ADR-0015).
#
# Reads a JSON request from stdin describing an indexing call:
#   {"op": "slice|take|mask|where|single",
#    "a": {"dtype": "...", "shape": [...], "data": [...]},
#    ... op-specific fields ...}
#
# Runs the upstream `numpy` package, serialises the resulting array as
#   {"dtype": "Int32|...", "shape": [...], "data": [...]}
# and writes that JSON to stdout. The Rust differential test
# (`crates/cobrust-numpy/tests/index_differential.rs`) drives this
# script as a subprocess and bytewise-compares the output against
# `cobrust_numpy::Array::<op>(...).to_json()` for the same inputs.

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

    if op == "slice":
        # Basic slicing on the first axis.
        start = req.get("start")
        stop = req.get("stop")
        step = req.get("step")
        sl = slice(start, stop, step)
        try:
            out = a[sl]
        except (IndexError, ValueError) as e:
            return {"error": str(e)}
        return to_payload(out)

    if op == "single":
        i = req["index"]
        try:
            out = a[i]
            # numpy returns a scalar for single-int on 1D arrays;
            # promote to 0-d for a stable JSON shape.
            out = np.asarray(out)
        except IndexError as e:
            return {"error": str(e)}
        return to_payload(out)

    if op == "take":
        indices = req["indices"]
        try:
            out = a[np.asarray(indices, dtype="int64")]
        except IndexError as e:
            return {"error": str(e)}
        return to_payload(out)

    if op == "mask":
        bool_arr = make_array(req["bool"])
        try:
            out = a[bool_arr]
        except (IndexError, ValueError) as e:
            return {"error": str(e)}
        return to_payload(out)

    if op == "where":
        cond = make_array(req["cond"])
        x = make_array(req["x"])
        y = make_array(req["y"])
        with np.errstate(all="ignore"):
            out = np.where(cond, x, y)
        return to_payload(out)

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
