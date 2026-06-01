// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: redis (redis-rs) 1.2.x — SYNC path
// oracle: redis-py canonical KV surface (set/get/delete/exists)
// see PROVENANCE.toml for the full manifest.

//! Cobrust cache/KV ecosystem module — the redis rebrand (ADR-0078
//! Phase-1c). Surface-translates the redis-py KV verbs onto Rust's
//! `redis` crate (redis-rs) **synchronous** path (`Client::open(url) ->
//! get_connection() -> Commands`), keeping the public API sync
//! (constitution §2.2: "no async / sync coloring").
//!
//! This is the eleventh ecosystem module, the den/strike handle-pattern
//! template applied verbatim: a single opaque `Client` handle (a
//! `den.Connection`-shaped stateful resource) plus a free-function
//! `connect` entrypoint (like `den.connect`). The redis-rs sync path
//! means NO async-收编 is needed (ADR-0078 §3.5) — strictly simpler than
//! a `block_on` bridge.
//!
//! Public surface (Phase A — the v0.7.0 MUST-ship):
//! - `redis.connect(url) -> Client` — open a `redis://` connection.
//! - `client.set(key, value)`       — `SET` (str value, first proof).
//! - `client.get(key) -> str`       — `GET` ("" sentinel if absent).
//! - `client.delete(key) -> i64`    — `DEL` (count removed).
//! - `client.exists(key) -> bool`   — `EXISTS`.
//!
//! Phase-B adds `expire`/`incr`/`incr_by`/`hset`/`hget`; Phase-C adds the
//! list/set verbs `lpush`/`rpush`/`lpop`/`rpop`/`llen` +
//! `sadd`/`srem`/`sismember`/`scard` (all scalar/str returns; the
//! multi-element LIST-of-str returns lrange/smembers/hgetall/hkeys are a
//! deferred follow-up).
//!
//! Error path (constitution §2.2 — no exceptions-as-control-flow): a
//! connect failure yields a *disconnected sentinel* `Client` (never a
//! null handle, never a panic across the C ABI); a missing key reads as
//! the empty-string sentinel. The `RedisError { kind, message }` Rust
//! taxonomy is the in-Rust error surface (the cabi shims map every
//! `Err` to a fail-clean sentinel).

pub mod cabi;
mod client;

pub use crate::client::{Client, RedisError, RedisErrorKind};

#[cfg(feature = "pyo3")]
mod pyo3_bindings;

#[cfg(feature = "pyo3")]
pub use pyo3_bindings::*;
