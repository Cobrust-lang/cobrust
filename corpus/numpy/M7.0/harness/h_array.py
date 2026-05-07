# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.0 (per ADR-0013 §5).
#
# Reads a JSON request from stdin describing a constructor call:
#   {"op": "zeros|ones|array|arange", "args": {...}}
#
# Runs the upstream `numpy` package, serialises the resulting array as
#   {"dtype": "Int32|...", "shape": [...], "data": [...]}
# and writes that JSON to stdout. The Rust differential test
# (`crates/cobrust-numpy/tests/numpy_differential.rs`) drives this
# script as a subprocess and bytewise-compares the output against
# `cobrust_numpy::Array::to_json()` for the same inputs.
#
# This is the **upstream-numpy** oracle. The pure-Python reference
# (`corpus/numpy/M7.0/upstream/array_core.py`) is the **pipeline
# oracle** — it lets the synthetic translator pipeline run without
# importing numpy.

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


def to_payload(arr) -> dict:
    """Serialise a numpy array to the cobrust-numpy `to_json()` shape."""
    import numpy as np

    py_dtype_name = arr.dtype.name
    if py_dtype_name not in PY_TO_RUST:
        raise ValueError(f"unsupported dtype: {py_dtype_name}")
    rust_variant = PY_TO_RUST[py_dtype_name]
    return {
        "dtype": rust_variant,
        "shape": list(arr.shape),
        "data": arr.flatten().tolist(),
    }


def run_one(req: dict) -> dict:
    import numpy as np

    op = req["op"]
    args = req["args"]
    dtype = args.get("dtype", "float64")

    if op == "zeros":
        arr = np.zeros(tuple(args["shape"]), dtype=dtype)
    elif op == "ones":
        arr = np.ones(tuple(args["shape"]), dtype=dtype)
    elif op == "array":
        flat = np.array(args["values"], dtype=dtype)
        arr = flat.reshape(tuple(args["shape"]))
    elif op == "arange":
        arr = np.arange(args["start"], args["stop"], args["step"], dtype=dtype)
    else:
        raise ValueError(f"unsupported op: {op}")

    return to_payload(arr)


def main() -> int:
    raw = sys.stdin.read()
    req = json.loads(raw)
    if isinstance(req, list):
        # Batch mode: one request per line; emit one response per line.
        for r in req:
            sys.stdout.write(json.dumps(run_one(r)) + "\n")
    else:
        sys.stdout.write(json.dumps(run_one(req)) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
