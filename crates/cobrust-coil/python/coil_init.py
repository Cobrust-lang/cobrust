# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-coil. DO NOT EDIT BY HAND.
"""Cobrust coil — translated ndarray foundation (PyO3 placeholder); translated from CPython numpy 2.0.2.

M7.0 ndarray foundation per ADR-0013. When built with `cargo build -p
cobrust-coil --features pyo3`, the extension exposes `array`,
`zeros`, `ones`, and `arange` from the native module
`coil`. Each returns a `dict` of shape `{"dtype": str,
"shape": list[int], "data": list}` — that's the M7.0 surface.
M7.1+ may upgrade to a richer numpy-compatible type.
"""

__version__ = "2.0.2+cobrust"

# When compiled with --features pyo3, importing `coil` from
# the built native extension provides `zeros / ones / arange / array`.
# Without the feature, this stub is the only Python-side surface; the
# Rust lib remains importable from Rust crates.
