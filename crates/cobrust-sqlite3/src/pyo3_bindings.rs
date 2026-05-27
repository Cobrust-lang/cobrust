//! PyO3 bindings for cobrust-sqlite3.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 (mirrored for the
//! ecosystem batch by ADR-0022 §6). When compiled with the feature,
//! this module exposes a `cobrust_sqlite3` Python extension whose
//! surface mirrors the canonical `sqlite3` priors:
//! `connect(path)` -> `Connection`, `.cursor()` -> `Cursor`,
//! `.execute(sql, params)`, `.fetchone() / .fetchmany(n) /
//! .fetchall()`. Cells cross the FFI boundary as native Python objects
//! (`None / int / float / str / bytes`), matching `sqlite3`'s mapping
//! of the five storage classes.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};

use crate::{Connection, Cursor, SqliteError, Value, connect};

fn sqlite_err_to_py(err: SqliteError) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(format!("{err}"))
}

/// Lift a Python object into our `Value` for qmark binding.
fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Value::Integer(i));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(Value::Real(f));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::Text(s));
    }
    if let Ok(b) = obj.extract::<Vec<u8>>() {
        return Ok(Value::Blob(b));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "unsupported bind parameter type (expected None/int/float/str/bytes)",
    ))
}

/// Project a cell back into a native Python object.
fn value_to_py(py: Python<'_>, value: &Value) -> PyObject {
    match value {
        Value::Null => py.None(),
        Value::Integer(i) => i
            .into_pyobject(py)
            .map_or_else(|_| py.None(), |o| o.into_any().unbind()),
        Value::Real(r) => r
            .into_pyobject(py)
            .map_or_else(|_| py.None(), |o| o.into_any().unbind()),
        Value::Text(s) => s
            .into_pyobject(py)
            .map_or_else(|_| py.None(), |o| o.into_any().unbind()),
        Value::Blob(b) => PyBytes::new(py, b).into_any().unbind(),
    }
}

fn row_to_py_tuple(py: Python<'_>, row: &crate::Row) -> PyResult<PyObject> {
    let cells: Vec<PyObject> = row.cells().iter().map(|c| value_to_py(py, c)).collect();
    let tuple = PyTuple::new(py, cells)?;
    Ok(tuple.into_any().unbind())
}

#[pyclass(name = "Cursor", unsendable)]
struct PyCursor {
    inner: Cursor,
}

#[pymethods]
impl PyCursor {
    #[pyo3(signature = (sql, params=None))]
    fn execute(&mut self, sql: &str, params: Option<Vec<Bound<'_, PyAny>>>) -> PyResult<()> {
        let bound: Vec<Value> = match params {
            Some(list) => list
                .iter()
                .map(py_to_value)
                .collect::<PyResult<Vec<Value>>>()?,
            None => Vec::new(),
        };
        self.inner.execute(sql, &bound).map_err(sqlite_err_to_py)?;
        Ok(())
    }

    fn fetchone(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        match self.inner.fetchone() {
            Some(row) => row_to_py_tuple(py, &row),
            None => Ok(py.None()),
        }
    }

    fn fetchmany(&mut self, py: Python<'_>, size: usize) -> PyResult<PyObject> {
        let rows = self.inner.fetchmany(size);
        let tuples: Vec<PyObject> = rows
            .iter()
            .map(|r| row_to_py_tuple(py, r))
            .collect::<PyResult<Vec<PyObject>>>()?;
        Ok(PyList::new(py, tuples)?.into_any().unbind())
    }

    fn fetchall(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let rows = self.inner.fetchall();
        let tuples: Vec<PyObject> = rows
            .iter()
            .map(|r| row_to_py_tuple(py, r))
            .collect::<PyResult<Vec<PyObject>>>()?;
        Ok(PyList::new(py, tuples)?.into_any().unbind())
    }

    #[getter]
    fn rowcount(&self) -> i64 {
        self.inner.rowcount()
    }

    #[getter]
    fn lastrowid(&self) -> Option<i64> {
        self.inner.lastrowid()
    }
}

#[pyclass(name = "Connection", unsendable)]
struct PyConnection {
    inner: Connection,
}

#[pymethods]
impl PyConnection {
    fn cursor(&self) -> PyCursor {
        PyCursor {
            inner: self.inner.cursor(),
        }
    }

    #[pyo3(signature = (sql, params=None))]
    fn execute(&self, sql: &str, params: Option<Vec<Bound<'_, PyAny>>>) -> PyResult<PyCursor> {
        let bound: Vec<Value> = match params {
            Some(list) => list
                .iter()
                .map(py_to_value)
                .collect::<PyResult<Vec<Value>>>()?,
            None => Vec::new(),
        };
        let mut cur = self.inner.cursor();
        cur.execute(sql, &bound).map_err(sqlite_err_to_py)?;
        Ok(PyCursor { inner: cur })
    }

    fn commit(&self) -> PyResult<()> {
        self.inner.commit().map_err(sqlite_err_to_py)
    }

    fn rollback(&self) -> PyResult<()> {
        self.inner.rollback().map_err(sqlite_err_to_py)
    }

    fn close(&self) -> PyResult<()> {
        self.inner.close().map_err(sqlite_err_to_py)
    }
}

#[pyfunction]
fn connect_py(path: &str) -> PyResult<PyConnection> {
    let inner = connect(path).map_err(sqlite_err_to_py)?;
    Ok(PyConnection { inner })
}

#[pymodule]
fn cobrust_sqlite3(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(connect_py, m)?)?;
    // Expose under the canonical Python name `connect`.
    m.add("connect", m.getattr("connect_py")?)?;
    m.add_class::<PyConnection>()?;
    m.add_class::<PyCursor>()?;
    Ok(())
}
