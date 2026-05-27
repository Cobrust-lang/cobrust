// AUTO-GENERATED — DO NOT EDIT BY HAND.
// PyO3 wrapper for cobrust-coil M7.0 per ADR-0011 + ADR-0013.

//! PyO3 bindings for cobrust-coil.
//!
//! Gated by `--features pyo3` per ADR-0011 §3. When compiled with
//! the feature, this module exposes a `coil` Python
//! extension whose public functions are `zeros(shape, dtype) -> dict`
//! / `ones(shape, dtype) -> dict` / `arange(start, stop, step, dtype)
//! -> dict` / `array(values, shape, dtype) -> dict`. Each returns a
//! `dict` matching the cobrust-coil `to_json` shape — that's enough
//! for the M7.0 differential gate; richer numpy-compatible Python
//! types (e.g. a real `numpy.ndarray` view) lift in M7.1+.

// CQ P1-4 + template-fix: single consolidated block; future emits use #[allow] at item level.
#![allow(clippy::needless_pass_by_value)]

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{
    Array, Dtype, NumpyError, arange as native_arange, array as native_array, ones as native_ones,
    zeros as native_zeros,
};

fn parse_dtype(s: &str) -> PyResult<Dtype> {
    Dtype::from_python_string(s)
        .map_err(|e: NumpyError| pyo3::exceptions::PyValueError::new_err(format!("{e}")))
}

fn array_to_pydict<'py>(py: Python<'py>, arr: &Array) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("dtype", arr.dtype().to_rust_variant_name())?;
    dict.set_item("shape", arr.shape())?;
    let data = arr.to_json()["data"].clone();
    dict.set_item("data", serde_json::to_string(&data).unwrap_or_default())?;
    Ok(dict)
}

#[pyfunction]
fn zeros<'py>(py: Python<'py>, shape: Vec<usize>, dtype: &str) -> PyResult<Bound<'py, PyDict>> {
    let dt = parse_dtype(dtype)?;
    let arr = native_zeros(&shape, dt)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    array_to_pydict(py, &arr)
}

#[pyfunction]
fn ones<'py>(py: Python<'py>, shape: Vec<usize>, dtype: &str) -> PyResult<Bound<'py, PyDict>> {
    let dt = parse_dtype(dtype)?;
    let arr = native_ones(&shape, dt)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    array_to_pydict(py, &arr)
}

#[pyfunction]
fn arange<'py>(
    py: Python<'py>,
    start: f64,
    stop: f64,
    step: f64,
    dtype: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let dt = parse_dtype(dtype)?;
    let arr = native_arange(start, stop, step, dt)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    array_to_pydict(py, &arr)
}

#[pyfunction]
fn array<'py>(
    py: Python<'py>,
    values: Vec<f64>,
    shape: Vec<usize>,
    dtype: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let dt = parse_dtype(dtype)?;
    let arr = native_array(&values, &shape, dt)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    array_to_pydict(py, &arr)
}

/// Module entrypoint per ADR-0011: `coil.zeros(...)` etc.
#[pymodule]
fn coil(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(zeros, m)?)?;
    m.add_function(wrap_pyfunction!(ones, m)?)?;
    m.add_function(wrap_pyfunction!(arange, m)?)?;
    m.add_function(wrap_pyfunction!(array, m)?)?;
    Ok(())
}
