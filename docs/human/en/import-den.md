# `import den` — use a SQLite database from Cobrust

> Status: ADR-0072 first proof. This is the first ecosystem library you
> can `import` from a `.cb` program and actually call end-to-end
> (compile → link → run). It wires `den` (Cobrust's `sqlite3`) onto the
> compiler's intrinsic / C-ABI / static-link chain.

## Example first

```python
import den

fn main() -> i64:
    let conn = den.connect(":memory:")
    let cur = conn.execute("CREATE TABLE t(x INTEGER)")
    let _ = conn.execute("INSERT INTO t VALUES (42)")
    let rows = conn.execute("SELECT x FROM t").fetchall()
    print(rows)        # -> [(42,)]
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
# [(42,)]
```

## What you get (first proof surface)

- **`den.connect(path)`** — open a database. Pass `":memory:"` for an
  in-memory database, or a file path. Returns a `Connection`.
- **`conn.execute(sql)`** — run one SQL statement (CREATE / INSERT /
  SELECT / …). Returns a `Cursor`.
- **`cur.fetchall()`** — return the result rows. In this first proof the
  rows come back rendered as a string the way Python prints them — a
  list of tuples, e.g. `[(42,)]`. (A typed `list[tuple]` result is the
  next step.)

`Connection` and `Cursor` are real, distinct handle types: the compiler
knows `execute` is a `Connection` method and `fetchall` is a `Cursor`
method, and rejects mixing them up — you get a compile error, not a
runtime surprise.

## Why this design?

- **It reuses the proven path.** Calling `den.connect` compiles down to
  the exact same kind of C-ABI call that `print` and `json_loads`
  already use; nothing new at runtime, so it is fast and predictable.
- **Handles clean up automatically.** Each `Connection` / `Cursor` is
  freed exactly once when it goes out of scope — no manual `close()`, no
  leaks, no double-free. The compiler schedules the cleanup for you.
- **Only what you import is linked.** A program that imports `den` links
  `libden.a`; a program that doesn't, doesn't. No bloat.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are a
  separate, not-yet-finished part of the toolchain).
- Keep handles local to the function for now — don't return or store a
  `Connection` / `Cursor` across scopes yet.
- Single-threaded: don't hand a connection to a spawned task.

These are tracked follow-ups, not dead ends — the wiring generalizes to
the rest of the ecosystem libraries (`coil`, `pit`, …) from here.
