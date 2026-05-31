---
doc_kind: module
module_id: mod:redis
crate: cobrust-redis
last_verified_commit: pending
dependencies: [mod:translator, mod:types, mod:mir, mod:codegen, mod:cli]
---

# Module: redis

## Purpose

Cobrust cache/KV ecosystem module — the redis rebrand (ADR-0078
Phase-1c). Surface-translates the redis-py KV verbs onto the Rust
`redis` crate (redis-rs) **synchronous** path (`Client::open(url) ->
get_connection() -> Commands`), keeping the public API sync (constitution
§2.2: "no async / sync coloring").

The eleventh ecosystem module, the den/strike handle-pattern template
applied verbatim: a single opaque `Client` handle (a
`den.Connection`-shaped stateful resource) plus a free-function
`connect` entrypoint (like `den.connect`). The redis-rs sync path means
**NO async-收编 is needed** (ADR-0078 §3.5) — strictly simpler than a
`block_on` bridge; `tokio` is NOT in the dep tree.

LLM-first (constitution §2.5): the `.cb` surface mirrors the canonical
redis-py priors so an LLM writes it correctly first try
(maximize-overlap-with-training-data: `client.set(k, v)` /
`client.get(k)` / `client.delete(k)` / `client.exists(k)`), and the
error path is fail-clean sentinels — never an exception, never a panic
across the C ABI (constitution §2.2).

## Status

- **ADR-0078 Phase-1c (Phase A) — delivered.** `connect` + the four KV
  verbs translated via the synthetic-LLM pattern (hand-authored to the
  redis-py spec + the den/strike handle template, real-LLM pipeline
  rerun pending — same posture as `mod:strike` / `mod:den`). Backend
  bound to `redis = "1.2"` (`default-features = false`, sync path only).
  The always-on, server-LESS fail-clean e2e
  (`tests/redis_fail_clean_e2e.rs`) is GREEN in CI; the live round-trip
  (`tests/redis_live_e2e.rs`) self-skips when no server is reachable.

## The `.cb` surface (Phase A — the v0.7.0 MUST-ship)

```text
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")   # -> Client handle
    client.set("greeting", "hello")                     # -> () (side effect)
    let v: str = client.get("greeting")                 # -> str ("" if absent)
    let n: i64 = client.delete("greeting")              # -> i64 (# keys removed)
    let present: bool = client.exists("greeting")       # -> bool
    print(v)
    return 0
```

| `.cb` call | redis-rs sync call (Commands trait) | C-ABI shim | ret |
|---|---|---|---|
| `redis.connect(url)` | `Client::open(url)?.get_connection()?` | `__cobrust_redis_connect(url) -> *mut u8` | `Client` |
| `client.set(k, v)` | `con.set::<_,_,()>(k, v)` | `__cobrust_redis_client_set(c, k, v) -> ()` | `None` |
| `client.get(k)` | `con.get::<_, Option<String>>(k)` | `__cobrust_redis_client_get(c, k) -> *mut u8` | `str` |
| `client.delete(k)` | `con.del::<_, i64>(k)` | `__cobrust_redis_client_delete(c, k) -> i64` | `i64` |
| `client.exists(k)` | `con.exists::<_, bool>(k)` | `__cobrust_redis_client_exists(c, k) -> bool` | `bool` |

The `.cb` names are the readable redis-py-idiom verbs
(`delete`/`exists`, not redis-rs's `del`/`exists`), §2.5-aligned. `set`
returns `None` (side effect — no second drop-eligible handle minted,
mirrors pit `app.route`). `get` returns the str value, with `""` for an
absent key ("absent == empty", ADR-0078 §2.3-1).

## Rust public surface

```rust
pub struct Client { /* private: Option<redis::Connection> (None = disconnected sentinel) */ }

impl Client {
    pub fn connect(url: &str) -> Result<Self, RedisError>;
    pub fn disconnected() -> Self;          // the fail-clean sentinel constructor
    pub fn is_connected(&self) -> bool;
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), RedisError>;
    pub fn get(&mut self, key: &str) -> Result<Option<String>, RedisError>;
    pub fn delete(&mut self, key: &str) -> Result<i64, RedisError>;
    pub fn exists(&mut self, key: &str) -> Result<bool, RedisError>;
}

#[derive(Clone, Debug)]
pub struct RedisError { pub kind: RedisErrorKind, pub message: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedisErrorKind { InvalidUrl, Connection, Command, Disconnected }
```

## C-ABI shims (`src/cabi.rs`)

```
__cobrust_redis_connect(url: *mut Str) -> *mut Client     // disconnected sentinel on failure (NEVER null)
__cobrust_redis_client_set(c, key, value: *mut Str)       // &mut borrow; silent no-op on error
__cobrust_redis_client_get(c, key: *mut Str) -> *mut Str  // &mut borrow; "" sentinel on absent/error
__cobrust_redis_client_delete(c, key: *mut Str) -> i64    // &mut borrow; 0 on error
__cobrust_redis_client_exists(c, key: *mut Str) -> bool   // &mut borrow; false on error
__cobrust_redis_client_drop(c)                            // Box::from_raw + DROP_COUNT, idempotent on null
```

- **Handle**: `Client` crosses as opaque `*mut u8`, `Box::into_raw`'d by
  `connect`, `Box::from_raw`'d once at scope-exit (`_drop`). Dropping
  closes the TCP connection (RAII — no forgot-to-close footgun).
- **`&mut` receiver** (the one delta from strike, ADR-0078 §3.7): redis
  sync command methods take `&mut self`; the shims cast
  `&mut *c.cast::<Client>()`. The `&mut` is entirely inside the shim,
  invisible to the `.cb` aliasing model (each call is a separate
  borrow-then-release, exactly like two sequential `conn.execute` calls).
- **Strings** cross as Cobrust `Str` buffers; `__cobrust_str_*` are
  declared `extern "C"` and resolved from `libcobrust_stdlib.a` at the
  `cobrust build` link step (ADR-0072 Q5 — no Rust-level stdlib dep).
- **`DROP_COUNT`** proves each handle drops exactly once (no leak, no
  double-free).

## The 5-layer wiring (each anchored to a named function)

| Layer | File | Site | Edit |
|---|---|---|---|
| Module registry | `cobrust-types/src/ecosystem.rs` | `is_ecosystem_module` | add `"redis"` to the alternation |
| Free-fn manifest | same | `lookup_module_fn` | `("redis","connect") -> __cobrust_redis_connect : (Str) -> Client` |
| Handle-method manifest | same | `lookup_handle_method` | `(REDIS_CLIENT_ADT, "set"/"get"/"delete"/"exists")` rows |
| Drop symbol | same | `handle_drop_symbol` | `REDIS_CLIENT_ADT => Some("__cobrust_redis_client_drop")` |
| ADT block | same | `REDIS_CLIENT_ADT` | `ECO_ADT_BASE + 0x800` (the NINTH 256-slot block, next-free past coil's `0x700`) |
| Codegen externs | `cobrust-codegen/src/llvm_backend.rs` | `declare_runtime_helpers` | extern decls for the six `__cobrust_redis_*` symbols |
| Link recognizer | `cobrust-cli/src/build/intrinsics.rs` | `ecosystem_module_for_symbol` | `sym.starts_with("__cobrust_redis_") => Some("redis")` |
| MIR | `cobrust-mir/src/lower.rs` | `try_lower_ecosystem_call` | **no edit** — generic (consults the manifest by name) |
| Archive locate | `cobrust-cli/src/build.rs` | `locate_ecosystem_archive` | **no edit** — module-name-generic (`lib{module}.a` + `cargo build -p cobrust-{module}`) |

`[lib] name = "redis"` → produces `libredis.a` (what `locate_ecosystem_archive` keys on, mirrors `strike` → `libstrike.a`). `collect_ecosystem_modules` links `libredis.a` ONLY when a program `import redis` (link-bloat guard).

## Elegant-ecosystem footgun-ledger (CLAUDE.md elegant-ecosystem law)

The `.cb` surface deliberately drops every footgun a redis client
usually carries:

- **No stringly-typed `execute("SET k v")`** — typed methods only
  (`client.set(k, v)`); no raw-command escape hatch (injection /
  arg-quoting footgun). Internally the shim uses the `Commands` trait;
  the `.cb` side never sees a command string.
- **No connection-vs-pool-vs-client sprawl** (redis-py's
  `Redis()`/`ConnectionPool()`/`StrictRedis()`) — ONE handle, `Client`.
- **No exceptions-as-control-flow** — a missing key is `get` returning
  `""`, a connection error is a fail-clean sentinel return, NOT a raised
  exception, NOT a panic across the C ABI. `Result`-shaped (§2.2).
- **No implicit reconnect/retry magic** — the first proof is explicit;
  `ConnectionManager`-style auto-reconnect is a deferred opt-in.
- **No `db=`/`decode_responses=`/`socket_timeout=` option-bag sprawl** —
  `connect(url)` takes a single canonical `redis://` URL (db index,
  auth, TLS all live IN the URL, redis-rs's native model).

## §2.5 compliance (the audit checklist)

- **compile-time-catch-errors**: the manifest gives `connect`/`set`/
  `get`/`delete`/`exists` concrete typed signatures; a wrong-arity or
  wrong-type call is a typecheck error, not a runtime surprise. The
  `RedisErrorKind` Rust enum is closed + exhaustively matchable.
- **maximize-overlap-with-training-data**: the verbs ARE the redis-py
  surface (`r.set`/`r.get`/`r.delete`/`r.exists`); the un-suffixed names
  have strictly higher training-data overlap than a `set_str`/`get_str`
  type-suffix (the suffix only returns IF a `get_int`/`get_bytes`
  sibling ships — Phase C).

## License / provenance

- The wrapper crate is dual-licensed `Apache-2.0 OR MIT` (every Cobrust
  crate). The wrapped dependency **redis-rs is BSD-3-Clause** — the
  FIRST non-(Apache/MIT) crate in the wrap-a-crate set (ADR-0078 §5 R1).
  Permissive + non-copyleft → distribution-compatible with the dual
  workspace license.
- The BSD-3 attribution clause is satisfied by `crates/cobrust-redis/NOTICE`
  (the copyright notice + license terms reproduced) and recorded in
  `PROVENANCE.toml` (`dependency_license = "BSD-3-Clause"`).

## Tests

- `src/client.rs` `#[cfg(test)]`: disconnected-sentinel command behaviour,
  invalid-URL vs unreachable-port error-kind classification, Display.
- `src/cabi.rs` `#[cfg(test)]`: the full fail-clean vertical slice
  (server-less) returns per-type sentinels + drops exactly once;
  null-pointer tolerance; invalid-URL non-null sentinel.
- `crates/cobrust-cli/tests/redis_fail_clean_e2e.rs` (ALWAYS-ON):
  `.cb` source → compile → link → run against an unreachable redis →
  prints the empty-str / `0` / `False` sentinels. Proves the chain + the
  no-panic-at-C-ABI guarantee with NO server. GREEN in CI.
- `crates/cobrust-cli/tests/redis_live_e2e.rs` (SELF-SKIP): the live
  `set`→`get`→`delete`→`exists` round-trip; runs when `$REDIS_URL` /
  `127.0.0.1:6379` is reachable, self-skips (clean `return` + diagnostic)
  otherwise. Mirrors the cross-target / python-version self-skip pattern.

## Ownership / lifecycle (ADR-0078 §3.7)

- **`!Send` like den.** A sync `redis::Connection` is `!Send`/`!Sync`
  (single TCP connection, single-threaded use). This matches den's
  `!Send` `Connection` constraint (ADR-0072 §5 R2). The `.cb` object
  model is single-threaded for ecosystem handles today, so this is the
  existing accepted constraint, not a new one.
- **Connection lifetime = handle lifetime.** Opens at `redis.connect`,
  closes when the `Client` drops at `.cb` scope exit (TCP FIN on
  `Box::from_raw` drop). No explicit `.close()` needed (RAII default).

## Non-goals (deferred follow-ups — ADR-0078 Phase B / C)

- Phase B verbs: `set_expiry`/`expire`/`incr`/`incr_by`/`hset`/`hget`
  (additive manifest rows + cabi shims, same handle, no new mechanism).
- `Option<str>` return for `get` (distinguishes absent from stored-`""`
  — the first `.cb` `Option`-across-C-ABI design; §2.2-correct upgrade).
- Typed `get_int`/`get_bytes` siblings (then the `_str` suffix returns).
- Connection **pooling** (`r2d2`/`deadpool-redis`) — needed only for a
  multi-threaded `.cb` server (pit) sharing one redis; the `!Send`
  connection forces a pool there (ADR-0078 §5 R3).
- `rediss://` **TLS** (`tls-rustls` feature) — for non-loopback redis.
- Raw-`cmd` escape hatch — deliberately NOT shipped (footgun).
- `.cb`-source `import redis` intrinsic/extern wiring is delivered (the
  5-layer wiring above); a real-LLM translation rerun is the open item.
