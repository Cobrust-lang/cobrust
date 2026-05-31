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
returns the safe default (`get` → `""`, `delete` → `0`, `exists` →
`false`), and `set` is silently a no-op. Your program never crashes at
the boundary. This is what lets the test suite prove the whole pipeline
works without needing a real Redis server running.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are a
  separate, not-yet-finished part of the toolchain).
- Values are strings for now (`get_int` / `get_bytes` are follow-ups).
- A missing key and a key that holds an empty string both read as `""`
  for now (an `Option`-returning `get` that distinguishes them is a
  tracked follow-up).
- Keep the `Client` local to the function; single-threaded — don't share
  one connection across spawned tasks (a connection pool is a follow-up).
- `expire` / `incr` / hash operations (`hset` / `hget`) are the next
  batch (Phase B).

These are tracked follow-ups, not dead ends.

## Attribution

Cobrust's `redis` module is built on the BSD-3-Clause-licensed `redis`
crate (redis-rs). That license is permissive and compatible with
Cobrust; the attribution is recorded in `crates/cobrust-redis/NOTICE`.
