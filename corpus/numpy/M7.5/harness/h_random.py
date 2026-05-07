# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.5 (per ADR-0018).
#
# Reads a JSON request from stdin describing a random sampling call:
#   {"op": "integers|random|normal|uniform|choice",
#    "seed": <int>,
#    "n_samples": <int>,
#    "params": {...op-specific...}}
#
# Runs upstream `numpy.random.Generator(default_rng(seed))` and returns
# the sampled values as JSON:
#   {"data": [...], "dtype": "float64|int64|...", "shape": [n]}
#
# The Rust differential test (`crates/cobrust-numpy/tests/random_differential.rs`)
# drives this script as a subprocess and runs a 2-sample KS-test
# comparing cobrust-numpy's stream against numpy's. Per ADR-0018 §5,
# bit-identical reproducibility against numpy's PCG64 stream is NOT a
# hard requirement — distribution-level agreement is what we assert.

from __future__ import annotations

import json
import sys


def make_rng(seed):
    import numpy as np
    return np.random.default_rng(seed)


def run_one(req: dict) -> dict:
    import numpy as np

    op = req["op"]
    seed = req.get("seed", 42)
    n = req.get("n_samples", 10000)
    params = req.get("params", {})

    rng = make_rng(seed)
    try:
        if op == "integers":
            low = int(params["low"])
            high = int(params["high"])
            arr = rng.integers(low, high, size=n)
            return {
                "dtype": "Int64",
                "shape": [int(n)],
                "data": [int(v) for v in arr.tolist()],
            }
        elif op == "random":
            arr = rng.random(size=n)
            return {
                "dtype": "Float64",
                "shape": [int(n)],
                "data": [float(v) for v in arr.tolist()],
            }
        elif op == "normal":
            loc = float(params.get("loc", 0.0))
            scale = float(params.get("scale", 1.0))
            arr = rng.normal(loc=loc, scale=scale, size=n)
            return {
                "dtype": "Float64",
                "shape": [int(n)],
                "data": [float(v) for v in arr.tolist()],
            }
        elif op == "uniform":
            low = float(params.get("low", 0.0))
            high = float(params.get("high", 1.0))
            arr = rng.uniform(low=low, high=high, size=n)
            return {
                "dtype": "Float64",
                "shape": [int(n)],
                "data": [float(v) for v in arr.tolist()],
            }
        elif op == "choice":
            values = params["values"]
            replace = params.get("replace", True)
            p = params.get("p", None)
            arr = rng.choice(values, size=n, replace=replace, p=p)
            return {
                "dtype": "Float64",
                "shape": [int(n)],
                "data": [float(v) for v in arr.tolist()],
            }
        else:
            return {"error": f"unknown op: {op}"}
    except (ValueError, IndexError, TypeError) as e:
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
