# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored numpy 2.0.2 reference subset for cobrust M7.5 (per ADR-0018).
# This is the pipeline-time reference: the L0 differential harness
# imports this module to validate the cobrust-numpy translator pipeline
# end-to-end. The runtime correctness contract against upstream `numpy`
# is exercised at the test layer (`tests/random_differential.rs`) using
# distribution-level KS-test agreement.
#
# Scope (M7.5 per ADR-0018):
#   - 7 distributions: default_rng, seed, integers, random, normal,
#     uniform, choice.
#   - PRNG backend: Mersenne Twister (Python stdlib `random`), used here
#     only for the pipeline-time pure-Python reference; production path
#     uses rand_pcg::Pcg64.
#   - Seed reproducibility: same seed → same stream within this module
#     (Python's random module is deterministic).
#
# This module does NOT attempt to match numpy's PCG64 stream byte-for-byte
# — that's an explicit non-goal per ADR-0018 §2. The differential harness
# at h_random.py uses upstream `numpy.random.Generator` for the actual
# distribution comparison.

from __future__ import annotations

import math
import random as _random
from typing import List, Optional, Tuple


def _validate_int_range(low: int, high: int) -> None:
    """Validate low < high; raise ValueError otherwise (numpy parity)."""
    if low >= high:
        raise ValueError(f"low >= high (low={low}, high={high})")


def _validate_distribution_params(
    scale: Optional[float] = None,
    low: Optional[float] = None,
    high: Optional[float] = None,
) -> None:
    """Validate scale > 0 / low < high / finite. Numpy raises ValueError."""
    if scale is not None:
        if not math.isfinite(scale) or scale <= 0:
            raise ValueError(f"scale must be > 0 and finite, got {scale}")
    if low is not None and high is not None:
        if not math.isfinite(low) or not math.isfinite(high):
            raise ValueError(f"low/high must be finite, got low={low}, high={high}")
        if low >= high:
            raise ValueError(f"low >= high (low={low}, high={high})")


def _validate_probabilities(p: List[float], n: int) -> None:
    """Validate p sums to 1 (rtol 1e-8), no negatives, length == n."""
    if len(p) != n:
        raise ValueError(f"p length {len(p)} != values length {n}")
    s = 0.0
    for v in p:
        if not math.isfinite(v) or v < 0:
            raise ValueError(f"p contains invalid value {v}")
        s += v
    if abs(s - 1.0) > 1e-8:
        raise ValueError(f"probabilities do not sum to 1 (sum={s})")


def _box_muller(u1: float, u2: float) -> Tuple[float, float]:
    """Box-Muller transform reference. Two unit-uniform inputs → two N(0,1)."""
    r = math.sqrt(-2.0 * math.log(u1))
    theta = 2.0 * math.pi * u2
    return r * math.cos(theta), r * math.sin(theta)


# Module-level Generator state for the pipeline-time reference.
# Production path uses rand_pcg::Pcg64 in cobrust-numpy/src/random.rs.

class Generator:
    def __init__(self, seed: Optional[int]) -> None:
        self._rng = _random.Random()
        if seed is not None:
            self._rng.seed(seed)
        self._seed_value = seed

    def seed(self, s: int) -> None:
        self._rng.seed(s)
        self._seed_value = s

    def seed_value(self) -> Optional[int]:
        return self._seed_value


def default_rng(seed: Optional[int] = None) -> Generator:
    """Construct a Generator from an optional seed."""
    return Generator(seed)


def integers(
    gen: Generator, low: int, high: int, size: List[int]
) -> Tuple[List[int], List[int]]:
    """Uniform integers in [low, high). Returns (data, shape)."""
    _validate_int_range(low, high)
    n = 1
    for d in size:
        n *= d
    data = [gen._rng.randrange(low, high) for _ in range(n)]
    return data, list(size)


def random(gen: Generator, size: List[int]) -> Tuple[List[float], List[int]]:
    """Uniform floats in [0, 1)."""
    n = 1
    for d in size:
        n *= d
    data = [gen._rng.random() for _ in range(n)]
    return data, list(size)


def normal(
    gen: Generator, loc: float, scale: float, size: List[int]
) -> Tuple[List[float], List[int]]:
    """Gaussian N(loc, scale²)."""
    _validate_distribution_params(scale=scale)
    n = 1
    for d in size:
        n *= d
    data = [gen._rng.gauss(loc, scale) for _ in range(n)]
    return data, list(size)


def uniform(
    gen: Generator, low: float, high: float, size: List[int]
) -> Tuple[List[float], List[int]]:
    """Uniform floats in [low, high)."""
    _validate_distribution_params(low=low, high=high)
    n = 1
    for d in size:
        n *= d
    data = [low + gen._rng.random() * (high - low) for _ in range(n)]
    return data, list(size)


def choice(
    gen: Generator,
    values: List[float],
    size: List[int],
    replace: bool,
    p: Optional[List[float]],
) -> Tuple[List[float], List[int]]:
    """Uniform / weighted selection from values."""
    if not values:
        raise ValueError("a must be non-empty")
    n_values = len(values)
    if p is not None:
        _validate_probabilities(p, n_values)
    n_out = 1
    for d in size:
        n_out *= d
    if not replace and n_out > n_values:
        raise ValueError("cannot take more samples than values when replace=False")

    data: List[float] = []
    if replace:
        if p is None:
            for _ in range(n_out):
                data.append(values[gen._rng.randrange(n_values)])
        else:
            # Weighted with replacement: invert CDF.
            cdf = []
            running = 0.0
            for v in p:
                running += v
                cdf.append(running)
            for _ in range(n_out):
                u = gen._rng.random()
                idx = 0
                for i, c in enumerate(cdf):
                    if u <= c:
                        idx = i
                        break
                data.append(values[idx])
    else:
        # Without replacement: Fisher-Yates partial shuffle.
        pool = list(values)
        for i in range(n_out):
            j = gen._rng.randrange(i, n_values)
            pool[i], pool[j] = pool[j], pool[i]
            data.append(pool[i])

    return data, list(size)
