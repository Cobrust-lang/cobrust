# `import strike` — make HTTP calls from Cobrust

> Status: ADR-0072 third-module proof. After `den` (SQLite) showed the
> ecosystem-import chain end-to-end and `nest` (TOML) showed it
> generalizes to a pure value-in-value-out function, `strike` (HTTP,
> the rebrand of `requests`) shows the chain handles a SECOND
> handle-pattern module — with its own `Response` type, its own drop
> symbol, and its own borrow-don't-move method surface — without
> touching any chain logic.

## Example first

```python
import strike

fn main() -> i64:
    let resp = strike.get("http://127.0.0.1:8080/ping")
    let body: str = resp.text()
    let code: i64 = resp.status_code()
    print(body)
    print(code)
    return 0
```

Build and run it (against any HTTP server listening on the URL):

```bash
cobrust build prog.cb -o prog
./prog
# pong
# 200
```

## What you get (third-module proof surface)

- **`strike.get(url) -> Response`** — issue an HTTP `GET` to `url`,
  returning an owned `Response` handle.
- **`strike.post(url, body) -> Response`** — same for `POST`, with the
  request body as a string.
- **`Response.text() -> str`** — read the response body as a UTF-8
  string (non-UTF-8 bytes are lossy-replaced).
- **`Response.status_code() -> i64`** — read the HTTP status code
  (`200`, `404`, etc).
- **`Response.json() -> str`** — read the response body parsed as
  JSON and re-rendered as canonical compact JSON. Same shape as
  `den.fetchall() -> str` for the first proof; a structured-value
  surface is a tracked follow-up.

A `Response` handle is owned by the `let`-binding it lands in; the
compiler frees it exactly once at scope exit. You don't write any
`del` / `close` / `free` — the drop schedule does it for you.

## What happens when the network breaks?

The HTTP surface **never panics, never returns null**. On any network
failure, invalid URL, or DNS hiccup, you still get a `Response` —
just one whose `status_code()` is `0` and whose `text()` is empty.
The idiomatic check is:

```python
let resp = strike.get(some_url)
if resp.status_code() == 0:
    print("network unreachable")
else:
    print(resp.text())
```

`json()` on a body that isn't actually JSON returns the empty-object
sentinel `{}`. Same convention as the rest of Cobrust's runtime — fail
cleanly, never panic across the C-ABI boundary.

## Why this design?

- **It proves the chain handles a SECOND handle-pattern module.** `den`
  was the first handle module; `strike` is the second. The wiring
  reused every layer the den/nest proofs landed — manifest, type-check,
  MIR retarget, codegen extern, drop schedule, link locator — without
  changes. Only data was added.
- **Per-module 256-slot AdtId block.** `den` reserves
  `0xE000_0000..0xE000_00FF`; `strike` reserves
  `0xE000_0100..0xE000_01FF`. Each new handle-typed module gets its
  own block. Handles never collide across modules, and there's room
  for ~256 handles per module before bumping the convention.
- **Borrowing methods, not consuming methods.** `resp.text()`,
  `resp.status_code()`, and `resp.json()` BORROW the handle (the same
  way `cur.fetchall()` does in `den`). The runtime never takes the
  Response away from you — you can call `status_code()` and then
  `text()` and then `status_code()` again, all on the same `resp`.
- **Only what you import is linked.** A program that imports `strike`
  links `libstrike.a`; a program that doesn't, doesn't. No bloat.

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are
  a separate, not-yet-finished part of the toolchain).
- The `json()` accessor returns a canonical JSON string today, not a
  typed Cobrust value tree. Downstream code parses it back with any
  JSON consumer — same shape as `den.fetchall()` returns row text.
- Source-level explicit type annotations for the `Response` handle
  (`let resp: strike.Response = ...`) don't yet route through the
  ecosystem manifest. Drop the annotation and let inference do the
  work, as the example above does. Tracked as a follow-up.
- The error path is the `status_code() == 0` sentinel; a typed
  `Result[Response, HttpError]` surface is a tracked follow-up.
- The `Response` handle is scope-local (no return / store / capture
  escape). Single-threaded only. The Cobrust structured-concurrency
  runtime arrives at M8+; until then `strike` is sync-only.

These are tracked follow-ups, not dead ends — the wiring generalizes
to the rest of the ecosystem libraries from here.
