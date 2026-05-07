//! PyO3 bindings for cobrust-msgpack.
//!
//! Gated by `--features pyo3` per ADR-0011 §3. When compiled with the
//! feature, this module exposes a `cobrust_msgpack` Python extension
//! whose public functions are `pack(obj) -> bytes` and
//! `unpack(bytes) -> obj`. The Python types accepted by `pack` are
//! the M6 value scope (None / bool / int / float / str / bytes /
//! list / dict).
//!
//! Translation between Python and the native [`crate::MsgValue`]
//! representation goes through `serde_json::Value` to keep the M6
//! wrapper pure-Rust on its public surface; M7+ may inline a faster
//! direct Python-object path if the wrapper becomes a hot path.

#![allow(clippy::needless_pass_by_value)]

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

use crate::{MsgValue, pack_to_vec, unpack as native_unpack};

#[pyfunction]
fn pack(py: Python<'_>, obj: &PyAny) -> PyResult<Py<PyBytes>> {
    let value = py_to_msg(obj)?;
    let bytes =
        pack_to_vec(&value).map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    Ok(PyBytes::new_bound(py, &bytes).unbind())
}

#[pyfunction]
fn unpack(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let value =
        native_unpack(data).map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    msg_to_py(py, &value)
}

fn py_to_msg(obj: &PyAny) -> PyResult<MsgValue> {
    if obj.is_none() {
        return Ok(MsgValue::Nil);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(MsgValue::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        if i >= 0 {
            return Ok(MsgValue::UInt(i as u64));
        }
        return Ok(MsgValue::Int(i));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(MsgValue::Float(f));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(MsgValue::Str(s));
    }
    if let Ok(b) = obj.extract::<Vec<u8>>() {
        return Ok(MsgValue::Bin(b));
    }
    if let Ok(list) = obj.downcast::<PyList>() {
        let mut items: Vec<MsgValue> = Vec::with_capacity(list.len());
        for item in list.iter() {
            items.push(py_to_msg(item.as_gil_ref())?);
        }
        return Ok(MsgValue::Array(items));
    }
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let mut items: Vec<MsgValue> = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            items.push(py_to_msg(item.as_gil_ref())?);
        }
        return Ok(MsgValue::Array(items));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut items: Vec<(String, MsgValue)> = Vec::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            items.push((key, py_to_msg(v.as_gil_ref())?));
        }
        return Ok(MsgValue::Map(items));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "M6 scope: unsupported python type",
    ))
}

fn msg_to_py(py: Python<'_>, value: &MsgValue) -> PyResult<PyObject> {
    match value {
        MsgValue::Nil => Ok(py.None()),
        MsgValue::Bool(b) => Ok(b.into_py(py)),
        MsgValue::Int(i) => Ok(i.into_py(py)),
        MsgValue::UInt(u) => Ok(u.into_py(py)),
        MsgValue::Float(f) => Ok(f.into_py(py)),
        MsgValue::Str(s) => Ok(s.into_py(py)),
        MsgValue::Bin(b) => Ok(PyBytes::new_bound(py, b).unbind().into_py(py)),
        MsgValue::Array(items) => {
            let list = PyList::empty_bound(py);
            for v in items {
                list.append(msg_to_py(py, v)?)?;
            }
            Ok(list.unbind().into_py(py))
        }
        MsgValue::Map(items) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in items {
                dict.set_item(k, msg_to_py(py, v)?)?;
            }
            Ok(dict.unbind().into_py(py))
        }
    }
}

#[pymodule]
fn cobrust_msgpack(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pack, m)?)?;
    m.add_function(wrap_pyfunction!(unpack, m)?)?;
    Ok(())
}
