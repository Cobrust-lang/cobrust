# corpus/numpy/M7.0

This directory holds the M7.0 ndarray-foundation translation corpus
per ADR-0013 (which extends ADR-0012's M7 sub-milestone plan).

## Scope window (M7.0)

Per ADR-0013 §"M7.0 scope window":

- **In scope**:
  - `Array` tagged-union with 5 dtype variants (`Int32 / Int64 /
    Float32 / Float64 / Bool`).
  - `Dtype` enum + Python-string mapping (10 strings: long form +
    type-char form).
  - `array(values, shape, dtype)` — flat-buffer construction.
  - `zeros(shape, dtype)` / `ones(shape, dtype)` — fill-value
    constructors.
  - `arange(start, stop, step, dtype)` — half-open range.
  - `Array::shape() / ndim() / size() / dtype() / repr() / to_json()`.

- **Out of scope (M7.1+)**:
  - Universal functions / element-wise math (M7.1).
  - Broadcasting (M7.1).
  - Indexing / views / slicing (M7.2).
  - Reductions (M7.3).
  - Additional dtypes (i8/i16/u*, f16, complex) — M7.1+.
  - `linspace`, `logspace`, `geomspace`, `empty`, `full`, `eye` —
    deferred (M7.1 covers `full`; the rest land opportunistically).
  - L3 downstream dependents (numpy ecosystem too large for M7.0;
    M7.6+).

## Layout

```
corpus/numpy/M7.0/
    UPSTREAM_VERSION              # "2.0.2"
    UPSTREAM_LICENSE              # BSD-3-Clause (license-compat per ADR-0001)
    spec.toml                     # L0 spec (4 constructors + ancillaries)
    upstream/
        array_core.py             # vendored Python reference subset
    upstream_tests/
        test_array_core.py        # vendored upstream pytest subset
    canned_llm_responses.toml     # synthetic-mode response table
    harness/
        h_array.py                # L0 differential harness (subprocess oracle)
    perf.toml                     # threshold = 0.5 (informational at M7.0)
```

## Differential gate

The gate at `crates/cobrust-numpy/tests/numpy_differential.rs` drives
the upstream numpy 2.0.2 oracle via subprocess:

- For `int32`, `int64`, `bool` dtypes — bytes-identical comparison
  of the `{dtype, shape, data}` JSON payload.
- For `float32`, `float64` dtypes — `rtol = 1e-12` agreement.

When upstream numpy is unavailable on the host (e.g., CI without
Python+numpy), the gate skips with a clear message — same pattern
as M6 msgpack's `tests/msgpack_pyo3_compiles.rs`.

## Why this corpus shape

Per ADR-0012 §"Backend strategy: translate the surface, bind the
core" — cobrust-numpy translates the **public Python surface** of
numpy (constructors, dtype enum, repr, observers) and **binds** the
numerical core via the `ndarray` Rust crate. We do not reimplement
`ArrayD::zeros` in Rust; we call it.

This corpus contains the **surface translation** payload: the
`array_core.py` reference, the canned LLM responses that emit the
Rust translation, and the differential harness that compares both
against upstream numpy.

## License

BSD-3-Clause for the vendored numpy subset (`upstream/array_core.py`
is hand-written but mirrors numpy's documented public API). The
cobrust-numpy crate it generates is dual-licensed Apache-2.0 OR MIT
per ADR-0001.
