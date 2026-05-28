# `import molt` — read the current time and format it from Cobrust

> Status: ADR-0072 fifth-module proof. After `den` (SQLite),
> `nest` (TOML), `strike` (HTTP), and `scale` (msgpack), `molt`
> (datetime, the rebrand of `python-dateutil`) is the FIFTH module
> wired through the same ecosystem-import chain — a handle-pattern
> generalization on top of den's and strike's, with a `DateTime`
> handle and borrowing accessors.

## Example first

```python
import molt

fn main() -> i64:
    let now = molt.now()
    let iso: str = now.isoformat()
    let stamp: i64 = now.unix_timestamp()
    print(iso)
    print(stamp)
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
# 2026-05-28T12:34:56.789012Z
# 1748434496
```

(The exact values depend on the wall-clock when you run it.)

## What you get (fifth-module proof surface)

- **`molt.now() -> DateTime`** — capture the current UTC time and
  return an owned `DateTime` handle.
- **`DateTime.isoformat() -> str`** — render the datetime as an
  RFC3339 string (the ISO-8601 subset Python `datetime.isoformat()`
  produces for UTC-aware datetimes).
- **`DateTime.unix_timestamp() -> i64`** — read the UNIX epoch
  seconds (UTC). Same semantic as Python `int(dt.timestamp())` for a
  UTC-aware datetime.

A `DateTime` handle is owned by the `let`-binding it lands in; the
compiler frees it exactly once at scope exit. You don't write any
`del` / `close` / `free` — the drop schedule does it for you.

## What happens when something goes wrong?

Both accessors **never panic, never return null**. On a null handle
`isoformat()` returns an empty string and `unix_timestamp()` returns
`0`. `molt.now()` itself is a total operation on every supported
platform.

This matches Cobrust's runtime convention — fail cleanly, never panic
across the C-ABI boundary.

## Why this design?

- **It proves the chain handles a THIRD handle-pattern module.** `den`
  was the first handle module, `strike` the second, `molt` the third.
  The wiring reused every layer the previous proofs landed — manifest,
  type-check, MIR retarget, codegen extern, drop schedule, link
  locator — without changes. Only data was added.
- **Per-module 256-slot AdtId block.** `den` reserves
  `0xE000_0000..0xE000_00FF`; `strike` reserves
  `0xE000_0100..0xE000_01FF`; `scale` reserves
  `0xE000_0200..0xE000_02FF` (no handles yet, but the block is theirs
  if a future raw-bytes ABI needs one); `molt` reserves
  `0xE000_0300..0xE000_03FF` for `DateTime`. Each new handle-typed
  module gets its own block — handles never collide across modules.
- **Borrowing methods.** `isoformat()` and `unix_timestamp()` BORROW
  the handle (the same way `cur.fetchall()` does in `den`, and
  `resp.text()` does in `strike`). You can call them as many times as
  you like on the same `now` binding; the handle survives until scope
  exit.
- **Only what you import is linked.** A program that imports `molt`
  links `libmolt.a`; a program that doesn't, doesn't. No bloat.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are
  a separate, not-yet-finished part of the toolchain).
- The first proof exposes only `now()` + `isoformat()` +
  `unix_timestamp()`. A `parse(s: str) -> DateTime` constructor and
  the full `python-dateutil` parser surface are tracked follow-ups.
- Source-level explicit type annotations for the `DateTime` handle
  (`let now: molt.DateTime = ...`) don't yet route through the
  ecosystem manifest. Drop the annotation and let inference do the
  work, as the example above does. Tracked as a follow-up to ADR-0072.
- The error path is the empty-string / `0` sentinel on null; a typed
  `Result[DateTime, MoltError]` surface is a tracked follow-up.
- The `DateTime` handle is scope-local (no return / store / capture
  escape). Single-threaded only. The Cobrust structured-concurrency
  runtime arrives at M8+; until then `molt` is sync-only.

These are tracked follow-ups, not dead ends — the wiring generalizes
to the rest of the ecosystem libraries from here.
