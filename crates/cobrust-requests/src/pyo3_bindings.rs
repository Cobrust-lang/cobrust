//! PyO3 bindings for cobrust-requests.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 (mirrored for the
//! ecosystem batch by ADR-0022 §6). When compiled with the feature,
//! this module exposes a `cobrust_requests` Python extension whose
//! public functions are `get / post / put / patch / delete / head`
//! plus a `Session` class. Returns Python dicts shaped like
//! `{"status": int, "headers": dict, "body": str}` to keep the
//! Python-side surface minimal — M9+ widens to a full `Response`
//! class.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::{HttpError, Session};

fn response_to_py(py: Python<'_>, resp: crate::Response) -> PyResult<PyObject> {
    let dict = PyDict::new_bound(py);
    dict.set_item("status", resp.status_code())?;
    let headers_py = PyDict::new_bound(py);
    for (k, v) in resp.headers() {
        headers_py.set_item(k, v)?;
    }
    dict.set_item("headers", headers_py)?;
    let body = resp.bytes();
    dict.set_item("body", PyBytes::new_bound(py, &body))?;
    Ok(dict.unbind().into_py(py))
}

fn http_err_to_py(err: HttpError) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(format!("{err}"))
}

#[pyfunction]
fn get(py: Python<'_>, url: &str) -> PyResult<PyObject> {
    let resp = crate::get(url).map_err(http_err_to_py)?;
    response_to_py(py, resp)
}

#[pyfunction]
fn post(py: Python<'_>, url: &str, body: &[u8]) -> PyResult<PyObject> {
    let resp = crate::post(url, body).map_err(http_err_to_py)?;
    response_to_py(py, resp)
}

#[pyfunction]
fn put(py: Python<'_>, url: &str, body: &[u8]) -> PyResult<PyObject> {
    let resp = crate::put(url, body).map_err(http_err_to_py)?;
    response_to_py(py, resp)
}

#[pyfunction]
fn patch(py: Python<'_>, url: &str, body: &[u8]) -> PyResult<PyObject> {
    let resp = crate::patch(url, body).map_err(http_err_to_py)?;
    response_to_py(py, resp)
}

#[pyfunction]
fn delete(py: Python<'_>, url: &str) -> PyResult<PyObject> {
    let resp = crate::delete(url).map_err(http_err_to_py)?;
    response_to_py(py, resp)
}

#[pyfunction]
fn head(py: Python<'_>, url: &str) -> PyResult<PyObject> {
    let resp = crate::head(url).map_err(http_err_to_py)?;
    response_to_py(py, resp)
}

#[pyclass(name = "Session")]
struct PySession {
    inner: Session,
}

#[pymethods]
impl PySession {
    #[new]
    fn new() -> Self {
        Self {
            inner: Session::new(),
        }
    }

    fn get(&self, py: Python<'_>, url: &str) -> PyResult<PyObject> {
        let resp = self.inner.get(url).map_err(http_err_to_py)?;
        response_to_py(py, resp)
    }

    fn post(&self, py: Python<'_>, url: &str, body: &[u8]) -> PyResult<PyObject> {
        let resp = self.inner.post(url, body).map_err(http_err_to_py)?;
        response_to_py(py, resp)
    }
}

#[pymodule]
fn cobrust_requests(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get, m)?)?;
    m.add_function(wrap_pyfunction!(post, m)?)?;
    m.add_function(wrap_pyfunction!(put, m)?)?;
    m.add_function(wrap_pyfunction!(patch, m)?)?;
    m.add_function(wrap_pyfunction!(delete, m)?)?;
    m.add_function(wrap_pyfunction!(head, m)?)?;
    m.add_class::<PySession>()?;
    Ok(())
}
