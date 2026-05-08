# SPDX-License-Identifier: BSD-3-Clause
#
# L0 differential harness driver for cobrust-numpy M7.6 (per ADR-0021).
#
# Reads a JSON request from stdin describing an op call:
#   {"op": "fft|ifft|rfft|irfft|polyval|polyfit|poly|cumsum|cumprod|median|percentile|nansum|nanmean|nanmin|nanmax|complex_add|complex_mul|complex_sin|...",
#    "params": {...op-specific...},
#    "data": [...input...]}
#
# Returns numpy 2.0.2 oracle output as JSON:
#   {"dtype": "...", "shape": [...], "data": [...]} (real arrays)
#   or {"dtype": "Complex64|Complex128", "shape": [...], "data": [[re, im], ...]}
#   or {"error": "..."}
#
# Per ADR-0021 §12 tolerances:
#   - bit-identical for Int32/Int64/Bool
#   - rtol=1e-7 for Float32/Float64
#   - rtol=1e-5 for Complex64/Complex128

from __future__ import annotations

import json
import sys


def encode_complex(arr):
    """numpy complex array → list[[re, im], ...]"""
    import numpy as np
    flat = np.asarray(arr).ravel()
    return [[float(c.real), float(c.imag)] for c in flat]


def decode_complex(data):
    """list[[re, im], ...] → list[complex]"""
    return [complex(re, im) for re, im in data]


def run_one(req: dict) -> dict:
    import numpy as np

    op = req["op"]
    params = req.get("params", {})
    data = req.get("data", [])

    try:
        # ---- Bucket A: FFT ----
        if op == "fft":
            arr = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.fft.fft(arr)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "ifft":
            arr = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.fft.ifft(arr)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "rfft":
            arr = np.asarray(data, dtype=np.float64)
            out = np.fft.rfft(arr)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "irfft":
            n = int(params["n"])
            arr = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.fft.irfft(arr, n=n)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.tolist()]}

        # ---- Bucket A: polynomial ----
        if op == "polyval":
            coeffs = np.asarray(params["coeffs"], dtype=np.float64)
            x = np.asarray(data, dtype=np.float64)
            out = np.polynomial.polynomial.polyval(x, coeffs[::-1])  # numpy uses low-to-high
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.tolist()]}
        if op == "polyfit":
            x = np.asarray(params["x"], dtype=np.float64)
            y = np.asarray(data, dtype=np.float64)
            deg = int(params["deg"])
            coeffs = np.polyfit(x, y, deg)
            return {"dtype": "Float64", "shape": list(coeffs.shape), "data": [float(v) for v in coeffs.tolist()]}
        if op == "poly":
            roots = np.asarray(data, dtype=np.float64)
            coeffs = np.poly(roots)
            return {"dtype": "Float64", "shape": list(coeffs.shape), "data": [float(v) for v in coeffs.tolist()]}

        # ---- Bucket B: complex arithmetic / unary ----
        if op == "complex_add":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            b = np.asarray(decode_complex(params["b"]), dtype=np.complex128)
            out = a + b
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_sub":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            b = np.asarray(decode_complex(params["b"]), dtype=np.complex128)
            out = a - b
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_mul":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            b = np.asarray(decode_complex(params["b"]), dtype=np.complex128)
            out = a * b
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_div":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            b = np.asarray(decode_complex(params["b"]), dtype=np.complex128)
            out = a / b
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_sin":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.sin(a)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_cos":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.cos(a)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_exp":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.exp(a)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_log":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.log(a)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_sqrt":
            a = np.asarray(decode_complex(data), dtype=np.complex128)
            out = np.sqrt(a)
            return {"dtype": "Complex128", "shape": list(out.shape), "data": encode_complex(out)}
        if op == "complex_eigh":
            shape = params["shape"]
            arr = np.asarray(decode_complex(data), dtype=np.complex128).reshape(shape)
            w, v = np.linalg.eigh(arr)
            return {
                "eigenvalues": {"dtype": "Float64", "shape": list(w.shape), "data": [float(x) for x in w.tolist()]},
                "eigenvectors": {"dtype": "Complex128", "shape": list(v.shape), "data": encode_complex(v)},
            }

        # ---- Bucket C: reductions ----
        if op == "cumsum":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.cumsum(arr, axis=axis)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "cumprod":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.cumprod(arr, axis=axis)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "median":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.median(arr, axis=axis)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "percentile":
            q = float(params["q"])
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.percentile(arr, q, axis=axis)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "nansum":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.nansum(arr, axis=axis)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "nanmean":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                out = np.nanmean(arr, axis=axis)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "nanmin":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                out = np.nanmin(arr, axis=axis)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "nanmax":
            shape = params.get("shape", [len(data)])
            axis = params.get("axis", None)
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter("ignore")
                out = np.nanmax(arr, axis=axis)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "sum_axes":
            shape = params["shape"]
            axes = tuple(params["axes"])
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.sum(arr, axis=axes)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "prod_axes":
            shape = params["shape"]
            axes = tuple(params["axes"])
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.prod(arr, axis=axes)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "mean_axes":
            shape = params["shape"]
            axes = tuple(params["axes"])
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.mean(arr, axis=axes)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "min_axes":
            shape = params["shape"]
            axes = tuple(params["axes"])
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.min(arr, axis=axes)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}
        if op == "max_axes":
            shape = params["shape"]
            axes = tuple(params["axes"])
            arr = np.asarray(data, dtype=np.float64).reshape(shape)
            out = np.max(arr, axis=axes)
            out = np.atleast_1d(out)
            return {"dtype": "Float64", "shape": list(out.shape), "data": [float(v) for v in out.ravel().tolist()]}

        return {"error": f"unknown op: {op}"}
    except (ValueError, IndexError, TypeError, ImportError) as e:
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
    print(json.dumps(run_one(req)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
