# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.4 (per ADR-0017).
#
# Reads a JSON request from stdin describing a linalg call:
#   {"op": "matmul|dot|det|solve|inv|svd|eigh|cholesky",
#    "a": {"dtype": "Float32|Float64", "shape": [...], "data": [...]},
#    "b": {...} (only for matmul / dot / solve)}
#
# Runs upstream `numpy` and serialises the resulting array(s) per the
# cobrust-numpy `to_json()` shape:
#   {"dtype": "Float32|Float64", "shape": [...], "data": [...]}
# For multi-array returns (svd, eigh) the payload is a list of
# {dtype, shape, data} entries in the order documented in ADR-0017.

from __future__ import annotations

import json
import sys


PY_TO_RUST = {
    "float32": "Float32",
    "float64": "Float64",
    "int32": "Int32",
    "int64": "Int64",
    "bool": "Bool",
}

RUST_TO_NUMPY = {v: k for k, v in PY_TO_RUST.items()}


def make_array(req):
    import numpy as np

    dtype_str = RUST_TO_NUMPY[req["dtype"]]
    shape = req["shape"]
    data = req["data"]
    if shape:
        return np.array(data, dtype=dtype_str).reshape(shape)
    return np.array(data[0] if data else 0, dtype=dtype_str)


def to_payload(arr):
    import numpy as np

    arr = np.asarray(arr)
    py_dtype_name = arr.dtype.name
    if py_dtype_name not in PY_TO_RUST:
        raise ValueError(f"unsupported dtype: {py_dtype_name}")
    rust_variant = PY_TO_RUST[py_dtype_name]
    flat = arr.flatten().tolist()
    return {"dtype": rust_variant, "shape": list(arr.shape), "data": flat}


def run_one(req):
    import numpy as np

    op = req["op"]
    a = make_array(req["a"])
    b = make_array(req["b"]) if "b" in req else None

    try:
        with np.errstate(all="ignore"):
            if op == "matmul":
                out = np.matmul(a, b)
                return to_payload(out)
            if op == "dot":
                out = np.dot(a, b)
                return to_payload(out)
            if op == "det":
                out = np.linalg.det(a)
                return to_payload(np.array(out, dtype=a.dtype))
            if op == "solve":
                out = np.linalg.solve(a, b)
                return to_payload(out)
            if op == "inv":
                out = np.linalg.inv(a)
                return to_payload(out)
            if op == "svd":
                u, s, vt = np.linalg.svd(a, full_matrices=True)
                return [to_payload(u), to_payload(s), to_payload(vt)]
            if op == "eigh":
                w, v = np.linalg.eigh(a)
                return [to_payload(w), to_payload(v)]
            if op == "cholesky":
                out = np.linalg.cholesky(a)
                return to_payload(out)
            return {"error": f"unknown op: {op}"}
    except (ValueError, np.linalg.LinAlgError, TypeError) as e:
        return {"error": str(e)}


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
