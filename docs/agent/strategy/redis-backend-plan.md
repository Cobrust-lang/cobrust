---
doc_kind: strategy
strategy_id: redis-backend-plan
title: redis backend plan — cobrust-redis (cache/KV) wrap of the redis-rs crate, den-shaped Connection handle, sync-path (ADR-0078 Phase-1c)
status: plan
date: 2026-06-01
last_verified_commit: 936f13c
governs: [adr:0078, "issue:155"]
relates_to: [adr:0019, adr:0022, adr:0028, adr:0072, adr:0073, "strategy:v0.7.0-network-backend-libraries-roadmap", "claude.md:§2.2", "claude.md:§2.5", finding:f64]
sourced_from: P10 #155/ADR-0078 redis scoping dispatch 2026-06-01 (research-only; zero src edits)
---

# Redis Backend Plan — `cobrust-redis` (cache / KV) over redis-rs

> A PLAN, not a spec. Scopes a future impl sprint so it executes without
> re-discovery. Wiring anchors are at `936f13c`; the impl sprint re-greps
> the named functions (F44 anti-stale discipline). Web-research facts are
> dated 2026-06-01 and cited; design choices that have not been verified
> against a live build are marked `[UNVERIFIED]`.

ADR-0078 already DECIDED the road (wrap-the-crate, §2/§3), RATED redis
**FLAT** (§3 table row + §3.1 FLAT bucket), placed it in **Phase 1c**
(§7), and scored its §2.5 surface (~0.9, §8). This plan does the next
layer down: it pins the *exact* crate + version + license, designs the
clean `.cb` surface, and maps every wiring seam to a named function so
the sprint is a fill-in-the-blanks exercise off the den/strike template.

---

## 1. Bottom-line

- **Crate: `redis` (redis-rs), v1.2.2** (latest stable, released
  2026-05-29 — crates.io / docs.rs). Use the **synchronous path**
  (`Client::open(url)` → `client.get_connection()` → the `Commands`
  trait), driven through a `default-features = false` build with a
  minimal feature set. This is **den's `Connection`-handle pattern
  verbatim** (opaque `*mut u8` handle, borrow-receiver method calls,
  return-fresh-handle on connect) — the lowest-new-surface handle wrap on
  the chain.
- **NOT `fred`** (async-only, Tokio-required, heavier — its value is
  cluster/pubsub/replica-routing none of which the v0.7.0 KV surface
  needs) and **NOT `deadpool-redis` / `r2d2`** for the first proof
  (pooling is a follow-up; the first proof holds ONE `Connection` in the
  handle, exactly as `strike` holds one blocking `reqwest::Client`).
- **License: BSD-3-Clause** (verified `redis-rs/main/LICENSE`,
  2026-06-01). Permissive, no copyleft — **compatible** with shipping
  inside Cobrust's `Apache-2.0 OR MIT` workspace, BUT it is a **third
  license distinct from every crate wrapped so far** (argon2 / jsonwebtoken
  / reqwest / rusqlite are all Apache-2.0/MIT). This is the single most
  important carry-forward (§5 R1) — it needs a one-line CTO sign-off + a
  NOTICE/provenance entry, not a code change.
- **v0.7.0 MUST-ship surface:** `redis.connect(url) -> Client`;
  `client.set(key, value)`; `client.get(key) -> str`; `client.delete(key)
  -> i64`; `client.exists(key) -> bool`. That is "connect + the four KV
  verbs an LLM writes for a cache" — Phase A below. `expire` / `incr` /
  hash ops are Phase B (still in-scope for v0.7.0 if the sprint has
  budget; not the MUST line).

**Recommendation (1 paragraph).** Ship `cobrust-redis` as the eleventh
ecosystem module on the EXACT den/strike template: a new `crates/cobrust-redis`
staticlib crate whose `cabi.rs` holds a `redis::Connection` behind a `*mut
u8` handle (the strike `Response`-handle code is the line-for-line model,
including `DROP_COUNT`), plus the standard 5-row wiring (ecosystem.rs
manifest + the `is_ecosystem_module`/`ecosystem_module_for_symbol`
registrations + codegen externs + the new ECO_ADT_BASE id block + a
`.cb` e2e). The redis-rs **sync** path means **no async-收编 is needed at
all** (§3.5) — this is strictly simpler than ADR-0078's own Phase-1c note,
which budgeted an S1 `block_on` for sqlx; redis gets strike's "the crate
ships a blocking facade" (S2) for free, so it is the cheaper half of
Phase 1c. The only genuinely new design decisions versus strike are (a)
the get/set value-type story (redis values are bytes; the first proof
fixes them to `str`, dropping the generic `T` exactly as ADR-0078 §3
prescribes) and (b) the connection-absent test story (§3.6): CI has no
Redis server, so the e2e must self-skip on connect-failure rather than
fail. Everything else is mechanical.

---

## 2. The `.cb` API surface (clean, no-legacy-debt — CLAUDE.md "elegant ecosystem surface" law)

Design law (the elegant-ecosystem + §2.2/§2.5 ledger): drop the footguns
every other language's redis client carries. The footgun-ledger this
surface deliberately avoids:

- **No stringly-typed "do everything through `execute("SET k v")`"** — the
  raw-command escape hatch is a footgun (injection, arg-quoting). The `.cb`
  surface is **typed methods** (`client.set(k, v)`), not a command string.
  (Internally the shim MAY use `redis::cmd(...)` or the `Commands` trait;
  the `.cb` side never sees a command string.)
- **No connection-vs-pool-vs-client confusion** (redis-py's
  `Redis()`/`ConnectionPool()`/`StrictRedis()` sprawl) — ONE handle type,
  `Client`. The first proof's `Client` wraps a single `Connection`;
  pooling is an internal upgrade that never changes the `.cb` surface.
- **No exceptions-as-control-flow** — a missing key is `get` returning the
  empty-string sentinel (or, in a richer follow-up, `Option<str>`), NOT a
  raised exception; a connection/protocol error is a fail-clean sentinel
  return (status-style), NOT a panic across the C ABI. `Result`-shaped,
  per CLAUDE.md §2.2. (See §2.3 for the sentinel-vs-Option tension.)
- **No implicit reconnect/retry magic** the user can't see — the first
  proof is explicit; `ConnectionManager`-style auto-reconnect is an opt-in
  follow-up, not a hidden default.
- **No `db=`/`decode_responses=`/`socket_timeout=` option-bag sprawl in the
  constructor** — `connect(url)` takes a single canonical `redis://` URL
  (the db index, auth, TLS all live IN the URL, redis-rs's native model).

### 2.1 Phase-A surface (the v0.7.0 MUST-ship)

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
| `redis.connect(url)` | `Client::open(url)?.get_connection()?` | `__cobrust_redis_connect(url) -> *mut u8` | `Client` handle |
| `client.set(k, v)` | `con.set::<_,_,()>(k, v)` | `__cobrust_redis_client_set(c, k, v) -> ()` | `None` |
| `client.get(k)` | `con.get::<_, Option<String>>(k)` | `__cobrust_redis_client_get(c, k) -> *mut u8` | `str` |
| `client.delete(k)` | `con.del::<_, i64>(k)` | `__cobrust_redis_client_delete(c, k) -> i64` | `i64` |
| `client.exists(k)` | `con.exists::<_, bool>(k)` | `__cobrust_redis_client_exists(c, k) -> bool` | `bool` |

Note `delete`/`exists` (not `del`/`del?`): the `.cb` name is the readable
Python-idiom verb (redis-py is `r.delete(k)` / `r.exists(k)`), §2.5-aligned;
the redis-rs method underneath is `del`/`exists`. `set` returns `None`
(side-effect, mirrors pit `app.route`'s None-return discipline — no second
drop-eligible handle minted).

### 2.2 Phase-B surface (in-scope for v0.7.0 if budget; not the MUST line)

| `.cb` call | redis-rs sync call | C-ABI shim | ret |
|---|---|---|---|
| `client.set_expiry(k, v, secs)` | `con.set_ex(k, v, secs)` | `__cobrust_redis_client_set_expiry` | `None` |
| `client.expire(k, secs)` | `con.expire(k, secs)` | `__cobrust_redis_client_expire` | `bool` |
| `client.incr(k) -> i64` | `con.incr(k, 1)` | `__cobrust_redis_client_incr` | `i64` |
| `client.incr_by(k, n) -> i64` | `con.incr(k, n)` | `__cobrust_redis_client_incr_by` | `i64` |
| `client.hset(k, field, v)` | `con.hset(k, field, v)` | `__cobrust_redis_client_hset` | `None` |
| `client.hget(k, field) -> str` | `con.hget::<_,_,Option<String>>` | `__cobrust_redis_client_hget` | `str` |

These are pure additive manifest rows + cabi shims (same handle, same
borrow-receiver discipline) — no new mechanism over Phase A.

### 2.3 The two real surface-design tensions (decide in the sprint, flagged here)

1. **`get` return type — sentinel `str` vs `Option<str>`.** ADR-0078 §8
   scores `get_str` returning a `str`. A redis GET on a missing key is
   genuinely "absent", and CLAUDE.md §2.2 explicitly prefers
   `Option<T>`/`x.is_some()` over a sentinel. **First-proof recommendation:
   the empty-string sentinel `str`** (matches strike's `text()` / den's
   `fetchall()` first-proof rendering convention, ships on the existing
   value ABI with no new marshalling). **`Option<str>` is the §2.2-correct
   follow-up** — it needs the `.cb` `Option` ABI across the C boundary,
   which no ecosystem module exercises yet, so it is its own small design
   (carry-forward §5 R4). `[UNVERIFIED]` whether an empty *stored* value
   ("" set deliberately) vs a missing key must be distinguished for the
   v0.7.0 surface — if yes, the sentinel is insufficient and `Option<str>`
   is forced; the MUST line assumes "absent == empty" is acceptable for a
   cache (it usually is).

2. **Method name `set`/`get`/`delete` vs a typed-suffix `set_str`/`get_str`.**
   ADR-0078 §8 wrote `set_str`/`get_str` (the type-suffix that pays for the
   absent generic `T`). **Recommendation: prefer the un-suffixed
   `set`/`get`/`delete`** for the §2.5 win (redis-py is `r.set`/`r.get`;
   the un-suffixed name has strictly higher training-data overlap). The
   first proof fixes the value type to `str` *without* advertising it in
   the name — the suffix only becomes necessary IF a sibling `get_int` /
   `get_bytes` ships alongside (then the family needs disambiguation,
   exactly like coil's `get_str`/`get_int` precedent ADR-0078 cites). For
   v0.7.0 MUST (str only), un-suffixed is cleaner. This is a reversible
   naming call; record the decision in the impl ADR/commit.

---

## 3. The wiring plan (per the ADR-0078 / ADR-0072 chain)

`cobrust-redis` is the **eleventh ecosystem module**, the den/strike
handle-pattern template applied verbatim. Eight layers, each anchored to a
named function verified at `936f13c`.

### 3.0 The proven template to copy

The closest existing module is **`strike`** (`crates/cobrust-strike/`):
it pairs a free-function entrypoint (`get`/`post`, like `redis.connect`)
with a borrow-receiver handle (`Response`, like `redis.Client`), drives an
async-by-default crate through its **blocking facade**
(`reqwest::blocking`, exactly as redis-rs's `get_connection()` is the
blocking facade over the `aio` path), and carries the `DROP_COUNT`
exactly-once instrument. **`crates/cobrust-strike/src/cabi.rs` is the
line-for-line model for `crates/cobrust-redis/src/cabi.rs`.** `den`
(`crates/cobrust-den/`) is the secondary model for a connection handle
that owns a stateful resource.

### 3.1 The new crate — `crates/cobrust-redis/`

- `Cargo.toml`: `name = "cobrust-redis"`, `license = "Apache-2.0 OR MIT"`
  (the Cobrust *wrapper* is dual-licensed; the *dependency* redis-rs is
  BSD-3-Clause — record in PROVENANCE/NOTICE, §5 R1). `crate-type =
  ["rlib", "cdylib", "staticlib"]` (the fang/strike trio — `staticlib`
  produces `libredis.a` for per-import link). `[dependencies]`:
  ```toml
  redis = { version = "1.2", default-features = false, features = [...] }
  ```
  Feature set `[UNVERIFIED — confirm in sprint against v1.2.2 feature
  list]`: the sync path needs **no async runtime feature** (`tokio-comp`
  is async-only). The minimal sync build is `default-features = false`
  with NO features (the bare `Client`/`Connection`/`Commands` sync surface
  is in the crate's default-free core per docs.rs). Add `tls-rustls` ONLY
  if `rediss://` TLS URLs are in-scope for v0.7.0 (recommend deferring TLS
  to a follow-up — plaintext loopback is the first proof). Do NOT add
  `r2d2`/`connection-manager` (pooling/reconnect — follow-ups). **`tokio`
  must NOT be pulled** — its absence is what keeps this build light and
  the wasm32/riscv64 cross-builds (ADR-0075) green.
- `[dev-dependencies] cobrust-stdlib = { path = "../cobrust-stdlib" }` (the
  ADR-0072 Q5 pattern — `__cobrust_str_*` resolve from `libcobrust_stdlib.a`
  at the `cobrust build` link step in production; dev-dep only for the
  in-crate cabi unit tests). Copy `crates/cobrust-fang/build.rs` verbatim
  (the macOS `-undefined dynamic_lookup` cdylib flag).
- `src/lib.rs`: `pub mod cabi;` + a thin `src/client.rs` holding the
  `Client` newtype (wrapping `redis::Connection`) + a `fail_clean`
  constructor, mirroring `strike/src/client.rs`'s `Response`.
- `src/cabi.rs`: copy strike's `read_str_buf` / `alloc_str_buffer` /
  `DROP_COUNT` block verbatim, then the shims:
  - `__cobrust_redis_connect(url: *mut u8) -> *mut u8` — `read_str_buf`
    the url, `Client::open(&url).and_then(|c| c.get_connection())`,
    `Box::into_raw(Box::new(Client(con)))` on success; on ANY error return
    a **fail-clean sentinel handle** (a `Client` in a "disconnected" state
    whose every command returns the sentinel) — NEVER null, NEVER panic.
    `[UNVERIFIED]` exact disconnected-sentinel representation — options:
    (a) `Box` an `Option<Connection>` (None == disconnected), (b) a bool
    flag on the newtype. (a) is cleaner. The e2e self-skip (§3.6) keys off
    a successful connect, so the sentinel mainly guards the
    no-redis-at-runtime path.
  - `__cobrust_redis_client_set(c, k, v) -> ()` / `_get(c, k) -> *mut u8`
    / `_delete(c, k) -> i64` / `_exists(c, k) -> bool`: each **borrows**
    the handle (`&mut *c.cast::<Client>()` — redis `Connection` command
    methods take `&mut self`; note this is `&mut`, unlike strike's `&`
    read-only borrow — see §3.7 ownership note), reads the str args, runs
    the `Commands` call, maps `Err` → sentinel (empty str / `0` / `false`)
    fail-clean. `get` on a missing key → empty-str sentinel.
  - `__cobrust_redis_client_drop(c)` — `Box::from_raw` + drop + `DROP_COUNT
    += 1`, idempotent on null (strike's `_drop` verbatim).
- Unit tests (`#[cfg(test)]`, the strike `extern crate cobrust_stdlib` +
  `#[used]` anchor): a round-trip needs a live server, so the **pure-unit**
  tests assert the **fail-clean / null-tolerance / drop-once** paths
  WITHOUT a server (construct a disconnected-sentinel `Client`, run every
  shim, assert sentinels + exactly-one drop). The live round-trip is the
  e2e (§3.6), self-skipping when no server.

### 3.2 The ECO_ADT_BASE id block — redis gets `0xE000_0800`

`crates/cobrust-types/src/ecosystem.rs` allocates one 256-slot block per
handle-typed module off `ECO_ADT_BASE = 0xE000_0000` (ecosystem.rs:43).
Current allocation (verified `936f13c`):

| block | module | base const |
|---|---|---|
| `0x000` | den (`Connection`+`Cursor`) | `DEN_CONNECTION_ADT` |
| `0x100` | strike (`Response`) | `STRIKE_RESPONSE_ADT` |
| `0x200` | scale (msgpack) — **RESERVED, ships no handle** | (comment only) |
| `0x300` | molt (`DateTime`) | `MOLT_DATETIME_ADT` |
| `0x400` | pit (`App`/`Request`/`Response`/`Server`/sentinel) | `PIT_APP_ADT` |
| `0x500` | hood (`Command`) | `HOOD_COMMAND_ADT` |
| `0x600` | dora (`Node`+`Event`) | `DORA_NODE_ADT` |
| `0x700` | coil (`Buffer`) | `COIL_BUFFER_ADT` |
| **`0x800`** | **redis (`Client`) — NEW** | **`REDIS_CLIENT_ADT` (proposed)** |

**Decision: allocate redis at `0xE000_0800`** (the next free block past
coil). Do NOT reuse the `0x200` scale gap — keeping blocks monotonic with
allocation order matches every existing comment and the per-block range
assertions (ecosystem.rs:~1717-2311). Add:
```rust
pub const REDIS_CLIENT_ADT: AdtId = AdtId(ECO_ADT_BASE + 0x800);
pub fn redis_client_ty() -> Ty { Ty::Adt(REDIS_CLIENT_ADT, vec![]) }
```
The first proof ships ONE handle (`Client`); `0x801`+ stay free for a
future `redis.Pipeline` / `redis.PubSub` handle.

### 3.3 The five wiring layers (each a named function)

| Layer | File | Function / site (anchored `936f13c`) | Edit |
|---|---|---|---|
| **Module registry** | `crates/cobrust-types/src/ecosystem.rs` | `is_ecosystem_module` @1591 (the `matches!` list) | add `"redis"` to the alternation |
| **Free-fn manifest** | same | `lookup_module_fn` @426 (the `(module,func)` match) | add `("redis","connect") => EcoSig::from_values("__cobrust_redis_connect", vec![Ty::Str], redis_client_ty(), PyCompatTier::Semantic)` |
| **Handle-method manifest** | same | `lookup_handle_method` @921 (the `(AdtId,method)` match) | add `(REDIS_CLIENT_ADT,"set")` / `"get"` / `"delete"` / `"exists"` rows (params per §2.1 table; receiver implicit; `Semantic` tier) |
| **Drop symbol** | same | `handle_drop_symbol` @326 | add `REDIS_CLIENT_ADT => Some("__cobrust_redis_client_drop")` |
| **Block range assert** | same | the `#[cfg(test)]` per-block range asserts (~1717+) | add a `redis_client_id_recognized_and_in_reserved_block` test: `>= ECO_ADT_BASE+0x800 && < +0x900` (mirror the strike/coil tests) |
| **Typecheck** | `crates/cobrust-types/src/check.rs` | `try_synth_method_call` / the handle-method dispatch (the `lookup_handle_method` consult) | **NO new mechanism** — the new rows resolve through the existing handle-method path (the den/strike `Connection`/`Response` path); verify the str-arg method-call shape type-checks |
| **MIR** | `crates/cobrust-mir/src/lower.rs` | `try_lower_ecosystem_call` @2110 (Case 1 free-fn @2163 + Case 2 handle-method @2185) + `emit_ecosystem_call` @2230 | **NO new mechanism** — `redis.connect(url)` rides Case 1 (`lookup_module_fn` → `Constant::Str("__cobrust_redis_connect")`); `client.set(k,v)` rides Case 2 (borrow-receiver Move→Copy upgrade @2197, then `emit_ecosystem_call`). Identical to `conn.execute` |
| **Codegen externs** | `crates/cobrust-codegen/src/llvm_backend.rs` | `declare_runtime_helpers` (the den/strike extern block) | add extern decls for the six `__cobrust_redis_*` symbols (`connect`: `ptr->ptr`; `set`: `ptr,ptr,ptr->void`; `get`: `ptr,ptr->ptr`; `delete`: `ptr,ptr->i64`; `exists`: `ptr,ptr->i1`; `drop`: `ptr->void`). Mirror strike's extern shapes |
| **Link recognizer** | `crates/cobrust-cli/src/build/intrinsics.rs` | `ecosystem_module_for_symbol` @1374 (the prefix `if`-chain) | add `else if sym.starts_with("__cobrust_redis_") { Some("redis") }` — this is what makes `collect_ecosystem_modules` (@1323) pull `libredis.a` into the link |
| **Archive locate** | `crates/cobrust-cli/src/build.rs` | `locate_ecosystem_archive` @984 | **NO edit** — it is module-name-generic (`lib{module}.a` + `cargo build -p cobrust-{module}`); naming the crate `cobrust-redis` → `libredis.a` makes it work automatically (the `[lib] name = "redis"` in Cargo.toml must produce `libredis.a`, matching `strike`→`libstrike.a`) |
| **Docs** | `docs/{agent,human/zh,human/en}` | new redis module spec pages | per CLAUDE.md §3.3 sync rule, IN the impl commit |

### 3.4 Cargo.lock + workspace registration (finding F64 — do not skip)

- Add `"crates/cobrust-redis"` to the root `Cargo.toml` `members` list
  (after `cobrust-fang`, line ~24).
- Pin `redis` in the workspace (the v0.7.0 roadmap §Z.0.b mandate already
  calls for pinning redis in the workspace `Cargo.toml` deps audit).
- **Stage `Cargo.lock`** in the impl commit (finding F64 — `--locked` CI
  rejects an unstaged lockfile, cluster-failing build/clippy/test with
  exit 101). Every redis-rs transitive dep (combine/itoa/ryu/url/socket2/
  arcstr/xxhash-rust/percent-encoding/…) lands in the lockfile; `git
  status -- Cargo.lock` must be clean-after-add before commit.

### 3.5 Async→sync bridge — **none needed** (the key simplification)

ADR-0078 §5 names two 收编 strategies: **S1** (`block_on` a singleton
tokio runtime, pit's path, for async-ONLY crates) and **S2** (use the
crate's own blocking facade, strike's path). redis-rs's
`Client::get_connection()` / sync `Connection` / `Commands` trait **IS the
blocking facade** (verified docs.rs — the non-`aio` path) → **redis takes
S2 with ZERO new runtime code**, exactly like strike. No `OnceLock<Runtime>`,
no `block_on`, no `tokio` dependency. This is *simpler* than ADR-0078
Phase-1c's own note (which budgeted S1 `block_on` for sqlx); redis is the
cheaper half of 1c precisely because its sync path exists. The `.cb`
surface is sync; CLAUDE.md §2.2 (no async/sync coloring) is honored at the
cabi boundary by the crate's own blocking API, not by us.

The one runtime-affinity caveat ADR-0078 §5 flags (a handle built under a
runtime must be used under it) **does not apply** — there is no runtime; a
sync `redis::Connection` is a plain stateful TCP connection. (But see §3.7
`!Send`.)

### 3.6 The `.cb` e2e — real-redis-optional, CI-self-skip

CI has **no Redis server** (the v0.7.0 roadmap §Z notes "NO existing
crates/infra for Redis"). The e2e (`crates/cobrust-cli/tests/ecosystem_redis_e2e.rs`,
modeled on `ecosystem_strike_e2e.rs` / `ecosystem_den_e2e.rs`) must NOT
hard-fail when redis is absent. Three options, recommended in order:

1. **Self-skip on connect-failure (RECOMMENDED for the live round-trip).**
   Before building the `.cb` program, the test probes
   `TcpStream::connect("127.0.0.1:6379")` (or `$REDIS_URL`) with a short
   timeout; if it fails, the test `return`s early (an effective skip —
   Rust has no first-class `#[ignore]`-at-runtime, so an early-return +ed
   an eprintln "skipped: no redis" is the project idiom; `[UNVERIFIED]`
   whether the repo prefers a `#[ignore]` attribute + a CI job that starts
   a redis service-container, vs the runtime-probe self-skip — pick to
   match whatever den/strike-adjacent network tests already do; strike's
   e2e sidesteps this by spinning its OWN loopback server via `pit::App`,
   which redis cannot do — there is no Rust in-process redis server in the
   workspace). When a server IS present (local dev, or a CI redis
   service-container), the test runs the full set/get/delete/exists
   round-trip and asserts the printed values.
2. **Fail-clean path is ALWAYS testable WITHOUT a server (RECOMMENDED as
   the always-on e2e).** A `.cb` program that does `redis.connect(
   "redis://127.0.0.1:1/")` (an unreachable/invalid port → connect fails →
   fail-clean sentinel handle) then `client.get("k")` and prints the
   empty-string sentinel + a `client.exists("k")` printing `false` — this
   exercises the FULL compile→link→run vertical slice and the no-panic
   guarantee at the C ABI boundary, with NO server, ALWAYS green in CI.
   This is the strike `test_e2e_strike_unreachable_url_yields_fail_clean_sentinel`
   pattern adapted — make it the primary always-on e2e.
3. **A CI redis service-container** (GitHub Actions `services: redis:`) to
   run option-1's live round-trip in CI too — a follow-up nicety, not a
   blocker. `[UNVERIFIED]` whether the project's CI yaml already has a
   service-container precedent; if not, adding one is its own small CI PR.

**Done-means for the e2e layer:** option-2 (fail-clean, server-less) is
GREEN in CI and proves the chain; option-1 (live round-trip) runs locally
+ self-skips in server-less CI.

### 3.7 Ownership / lifecycle notes (the one delta from strike)

- **`&mut` receiver, not `&`.** redis-rs sync command methods take `&mut
  self` on the `Connection` (the connection is stateful — it writes the
  request + reads the reply). strike's `Response` accessors borrow `&`
  (read-only). So redis shims cast to `&mut *c.cast::<Client>()` and the
  `Client` newtype must hold the `Connection` by value (not behind a
  shared `&`). The MIR borrow-receiver Move→Copy upgrade (lower.rs:2197)
  still applies unchanged — the handle local survives the call and drops
  once at scope exit; the `&mut` is entirely inside the shim, invisible to
  the `.cb` aliasing model. `[UNVERIFIED]` — confirm no `.cb`-level
  aliasing rule is violated by two sequential `&mut`-taking method calls on
  the same handle local (it should be fine: each call is a separate
  borrow-then-release at the shim boundary, exactly like two `conn.execute`
  calls in the den e2e).
- **`!Send` like den.** A sync `redis::Connection` is `!Send`/`!Sync`
  (single TCP connection, single-threaded use — `[UNVERIFIED]`, confirm
  against v1.2.2). This matches den's `Rc<RefCell<Connection>>` `!Send`
  constraint (ADR-0072 §5 R2, recorded-not-fixed). The `.cb` object model
  is single-threaded for ecosystem handles today, so this is the existing
  accepted constraint, not a new one. Record it in the crate doc + the
  impl ADR done-means (the per-crate Send/affinity check ADR-0078 §10
  mandates).
- **Connection lifetime = handle lifetime.** The `Connection` opens at
  `redis.connect` and closes when the `Client` handle drops at `.cb` scope
  exit (TCP FIN on `Box::from_raw` drop). No explicit `.close()` is needed
  in the first proof (a `client.close()` is a trivial follow-up if desired,
  but scope-exit drop is the §2.2-clean default — RAII, no
  forgot-to-close footgun).

---

## 4. Phased plan

Each phase: scope, done-means, the heaviest risk it carries.

### Phase A (the v0.7.0 MUST-ship) — connect + set/get/delete/exists

- **Scope:** the `crates/cobrust-redis` crate (§3.1), the ECO_ADT_BASE
  `0x800` block (§3.2), the five wiring layers (§3.3) for `connect` + the
  four KV verbs, the Cargo.lock staging (§3.4), and BOTH e2e shapes (§3.6
  option-2 always-on + option-1 self-skipping live).
- **Done-means:**
  1. A server-LESS `.cb` e2e (fail-clean sentinel path) is GREEN in CI and
     prints the empty-str + `false` sentinels (proves compile→link→run +
     no-panic-at-C-ABI).
  2. A live round-trip e2e (`set "greeting" "hello"` → `get` prints
     `hello` → `delete` prints `1` → `exists` prints `false`) passes when a
     redis is reachable, self-skips when not.
  3. cabi unit tests assert fail-clean sentinels + null-tolerance +
     `DROP_COUNT` exactly-once (server-less).
  4. `libredis.a` links ONLY when a program `import redis` (the
     `collect_ecosystem_modules` link-bloat guard, ADR-0072 risk 3).
  5. Workspace gates green (build/clippy/test, `--locked` with staged
     Cargo.lock); the cross-builds (wasm32/riscv64, ADR-0075) still build
     `libredis.a` OR redis is explicitly excluded from the cross matrix
     with a rationale (§5 R5).
  6. Docs in all three trees (zh/en/agent), per §3.3.
  7. A paired ADSD post-author audit (the "did this respect §2.5
     compile-time-catch + training-data-overlap" + the elegant-ecosystem
     footgun-ledger score) BEFORE merge.
- **Heaviest risk:** the **BSD-3-Clause license sign-off** (§5 R1 — the
  one truly-new, non-mechanical item) + the **CI-has-no-redis test story**
  (§3.6 — mitigated by making the server-less fail-clean path the primary
  e2e).

### Phase B (in-scope for v0.7.0 if budget) — expire / incr / hash

- **Scope:** the §2.2 rows (`set_expiry`/`expire`/`incr`/`incr_by`/`hset`/
  `hget`) — additive manifest rows + cabi shims, same handle.
- **Done-means:** per-verb round-trip in the live e2e (`set_expiry` then
  `exists`-after-TTL; `incr` twice prints `2`; `hset`/`hget` round-trip);
  self-skip when server-less; gates green.
- **Heaviest risk:** **`expire`-TTL test timing** (a TTL test that sleeps
  past expiry is slow/flaky — assert the `expire` return bool + a short
  `set_expiry` then immediate `exists==true`, defer actual-expiry timing
  to a long/ignored test). Low overall — no new mechanism.

### Phase C (follow-ups, deferred — own small ADRs/decisions)

- `Option<str>` return for `get` (§2.3-1, the §2.2-correct upgrade — needs
  the `.cb` `Option`-across-C-ABI ABI, the first ecosystem use).
- Typed `get_int`/`get_bytes` siblings (then the `_str` suffix returns,
  §2.3-2).
- Connection **pooling** (`r2d2`/`deadpool-redis`) — transparent internal
  upgrade, no `.cb` surface change; only needed for a multi-threaded `.cb`
  server (pit) sharing one redis. `[UNVERIFIED]` whether v0.7.0's pit
  integration needs a shared pool — if a pit handler wants redis, the
  `!Send` connection + the pool question becomes load-bearing (§5 R3).
- `rediss://` **TLS** (`tls-rustls` feature) — for non-loopback redis.
- `ConnectionManager` **auto-reconnect** — opt-in, explicit.
- Raw-`cmd` escape hatch — deliberately NOT shipped (footgun, §2).

---

## 5. Risks / uncertainties (carry-forward for the CTO)

- **R1 — BSD-3-Clause license (the headline carry-forward).** redis-rs is
  **BSD-3-Clause** (verified `redis-rs/main/LICENSE`, 2026-06-01) — the
  FIRST non-(Apache/MIT) crate in the wrap-a-crate set. Permissive + no
  copyleft → **compatible** with Cobrust's `Apache-2.0 OR MIT` workspace,
  BUT requires (a) a CTO one-line OK that a BSD-3-Clause runtime dep is
  acceptable for the project's distribution story, and (b) a NOTICE /
  `PROVENANCE.toml` attribution entry (the BSD-3 attribution clause). This
  is the one item that is NOT mechanical engineering — surface it before
  the sprint starts. `[UNVERIFIED]` whether the project already has a
  third-party-license NOTICE mechanism, or whether ADR-0001 (license)
  scopes acceptable dependency licenses.
- **R2 — redis crate version churn.** ADR-0078 (2026-05-29) and the
  network roadmap (2026-05-25) cite redis `0.27`; the actual latest is
  **`1.2.2`** (2026-05-29) — the crate went 1.0 in this window. The 1.x
  feature-flag names + the sync API surface used here (`Client::open` /
  `get_connection` / `Commands`) are stable across the 0.x→1.x boundary
  (verified docs.rs latest), but the sprint MUST pin against the actual
  1.2.x feature list, not the roadmap's stale `0.27` note. `[UNVERIFIED]`
  the exact `default-features = false` minimal-sync feature set for 1.2.2
  (confirm the bare sync `Connection`/`Commands` path needs NO feature
  flag, only the absence of `tokio-comp`).
- **R3 — pit-integration / shared-connection ownership.** The v0.7.0
  master-mandate pairs redis with the FastAPI-real (pit) backend. A pit
  handler that touches redis needs a redis handle reachable from the
  handler trampoline (ADR-0073 callback chain) — but the sync
  `Connection` is `!Send`, and pit serves under a tokio runtime
  (multi-task). **A per-request `redis.connect` is the simple first answer**
  (each handler opens its own connection — correct, just not pooled);
  **sharing ONE connection across async handlers is unsafe** (`!Send` +
  `&mut`) and forces the pool (Phase C). This coupling is the deepest
  design uncertainty and is OUT of this plan's MUST scope — flag it as the
  redis×pit sub-question for whoever wires the FastAPI-real demo.
  `[UNVERIFIED]` whether v0.7.0's demo actually calls redis from inside a
  pit handler (if it's a standalone redis demo, R3 doesn't bite).
- **R4 — `get` sentinel vs `Option<str>` (§2.3-1).** The first-proof
  empty-string sentinel cannot distinguish "key absent" from "key holds an
  empty string". Acceptable for a cache MUST line; if a v0.7.0 use-case
  needs the distinction, `Option<str>` is forced and becomes the first
  `.cb` `Option`-across-C-ABI design. Carry-forward, not a blocker.
- **R5 — cross-build (wasm32/riscv64, ADR-0075).** redis-rs pulls
  `socket2` + raw TCP — **TCP sockets may not exist / may not link on
  wasm32-wasip1** (the ADR-0075 Phase-2 cross target). `[UNVERIFIED]` —
  the sprint must EITHER confirm `libredis.a` cross-builds for the wasm32/
  riscv64 matrix, OR explicitly exclude redis from the cross matrix with a
  documented rationale (a network module on a sandboxed wasm target is
  arguably out-of-scope anyway). This is the most likely place the
  "mechanical" story hits a real wall — budget a spike for it.
- **R6 — no in-process redis server for the e2e.** Unlike strike (which
  spins its own `pit::App` loopback server), there is NO Rust in-process
  redis the workspace can start, so the live round-trip e2e CANNOT be
  hermetic the way strike's is. Mitigated by §3.6 option-2 (the server-less
  fail-clean path IS hermetic + always-on); the live round-trip is
  best-effort self-skipping. `[UNVERIFIED]` whether an embedded/mock redis
  Rust crate exists that's worth a dev-dependency (most are incomplete;
  the self-skip + fail-clean combo is the pragmatic answer).
- **R7 — manifest drift (ADR-0078 §10 accepted debt).** redis adds ~10
  hand-maintained manifest rows; the ecosystem-manifest generator is still
  deferred. Same accepted debt as every prior module — noted, not new.
- **R8 — doc/ADR naming reconciliation.** ADR-0078 §8 wrote
  `redis.connect(url)` + `set_str`/`get_str`; the network roadmap §Z.7.b
  wrote `redis.Redis(host=...).get(key)` (redis-py style) and the crate
  name `cobrust-redis`. This plan reconciles to **`redis.connect(url)` +
  un-suffixed `set`/`get`** (§2.1, §2.3-2) — the impl ADR should record
  the final names so the two upstream docs don't keep diverging.

---

## 6. Source citations (web research, 2026-06-01)

- redis crate version `1.2.2` (2026-05-29), feature flags (`tokio-comp`,
  `smol-comp`, `r2d2`, `connection-manager`, `tls-native-tls`,
  `tls-rustls`, `cluster`, `cluster-async`, `json`), deps —
  https://docs.rs/crate/redis/latest , https://crates.io/crates/redis
- redis sync API (`Client::open` → `get_connection` → `Connection` +
  `Commands` trait set/get/del/expire/incr/hget/hset/exists/set_ex,
  `RedisResult<T> = Result<T, RedisError>`, low-level
  `redis::cmd(...).arg(...).query(&mut con)`) —
  https://docs.rs/redis/latest/redis/
- redis-rs license **BSD-3-Clause** —
  https://github.com/redis-rs/redis-rs (LICENSE),
  raw `redis-rs/main/LICENSE`
- fred (async-only, Tokio/Futures, cluster/pubsub/replica) vs
  deadpool-redis / r2d2 (sync pooling) comparison; redis-rs sync-path
  recommends r2d2 pooling for multi-threaded/disconnect handling —
  https://docs.rs/fred , https://lib.rs/crates/deadpool-redis ,
  https://github.com/redis-rs/redis-rs
- redis-py canonical patterns (`r = redis.Redis(); r.set(k,v); r.get(k);
  r.delete(k); r.exists(k)`) — the §2.5 training-data target (general
  redis-py knowledge, corroborated by the network roadmap §Z.7.b).
