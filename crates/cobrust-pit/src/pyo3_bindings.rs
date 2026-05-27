//! PyO3 bindings for cobrust-pit.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 (mirrored for the
//! ecosystem batch by ADR-0022 §6). When compiled with the feature,
//! this module exposes a `pit` Python extension with an `App` class
//! whose route registration takes a Python callable
//! `handler(request_dict) -> (status, body)` and a blocking `run`.
//!
//! This is the minimal Python-side shape — a full Flask-parity wrapper
//! (decorator sugar, `Request`/`Response` classes) lands with the
//! `.cb`-source wiring follow-on. The build-path test
//! (`tests/pit_pyo3_compiles.rs`) only requires that this module
//! compiles when libpython is present.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::{App, PitError, Request, Response};

fn pit_err_to_py(err: PitError) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(format!("{err}"))
}

/// Python-side `App`. Routes registered here invoke a Python callable
/// `handler(request: dict) -> (status: int, body: str)`.
#[pyclass(name = "App")]
struct PyApp {
    // Built incrementally via add_route; consumed by run().
    inner: Mutex<Option<App>>,
}

#[pymethods]
impl PyApp {
    #[new]
    fn new() -> Self {
        Self {
            inner: Mutex::new(Some(App::new())),
        }
    }

    /// Register `(method, path, handler)`. The Python handler receives a
    /// dict shaped like `{"method", "path", "body"}` and must return
    /// `(status: int, body: str)`.
    fn route(&self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| pit_err_to_py(PitError::runtime("App mutex poisoned")))?;
        let app = guard
            .as_mut()
            .ok_or_else(|| pit_err_to_py(PitError::runtime("App already consumed by run()")))?;
        let cb = handler;
        app.route(method, path, move |req: Request| call_py_handler(&cb, &req))
            .map_err(pit_err_to_py)
    }

    /// Run the server, blocking. Mirrors `app.run(host, port)`.
    fn run(&self, host: &str, port: u16) -> PyResult<()> {
        let app = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| pit_err_to_py(PitError::runtime("App mutex poisoned")))?;
            guard
                .take()
                .ok_or_else(|| pit_err_to_py(PitError::runtime("App already consumed")))?
        };
        app.run(host, port).map_err(pit_err_to_py)
    }
}

/// Invoke a Python handler with a request dict; coerce the
/// `(status, body)` tuple result back into a [`Response`]. Any Python
/// exception or shape mismatch degrades to a 500 text response (the
/// server path must never panic).
fn call_py_handler(cb: &PyObject, req: &Request) -> Response {
    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        let _ = dict.set_item("method", req.method());
        let _ = dict.set_item("path", req.path());
        let _ = dict.set_item("body", String::from_utf8_lossy(req.body()).into_owned());
        let params = PyDict::new(py);
        for (k, v) in req.path_params() {
            let _ = params.set_item(k, v);
        }
        let _ = dict.set_item("path_params", params);

        match cb.call1(py, (dict,)) {
            Ok(ret) => coerce_response(py, &ret),
            Err(e) => Response::text(format!("handler raised: {e}")).with_status(500),
        }
    })
}

fn coerce_response(py: Python<'_>, ret: &PyObject) -> Response {
    // Accept either a (status, body) tuple or a bare string body.
    if let Ok((status, body)) = ret.extract::<(u16, String)>(py) {
        return Response::text(body).with_status(status);
    }
    if let Ok(body) = ret.extract::<String>(py) {
        return Response::text(body);
    }
    let _ = HashMap::<String, String>::new();
    Response::text("handler returned an unsupported type").with_status(500)
}

#[pymodule]
fn pit(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyApp>()?;
    Ok(())
}
