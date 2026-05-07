# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored numpy 2.0.2 reference subset for cobrust M7.4 (per ADR-0017).
# This is the pipeline-time reference: the L0 differential harness
# imports this module to validate the cobrust-numpy translator pipeline
# end-to-end. The runtime correctness contract against upstream `numpy`
# is exercised at the test layer (`tests/linalg_differential.rs`).
#
# Scope (M7.4 per ADR-0017):
#   - 8 linalg ops: matmul, dot, det, solve, inv, svd, eigh, cholesky.
#   - Float-only inputs (Float32/Float64).
#   - rtol=1e-6 against numpy on cond <= 100 inputs.
#   - Pure-Python LU / Cholesky / Jacobi reference implementations
#     (used only for the canned-translator path; runtime gate
#     compares against actual numpy).

from __future__ import annotations

import math
from typing import List, Sequence, Tuple


def _shape_size(shape: Sequence[int]) -> int:
    n = 1
    for d in shape:
        n *= d
    return n


def _zeros(shape: Sequence[int]) -> List[float]:
    return [0.0] * _shape_size(shape)


def _identity(n: int) -> List[float]:
    out = [0.0] * (n * n)
    for i in range(n):
        out[i * n + i] = 1.0
    return out


def matmul(
    a_data: List[float],
    a_shape: List[int],
    b_data: List[float],
    b_shape: List[int],
) -> Tuple[List[float], List[int]]:
    """Matrix multiplication. Supports 1-D x 1-D, 1-D x 2-D, 2-D x 1-D, 2-D x 2-D."""
    if len(a_shape) == 1 and len(b_shape) == 1:
        if a_shape[0] != b_shape[0]:
            raise ValueError(f"shapes {a_shape} and {b_shape} not aligned")
        s = 0.0
        for k in range(a_shape[0]):
            s += a_data[k] * b_data[k]
        return [s], []
    if len(a_shape) == 1 and len(b_shape) == 2:
        k_dim, n = b_shape
        if a_shape[0] != k_dim:
            raise ValueError(f"shapes {a_shape} and {b_shape} not aligned")
        out = [0.0] * n
        for j in range(n):
            s = 0.0
            for k in range(k_dim):
                s += a_data[k] * b_data[k * n + j]
            out[j] = s
        return out, [n]
    if len(a_shape) == 2 and len(b_shape) == 1:
        m, k_dim = a_shape
        if k_dim != b_shape[0]:
            raise ValueError(f"shapes {a_shape} and {b_shape} not aligned")
        out = [0.0] * m
        for i in range(m):
            s = 0.0
            for k in range(k_dim):
                s += a_data[i * k_dim + k] * b_data[k]
            out[i] = s
        return out, [m]
    if len(a_shape) == 2 and len(b_shape) == 2:
        m, k_dim = a_shape
        k2, n = b_shape
        if k_dim != k2:
            raise ValueError(f"shapes {a_shape} and {b_shape} not aligned")
        out = [0.0] * (m * n)
        for i in range(m):
            for j in range(n):
                s = 0.0
                for k in range(k_dim):
                    s += a_data[i * k_dim + k] * b_data[k * n + j]
                out[i * n + j] = s
        return out, [m, n]
    raise ValueError("matmul supports only rank 1 / 2 inputs at M7.4")


def dot(
    a_data: List[float],
    a_shape: List[int],
    b_data: List[float],
    b_shape: List[int],
) -> Tuple[List[float], List[int]]:
    """1-D x 1-D inner product or 2-D x 2-D matmul. Defers to matmul."""
    return matmul(a_data, a_shape, b_data, b_shape)


def _lu_decompose(a_data: List[float], n: int) -> Tuple[List[float], List[int], int]:
    """LU decomposition with partial pivoting.

    Returns (lu, pivot, sign) where:
      - `lu` is the in-place LU factor (n x n flat row-major; L below
        diagonal with implicit unit diagonal, U on/above diagonal).
      - `pivot[i]` is the row swapped into row i during step i.
      - `sign` is (-1)^(number-of-swaps) for det's sign.
    Raises if the matrix is singular at the pivot.
    """
    lu = list(a_data)
    pivot = list(range(n))
    sign = 1
    eps_zero = 1e-30
    for k in range(n):
        # Find pivot
        max_v = abs(lu[k * n + k])
        max_i = k
        for i in range(k + 1, n):
            v = abs(lu[i * n + k])
            if v > max_v:
                max_v = v
                max_i = i
        if max_v < eps_zero:
            raise ValueError("Singular matrix")
        if max_i != k:
            # Swap rows k and max_i
            for j in range(n):
                lu[k * n + j], lu[max_i * n + j] = lu[max_i * n + j], lu[k * n + j]
            pivot[k], pivot[max_i] = pivot[max_i], pivot[k]
            sign = -sign
        # Eliminate
        for i in range(k + 1, n):
            lu[i * n + k] /= lu[k * n + k]
            factor = lu[i * n + k]
            for j in range(k + 1, n):
                lu[i * n + j] -= factor * lu[k * n + j]
    return lu, pivot, sign


def det(a_data: List[float], a_shape: List[int]) -> float:
    """Determinant via LU decomposition."""
    if len(a_shape) != 2 or a_shape[0] != a_shape[1]:
        raise ValueError("det requires a square matrix")
    n = a_shape[0]
    if n == 0:
        return 1.0
    try:
        lu, _, sign = _lu_decompose(a_data, n)
    except ValueError:
        return 0.0
    d = float(sign)
    for i in range(n):
        d *= lu[i * n + i]
    return d


def _lu_solve(lu: List[float], pivot: List[int], n: int, b: List[float]) -> List[float]:
    """Solve (P · L · U) · x = b given LU factors. Returns x."""
    # Apply pivot to b
    pb = [b[pivot[i]] for i in range(n)]
    # Forward substitution L · y = pb (L unit diagonal)
    y = list(pb)
    for i in range(n):
        s = y[i]
        for k in range(i):
            s -= lu[i * n + k] * y[k]
        y[i] = s
    # Backward substitution U · x = y
    x = list(y)
    for i in range(n - 1, -1, -1):
        s = x[i]
        for k in range(i + 1, n):
            s -= lu[i * n + k] * x[k]
        x[i] = s / lu[i * n + i]
    return x


def solve(
    a_data: List[float],
    a_shape: List[int],
    b_data: List[float],
    b_shape: List[int],
) -> Tuple[List[float], List[int]]:
    if len(a_shape) != 2 or a_shape[0] != a_shape[1]:
        raise ValueError("solve requires square A")
    n = a_shape[0]
    lu, pivot, _ = _lu_decompose(a_data, n)
    if len(b_shape) == 1:
        if b_shape[0] != n:
            raise ValueError("incompatible b shape")
        return _lu_solve(lu, pivot, n, b_data), [n]
    if len(b_shape) == 2:
        if b_shape[0] != n:
            raise ValueError("incompatible b shape")
        nrhs = b_shape[1]
        out = [0.0] * (n * nrhs)
        for j in range(nrhs):
            col = [b_data[i * nrhs + j] for i in range(n)]
            x = _lu_solve(lu, pivot, n, col)
            for i in range(n):
                out[i * nrhs + j] = x[i]
        return out, [n, nrhs]
    raise ValueError("solve supports rank-1 or rank-2 b at M7.4")


def inv(a_data: List[float], a_shape: List[int]) -> Tuple[List[float], List[int]]:
    if len(a_shape) != 2 or a_shape[0] != a_shape[1]:
        raise ValueError("inv requires square A")
    n = a_shape[0]
    return solve(a_data, a_shape, _identity(n), [n, n])


def cholesky(a_data: List[float], a_shape: List[int]) -> Tuple[List[float], List[int]]:
    """Lower-triangular Cholesky factorisation (numpy default)."""
    if len(a_shape) != 2 or a_shape[0] != a_shape[1]:
        raise ValueError("cholesky requires square A")
    n = a_shape[0]
    out = [0.0] * (n * n)
    for i in range(n):
        for j in range(i + 1):
            s = a_data[i * n + j]
            for k in range(j):
                s -= out[i * n + k] * out[j * n + k]
            if i == j:
                if s <= 0.0:
                    raise ValueError("Matrix is not positive definite")
                out[i * n + j] = math.sqrt(s)
            else:
                out[i * n + j] = s / out[j * n + j]
    return out, [n, n]


def eigh(
    a_data: List[float], a_shape: List[int]
) -> Tuple[List[float], List[int], List[float], List[int]]:
    """Symmetric eigendecomposition via cyclic Jacobi.

    Returns (w_data, w_shape, v_data, v_shape) such that
    a == v · diag(w) · vᵀ. Eigenvalues sorted ascending.
    """
    if len(a_shape) != 2 or a_shape[0] != a_shape[1]:
        raise ValueError("eigh requires square A")
    n = a_shape[0]
    # Symmetry sniff
    for i in range(n):
        for j in range(i + 1, n):
            if abs(a_data[i * n + j] - a_data[j * n + i]) > 1e-9 * max(
                1.0, abs(a_data[i * n + j])
            ):
                raise ValueError("eigh input not symmetric")
    a = list(a_data)
    v = _identity(n)
    max_sweeps = 100
    eps = 1e-14
    for sweep in range(max_sweeps):
        off = 0.0
        for i in range(n):
            for j in range(i + 1, n):
                off += a[i * n + j] * a[i * n + j]
        if off < eps:
            break
        for p in range(n - 1):
            for q in range(p + 1, n):
                apq = a[p * n + q]
                if abs(apq) < 1e-18:
                    continue
                app = a[p * n + p]
                aqq = a[q * n + q]
                tau = (aqq - app) / (2.0 * apq)
                if tau >= 0:
                    t = 1.0 / (tau + math.sqrt(1.0 + tau * tau))
                else:
                    t = 1.0 / (tau - math.sqrt(1.0 + tau * tau))
                c = 1.0 / math.sqrt(1.0 + t * t)
                s = t * c
                a[p * n + p] = app - t * apq
                a[q * n + q] = aqq + t * apq
                a[p * n + q] = 0.0
                a[q * n + p] = 0.0
                for k in range(n):
                    if k != p and k != q:
                        akp = a[k * n + p]
                        akq = a[k * n + q]
                        a[k * n + p] = c * akp - s * akq
                        a[p * n + k] = a[k * n + p]
                        a[k * n + q] = s * akp + c * akq
                        a[q * n + k] = a[k * n + q]
                for k in range(n):
                    vkp = v[k * n + p]
                    vkq = v[k * n + q]
                    v[k * n + p] = c * vkp - s * vkq
                    v[k * n + q] = s * vkp + c * vkq
    # Extract eigenvalues + sort ascending
    w = [a[i * n + i] for i in range(n)]
    order = sorted(range(n), key=lambda i: w[i])
    w_sorted = [w[i] for i in order]
    v_sorted = [0.0] * (n * n)
    for new_col, old_col in enumerate(order):
        for row in range(n):
            v_sorted[row * n + new_col] = v[row * n + old_col]
    return w_sorted, [n], v_sorted, [n, n]


def svd(
    a_data: List[float], a_shape: List[int]
) -> Tuple[
    List[float], List[int], List[float], List[int], List[float], List[int]
]:
    """SVD via eigh of AᵀA. Yields U (M,M), s (min(M,N),), Vt (N,N)."""
    if len(a_shape) != 2:
        raise ValueError("svd requires a 2-D matrix at M7.4")
    m, n = a_shape
    # Compute AᵀA (n x n)
    ata = [0.0] * (n * n)
    for i in range(n):
        for j in range(n):
            s = 0.0
            for k in range(m):
                s += a_data[k * n + i] * a_data[k * n + j]
            ata[i * n + j] = s
    # eigh on AᵀA gives V columns (eigenvectors) and singular-values squared.
    w, _, v_data, _ = eigh(ata, [n, n])
    # Sort descending
    order = sorted(range(n), key=lambda i: -w[i])
    sigma = [math.sqrt(max(0.0, w[order[i]])) for i in range(n)]
    v_sorted = [0.0] * (n * n)
    for new_col, old_col in enumerate(order):
        for row in range(n):
            v_sorted[row * n + new_col] = v_data[row * n + old_col]
    # Construct U = A · V · diag(1/sigma) for nonzero sigma; else fill with
    # an arbitrary orthonormal basis (we use canonical e_k).
    u_data = [0.0] * (m * m)
    k_min = min(m, n)
    for k in range(k_min):
        if sigma[k] > 1e-14:
            for i in range(m):
                s = 0.0
                for j in range(n):
                    s += a_data[i * n + j] * v_sorted[j * n + k]
                u_data[i * m + k] = s / sigma[k]
        else:
            u_data[k * m + k] = 1.0
    # Fill remaining columns with canonical basis (only used when m > n).
    for k in range(k_min, m):
        u_data[k * m + k] = 1.0
    s_full = [sigma[i] for i in range(k_min)]
    # Vt = V transposed (numpy returns Vᵀ).
    vt_data = [0.0] * (n * n)
    for i in range(n):
        for j in range(n):
            vt_data[i * n + j] = v_sorted[j * n + i]
    return u_data, [m, m], s_full, [k_min], vt_data, [n, n]
