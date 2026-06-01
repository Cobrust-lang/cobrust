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
- **ADR-0078 Phase-1c (Phase B) — delivered.** The top cache/counter/hash
  verbs after the KV core: `expire` (TTL), `incr` / `incr_by` (atomic
  counter), `hset` / `hget` (hash field). Additive manifest rows + cabi
  shims, same `Client` handle, same borrow-receiver + fail-clean
  discipline — no new mechanism over Phase A (the 3-arg `hset` rides the
  arity-generic MIR Case-2 lowering the 2-arg `set` already proved). The
  Phase-B fail-clean error paths are exercised by an always-on server-LESS
  e2e; the Phase-B live round-trips (counter / expire+exists / hash)
  self-skip when no server is reachable.
- **ADR-0078 Phase-1c (Phase C) — delivered.** The LIST + SET verbs:
  `lpush` / `rpush` / `lpop` / `rpop` / `llen` (lists) and `sadd` / `srem`
  / `sismember` / `scard` (sets). ALL scalar/str returns (the
  get/hget/incr shapes) — `lpush`/`rpush`/`llen`/`sadd`/`srem`/`scard`
  return an i64 count/length; `lpop`/`rpop` return the popped str ("" if
  the list is empty/absent, mirroring `get`); `sismember` returns a bool.
  Additive manifest rows + cabi shims (the `lpush`/`rpush`/`sadd`/`srem`
  3-ptr→i64 shape + the `sismember` 3-ptr→bool shape are the two new
  codegen fn-types; `lpop`/`rpop` reuse `get`'s ptr→ptr, `llen`/`scard`
  reuse `delete`'s ptr→i64), same `Client` handle, same borrow-receiver +
  fail-clean discipline — no new mechanism over Phase A/B. The Phase-C
  fail-clean error paths are exercised by an always-on server-LESS e2e;
  the Phase-C live round-trips (list / set) self-skip when no server is
  reachable.
- **ADR-0078 Phase-1c (Phase 1d) — delivered.** The multi-element
  LIST-of-str returns: `lrange` / `smembers` / `hkeys` / `hgetall`. Each
  returns the first-class `Ty::List(Box::new(Ty::Str))` (`list[str]`). A
  read-only design pass CONFIRMED this is a BOUNDED follow-the-pattern
  batch, NOT a language increment: `Ty::List(Str)` + the C-ABI list
  machinery (`__cobrust_list_new`/`_set`/`_get`/`_len`/`_drop_elems`) +
  the `.cb` for-loop / index / `Ty::List(Str)` drop schedule were ALL
  already shipping. The cabi shims mint the list with the SAME recipe
  `__cobrust_llm_stream` (cobrust-stdlib/src/llm.rs:466-489) and
  `__cobrust_coil_buffer_shape` (cobrust-coil/src/cabi.rs:1715-1734) use
  for their list returns — `__cobrust_list_new(8, len)` + a per-element
  `Str` buffer + `__cobrust_list_set`; the `.cb` scope owns + drops the
  returned list once (codegen selects `__cobrust_list_drop_elems(list,
  __cobrust_str_drop)` from `Ty::List(Str)`), so the shim does NOT free
  it. Codegen derives the extern fn-type + return generically from
  `EcoSig.ret` (a `Ty::List` return maps to an LLVM ptr return — NO new
  codegen fn-type; only `lrange`'s extra `(start, stop)` i64 pair needs a
  4-arg type, the other three reuse `get`'s `(ptr,ptr)->ptr`). The stale
  Phase-C "redis has no list-handle precedent" deferral note (which lived
  in cabi.rs:13-17 + the Non-goals below) is CORRECTED. `hgetall` returns
  a FLAT `[k, v, k, v, ...]` `list[str]` — a documented Semantic
  divergence from Python's dict, mirroring `coil.shape`'s list-vs-tuple
  divergence note. The Phase-1d fail-clean error paths (each verb mints an
  EMPTY `list[str]` on the disconnected sentinel) are exercised by an
  always-on server-LESS e2e (including a `for x in xs` loop over the
  returned list); the Phase-1d live round-trips (`lrange` → values +
  iterate + index, `hgetall` → flat pairs) self-skip when no server is
  reachable.

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

## The `.cb` surface (Phase B — cache / counter / hash)

```text
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")
    client.set("counter", "10")
    let n: i64 = client.incr("counter")            # -> 11 (new value)
    let m: i64 = client.incr_by("counter", 5)      # -> 16
    let ttl_set: bool = client.expire("counter", 60)   # -> True (TTL applied)
    let created: bool = client.hset("h", "field", "v") # -> True (new field)
    let v: str = client.hget("h", "field")             # -> "v" ("" if absent)
    print(v)
    return 0
```

| `.cb` call | redis-rs sync call (Commands trait) | C-ABI shim | ret |
|---|---|---|---|
| `client.expire(k, secs)` | `con.expire::<_, bool>(k, secs)` | `__cobrust_redis_client_expire(c, k, secs) -> bool` | `bool` |
| `client.incr(k)` | `con.incr::<_,_,i64>(k, 1)` | `__cobrust_redis_client_incr(c, k) -> i64` | `i64` |
| `client.incr_by(k, n)` | `con.incr::<_,_,i64>(k, n)` | `__cobrust_redis_client_incr_by(c, k, n) -> i64` | `i64` |
| `client.hset(k, f, v)` | `con.hset::<_,_,_,i64>(k, f, v)` | `__cobrust_redis_client_hset(c, k, f, v) -> bool` | `bool` |
| `client.hget(k, f)` | `con.hget::<_,_,Option<String>>(k, f)` | `__cobrust_redis_client_hget(c, k, f) -> *mut u8` | `str` |

`incr` / `incr_by` return the value AFTER the increment (the atomic-counter
new value; a fresh key starts at `0`, so the first `incr` yields `1`).
`expire` returns whether the TTL was set (`True` when the key exists).
`hset` returns whether a NEW field was created (the `HSET` reply count
rendered as a bool — `True` for a new field, `False` for an overwrite).
`hget` mirrors `get`: the str value, `""` for an absent field/hash.

## The `.cb` surface (Phase C — lists + sets)

```text
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")
    # Lists (a deque — push/pop both ends).
    let n1: i64 = client.lpush("tasks", "a")     # -> 1 (new length; prepend head)
    let n2: i64 = client.rpush("tasks", "b")     # -> 2 (append tail)
    let len: i64 = client.llen("tasks")          # -> 2
    let head: str = client.lpop("tasks")         # -> "a" ("" if empty/absent)
    let tail: str = client.rpop("tasks")         # -> "b"
    # Sets (unique members).
    let added: i64 = client.sadd("tags", "x")    # -> 1 (0 if already present)
    let present: bool = client.sismember("tags", "x")  # -> True
    let card: i64 = client.scard("tags")         # -> 1 (cardinality, 0 if absent)
    let removed: i64 = client.srem("tags", "x")  # -> 1 (0 if absent)
    print(head)
    return 0
```

| `.cb` call | redis-rs sync call (Commands trait) | C-ABI shim | ret |
|---|---|---|---|
| `client.lpush(k, v)` | `con.lpush::<_,_,i64>(k, v)` | `__cobrust_redis_client_lpush(c, k, v) -> i64` | `i64` |
| `client.rpush(k, v)` | `con.rpush::<_,_,i64>(k, v)` | `__cobrust_redis_client_rpush(c, k, v) -> i64` | `i64` |
| `client.lpop(k)` | `con.lpop::<_,Option<String>>(k, None)` | `__cobrust_redis_client_lpop(c, k) -> *mut u8` | `str` |
| `client.rpop(k)` | `con.rpop::<_,Option<String>>(k, None)` | `__cobrust_redis_client_rpop(c, k) -> *mut u8` | `str` |
| `client.llen(k)` | `con.llen::<_,i64>(k)` | `__cobrust_redis_client_llen(c, k) -> i64` | `i64` |
| `client.sadd(k, m)` | `con.sadd::<_,_,i64>(k, m)` | `__cobrust_redis_client_sadd(c, k, m) -> i64` | `i64` |
| `client.srem(k, m)` | `con.srem::<_,_,i64>(k, m)` | `__cobrust_redis_client_srem(c, k, m) -> i64` | `i64` |
| `client.sismember(k, m)` | `con.sismember::<_,_,bool>(k, m)` | `__cobrust_redis_client_sismember(c, k, m) -> bool` | `bool` |
| `client.scard(k)` | `con.scard::<_,i64>(k)` | `__cobrust_redis_client_scard(c, k) -> i64` | `i64` |

`lpush` prepends (head), `rpush` appends (tail); both return the list's
NEW length. `lpop`/`rpop` pop one element from the head/tail and return
it as a str, mirroring `get` (`""` for an empty or absent list — NOT the
multi-element list `lrange` returns; that LIST-of-str return is Phase-1d
below). `llen` is the list length (`0` if absent). `sadd`
returns the number ADDED (`1` if new, `0` if already present); `srem` the
number removed; `sismember` membership as a bool; `scard` the set
cardinality (`0` if absent).

## The `.cb` surface (Phase 1d — LIST-of-str returns)

```text
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")
    client.rpush("tasks", "a")
    client.rpush("tasks", "b")
    let xs: list[str] = client.lrange("tasks", 0, -1)   # -> ["a", "b"] (0,-1 = whole list)
    print(xs.len())                                      # -> 2
    for t in xs:                                          # for-loop over the returned list
        print(t)                                         # -> a / b
    let m: list[str] = client.smembers("tags")           # -> all set members
    let hk: list[str] = client.hkeys("user:1")           # -> all hash field names
    let hga: list[str] = client.hgetall("user:1")        # -> FLAT [field, value, ...]
    return 0
```

| `.cb` call | redis-rs sync call (Commands trait) | C-ABI shim | ret |
|---|---|---|---|
| `client.lrange(k, start, stop)` | `con.lrange::<_,Vec<String>>(k, start, stop)` | `__cobrust_redis_client_lrange(c, k, start, stop) -> *mut List` | `list[str]` |
| `client.smembers(k)` | `con.smembers::<_,Vec<String>>(k)` | `__cobrust_redis_client_smembers(c, k) -> *mut List` | `list[str]` |
| `client.hkeys(k)` | `con.hkeys::<_,Vec<String>>(k)` | `__cobrust_redis_client_hkeys(c, k) -> *mut List` | `list[str]` |
| `client.hgetall(k)` | `con.hgetall::<_,Vec<String>>(k)` | `__cobrust_redis_client_hgetall(c, k) -> *mut List` | `list[str]` (FLAT) |

Each returns the first-class `Ty::List(Box::new(Ty::Str))`. `lrange`
takes the redis-native `(start, stop)` index pair (both inclusive,
negative = tail-relative; `0, -1` is the whole list). An absent key /
disconnected sentinel / command error mints an EMPTY `list[str]` (the
list analogue of `get`'s `""` sentinel — never null, never a panic). The
`.cb` scope owns + drops the returned list once (the `Ty::List(Str)` drop
schedule → `__cobrust_list_drop_elems(list, __cobrust_str_drop)`); the
shim does NOT free it. The `.cb` consumes it with the EXISTING for-loop /
index / len machinery (NO new code).

**`hgetall` flat-list divergence**: Python's `redis` returns `hgetall` as
a `dict`; Cobrust returns a FLAT `list[str]` `[field, value, field,
value, ...]`. This is a documented Semantic divergence mirroring
`coil.shape`'s list-vs-tuple divergence (numpy tuple → coil `list[i64]`):
the flat list is the §2.5-closest honest shape the already-shipping
`Ty::List(Str)` machinery supports without a new `Dict`-across-C-ABI
return shape. A `dict`-returning `hgetall` is a tracked follow-up.

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
    // Phase B:
    pub fn expire(&mut self, key: &str, seconds: i64) -> Result<bool, RedisError>;
    pub fn incr(&mut self, key: &str) -> Result<i64, RedisError>;          // +1
    pub fn incr_by(&mut self, key: &str, delta: i64) -> Result<i64, RedisError>;
    pub fn hset(&mut self, key: &str, field: &str, value: &str) -> Result<bool, RedisError>;
    pub fn hget(&mut self, key: &str, field: &str) -> Result<Option<String>, RedisError>;
    // Phase C — lists:
    pub fn lpush(&mut self, key: &str, value: &str) -> Result<i64, RedisError>;   // new length
    pub fn rpush(&mut self, key: &str, value: &str) -> Result<i64, RedisError>;
    pub fn lpop(&mut self, key: &str) -> Result<Option<String>, RedisError>;      // None if empty/absent
    pub fn rpop(&mut self, key: &str) -> Result<Option<String>, RedisError>;
    pub fn llen(&mut self, key: &str) -> Result<i64, RedisError>;
    // Phase C — sets:
    pub fn sadd(&mut self, key: &str, member: &str) -> Result<i64, RedisError>;   // # added
    pub fn srem(&mut self, key: &str, member: &str) -> Result<i64, RedisError>;   // # removed
    pub fn sismember(&mut self, key: &str, member: &str) -> Result<bool, RedisError>;
    pub fn scard(&mut self, key: &str) -> Result<i64, RedisError>;               // cardinality
    // Phase 1d — LIST-of-str returns (empty Vec on absent key):
    pub fn lrange(&mut self, key: &str, start: i64, stop: i64) -> Result<Vec<String>, RedisError>;
    pub fn smembers(&mut self, key: &str) -> Result<Vec<String>, RedisError>;
    pub fn hkeys(&mut self, key: &str) -> Result<Vec<String>, RedisError>;
    pub fn hgetall(&mut self, key: &str) -> Result<Vec<String>, RedisError>;      // FLAT [k,v,k,v,...]
}

#[derive(Clone, Debug)]
pub struct RedisError { pub kind: RedisErrorKind, pub message: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedisErrorKind { InvalidUrl, Connection, Command, Disconnected }
```

## C-ABI shims (`src/cabi.rs`)

```
__cobrust_redis_connect(url: *mut Str) -> *mut Client      // disconnected sentinel on failure (NEVER null)
__cobrust_redis_client_set(c, key, value: *mut Str)        // &mut borrow; silent no-op on error
__cobrust_redis_client_get(c, key: *mut Str) -> *mut Str   // &mut borrow; "" sentinel on absent/error
__cobrust_redis_client_delete(c, key: *mut Str) -> i64     // &mut borrow; 0 on error
__cobrust_redis_client_exists(c, key: *mut Str) -> bool    // &mut borrow; false on error
// Phase B:
__cobrust_redis_client_expire(c, key: *mut Str, secs: i64) -> bool    // &mut borrow; false on error
__cobrust_redis_client_incr(c, key: *mut Str) -> i64                  // &mut borrow; 0 on error
__cobrust_redis_client_incr_by(c, key: *mut Str, delta: i64) -> i64   // &mut borrow; 0 on error
__cobrust_redis_client_hset(c, key, field, value: *mut Str) -> bool   // &mut borrow; false on error
__cobrust_redis_client_hget(c, key, field: *mut Str) -> *mut Str      // &mut borrow; "" sentinel on absent/error
// Phase C (lists + sets):
__cobrust_redis_client_lpush(c, key, value: *mut Str) -> i64          // &mut borrow; 0 on error; new length
__cobrust_redis_client_rpush(c, key, value: *mut Str) -> i64          // &mut borrow; 0 on error; new length
__cobrust_redis_client_lpop(c, key: *mut Str) -> *mut Str             // &mut borrow; "" sentinel on empty/absent/error
__cobrust_redis_client_rpop(c, key: *mut Str) -> *mut Str             // &mut borrow; "" sentinel on empty/absent/error
__cobrust_redis_client_llen(c, key: *mut Str) -> i64                  // &mut borrow; 0 on error
__cobrust_redis_client_sadd(c, key, member: *mut Str) -> i64          // &mut borrow; 0 on error; # added
__cobrust_redis_client_srem(c, key, member: *mut Str) -> i64          // &mut borrow; 0 on error; # removed
__cobrust_redis_client_sismember(c, key, member: *mut Str) -> bool    // &mut borrow; false on error
__cobrust_redis_client_scard(c, key: *mut Str) -> i64                 // &mut borrow; 0 on error; cardinality
// Phase 1d (LIST-of-str returns — mint an owned list[str] the .cb scope drops; EMPTY list on error):
__cobrust_redis_client_lrange(c, key: *mut Str, start, stop: i64) -> *mut List   // &mut borrow; empty list[str] on absent/error
__cobrust_redis_client_smembers(c, key: *mut Str) -> *mut List        // &mut borrow; empty list[str] on absent/error
__cobrust_redis_client_hkeys(c, key: *mut Str) -> *mut List           // &mut borrow; empty list[str] on absent/error
__cobrust_redis_client_hgetall(c, key: *mut Str) -> *mut List         // &mut borrow; FLAT [k,v,...] list[str]; empty on absent/error
__cobrust_redis_client_drop(c)                             // Box::from_raw + DROP_COUNT, idempotent on null
```

- **`list[str]` return** (Phase 1d): the shim mints an owned `List<i64>`
  whose i64 slots store heap-`Str` pointers, via the `__cobrust_list_new`
  / `__cobrust_list_set` externs (declared in `cabi.rs`, resolved from
  `libcobrust_stdlib.a` — the SAME ADR-0072 Q5 cross-crate pattern the Str
  externs use, and the SAME mint recipe `__cobrust_llm_stream` +
  `__cobrust_coil_buffer_shape` use). The `.cb` scope OWNS the returned
  list and drops it once at scope exit (`__cobrust_list_drop_elems(list,
  __cobrust_str_drop)`, codegen-selected from `Ty::List(Str)`); the shim
  does NOT free it. A null handle / disconnected sentinel / command error
  mints an EMPTY list (len 0), NEVER null.

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
| Handle-method manifest | same | `lookup_handle_method` | Phase A `(REDIS_CLIENT_ADT, "set"/"get"/"delete"/"exists")` + Phase B `"expire"/"incr"/"incr_by"/"hset"/"hget"` + Phase C `"lpush"/"rpush"/"lpop"/"rpop"/"llen"/"sadd"/"srem"/"sismember"/"scard"` + Phase 1d `"lrange"/"smembers"/"hkeys"/"hgetall"` (all `Ty::List(Box::new(Ty::Str))`) rows |
| Drop symbol | same | `handle_drop_symbol` | `REDIS_CLIENT_ADT => Some("__cobrust_redis_client_drop")` |
| ADT block | same | `REDIS_CLIENT_ADT` | `ECO_ADT_BASE + 0x800` (the NINTH 256-slot block, next-free past coil's `0x700`) |
| Codegen externs | `cobrust-codegen/src/llvm_backend.rs` | `declare_runtime_helpers` | extern decls for the twenty-four `__cobrust_redis_*` symbols (six Phase-A + five Phase-B + nine Phase-C + four Phase-1d; Phase-C NEW fn-types are the `lpush`/`rpush`/`sadd`/`srem` 3-ptr→i64 + `sismember` 3-ptr→bool; Phase-1d's only NEW fn-type is `lrange`'s `(ptr,ptr,i64,i64)→ptr` — `smembers`/`hkeys`/`hgetall` reuse `get`'s ptr→ptr since a `Ty::List` return is just a ptr; NO new codegen fn-type design, ret driven by `EcoSig.ret`) |
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

- **compile-time-catch-errors**: the manifest gives every verb
  (`connect`/`set`/`get`/`delete`/`exists` + `expire`/`incr`/`incr_by`/
  `hset`/`hget` + the Phase-1d `lrange`/`smembers`/`hkeys`/`hgetall`) a
  concrete typed signature; a wrong-arity or wrong-type call is a
  typecheck error, not a runtime surprise (e.g. `expire(k)` missing the
  `secs: i64`, or `lrange(k)` missing the `start`/`stop` indices, is
  rejected at typecheck). The Phase-1d verbs' `list[str]` return type-
  checks against a `for`/index/`.len()` use, so iterating a non-list by
  mistake is a compile error too. The `RedisErrorKind` Rust enum is closed
  + exhaustively matchable.
- **maximize-overlap-with-training-data**: the verbs ARE the redis-py
  surface (`r.set`/`r.get`/`r.delete`/`r.exists`/`r.expire`/`r.incr`/
  `r.hset`/`r.hget` + the Phase-C `r.lpush`/`r.rpush`/`r.lpop`/`r.rpop`/
  `r.llen`/`r.sadd`/`r.srem`/`r.sismember`/`r.scard` + the Phase-1d
  `r.lrange`/`r.smembers`/`r.hkeys`/`r.hgetall`); the un-suffixed
  names have strictly higher training-data overlap than a
  `set_str`/`get_str` type-suffix (the suffix only returns IF a
  `get_int`/`get_bytes` sibling ships). `incr_by` is the readable spelling
  of redis-py's `r.incr(k, n)` / the `INCRBY` command (the `_by` suffix
  disambiguates the two-arg delta form from the bare one-arg `incr`). The
  ONE deliberate divergence is `hgetall` returning a FLAT `list[str]`
  rather than a `dict` — a documented Semantic-tier divergence (mirroring
  `coil.shape`'s list-vs-tuple note), the §2.5-closest honest shape the
  shipping `Ty::List(Str)` machinery supports today.

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

- `src/client.rs` `#[cfg(test)]`: disconnected-sentinel command behaviour
  (extended to the Phase-1d `lrange`/`smembers`/`hkeys`/`hgetall` →
  `Disconnected`), invalid-URL vs unreachable-port error-kind
  classification, Display.
- `src/cabi.rs` `#[cfg(test)]`: the full fail-clean vertical slice
  (server-less) returns per-type sentinels + drops exactly once — extended
  to the Phase-B verbs (`expire`→false / `incr`,`incr_by`→0 / `hset`→false
  / `hget`→"") and the Phase-C verbs (`lpush`,`rpush`→0 / `lpop`,`rpop`→""
  / `llen`→0 / `sadd`,`srem`→0 / `sismember`→false / `scard`→0); the
  Phase-1d `list[str]`-return verbs in a dedicated test
  (`cabi_phase_1d_str_list_verbs_mint_empty_lists_on_disconnected` — each
  mints an EMPTY `list[str]` len 0, never null, each freed clean via the
  codegen `Ty::List(Str)` drop, handle drops once; split out to stay under
  the 100-line lint ceiling); a `list[str]` mint+drop discipline test
  (`cabi_str_list_mint_roundtrips_and_drops_clean` — mints a NON-empty
  flat `[k,v,k,v]` list via the exact `alloc_str_list` shim helper, reads
  it back order+content-preserved, frees via
  `__cobrust_list_drop_elems(list, __cobrust_str_drop)` — the server-less
  proof of the new return shape + no-leak/double-free); null-pointer
  tolerance (all twenty-four shims; the four Phase-1d shims mint an empty
  list on null); invalid-URL non-null sentinel. The `__cobrust_list_*`
  externs resolve under `cargo test` via `#[used]` static link anchors
  (`_LIST_NEW_LINK_ANCHOR` / `_LIST_SET_LINK_ANCHOR`, mirroring the
  existing `__cobrust_str_new` anchor).
- `crates/cobrust-cli/tests/redis_fail_clean_e2e.rs` (ALWAYS-ON):
  `.cb` source → compile → link → run against an unreachable redis →
  prints the empty-str / `0` / `False` sentinels. Proves the chain + the
  no-panic-at-C-ABI guarantee with NO server. GREEN in CI. A second test
  (`..._phase_b_...`) does the same for `expire`/`incr`/`incr_by`/`hset`/
  `hget` (prints `False`/`0`/`0`/`False`/`""`); a third
  (`..._phase_c_...`) does the same for `lpush`/`rpush`/`lpop`/`rpop`/
  `llen`/`sadd`/`srem`/`sismember`/`scard` (prints
  `0`/`0`/`""`/`""`/`0`/`0`/`0`/`False`/`0`); a fourth
  (`..._phase_1d_...`) is the END-TO-END proof of the `list[str]` RETURN +
  the `.cb` for-loop + `.len()` + the `Ty::List(Str)` DROP: `let xs:
  list[str] = client.lrange("k", 0, -1)` mints an EMPTY list, `xs.len()`
  prints `0`, `for x in xs: print(x)` emits nothing (empty list), and
  `smembers`/`hkeys`/`hgetall` each `.len()`→`0` (prints `0\n0\n0\n0\n`) —
  the four minted lists drop clean (a leak/double-free would crash the
  run). So the Phase-B + Phase-C + Phase-1d paths are genuinely exercised
  server-less.
- `crates/cobrust-cli/tests/redis_live_e2e.rs` (SELF-SKIP): the live
  `set`→`get`→`delete`→`exists` round-trip; a second test does the
  Phase-B live round-trips — counter (`set "10"`→`incr`=11→`incr_by 5`=16),
  expire (`set`→`expire 100`=True→`exists`=True), hash (`hset`=True→`hget`=
  "a"→`hset` overwrite=False→`hget`="b"); a third does the Phase-C live
  round-trips — list (`lpush "a"`=1→`rpush "b"`=2→`llen`=2→`lpop`="a"→
  `rpop`="b"→`llen`=0), set (`sadd "x"`=1→`sadd "x"` dup=0→`sismember`=
  True→`scard`=1→`srem "x"`=1→`sismember`=False); a fourth does the
  Phase-1d NON-EMPTY `list[str]` round-trips — list (`rpush a/b/c` →
  `lrange 0 -1` = `[a,b,c]`, asserting `xs.len()`=3, `xs[0..2]`=a/b/c (the
  index `__cobrust_str_clone` path), and `for s in xs` = a/b/c (the
  loop-var clone path)), hash (`hset f1/f2` → `hgetall` flat list, asserts
  `.len()`=4). All run when `$REDIS_URL` / `127.0.0.1:6379` is reachable,
  self-skip (clean `return` + diagnostic) otherwise. The TTL-expiry timing
  is deliberately NOT slept-through (only the `expire` return + immediate
  `exists` are asserted) to avoid a flaky slow test (ADR-0078 §Phase-B
  heaviest-risk note).

## Ownership / lifecycle (ADR-0078 §3.7)

- **`!Send` like den.** A sync `redis::Connection` is `!Send`/`!Sync`
  (single TCP connection, single-threaded use). This matches den's
  `!Send` `Connection` constraint (ADR-0072 §5 R2). The `.cb` object
  model is single-threaded for ecosystem handles today, so this is the
  existing accepted constraint, not a new one.
- **Connection lifetime = handle lifetime.** Opens at `redis.connect`,
  closes when the `Client` drops at `.cb` scope exit (TCP FIN on
  `Box::from_raw` drop). No explicit `.close()` needed (RAII default).

## Non-goals (deferred follow-ups)

- **`dict`-returning `hgetall`** — Phase 1d ships `hgetall` as a FLAT
  `list[str]` `[field, value, ...]` (the documented Semantic divergence
  mirroring `coil.shape`'s list-vs-tuple note). A `hgetall` that returns a
  true `Dict[str, str]` is a tracked follow-up — it needs the first
  `Dict`-across-the-C-ABI return shape (mint + populate a `__cobrust_dict_*`
  handle the `.cb` scope drops via the existing `Ty::Dict` drop schedule),
  the dict analogue of the `Ty::List(Str)` return Phase 1d just landed.
  (Phase 1d itself is NOT deferred — the LIST-of-str returns `lrange` /
  `smembers` / `hkeys` / `hgetall` are SHIPPED; the prior "redis has no
  list-handle precedent" deferral note was stale and is corrected.)
- `set_expiry(k, v, secs)` (the `SETEX` set-with-TTL one-shot — an
  additive manifest row + cabi shim, same pattern as `expire`; not in the
  Phase-B five but a trivial follow-up if a use-case wants the atomic
  set-and-expire).
- `Option<str>` return for `get` / `hget` (distinguishes absent from
  stored-`""` — the first `.cb` `Option`-across-C-ABI design;
  §2.2-correct upgrade).
- Typed `get_int`/`get_bytes` siblings (then the `_str` suffix returns).
- Connection **pooling** (`r2d2`/`deadpool-redis`) — needed only for a
  multi-threaded `.cb` server (pit) sharing one redis; the `!Send`
  connection forces a pool there (ADR-0078 §5 R3).
- `rediss://` **TLS** (`tls-rustls` feature) — for non-loopback redis.
- Raw-`cmd` escape hatch — deliberately NOT shipped (footgun).
- `.cb`-source `import redis` intrinsic/extern wiring is delivered (the
  5-layer wiring above); a real-LLM translation rerun is the open item.
