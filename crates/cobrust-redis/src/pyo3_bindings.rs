//! PyO3 bindings for cobrust-redis.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 (mirrored for the
//! ecosystem batch by ADR-0022 §6). When compiled with the feature,
//! this module exposes a `redis` Python extension whose public surface
//! is a `connect(url) -> Client` function plus a `Client` class with the
//! nine KV verbs `set / get / delete / exists` (Phase-A) and
//! `expire / incr / incr_by / hset / hget` (Phase-B), kept in lock-step
//! with the `.cb` C-ABI surface (ADR-0078 Phase-1c). The Python-side `get`
//! and `hget` return `Optional[str]` (None for an absent key/field) — the
//! §2.2-correct shape the C-ABI first proof defers to a follow-up but PyO3
//! can express natively.

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

    fn expire(&mut self, key: &str, seconds: i64) -> PyResult<bool> {
        self.inner.expire(key, seconds).map_err(redis_err_to_py)
    }

    fn incr(&mut self, key: &str) -> PyResult<i64> {
        self.inner.incr(key).map_err(redis_err_to_py)
    }

    fn incr_by(&mut self, key: &str, delta: i64) -> PyResult<i64> {
        self.inner.incr_by(key, delta).map_err(redis_err_to_py)
    }

    fn hset(&mut self, key: &str, field: &str, value: &str) -> PyResult<bool> {
        self.inner.hset(key, field, value).map_err(redis_err_to_py)
    }

    fn hget(&mut self, key: &str, field: &str) -> PyResult<Option<String>> {
        self.inner.hget(key, field).map_err(redis_err_to_py)
    }
}

#[pymodule]
fn redis(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(connect, m)?)?;
    m.add_class::<PyClient>()?;
    Ok(())
}
