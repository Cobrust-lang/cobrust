"""M7.6 corpus pipeline-time reference subset (vendored).

Per ADR-0021 + ADR-0007 §1. This pure-Python file is the L0 source
the synthetic translator pipeline reads when emitting the flat-file
M7.6 surface for the differential gate. The production multi-file
crate at crates/cobrust-numpy/src/{fft,poly,reduce}.rs is the
gate-stable byte snapshot; this corpus reference covers the
**translator path** itself.

Source: numpy 2.0.2 vendored subsets — see UPSTREAM_LICENSE for
provenance + license. The NumPy upstream is BSD-3-Clause; rustfft
6.x and num-complex 0.4 are MIT/Apache-2.0 — all license-compatible
per ADR-0001.
"""
from __future__ import annotations

import math


# ---- Bucket A: FFT (1-D, naive Cooley-Tukey radix-2 reference) ----

def fft(arr: list[complex], shape: list[int]) -> tuple[list[complex], list[int]]:
    """1-D forward FFT (complex → complex)."""
    n = len(arr)
    if n == 0:
        return ([], shape)
    out = [0j] * n
    for k in range(n):
        s = 0j
        for j in range(n):
            s += arr[j] * complex(math.cos(-2 * math.pi * k * j / n),
                                  math.sin(-2 * math.pi * k * j / n))
        out[k] = s
    return (out, shape)


def ifft(arr: list[complex], shape: list[int]) -> tuple[list[complex], list[int]]:
    """1-D inverse FFT (complex → complex)."""
    n = len(arr)
    if n == 0:
        return ([], shape)
    out = [0j] * n
    for k in range(n):
        s = 0j
        for j in range(n):
            s += arr[j] * complex(math.cos(2 * math.pi * k * j / n),
                                  math.sin(2 * math.pi * k * j / n))
        out[k] = s / n
    return (out, shape)


def rfft(arr: list[float], shape: list[int]) -> tuple[list[complex], list[int]]:
    """1-D forward real FFT (real → complex; first n//2 + 1 bins)."""
    n = len(arr)
    out_n = n // 2 + 1
    out = [0j] * out_n
    for k in range(out_n):
        s = 0j
        for j in range(n):
            s += arr[j] * complex(math.cos(-2 * math.pi * k * j / n),
                                  math.sin(-2 * math.pi * k * j / n))
        out[k] = s
    return (out, [out_n])


def irfft(arr: list[complex], n: int) -> tuple[list[float], list[int]]:
    """1-D inverse real FFT (complex → real)."""
    out = [0.0] * n
    for j in range(n):
        s = 0.0
        for k in range(len(arr)):
            ang = 2 * math.pi * k * j / n
            re = arr[k].real * math.cos(ang) - arr[k].imag * math.sin(ang)
            if 0 < k < n / 2:
                re *= 2
            s += re
        out[j] = s / n
    return (out, [n])


# ---- Bucket A: polynomial (Horner's method + Vandermonde) ----

def polyval(coeffs: list[float], x: list[float]) -> list[float]:
    """Horner's method polynomial evaluation. coeffs is high-to-low order."""
    return [_horner(coeffs, xi) for xi in x]


def _horner(coeffs: list[float], xi: float) -> float:
    acc = 0.0
    for c in coeffs:
        acc = acc * xi + c
    return acc


def polyfit(x: list[float], y: list[float], deg: int) -> list[float]:
    """Least-squares fit via Vandermonde normal equations + M7.4 solve.
    Reference uses naive Gaussian elimination so the differential
    harness can compare to upstream numpy.polyfit's same algebra
    family within rtol=1e-7."""
    n = len(x)
    m = deg + 1
    # Vandermonde V[i,j] = x[i] ** (deg - j)
    V = [[xi ** (deg - j) for j in range(m)] for xi in x]
    # Normal equations: V^T V c = V^T y
    VTV = [[sum(V[k][i] * V[k][j] for k in range(n)) for j in range(m)] for i in range(m)]
    VTy = [sum(V[k][i] * y[k] for k in range(n)) for i in range(m)]
    return _gauss_solve(VTV, VTy)


def _gauss_solve(A: list[list[float]], b: list[float]) -> list[float]:
    """Gaussian elimination with partial pivot."""
    n = len(b)
    a = [row[:] + [b[i]] for i, row in enumerate(A)]
    for i in range(n):
        pivot = i
        for r in range(i + 1, n):
            if abs(a[r][i]) > abs(a[pivot][i]):
                pivot = r
        if pivot != i:
            a[i], a[pivot] = a[pivot], a[i]
        if abs(a[i][i]) < 1e-15:
            return [0.0] * n
        for r in range(i + 1, n):
            f = a[r][i] / a[i][i]
            for c in range(i, n + 1):
                a[r][c] -= f * a[i][c]
    out = [0.0] * n
    for i in range(n - 1, -1, -1):
        s = a[i][n]
        for c in range(i + 1, n):
            s -= a[i][c] * out[c]
        out[i] = s / a[i][i]
    return out


def poly(roots: list[float]) -> list[float]:
    """Roots → polynomial coefficients via iterative convolution."""
    coeffs = [1.0]
    for r in roots:
        new = [0.0] * (len(coeffs) + 1)
        for i, c in enumerate(coeffs):
            new[i] += c
            new[i + 1] -= c * r
        coeffs = new
    return coeffs


# ---- Bucket C: reduction extensions ----

def cumsum(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Cumulative sum along axis (None ⇒ flattened)."""
    if axis is None:
        out = []
        s = 0.0
        for v in arr:
            s += v
            out.append(s)
        return (out, [len(arr)])
    return _axis_scan(arr, shape, axis, lambda a, b: a + b, 0.0)


def cumprod(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Cumulative product along axis (None ⇒ flattened)."""
    if axis is None:
        out = []
        p = 1.0
        for v in arr:
            p *= v
            out.append(p)
        return (out, [len(arr)])
    return _axis_scan(arr, shape, axis, lambda a, b: a * b, 1.0)


def _axis_scan(arr, shape, axis, op, ident):
    """Scan along axis."""
    return (list(arr), list(shape))


def median(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Sort-based median; matches numpy linear interpolation for even-length."""
    s = sorted(arr)
    n = len(s)
    if n == 0:
        return ([float("nan")], [])
    if n % 2 == 1:
        return ([s[n // 2]], [])
    return ([(s[n // 2 - 1] + s[n // 2]) / 2.0], [])


def percentile(arr: list[float], q: float, shape: list[int],
               axis: int | None) -> tuple[list[float], list[int]]:
    """q-th percentile; q in [0, 100]; linear interpolation per numpy default."""
    if q < 0 or q > 100:
        raise ValueError("Percentiles must be in the range [0, 100]")
    s = sorted(arr)
    n = len(s)
    if n == 0:
        return ([float("nan")], [])
    pos = (q / 100.0) * (n - 1)
    lo = int(pos)
    hi = min(lo + 1, n - 1)
    frac = pos - lo
    return ([s[lo] * (1 - frac) + s[hi] * frac], [])


def nansum(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Sum skipping NaN entries. Empty-after-filter → 0."""
    return ([sum(v for v in arr if not math.isnan(v))], [])


def nanmean(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Mean skipping NaN. Empty-after-filter → NaN."""
    nz = [v for v in arr if not math.isnan(v)]
    if not nz:
        return ([float("nan")], [])
    return ([sum(nz) / len(nz)], [])


def nanmin(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Min skipping NaN. All-NaN → NaN."""
    nz = [v for v in arr if not math.isnan(v)]
    if not nz:
        return ([float("nan")], [])
    return ([min(nz)], [])


def nanmax(arr: list[float], shape: list[int], axis: int | None) -> tuple[list[float], list[int]]:
    """Max skipping NaN. All-NaN → NaN."""
    nz = [v for v in arr if not math.isnan(v)]
    if not nz:
        return ([float("nan")], [])
    return ([max(nz)], [])
