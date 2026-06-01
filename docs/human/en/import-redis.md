# `import redis` — use a Redis cache / key-value store from Cobrust

> Status: ADR-0078 Phase-1c/1d. The eleventh ecosystem library you can
> `import` from a `.cb` program and call end-to-end (compile → link →
> run). It wires `redis` (Cobrust's cache / KV client, the redis-py
> rebrand) onto the compiler's intrinsic / C-ABI / static-link chain,
> using the synchronous path of the Rust `redis` crate (redis-rs) — so
> there is no async runtime involved at all. Phase 1d adds the
> whole-collection reads (`lrange` / `smembers` / `hkeys` / `hgetall`)
> that return a `list[str]`.

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

## What you get (Phase C — lists and sets)

Redis lists (a double-ended queue) and sets (unique members):

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")

    # Lists — push and pop at either end.
    let n1: i64 = client.lpush("tasks", "a")   # -> 1 (new length; prepend at head)
    let n2: i64 = client.rpush("tasks", "b")   # -> 2 (append at tail)
    let count: i64 = client.llen("tasks")      # -> 2
    let head: str = client.lpop("tasks")       # -> "a"
    let tail: str = client.rpop("tasks")       # -> "b"

    # Sets — unique members, fast membership tests.
    let added: i64 = client.sadd("tags", "x")          # -> 1 (0 if already there)
    let present: bool = client.sismember("tags", "x")  # -> true
    let card: i64 = client.scard("tags")               # -> 1
    let removed: i64 = client.srem("tags", "x")        # -> 1 (0 if absent)
    print(head)
    return 0
```

- **`client.lpush(key, value)`** — prepend a value at the head of the
  list. Returns the list's new length.
- **`client.rpush(key, value)`** — append a value at the tail. Returns the
  list's new length.
- **`client.lpop(key)`** — pop one element off the head and return it as a
  `str`. An empty or missing list reads as the empty string `""`.
- **`client.rpop(key)`** — pop one element off the tail. Same `""` rule.
- **`client.llen(key)`** — the number of elements in the list (`0` if the
  key is absent).
- **`client.sadd(key, member)`** — add a member to a set. Returns the
  number added: `1` when the member is new, `0` when it was already
  present.
- **`client.srem(key, member)`** — remove a member. Returns the number
  removed (`1` or `0`).
- **`client.sismember(key, member)`** — returns `true` when the member is
  in the set.
- **`client.scard(key)`** — the number of members in the set (`0` if the
  key is absent).

Again, exactly the redis-py names (`lpush` / `rpush` / `lpop` / `rpop` /
`llen` / `sadd` / `srem` / `sismember` / `scard`).

## What you get (Phase 1d — read back a whole list, set, or hash)

The verbs that read back a *whole* collection at once. They each return a
`list[str]` (a list of strings) — so you can iterate it with a `for`
loop, index it (`xs[0]`), and ask its length (`xs.len()`), just like any
other Cobrust list.

```python
import redis

fn main() -> i64:
    let client = redis.connect("redis://127.0.0.1/")

    client.rpush("tasks", "a")
    client.rpush("tasks", "b")
    client.rpush("tasks", "c")

    # Read the whole list back. start=0, stop=-1 means "everything".
    let xs: list[str] = client.lrange("tasks", 0, -1)   # -> ["a", "b", "c"]
    print(xs.len())                                      # -> 3
    for task in xs:
        print(task)                                      # -> a / b / c

    # All members of a set.
    client.sadd("tags", "x")
    let tags: list[str] = client.smembers("tags")        # -> ["x"]

    # All field names of a hash.
    client.hset("user:1", "name", "ada")
    let fields: list[str] = client.hkeys("user:1")       # -> ["name"]

    # All field/value pairs of a hash — see the note below.
    let pairs: list[str] = client.hgetall("user:1")      # -> ["name", "ada"]
    return 0
```

- **`client.lrange(key, start, stop)`** — the elements of a list in the
  index range `start..stop` (both ends inclusive; negative indices count
  from the tail, so `0, -1` is the whole list — exactly redis's own rule).
  A missing key gives the empty list `[]`.
- **`client.smembers(key)`** — all members of a set, as a `list[str]`
  (redis sets have no order). A missing key gives `[]`.
- **`client.hkeys(key)`** — all field names of a hash, as a `list[str]`.
  A missing key gives `[]`.
- **`client.hgetall(key)`** — all field/value pairs of a hash.

These are, once more, the redis-py names (`lrange` / `smembers` /
`hkeys` / `hgetall`).

### `hgetall` returns a flat list, not a dict

Python's `redis` returns `hgetall` as a `dict`. Cobrust returns it as a
**flat** `list[str]` — `[field1, value1, field2, value2, ...]` — so the
example above gives `["name", "ada"]`. Read it two-at-a-time (a field
then its value). This is a deliberate, documented difference, the same
kind of difference `coil`'s `buffer.shape` makes (numpy returns a tuple,
`coil` returns a `list[i64]`): the flat list is the honest shape the
already-shipping list machinery supports cleanly today, without inventing
a new dict-across-the-boundary return shape. A `dict`-returning
`hgetall` is a tracked follow-up.

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
returns the safe default (`get` / `hget` / `lpop` / `rpop` → `""`,
`delete` / `incr` / `incr_by` / `lpush` / `rpush` / `llen` / `sadd` /
`srem` / `scard` → `0`, `exists` / `expire` / `hset` / `sismember` →
`false`, and the whole-collection reads `lrange` / `smembers` / `hkeys` /
`hgetall` → the empty list `[]`), and `set` is silently a no-op. Your
program never crashes at the boundary. This is what lets the test suite
prove the whole pipeline works — including the `for`-loop over a returned
list — without needing a real Redis server running.

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
- `hgetall` returns a flat `list[str]` (`[field, value, ...]`), not a
  `dict`; a `dict`-returning `hgetall` is a tracked follow-up.

These are tracked follow-ups, not dead ends.

## Attribution

Cobrust's `redis` module is built on the BSD-3-Clause-licensed `redis`
crate (redis-rs). That license is permissive and compatible with
Cobrust; the attribution is recorded in `crates/cobrust-redis/NOTICE`.
