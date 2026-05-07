# SPDX-License-Identifier: BSD-3-Clause
#
# Vendored test subset for cobrust M7.0 (per ADR-0013).
# Drives `array_core.py` (the M7.0 reference oracle) and asserts
# behavior against canonical numpy semantics. These tests are
# intentionally cheap and pure-Python so they can run inside the
# translator pipeline without depending on numpy itself; the rich
# differential-vs-numpy gate lives in the Rust test layer.

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'upstream'))

import array_core as ac


def test_zeros_int32():
    out = ac.zeros([3, 2], "int32")
    assert out["dtype"] == "Int32"
    assert out["shape"] == [3, 2]
    assert out["data"] == [0, 0, 0, 0, 0, 0]


def test_zeros_bool():
    out = ac.zeros([4], "bool")
    assert out["dtype"] == "Bool"
    assert out["shape"] == [4]
    assert out["data"] == [False, False, False, False]


def test_ones_float64():
    out = ac.ones([2, 2], "float64")
    assert out["dtype"] == "Float64"
    assert out["data"] == [1.0, 1.0, 1.0, 1.0]


def test_ones_bool():
    out = ac.ones([3], "bool")
    assert out["data"] == [True, True, True]


def test_array_int64_flat():
    out = ac.array([1, 2, 3, 4], [4], "int64")
    assert out["dtype"] == "Int64"
    assert out["data"] == [1, 2, 3, 4]


def test_array_float32_2d():
    out = ac.array([1, 2, 3, 4, 5, 6], [2, 3], "float32")
    assert out["shape"] == [2, 3]
    assert out["dtype"] == "Float32"


def test_array_shape_mismatch():
    try:
        ac.array([1, 2, 3], [2, 2], "int64")
    except ValueError:
        return
    assert False, "shape mismatch should raise"


def test_arange_int_basic():
    out = ac.arange(0, 5, 1, "int64")
    assert out["data"] == [0, 1, 2, 3, 4]


def test_arange_int_step():
    out = ac.arange(0, 10, 2, "int64")
    assert out["data"] == [0, 2, 4, 6, 8]


def test_arange_float():
    out = ac.arange(0, 1, 0.25, "float64")
    assert out["data"] == [0.0, 0.25, 0.5, 0.75]


def test_arange_empty_when_step_wrong_sign():
    out = ac.arange(0, 5, -1, "int64")
    assert out["data"] == []


def test_arange_zero_step_raises():
    try:
        ac.arange(0, 5, 0, "int64")
    except ValueError:
        return
    assert False, "zero step should raise"


def test_arange_bool_unsupported():
    try:
        ac.arange(0, 5, 1, "bool")
    except ValueError:
        return
    assert False, "arange dtype=bool should raise (matches numpy)"


def test_dtype_alias_strings():
    assert ac.parse_dtype("int32") == "Int32"
    assert ac.parse_dtype("i4") == "Int32"
    assert ac.parse_dtype("int64") == "Int64"
    assert ac.parse_dtype("i8") == "Int64"
    assert ac.parse_dtype("float32") == "Float32"
    assert ac.parse_dtype("f4") == "Float32"
    assert ac.parse_dtype("float64") == "Float64"
    assert ac.parse_dtype("f8") == "Float64"
    assert ac.parse_dtype("bool") == "Bool"
    assert ac.parse_dtype("?") == "Bool"


def test_dtype_unknown_raises():
    try:
        ac.parse_dtype("complex128")
    except ValueError:
        return
    assert False, "unknown dtype should raise"


def test_repr_zeros():
    s = ac.array_repr(ac.zeros([2, 2], "float64"))
    assert "array(" in s
    assert "dtype=float64" in s


def test_repr_arange():
    s = ac.array_repr(ac.arange(0, 5, 1, "int64"))
    assert "0" in s and "4" in s
    assert "dtype=int64" in s


def test_shape_size_negative_raises():
    try:
        ac.shape_size([-1, 2])
    except ValueError:
        return
    assert False, "negative dim should raise"


def test_shape_size_empty():
    assert ac.shape_size([]) == 1


if __name__ == "__main__":
    import sys
    fns = [v for k, v in globals().items() if k.startswith("test_") and callable(v)]
    failed = 0
    for fn in fns:
        try:
            fn()
            print(f"PASS {fn.__name__}")
        except Exception as e:
            print(f"FAIL {fn.__name__}: {e}")
            failed += 1
    sys.exit(failed)
