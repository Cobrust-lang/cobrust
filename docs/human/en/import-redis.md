# `import redis` — use a Redis cache / key-value store from Cobrust

> Status: ADR-0078 Phase-1c. The eleventh ecosystem library you can
> `import` from a `.cb` program and call end-to-end (compile → link →
> run). It wires `redis` (Cobrust's cache / KV client, the redis-py
> rebrand) onto the compiler's intrinsic / C-ABI / static-link chain,
> using the synchronous path of the Rust `redis` crate (redis-rs) — so
> there is no async runtime involved at all.

## Example first

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")
    client.set("greeting", "hello")
    let v: str = client.get("greeting")          # -> "hello"
    let n: i64 = client.delete("greeting")       # -> 1 (keys removed)
    let present: bool = client.exists("greeting") # -> false
    print(v)
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
# hello
```

## What you get (Phase A surface)

- **`redis.connect(url)`** — open a connection to a Redis server. Pass a
  single canonical `redis://[:password@]host[:port][/db]` URL (the
  database index, the password, and TLS all live inside the URL — there
  is no bag of `db=` / `decode_responses=` keyword options). Returns a
  `Client`.
- **`client.set(key, value)`** — store a string value under a key. It is
  a side effect (returns nothing).
- **`client.get(key)`** — read the value back as a `str`. A missing key
  reads as the empty string `""`.
- **`client.delete(key)`** — remove a key. Returns the number of keys
  removed (`0` or `1`).
- **`client.exists(key)`** — returns `true` when the key is present.

The method names are exactly the ones you already know from Python's
`redis` package (`set` / `get` / `delete` / `exists`), so an LLM — or
you — write them correctly on the first try.

## What you get (Phase B — expiry, counters, hashes)

The cache patterns you reach for right after the basic key-value verbs:

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")

    # Atomic counters — increment and read the new value in one step.
    client.set("hits", "10")
    let n: i64 = client.incr("hits")          # -> 11
    let m: i64 = client.incr_by("hits", 5)    # -> 16

    # Expiry (time-to-live) — make a key disappear after N seconds.
    let ttl_set: bool = client.expire("hits", 60)  # -> true (TTL applied)

    # Hashes — store named fields under a single key.
    let created: bool = client.hset("user:1", "name", "ada")  # -> true (new field)
    let name: str = client.hget("user:1", "name")             # -> "ada"
    print(name)
    return 0
```

- **`client.expire(key, seconds)`** — set a key's time-to-live. Returns
  `true` when the key exists and the timeout was applied, `false`
  otherwise.
- **`client.incr(key)`** — atomically add `1` to a counter and return the
  new value. A key that does not exist yet starts at `0`, so the first
  `incr` returns `1`.
- **`client.incr_by(key, n)`** — atomically add `n` and return the new
  value.
- **`client.hset(key, field, value)`** — set a field inside a hash.
  Returns `true` when the field is new, `false` when it overwrites an
  existing field.
- **`client.hget(key, field)`** — read a hash field back as a `str`. A
  missing field (or a missing hash) reads as the empty string `""`, just
  like `get`.

These are, again, exactly the redis-py names (`incr` / `expire` / `hset`
/ `hget`); `incr_by` is the readable spelling of `r.incr(key, n)`.

## Why this design?

- **Typed methods, never a command string.** You call `client.set(k, v)`,
  not `client.execute("SET k v")`. There is no raw-command escape hatch,
  so there is no command-injection or quoting footgun.
- **One handle type.** Just `Client` — none of the
  `Redis()` / `ConnectionPool()` / `StrictRedis()` confusion the Python
  library carries.
- **No exceptions for control flow.** A missing key is an empty string; a
  connection failure (no server running, bad URL) gives you a client
  whose reads quietly return the empty / `0` / `false` results — never a
  crash, never an exception you have to catch. This is the
  `Result`-shaped error discipline the rest of Cobrust uses.
- **It reuses the proven path.** `redis.connect` compiles down to the
  exact same kind of C-ABI call that `print` and `den.connect` already
  use; nothing new at runtime.
- **The connection cleans up automatically.** The `Client` is freed
  exactly once when it goes out of scope, which closes the TCP
  connection — no manual `close()`, no leaks.
- **Only what you import is linked.** A program that imports `redis`
  links `libredis.a`; one that doesn't, doesn't. No bloat. And because we
  use the synchronous path, no async runtime (`tokio`) is pulled in.

## A note on the fail-clean behaviour

If the server is unreachable or the URL is invalid, `connect` still hands
you a usable `Client` — a "disconnected" one. Every operation on it
returns the safe default (`get` / `hget` → `""`, `delete` / `incr` /
`incr_by` → `0`, `exists` / `expire` / `hset` → `false`), and `set` is
silently a no-op. Your program never crashes at the boundary. This is what
lets the test suite prove the whole pipeline works without needing a real
Redis server running.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are a
  separate, not-yet-finished part of the toolchain).
- Values are strings for now (`get_int` / `get_bytes` are follow-ups).
- A missing key and a key that holds an empty string both read as `""`
  for now (an `Option`-returning `get` that distinguishes them is a
  tracked follow-up).
- Keep the `Client` local to the function; single-threaded — don't share
  one connection across spawned tasks (a connection pool is a follow-up).
- A set-with-expiry one-shot (`SETEX`) is a small follow-up; for now use
  `set` then `expire`.

These are tracked follow-ups, not dead ends.

## Attribution

Cobrust's `redis` module is built on the BSD-3-Clause-licensed `redis`
crate (redis-rs). That license is permissive and compatible with
Cobrust; the attribution is recorded in `crates/cobrust-redis/NOTICE`.
