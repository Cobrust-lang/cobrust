//! PyO3 bindings for cobrust-redis.
//!
//! Gated by `--features pyo3` per ADR-0011 §3 (mirrored for the
//! ecosystem batch by ADR-0022 §6). When compiled with the feature,
//! this module exposes a `redis` Python extension whose public surface
//! is a `connect(url) -> Client` function plus a `Client` class with the
//! KV verbs `set / get / delete / exists` (Phase-A), the cache/counter/
//! hash verbs `expire / incr / incr_by / hset / hget` (Phase-B), and the
//! list/set verbs `lpush / rpush / lpop / rpop / llen / sadd / srem /
//! sismember / scard` (Phase-C), plus the multi-element LIST-of-str
//! returns `lrange / smembers / hkeys / hgetall` (Phase-1d), kept in
//! lock-step with the `.cb` C-ABI surface (ADR-0078 Phase-1c/1d). The
//! Python-side `get` / `hget` / `lpop` / `rpop` return `Optional[str]`
//! (None for an absent key/field / an empty list) — the §2.2-correct
//! shape the C-ABI first proof renders as the empty-string sentinel but
//! PyO3 can express natively. The Phase-1d verbs return a Python
//! `list[str]` (`List<String>`); `hgetall` returns a FLAT
//! `[field, value, field, value, ...]` list — the documented Semantic
//! divergence from Python's dict (mirroring `coil.shape`'s list-vs-tuple
//! divergence note).

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

    // Phase-C — list verbs.
    fn lpush(&mut self, key: &str, value: &str) -> PyResult<i64> {
        self.inner.lpush(key, value).map_err(redis_err_to_py)
    }

    fn rpush(&mut self, key: &str, value: &str) -> PyResult<i64> {
        self.inner.rpush(key, value).map_err(redis_err_to_py)
    }

    // `lpop`/`rpop` return `Optional[str]` (None for an empty/absent
    // list) — the §2.2-correct shape PyO3 expresses natively, mirroring
    // `get`/`hget`.
    fn lpop(&mut self, key: &str) -> PyResult<Option<String>> {
        self.inner.lpop(key).map_err(redis_err_to_py)
    }

    fn rpop(&mut self, key: &str) -> PyResult<Option<String>> {
        self.inner.rpop(key).map_err(redis_err_to_py)
    }

    fn llen(&mut self, key: &str) -> PyResult<i64> {
        self.inner.llen(key).map_err(redis_err_to_py)
    }

    // Phase-C — set verbs.
    fn sadd(&mut self, key: &str, member: &str) -> PyResult<i64> {
        self.inner.sadd(key, member).map_err(redis_err_to_py)
    }

    fn srem(&mut self, key: &str, member: &str) -> PyResult<i64> {
        self.inner.srem(key, member).map_err(redis_err_to_py)
    }

    fn sismember(&mut self, key: &str, member: &str) -> PyResult<bool> {
        self.inner.sismember(key, member).map_err(redis_err_to_py)
    }

    fn scard(&mut self, key: &str) -> PyResult<i64> {
        self.inner.scard(key).map_err(redis_err_to_py)
    }

    // Phase-1d — LIST-of-str returns (Python `list[str]`). Kept in
    // lock-step with the `.cb` C-ABI surface (the review lesson). An
    // absent key / disconnected sentinel yields the empty list.
    fn lrange(&mut self, key: &str, start: i64, stop: i64) -> PyResult<Vec<String>> {
        self.inner.lrange(key, start, stop).map_err(redis_err_to_py)
    }

    fn smembers(&mut self, key: &str) -> PyResult<Vec<String>> {
        self.inner.smembers(key).map_err(redis_err_to_py)
    }

    fn hkeys(&mut self, key: &str) -> PyResult<Vec<String>> {
        self.inner.hkeys(key).map_err(redis_err_to_py)
    }

    // `hgetall` returns a FLAT [field, value, field, value, ...] list —
    // the documented Semantic divergence from Python's dict (mirroring
    // `coil.shape`'s list-vs-tuple divergence note).
    fn hgetall(&mut self, key: &str) -> PyResult<Vec<String>> {
        self.inner.hgetall(key).map_err(redis_err_to_py)
    }
}

#[pymodule]
fn redis(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(connect, m)?)?;
    m.add_class::<PyClient>()?;
    Ok(())
}
