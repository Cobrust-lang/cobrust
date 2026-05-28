# `import pit` — serve HTTP from Cobrust (callback marshalling first proof)

> Status: ADR-0073 first proof. After `den` (SQLite, handle pattern),
> `nest` (TOML, pure value), `strike` (HTTP client, handle + free fn),
> `scale` (msgpack, value), and `molt` (datetime, handle) walked the
> chain through five generalizations, `pit` (Flask, web-server) brings
> the SIXTH module — and the FIRST that crosses a **callback** through
> the C ABI. A `.cb` top-level fn pointer becomes a fn pointer in the
> compiled binary, gets transmuted into a `move |req| -> resp` closure
> in the Rust trampoline, and runs from inside axum.

## Example first

```python
import pit

fn handle_ping(req: pit.Request) -> pit.Response:
    return pit.text_response(200, "pong")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route("GET", "/ping", handle_ping)
    let _server = app.serve_in_background("127.0.0.1", 0)
    # busy-wait keep-alive; the server stays bound until the process exits
    let i: i64 = 0
    while i < 10000000000:
        i = i + 1
    return 0
```

Build and run it, then probe with curl:

```bash
cobrust build prog.cb -o prog
./prog &
# find the ephemeral port and curl it
curl http://127.0.0.1:<port>/ping
# pong
```

## What you get (first proof surface)

- **`pit.App() -> App`** — construct an empty app.
- **`pit.text_response(status, body) -> Response`** — build a canned text response
  with the given status code and body string. Status is clamped to the
  valid HTTP range; out-of-range values yield 500.
- **`App.route(method, path, handler)`** — register a top-level `fn` as
  the handler for `method path`. The handler MUST be a top-level
  `fn handler(req: pit.Request) -> pit.Response: …`. Returns `None`;
  the canonical form is `let _ = app.route(...)`.
- **`App.serve_in_background(host, port) -> ServerHandle`** — bind on
  `host:port` (port `0` = ephemeral), spawn the axum server onto pit's
  internal tokio runtime, return a `ServerHandle`. The handle's drop
  aborts the server task. `pit.Request` accessors (path/method/body)
  are a paired follow-up; today the handler can ignore the Request and
  emit a canned Response.

## Why this design?

- **One callback ABI shape**: every handler crosses as
  `extern "C" fn(*mut u8) -> *mut u8`. The .cb codegen materialises the
  handler's fn pointer via the `function_ids` table; the trampoline
  transmutes it back. ADR-0073 §2 D4.
- **Compile-time-catch callback shape (§2.5 binding)**: the typechecker
  rejects everything but a top-level `fn` NAME — no lambdas, no
  fn-typed locals, no call-results, no parenthesized forms. The diagnostic
  prints the fix the LLM should apply (Direction B).
- **Abort-on-panic across the C boundary**: a panic in the .cb handler
  unwinds into Rust which would be UB. The trampoline wraps the
  callback in `catch_unwind` and aborts on panic, with a structured
  stderr message (ADR-0073 §3 Q5).
- **Drop discipline (§2 D6)**: the `Request` handle is Rust-owned (the
  trampoline allocates+frees the box around each callback invocation);
  the `.cb` source never drops a `pit.Request` local. The `Response`
  handle returned by `text_response` flows through `Terminator::Return`
  which the MIR drop pass treats as moved-out — no double-free.

## Today's limits

- **No closures / no lambdas as handlers**: must be a top-level `fn`.
- **No decorator sugar**: `@app.route("/x")` is ADR-0074 (next sprint).
- **`pit.Request` accessors not yet wired**: the handler must construct
  the Response without reading the Request's path/method/body. A paired
  follow-up adds the borrow shims.
- **Single-threaded handlers**: axum dispatches concurrently, but each
  handler invocation is one tokio task; the .cb handler must be
  re-entrant under Send + Sync (which it is by construction — extern
  fn pointers are unconditionally Send + Sync + Copy).
