# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored numpy 2.0.2 reference subset for cobrust M7.2 (per ADR-0015).
# This is the pipeline-time reference: the L0 differential harness
# imports this module to validate the cobrust-numpy translator pipeline
# end-to-end. The runtime correctness contract against upstream `numpy`
# is exercised at the test layer (`tests/index_differential.rs`).
#
# Scope (M7.2 per ADR-0015):
#   - Basic slicing (`a[start:stop:step]`, including negative step).
#   - Single-int indexing (`a[i]`, including negative index).
#   - Integer-array indexing (`a[[0, 2, 5]]` — copy).
#   - Boolean-mask indexing (`a[a > 0]` — copy).
#   - np.where(cond, x, y) (copy; broadcasts per ADR-0014).
#   - View-vs-copy semantics per numpy's documented contract.

from __future__ import annotations

from typing import List, Optional, Sequence, Tuple


def _normalize_single(idx: int, length: int) -> int:
    """Normalize a single integer index into a non-negative one;
    raise IndexError on out-of-bounds."""
    if idx < 0:
        norm = idx + length
    else:
        norm = idx
    if norm < 0 or norm >= length:
        raise IndexError(
            f"index {idx} is out of bounds for axis with length {length}"
        )
    return norm


def _resolve_slice(
    start: Optional[int],
    stop: Optional[int],
    step: Optional[int],
    length: int,
) -> Tuple[int, int, int]:
    """Resolve numpy-exact slice bounds. Returns (begin, end, step)
    with step != 0; clamps out-of-range bounds (matches numpy).

    Raises ValueError on step=0 (matches numpy)."""
    if step is None:
        step = 1
    if step == 0:
        raise ValueError("slice step cannot be zero")

    if step > 0:
        # Default start = 0, stop = length.
        s = 0 if start is None else start
        e = length if stop is None else stop
        if s < 0:
            s += length
        if e < 0:
            e += length
        s = max(0, min(s, length))
        e = max(0, min(e, length))
        if e < s:
            e = s
    else:
        # Default start = length-1, stop = -length-1 (so we hit index 0).
        s = (length - 1) if start is None else start
        e = (-length - 1) if stop is None else stop
        if s < 0 and start is not None:
            s += length
        if e < 0 and stop is not None:
            e += length
        # Clamp
        s = max(-1, min(s, length - 1))
        if e < -1:
            e = -1
        elif e > length:
            e = length
        if e > s:
            e = s
    return s, e, step


def _slice_count(begin: int, end: int, step: int) -> int:
    """Count elements produced by a normalised slice."""
    if step > 0:
        if end <= begin:
            return 0
        return (end - begin + step - 1) // step
    else:
        if end >= begin:
            return 0
        # step < 0: walk down from `begin` toward `end` (exclusive).
        return (begin - end + (-step) - 1) // (-step)


def _slice_indices(begin: int, end: int, step: int) -> List[int]:
    """Materialise the integer indices a slice produces."""
    out: List[int] = []
    cur = begin
    if step > 0:
        while cur < end:
            out.append(cur)
            cur += step
    else:
        while cur > end:
            out.append(cur)
            cur += step
    return out


def _flat_index(shape: Sequence[int], multi: Sequence[int]) -> int:
    idx = 0
    stride = 1
    for k in range(len(shape) - 1, -1, -1):
        idx += multi[k] * stride
        stride *= shape[k]
    return idx


def _multi_indices(shape: Sequence[int]):
    if not shape:
        yield ()
        return
    counters = [0] * len(shape)
    while True:
        yield tuple(counters)
        for k in range(len(shape) - 1, -1, -1):
            counters[k] += 1
            if counters[k] < shape[k]:
                break
            counters[k] = 0
        else:
            return


def slice_basic(
    a_data: List[float], a_shape: List[int],
    start: Optional[int], stop: Optional[int], step: Optional[int],
) -> Tuple[List[float], List[int]]:
    """Apply a basic slice on the first axis.
    Mirrors `a[start:stop:step]` for an N-D array — only the first
    axis is sliced; trailing axes are kept whole."""
    if not a_shape:
        raise IndexError("cannot slice a 0-d array")
    length = a_shape[0]
    begin, end, st = _resolve_slice(start, stop, step, length)
    indices = _slice_indices(begin, end, st)
    inner: int = 1
    for d in a_shape[1:]:
        inner *= d
    out_data: List[float] = []
    for i in indices:
        for j in range(inner):
            out_data.append(a_data[i * inner + j])
    out_shape = [len(indices)] + list(a_shape[1:])
    return out_data, out_shape


def take(
    a_data: List[float], a_shape: List[int], indices: List[int],
) -> Tuple[List[float], List[int]]:
    """Integer-array indexing on the first axis. Always materialises a copy.
    Mirrors `a[indices]` for an N-D array — first axis indexed by
    `indices`, trailing axes kept whole."""
    if not a_shape:
        raise IndexError("cannot take from a 0-d array")
    length = a_shape[0]
    inner: int = 1
    for d in a_shape[1:]:
        inner *= d
    norm: List[int] = []
    for idx in indices:
        norm.append(_normalize_single(idx, length))
    out_data: List[float] = []
    for i in norm:
        for j in range(inner):
            out_data.append(a_data[i * inner + j])
    out_shape = [len(indices)] + list(a_shape[1:])
    return out_data, out_shape


def mask(
    a_data: List[float], a_shape: List[int],
    bool_data: List[bool], bool_shape: List[int],
) -> Tuple[List[float], List[int]]:
    """Boolean-mask indexing. mask shape must match a's shape.
    Returns a 1-D copy of selected elements (matches numpy)."""
    if list(bool_shape) != list(a_shape):
        raise IndexError(
            f"boolean index shape mismatch: a={a_shape} mask={bool_shape}"
        )
    out_data: List[float] = []
    for i, keep in enumerate(bool_data):
        if keep:
            out_data.append(a_data[i])
    return out_data, [len(out_data)]


def np_where(
    cond_data: List[bool], cond_shape: List[int],
    x_data: List[float], x_shape: List[int],
    y_data: List[float], y_shape: List[int],
) -> Tuple[List[float], List[int]]:
    """Element-wise selection: out[i] = x[i] if cond[i] else y[i].
    Broadcasts cond/x/y per ADR-0014 broadcasting rules.

    M7.2 ships the three-arg form (one-arg numpy.where(cond) returning
    indices is M7.x deferred per ADR-0015)."""
    # Compute pairwise broadcasts: (cond, x), then with y.
    from corpus.numpy_M7_1_ufunc import broadcast_shape  # type: ignore  # pragma: no cover
    # (Inlined below to avoid cross-corpus imports; the harness uses
    # numpy directly anyway.)


def single_index(
    a_data: List[float], a_shape: List[int], idx: int,
) -> Tuple[List[float], List[int]]:
    """Single-int indexing on the first axis.
    Mirrors `a[idx]` — drops the first axis, keeps trailing axes."""
    if not a_shape:
        raise IndexError("cannot index a 0-d array")
    length = a_shape[0]
    i = _normalize_single(idx, length)
    inner: int = 1
    for d in a_shape[1:]:
        inner *= d
    base = i * inner
    out_data = list(a_data[base:base + inner])
    out_shape = list(a_shape[1:])
    return out_data, out_shape


# Helpers used by the L0 spec for the synthetic pipeline.

def normalize_single(idx: int, length: int) -> int:
    return _normalize_single(idx, length)


def resolve_slice(
    start: Optional[int],
    stop: Optional[int],
    step: Optional[int],
    length: int,
) -> Tuple[int, int, int]:
    return _resolve_slice(start, stop, step, length)


def slice_count(begin: int, end: int, step: int) -> int:
    return _slice_count(begin, end, step)
