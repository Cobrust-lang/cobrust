# cobrust-pit — a Flask-shaped web server for Cobrust

`cobrust-pit` is the Cobrust translation of Python's **Flask** web-server
surface. It gives you the familiar shape — make an app, register routes,
return text or JSON, run the server — backed by the mature Rust `axum`
stack on top of `tokio`. The public API is **synchronous**: you never
write `async`, and `app.run(...)` simply blocks until the process is
killed, exactly like Flask.

The Cobrust name is `pit` (per ADR-0071: "a snake pit handles many
callers"); the library it translates is Flask. It is the v0.7.0
"MUST-ship" HTTP server (Stream Z.1.a).

## Example first

A tiny REST app — a root page, a path parameter, and a JSON endpoint:

```rust
use pit::{App, Request, Response};
use serde_json::json;

let mut app = App::new();                       // == Flask(__name__)

// A plain-text route.
app.get("/", |_req: Request| Response::text("hello, pit"))?;

// A path parameter, captured with Flask's <name> syntax.
app.get("/users/<id>", |req: Request| {
    let id = req.path_param("id").unwrap_or("?");
    Response::json(&json!({ "id": id }))        // == jsonify(...)
})?;

// A POST that reads the request body.
app.post("/echo", |req: Request| {
    Response::text(req.text().unwrap_or_default())
})?;

// Block and serve (port 0 = pick a free port via serve_in_background).
app.run("127.0.0.1", 8080)?;
```

The shape matches what you would write in Python with Flask:

```python
from flask import Flask, jsonify, request
app = Flask(__name__)

@app.route("/")
def root():
    return "hello, pit"

@app.route("/users/<id>")
def user(id):
    return jsonify({"id": id})

@app.route("/echo", methods=["POST"])
def echo():
    return request.get_data(as_text=True)

app.run("127.0.0.1", 8080)
```

The one visible difference in this first cut: you register routes with a
**method call** (`app.get(path, handler)`) instead of the `@app.route`
decorator. The decorator lands later, with the Cobrust-source wiring.

## What you get

- **`App`** — `App::new()`, then `app.route(method, path, handler)` or
  the `app.get / post / put / delete(path, handler)` shorthands, and
  `app.run(host, port)` to serve (blocking). `serve_in_background` binds
  on an ephemeral port and serves in the background (used by tests).
- **`Request`** — what each handler receives: `.method()`, `.path()`,
  `.path_param(name)` (the `<name>` captures), `.query(name)` (the query
  string), `.header(name)` (case-insensitive), `.body()`, `.text()`,
  and `.json()`.
- **`Response`** — what a handler returns: `Response::text(body)` (200,
  `text/html`), `Response::json(value)` (200, `application/json` — this
  is `jsonify`), plus `.with_status(code)` and `.with_header(k, v)`
  builders.

## Routing

Routes are matched segment by segment. Two kinds of segment:

- **Literal** — `/users/list` matches only that path.
- **Capture** — `/users/<id>` matches `/users/42` and hands the handler
  `id = "42"` via `req.path_param("id")`.

An unmatched path returns **404**. A path registered for `GET` but
requested with `POST` also returns 404 in this first cut (Flask returns
405 — that refinement is deferred).

## Errors are values, not exceptions

Flask raises Python exceptions (an `OSError` if the port is taken, an
`AssertionError` if you register the same route twice). Cobrust returns a
`Result<T, PitError>` instead — you handle failure with `?` or a `match`,
and the compiler makes sure you do not forget. The error kinds:

| `PitErrorKind` | When | Flask equivalent |
|---|---|---|
| `Bind` | the listen socket cannot be bound | `OSError` at `app.run` |
| `DuplicateRoute` | the same `(method, path)` registered twice | endpoint-overwrite `AssertionError` |
| `InvalidRoute` | a malformed path (no leading `/`, unclosed `<...>`) | Werkzeug rule error |
| `Runtime` | the server task failed / a bad body | (internal) |

Registering the same route twice never panics — it returns `Err`:

```rust
let mut app = App::new();
app.get("/x", |_r| Response::text("a"))?;
let err = app.get("/x", |_r| Response::text("b")).unwrap_err();
assert_eq!(err.kind, pit::PitErrorKind::DuplicateRoute);
```

## Why this design?

- **Match Python's priors.** The constitution's LLM-first principle
  (§2.5) says Cobrust is the language an AI agent writes correctly on the
  first try. `@app.route("/path")` + `return jsonify(...)` is the
  most-trained sync web-server pattern in the Python corpus, so we keep
  the shape (only the name changes, per ADR-0071).
- **Sync, no async colouring.** Flask is a synchronous (WSGI) framework.
  Cobrust forbids the two-colour async/sync problem (§2.2), so the
  `pit` surface stays sync: `app.run(...)` drives the `axum` server under
  the hood on a `tokio` runtime via a `block_on` bridge — you never see a
  `Future`.
- **`Result`, never exceptions.** The constitution (§2.2) makes
  `Result<T, E>` the default error path. A closed `PitErrorKind` enum
  means a `match` over the failure modes is exhaustive — the type checker
  catches the case you forgot.

## Compatibility tier: `semantic`

`cobrust-pit` is tagged `@py_compat(semantic)`. It preserves Flask's
routing / request / response **shape** and observable behaviour for the
common REST path, but it is not byte-for-byte identical to Flask. The
known differences:

- Routes register with a **method call**, not the `@app.route`
  decorator (the decorator lands with the Cobrust-source wiring).
- The surface is **sync only** (matching Flask's own WSGI model).
- Errors are `Result::Err`, not raised exceptions.
- Route patterns support **literal + `<name>` capture** segments only —
  no `<int:id>` converters, regex rules, or trailing-slash redirects.
- Handler returns are a **string** (text), a **JSON value** (`jsonify`),
  or an explicit `(status, headers, body)` — not Flask's full
  return-value protocol.
- An unknown method on a known path returns 404, not 405.

## Not yet supported

- The `@pit.route` decorator and `import pit` from Cobrust `.cb` source —
  that wiring is a separate, deferred step (and the Z.8 REST demo built
  on it).
- Werkzeug converters (`<int:id>`, `<path:p>`), regex rules, 405
  responses.
- Blueprints, before/after-request hooks, sessions, cookies, Jinja
  templates, static files, streaming responses.
- The WSGI/ASGI app protocol and `app.test_client()`.
