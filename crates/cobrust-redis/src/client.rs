// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: redis (redis-rs) 1.2.x — SYNC path
// oracle: redis-py canonical KV surface (set/get/delete/exists)
// see PROVENANCE.toml for the full manifest.

//! Translated redis cache/KV body — the `Client` handle, its four
//! Phase-A KV verbs, and the single `RedisError` taxonomy. Per-function
//! provenance lines follow.
//!
//! ADR-0078 §3.5 — redis-rs's `Client::open(url) -> get_connection() ->
//! Commands` IS the synchronous blocking facade (the non-`aio` path), so
//! the Cobrust public surface stays sync with ZERO new runtime code
//! (no `block_on`, no `tokio`), exactly as strike rides
//! `reqwest::blocking`. Constitution §2.2 ("no async / sync coloring")
//! is honoured at the cabi boundary by the crate's own blocking API.
//!
//! Ownership (ADR-0078 §3.7 — the one delta from strike): redis-rs sync
//! command methods take `&mut self` on the `Connection` (the connection
//! is stateful — it writes the request + reads the reply). So the
//! verb methods here take `&mut self` and the `Client` holds the
//! `Connection` by value. A sync `redis_rs::Connection` is `!Send` (single
//! TCP connection, single-threaded use) — this matches den's `!Send`
//! `Connection` constraint (the `.cb` object model is single-threaded
//! for ecosystem handles today, so this is the existing accepted
//! constraint, not a new one).

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

use redis_rs::Commands;

/// Single error type for redis failures. Mirrors the union of the
/// redis-py exception hierarchy (`ConnectionError`, `ResponseError`,
/// `TimeoutError`) collapsed into one Rust enum because `Result<T, E>`
/// is the default error path (constitution §2.2). The `.cb` surface
/// never sees this type — the cabi shims map every `Err` to a
/// fail-clean sentinel — but the Rust surface + the unit tests use it.
#[derive(Clone, Debug)]
pub struct RedisError {
    pub kind: RedisErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedisErrorKind {
    /// The `redis://` URL did not parse / scheme unsupported.
    InvalidUrl,
    /// Could not open a TCP connection to the server (DNS, refused,
    /// unreachable) — the dominant fail-clean path when no redis is
    /// running.
    Connection,
    /// The server returned an error reply, or a command failed.
    Command,
    /// The `Client` is in the disconnected sentinel state (connect
    /// failed); every command short-circuits to this.
    Disconnected,
}

impl std::fmt::Display for RedisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            RedisErrorKind::InvalidUrl => "invalid url",
            RedisErrorKind::Connection => "connection",
            RedisErrorKind::Command => "command",
            RedisErrorKind::Disconnected => "disconnected",
        };
        write!(f, "redis {kind} error: {}", self.message)
    }
}

impl std::error::Error for RedisError {}

impl RedisError {
    pub(crate) fn invalid_url(message: impl Into<String>) -> Self {
        Self {
            kind: RedisErrorKind::InvalidUrl,
            message: message.into(),
        }
    }

    pub(crate) fn disconnected() -> Self {
        Self {
            kind: RedisErrorKind::Disconnected,
            message: "client is not connected to a redis server".to_string(),
        }
    }

    /// Lift a `redis_rs::RedisError` into our taxonomy. Connection-class
    /// errors (the no-server-running path) are distinguished from
    /// server-side command errors for diagnostic clarity; the cabi
    /// sentinel mapping does not depend on the distinction (both → the
    /// fail-clean return), but the Rust surface + tests do.
    pub(crate) fn from_redis(err: &redis_rs::RedisError) -> Self {
        let message = err.to_string();
        if err.is_connection_refusal()
            || err.is_connection_dropped()
            || err.is_io_error()
            || err.is_timeout()
        {
            return Self {
                kind: RedisErrorKind::Connection,
                message,
            };
        }
        Self {
            kind: RedisErrorKind::Command,
            message,
        }
    }
}

// fn:Client::connect provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate

/// A redis cache/KV client — ONE handle type (no
/// connection-vs-pool-vs-client sprawl, ADR-0078 §2 footgun-ledger).
/// Wraps a single sync `redis_rs::Connection`. The `inner` is `None` in
/// the **disconnected sentinel** state (a connect failure yields a
/// `Client` whose every command fails clean rather than a null handle
/// — so the `.cb` source surface never branches on null and the cabi
/// boundary never panics).
///
/// Constitution §5.1: 0 public fields (the connection is projected
/// through the verb methods, never exposed).
pub struct Client {
    inner: Option<redis_rs::Connection>,
}

impl Client {
    /// Open a connection to the redis server named by `url` (a single
    /// canonical `redis://[:password@]host[:port][/db]` URL — the db
    /// index, auth, and TLS all live IN the URL, redis-rs's native
    /// model; no `db=`/`decode_responses=` option-bag sprawl per
    /// ADR-0078 §2).
    ///
    /// # Errors
    /// Returns [`RedisError::invalid_url`] if `url` does not parse, or a
    /// [`RedisErrorKind::Connection`] error if the TCP connection cannot
    /// be established (the dominant path when no redis is running).
    pub fn connect(url: &str) -> Result<Self, RedisError> {
        let client =
            redis_rs::Client::open(url).map_err(|e| RedisError::invalid_url(e.to_string()))?;
        let con = client
            .get_connection()
            .map_err(|e| RedisError::from_redis(&e))?;
        Ok(Self { inner: Some(con) })
    }

    /// Construct a disconnected sentinel `Client` (connect failed). Every
    /// command on it returns the fail-clean sentinel. Used by the cabi
    /// `connect` shim so a connect failure still hands the `.cb` caller a
    /// well-defined handle (never null), and by the unit tests to
    /// exercise the no-server path without a server.
    #[must_use]
    pub fn disconnected() -> Self {
        Self { inner: None }
    }

    /// True when this `Client` holds a live connection (not the
    /// disconnected sentinel).
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.inner.is_some()
    }

    // fn:Client::set provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `SET key value`. Mirrors redis-py `r.set(key, value)`. The
    /// first-proof value type is fixed to `str` (ADR-0078 §2.3-2 — the
    /// un-suffixed name with the str value; a typed `set_int` sibling
    /// is a follow-up). Returns `()` on success (side-effect, no
    /// drop-eligible handle minted — mirrors pit's `app.route` None
    /// discipline).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a
    /// [`RedisErrorKind::Command`] / `Connection` error on a failed SET.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.set::<_, _, ()>(key, value)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::get provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `GET key`. Mirrors redis-py `r.get(key)`. Returns the stored
    /// value, or `None` when the key is absent (so the cabi shim can
    /// render the empty-string sentinel for "absent == empty", ADR-0078
    /// §2.3-1; an `Option<str>`-across-C-ABI surface that distinguishes
    /// a deliberately-stored "" is the §2.2-correct follow-up).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed GET.
    pub fn get(&mut self, key: &str) -> Result<Option<String>, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.get::<_, Option<String>>(key)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::delete provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `DEL key`. Mirrors redis-py `r.delete(key)`. Returns the number
    /// of keys removed (`0` or `1` for a single key) — the readable
    /// Python-idiom verb (`delete`, not redis-rs's `del`), ADR-0078 §2.1.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed DEL.
    pub fn delete(&mut self, key: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.del::<_, i64>(key)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::exists provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `EXISTS key`. Mirrors redis-py `r.exists(key)` (as a bool — `True`
    /// when the key is present). The readable Python-idiom verb.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed EXISTS.
    pub fn exists(&mut self, key: &str) -> Result<bool, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.exists::<_, bool>(key)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::expire provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `EXPIRE key seconds`. Mirrors redis-py `r.expire(key, seconds)`.
    /// Sets a key's time-to-live; returns whether the timeout was set
    /// (`True` when the key exists and the TTL was applied, `False` when
    /// the key does not exist) — the readable Python-idiom verb (ADR-0078
    /// §2.2). redis-rs's `Commands::expire` takes `seconds: i64` and
    /// returns a bool natively.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed EXPIRE.
    pub fn expire(&mut self, key: &str, seconds: i64) -> Result<bool, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.expire::<_, bool>(key, seconds)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::incr provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `INCR key` (atomic counter — increment by 1). Mirrors redis-py
    /// `r.incr(key)`. Returns the value AFTER the increment (the new
    /// value), per the redis `INCR` reply. A non-existent key is treated
    /// as `0` before the operation, so the first `incr` yields `1`. The
    /// stored value must be parseable as an integer (redis enforces this;
    /// a non-integer value surfaces a command error → the fail-clean
    /// sentinel at the cabi boundary).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed INCR (e.g. the value is not an integer).
    pub fn incr(&mut self, key: &str) -> Result<i64, RedisError> {
        self.incr_by(key, 1)
    }

    // fn:Client::incr_by provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `INCRBY key delta` (atomic counter — increment by `delta`). Mirrors
    /// redis-py `r.incrby(key, delta)` / `r.incr(key, delta)`. Returns the
    /// value AFTER the increment. redis-rs's `Commands::incr` routes to
    /// `INCRBY` for an integer delta (and `INCRBYFLOAT` for a float — we
    /// pass an `i64` so it is always the integer path).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed INCRBY.
    pub fn incr_by(&mut self, key: &str, delta: i64) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.incr::<_, _, i64>(key, delta)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::hset provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `HSET key field value` (hash set field). Mirrors redis-py
    /// `r.hset(key, field, value)`. Returns whether a NEW field was
    /// created (`True` when `field` did not previously exist in the hash,
    /// `False` when an existing field's value was overwritten) — the
    /// `HSET` reply is the count of newly-added fields (`0`/`1` for one
    /// field), which we render as the readable bool. The value type is
    /// fixed to `str` for the first proof (the typed-sibling story mirrors
    /// `set`, ADR-0078 §2.3-2).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed HSET.
    pub fn hset(&mut self, key: &str, field: &str, value: &str) -> Result<bool, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.hset::<_, _, _, i64>(key, field, value)
            .map(|added| added > 0)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::hget provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `HGET key field` (hash get field). Mirrors redis-py
    /// `r.hget(key, field)`. Returns the stored field value, or `None`
    /// when the field (or the hash) is absent — so the cabi shim renders
    /// the empty-string sentinel for "absent == empty", exactly mirroring
    /// `get` (ADR-0078 §2.3-1).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed HGET.
    pub fn hget(&mut self, key: &str, field: &str) -> Result<Option<String>, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.hget::<_, _, Option<String>>(key, field)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::lpush provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `LPUSH key value` (list — prepend at the head). Mirrors redis-py
    /// `r.lpush(key, value)`. Returns the list's NEW length after the push
    /// (the `LPUSH` reply). The value type is fixed to `str` for the first
    /// proof, mirroring `set` (ADR-0078 §2.3-2). redis-rs's
    /// `Commands::lpush` replies with the list length (a `usize`), which we
    /// narrow to the `.cb` `i64` (a redis list length is well under
    /// `i64::MAX`).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed LPUSH (e.g. the key holds a non-list).
    pub fn lpush(&mut self, key: &str, value: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.lpush::<_, _, i64>(key, value)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::rpush provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `RPUSH key value` (list — append at the tail). Mirrors redis-py
    /// `r.rpush(key, value)`. Returns the list's NEW length after the push,
    /// exactly like [`Client::lpush`] but pushing at the opposite end.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed RPUSH.
    pub fn rpush(&mut self, key: &str, value: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.rpush::<_, _, i64>(key, value)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::lpop provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `LPOP key` (list — pop one element from the head). Mirrors redis-py
    /// `r.lpop(key)`. Returns the popped value, or `None` when the list is
    /// empty or the key is absent — so the cabi shim renders the
    /// empty-string sentinel for "absent == empty", exactly mirroring
    /// `get` / `hget` (ADR-0078 §2.3-1). redis-rs's `Commands::lpop` takes a
    /// `count: Option<NonZeroUsize>`; the first proof always passes `None`
    /// (pop exactly one), so the reply is a single bulk value (or nil)
    /// deserialized as `Option<String>` (NOT the multi-element LIST shape
    /// `lrange` would need — that LIST-return is the deferred Phase-C+
    /// follow-up).
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed LPOP.
    pub fn lpop(&mut self, key: &str) -> Result<Option<String>, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.lpop::<_, Option<String>>(key, None)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::rpop provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `RPOP key` (list — pop one element from the tail). Mirrors redis-py
    /// `r.rpop(key)`. Returns the popped value, or `None` when the list is
    /// empty / absent, exactly like [`Client::lpop`] but popping the
    /// opposite end.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed RPOP.
    pub fn rpop(&mut self, key: &str) -> Result<Option<String>, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.rpop::<_, Option<String>>(key, None)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::llen provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `LLEN key` (list length). Mirrors redis-py `r.llen(key)`. Returns the
    /// number of elements in the list, or `0` when the key is absent (redis
    /// treats a missing key as an empty list). redis-rs's `Commands::llen`
    /// replies with a `usize`, narrowed to the `.cb` `i64`.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed LLEN (e.g. the key holds a non-list).
    pub fn llen(&mut self, key: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.llen::<_, i64>(key)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::sadd provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `SADD key member` (set — add a member). Mirrors redis-py
    /// `r.sadd(key, member)`. Returns the number of members ADDED (`1` when
    /// `member` was new to the set, `0` when it was already present) — the
    /// `SADD` reply. The member type is fixed to `str` for the first proof,
    /// mirroring `set`.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed SADD (e.g. the key holds a non-set).
    pub fn sadd(&mut self, key: &str, member: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.sadd::<_, _, i64>(key, member)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::srem provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `SREM key member` (set — remove a member). Mirrors redis-py
    /// `r.srem(key, member)`. Returns the number of members REMOVED (`1`
    /// when `member` was present, `0` when it was absent / the set does not
    /// exist) — the `SREM` reply.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed SREM.
    pub fn srem(&mut self, key: &str, member: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.srem::<_, _, i64>(key, member)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::sismember provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `SISMEMBER key member` (set — membership test). Mirrors redis-py
    /// `r.sismember(key, member)`. Returns `true` when `member` is in the
    /// set, `false` when it is absent (or the set does not exist) — the
    /// readable bool the `SISMEMBER` reply already is. redis-rs's
    /// `Commands::sismember` returns a bool natively.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed SISMEMBER.
    pub fn sismember(&mut self, key: &str, member: &str) -> Result<bool, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.sismember::<_, _, bool>(key, member)
            .map_err(|e| RedisError::from_redis(&e))
    }

    // fn:Client::scard provider=synthetic model=redis-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
    /// `SCARD key` (set cardinality). Mirrors redis-py `r.scard(key)`.
    /// Returns the number of members in the set, or `0` when the key is
    /// absent (redis treats a missing key as an empty set). redis-rs's
    /// `Commands::scard` replies with a `usize`, narrowed to the `.cb`
    /// `i64`.
    ///
    /// # Errors
    /// [`RedisError::disconnected`] on the sentinel client; a command /
    /// connection error on a failed SCARD (e.g. the key holds a non-set).
    pub fn scard(&mut self, key: &str) -> Result<i64, RedisError> {
        let con = self.inner.as_mut().ok_or_else(RedisError::disconnected)?;
        con.scard::<_, i64>(key)
            .map_err(|e| RedisError::from_redis(&e))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_client_is_not_connected() {
        let c = Client::disconnected();
        assert!(!c.is_connected());
    }

    #[test]
    fn disconnected_client_commands_return_disconnected_error() {
        let mut c = Client::disconnected();
        assert_eq!(
            c.set("k", "v").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.get("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.delete("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.exists("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        // Phase-B verbs — same disconnected-sentinel short-circuit.
        assert_eq!(
            c.expire("k", 60).expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.incr("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.incr_by("k", 5).expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.hset("k", "f", "v").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.hget("k", "f").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        // Phase-C verbs (lists + sets) — same disconnected-sentinel
        // short-circuit, every verb errors before any I/O.
        assert_eq!(
            c.lpush("k", "v").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.rpush("k", "v").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.lpop("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.rpop("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.llen("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.sadd("k", "m").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.srem("k", "m").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.sismember("k", "m").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
        assert_eq!(
            c.scard("k").expect_err("must error").kind,
            RedisErrorKind::Disconnected
        );
    }

    #[test]
    fn connect_to_invalid_url_is_invalid_url_kind() {
        // A bare non-URL string is rejected by redis-rs's URL parser
        // before any I/O — surfacing the InvalidUrl path. (`Client` is
        // not `Debug` — `redis_rs::Connection` is not `Debug` — so we match
        // the Result rather than use `expect_err`.)
        match Client::connect("not a redis url") {
            Ok(_) => panic!("invalid URL must error"),
            Err(e) => assert_eq!(e.kind, RedisErrorKind::InvalidUrl),
        }
    }

    #[test]
    fn connect_to_unreachable_port_is_connection_kind() {
        // Port 1 is the canonical "definitely-nothing-listening" port.
        // redis-rs parses the URL fine, then fails at TCP connect →
        // the Connection-class fail-clean path (no panic, a clean Err).
        match Client::connect("redis://127.0.0.1:1/") {
            Ok(_) => panic!("unreachable port must error"),
            Err(e) => assert_eq!(e.kind, RedisErrorKind::Connection),
        }
    }

    #[test]
    fn redis_error_display_carries_kind() {
        let e = RedisError::disconnected();
        let s = format!("{e}");
        assert!(s.contains("disconnected"));
    }
}
