# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored numpy 2.0.2 reference subset for cobrust M7.3 (per ADR-0016).
# This is the pipeline-time reference: the L0 differential harness
# imports this module to validate the cobrust-numpy translator pipeline
# end-to-end. The runtime correctness contract against upstream `numpy`
# is exercised at the test layer (`tests/reduce_differential.rs`).
#
# Scope (M7.3 per ADR-0016):
#   - 9 reductions: sum, prod, mean, std, var, min, max, argmin, argmax.
#   - axis=None (reduce-all) and axis=k (reduce single axis;
#     negative-axis aware).
#   - ddof for std/var (default 0).
#   - Pairwise summation for floats (matches numpy's accuracy).
#   - Empty-array semantics: sum/prod return identity; mean/std/var
#     return NaN; min/max/argmin/argmax raise ValueError.

from __future__ import annotations

import math
from typing import List, Optional, Sequence, Tuple


def _normalize_axis(axis: Optional[int], ndim: int) -> Optional[int]:
    """Normalise a negative axis index. None passes through (reduce-all)."""
    if axis is None:
        return None
    if axis < 0:
        axis += ndim
    if axis < 0 or axis >= ndim:
        raise IndexError(
            f"axis {axis} is out of bounds for array of dimension {ndim}"
        )
    return axis


def _shape_size(shape: Sequence[int]) -> int:
    n = 1
    for d in shape:
        n *= d
    return n


def _strides_for(shape: Sequence[int]) -> List[int]:
    n = len(shape)
    strides = [1] * n
    for k in range(n - 2, -1, -1):
        strides[k] = strides[k + 1] * shape[k + 1]
    return strides


def _gather_axis(
    a_data: Sequence[float],
    a_shape: Sequence[int],
    axis: int,
) -> Tuple[List[List[float]], List[int]]:
    """Group elements along the reduction axis.

    Returns (groups, out_shape) where each group is the slice along
    the reduced axis for one output position, and out_shape is the
    shape after dropping the reduced axis."""
    ndim = len(a_shape)
    out_shape = list(a_shape[:axis]) + list(a_shape[axis + 1 :])
    out_size = _shape_size(out_shape) if out_shape else 1
    axis_len = a_shape[axis]
    strides = _strides_for(a_shape)
    out_strides = _strides_for(out_shape) if out_shape else [1]
    groups: List[List[float]] = []
    for out_idx in range(out_size):
        # Decompose out_idx into per-axis multi-index for the *output*.
        multi = [0] * len(out_shape)
        rem = out_idx
        for k, st in enumerate(out_strides):
            multi[k] = rem // st
            rem = rem % st
        # Build the iteration along axis.
        group: List[float] = []
        for j in range(axis_len):
            full_multi = list(multi[:axis]) + [j] + list(multi[axis:])
            flat = 0
            for k in range(ndim):
                flat += full_multi[k] * strides[k]
            group.append(a_data[flat])
        groups.append(group)
    return groups, out_shape


def _pairwise_sum(values: Sequence[float]) -> float:
    """Pairwise summation matching numpy's algorithm.

    Chunk size 8: leaves of size <= 8 sum naively; recursive
    bisection above. Suppresses error from O(n*eps) (naive) to
    O(log n * eps)."""
    n = len(values)
    if n == 0:
        return 0.0
    if n <= 8:
        s = 0.0
        for v in values:
            s += v
        return s
    mid = n // 2
    return _pairwise_sum(values[:mid]) + _pairwise_sum(values[mid:])


def sum_all(a_data: List[float], a_shape: List[int]) -> float:
    """sum(a) — reduce all axes. Pairwise for floats."""
    if not a_data:
        return 0.0
    return _pairwise_sum(a_data)


def sum_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[float], List[int]]:
    """sum(a, axis=k). Returns (out_data, out_shape)."""
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data = [_pairwise_sum(g) for g in groups]
    return out_data, out_shape


def prod_all(a_data: List[float], a_shape: List[int]) -> float:
    if not a_data:
        return 1.0
    p = 1.0
    for v in a_data:
        p *= v
    return p


def prod_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[float], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[float] = []
    for g in groups:
        p = 1.0
        for v in g:
            p *= v
        out_data.append(p)
    return out_data, out_shape


def mean_all(a_data: List[float], a_shape: List[int]) -> float:
    n = len(a_data)
    if n == 0:
        return float("nan")
    return _pairwise_sum(a_data) / n


def mean_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[float], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[float] = []
    for g in groups:
        if not g:
            out_data.append(float("nan"))
        else:
            out_data.append(_pairwise_sum(g) / len(g))
    return out_data, out_shape


def var_all(a_data: List[float], a_shape: List[int], ddof: int) -> float:
    n = len(a_data)
    if n - ddof <= 0:
        return float("nan")
    m = _pairwise_sum(a_data) / n
    sq = [(x - m) ** 2 for x in a_data]
    return _pairwise_sum(sq) / (n - ddof)


def var_axis(
    a_data: List[float], a_shape: List[int], axis: int, ddof: int
) -> Tuple[List[float], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[float] = []
    for g in groups:
        n = len(g)
        if n - ddof <= 0:
            out_data.append(float("nan"))
            continue
        m = _pairwise_sum(g) / n
        sq = [(x - m) ** 2 for x in g]
        out_data.append(_pairwise_sum(sq) / (n - ddof))
    return out_data, out_shape


def std_all(a_data: List[float], a_shape: List[int], ddof: int) -> float:
    v = var_all(a_data, a_shape, ddof)
    return math.sqrt(v) if not math.isnan(v) else float("nan")


def std_axis(
    a_data: List[float], a_shape: List[int], axis: int, ddof: int
) -> Tuple[List[float], List[int]]:
    out_data, out_shape = var_axis(a_data, a_shape, axis, ddof)
    out_data = [math.sqrt(v) if not math.isnan(v) else float("nan") for v in out_data]
    return out_data, out_shape


def min_all(a_data: List[float], a_shape: List[int]) -> float:
    if not a_data:
        raise ValueError("zero-size array to reduction operation min")
    m = a_data[0]
    for v in a_data[1:]:
        if v < m:
            m = v
    return m


def min_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[float], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[float] = []
    for g in groups:
        if not g:
            raise ValueError("zero-size array to reduction operation min")
        m = g[0]
        for v in g[1:]:
            if v < m:
                m = v
        out_data.append(m)
    return out_data, out_shape


def max_all(a_data: List[float], a_shape: List[int]) -> float:
    if not a_data:
        raise ValueError("zero-size array to reduction operation max")
    m = a_data[0]
    for v in a_data[1:]:
        if v > m:
            m = v
    return m


def max_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[float], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[float] = []
    for g in groups:
        if not g:
            raise ValueError("zero-size array to reduction operation max")
        m = g[0]
        for v in g[1:]:
            if v > m:
                m = v
        out_data.append(m)
    return out_data, out_shape


def argmin_all(a_data: List[float], a_shape: List[int]) -> int:
    if not a_data:
        raise ValueError("attempt to get argmin of an empty sequence")
    best_i = 0
    best_v = a_data[0]
    for i, v in enumerate(a_data[1:], start=1):
        if v < best_v:
            best_v = v
            best_i = i
    return best_i


def argmin_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[int], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[int] = []
    for g in groups:
        if not g:
            raise ValueError("attempt to get argmin of an empty sequence")
        best_i = 0
        best_v = g[0]
        for i, v in enumerate(g[1:], start=1):
            if v < best_v:
                best_v = v
                best_i = i
        out_data.append(best_i)
    return out_data, out_shape


def argmax_all(a_data: List[float], a_shape: List[int]) -> int:
    if not a_data:
        raise ValueError("attempt to get argmax of an empty sequence")
    best_i = 0
    best_v = a_data[0]
    for i, v in enumerate(a_data[1:], start=1):
        if v > best_v:
            best_v = v
            best_i = i
    return best_i


def argmax_axis(
    a_data: List[float], a_shape: List[int], axis: int
) -> Tuple[List[int], List[int]]:
    axis = _normalize_axis(axis, len(a_shape))
    groups, out_shape = _gather_axis(a_data, a_shape, axis)
    out_data: List[int] = []
    for g in groups:
        if not g:
            raise ValueError("attempt to get argmax of an empty sequence")
        best_i = 0
        best_v = g[0]
        for i, v in enumerate(g[1:], start=1):
            if v > best_v:
                best_v = v
                best_i = i
        out_data.append(best_i)
    return out_data, out_shape
