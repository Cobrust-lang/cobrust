//! PyO3 bindings for cobrust-redis.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 (mirrored for the
//! ecosystem batch by ADR-0022 §6). When compiled with the feature,
//! this module exposes a `redis` Python extension whose public surface
//! is a `connect(url) -> Client` function plus a `Client` class with the
//! four Phase-A KV verbs `set / get / delete / exists` (ADR-0078
//! Phase-1c). The Python-side `get` returns `Optional[str]` (None for an
//! absent key) — the §2.2-correct shape the C-ABI first proof defers to
//! a follow-up but PyO3 can express natively.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use pyo3::prelude::*;

use crate::{Client, RedisError};

fn redis_err_to_py(err: RedisError) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(format!("{err}"))
}

#[pyfunction]
fn connect(url: &str) -> PyResult<PyClient> {
    let inner = Client::connect(url).map_err(redis_err_to_py)?;
    Ok(PyClient { inner })
}

#[pyclass(name = "Client")]
struct PyClient {
    inner: Client,
}

#[pymethods]
impl PyClient {
    fn set(&mut self, key: &str, value: &str) -> PyResult<()> {
        self.inner.set(key, value).map_err(redis_err_to_py)
    }

    fn get(&mut self, key: &str) -> PyResult<Option<String>> {
        self.inner.get(key).map_err(redis_err_to_py)
    }

    fn delete(&mut self, key: &str) -> PyResult<i64> {
        self.inner.delete(key).map_err(redis_err_to_py)
    }

    fn exists(&mut self, key: &str) -> PyResult<bool> {
        self.inner.exists(key).map_err(redis_err_to_py)
    }
}

#[pymodule]
fn redis(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(connect, m)?)?;
    m.add_class::<PyClient>()?;
    Ok(())
}
