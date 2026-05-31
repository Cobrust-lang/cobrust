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
- **`pit.json_response(status, body) -> Response`** (ADR-0081) — re-serialise
  a **validated request body** to a JSON response with the given status. The
  `body` is the typed body parameter your `route_validated` handler received;
  the response carries it verbatim as `application/json`. Because it
  re-serialises the SAME value validation produced, the response body cannot
  drift from the validated body. See "Validated request bodies" below.
- **`body.field` reads** (ADR-0081) — inside a `route_validated` handler,
  read the validated body's fields by typed attribute access (`body.rank` →
  `i64`, `body.name` → `str`), not a stringly-typed key. A typo'd field is a
  compile error. See "Reading body fields" below.
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

## Middleware (ADR-0078 Phase 1)

Enable a canned middleware preset by calling a method on `app` **before**
you serve. Each is `tower-http`'s ready-made `Layer`, registered on the
axum router:

```python
import pit

fn handle_root(req: pit.Request) -> pit.Response:
    return pit.text_response(200, "hello")

fn main() -> i64:
    let app = pit.App()
    let _ = app.use_cors()         # CORS — adds Access-Control-Allow-Origin
    let _ = app.use_trace()        # request tracing/logging (side-effect)
    let _ = app.use_compression()  # gzip/br/deflate/zstd response compression
    let _ = app.route("GET", "/", handle_root)
    let _server = app.serve_in_background("127.0.0.1", 0)
    let i: i64 = 0
    while i < 10000000000:
        i = i + 1
    return 0
```

- **`app.use_cors()`** — applies `CorsLayer::permissive()`; served
  responses carry `Access-Control-Allow-Origin`. The FastAPI/Flask-CORS
  shape (`app.add_middleware(CORSMiddleware, …)` / `CORS(app)`).
- **`app.use_trace()`** — applies `TraceLayer::new_for_http()`; emits
  tracing spans/events (a logging side-effect, not an HTTP header).
- **`app.use_compression()`** — applies `CompressionLayer`; compresses
  the response body when the client negotiates an accepted encoding,
  passes it through untouched otherwise.

All three return `None` (use the `let _ = …` form) and **must be called
before** `serve_in_background` / `run`: the flag is read once, when the
server builds its router. A call afterward is a no-op.

## Validated request bodies (`route_validated`, ADR-0080)

`app.route_validated(method, path, handler)` is FastAPI's defining
feature done the Cobrust way: **the request body is a typed `class`, and
the type IS the contract.** Field presence and field type are checked at
compile time; value-level constraints (a range) are checked once at the
request boundary and rendered to a typed **422** — never a thrown
exception, never an in-handler re-check.

```python
import pit

# A validated body is a `class` whose fields are typed. An optional
# `where`-clause adds a value constraint (here, an inclusive int range).
class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100

# The handler takes the body as a TYPED second parameter. pit validates
# the JSON body into it BEFORE the handler runs — so reaching the body
# means validation passed. `body.rank` is statically `i64`; a typo'd
# `body.nonexistent` is a COMPILE-TIME error, not a runtime KeyError.
fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    return pit.json_response(201, body)   # echo the validated body as JSON

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

What happens at the boundary:

- `POST /scores {"name":"a","rank":50}` → **201** with body
  `{"name":"a","rank":50}` (the validated body, re-serialised by
  `json_response`), the handler runs.
- `{"name":"a","rank":200}` → **422** (rank out of range), the handler is
  **never entered**.
- `{"rank":50}` (missing `name`) or `{"name":"a","rank":"x"}` (wrong type)
  → **422** — the body must match the declared shape EXACTLY (every
  declared field present, the right type, no extra keys).

The `where`-clause grammar is a small set of fixed forms, keyed on the
field's type:

- **int range** on an `i64` field: `0 <= self`, `self <= 100`, or
  `0 <= self and self <= 100` (`self` is the field's value; `>=` works too);
- **string length** on a `str` field: `len(self) <= 20`, `len(self) >= 1`,
  or `1 <= len(self) and len(self) <= 20` (see the next section);
- **string pattern** on a `str` field: `pattern(self, "<regex>")` (see the
  next section).

Any other predicate — or a length/pattern form on the wrong field type — is
a compile error that tells you the accepted forms.

Why this is better than Flask/FastAPI: the structure is caught by the
compiler (you cannot ship a handler that reads a field that isn't there),
the 422 is a `Result` rendered to a `Response` (not an exception that
unwinds), and the wiring is an explicit call (no hidden dependency-injection
registry).

### Reading body fields (`body.rank`, ADR-0081)

Inside a `route_validated` handler you can now **read the body's fields**
and act on them. `body.rank` reads an `i64`, `body.name` reads a `str` —
typed attribute access, never a stringly-typed `body["rank"]`:

```python
import pit

class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100

fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    let r: i64 = body.rank        # reads the validated rank, e.g. 50
    let n: str = body.name        # reads the validated name, e.g. "alice"
    if r >= 50:
        return pit.text_response(200, "high")
    return pit.json_response(201, body)   # or echo the whole body back

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

- `body.rank` is statically `i64`; a typo'd `body.nonexistent` is a
  **compile-time** error (it lists the real fields), never a runtime
  `KeyError`.
- The read is **total**: validation already proved the field is present,
  the right type, and in range before your handler ran — so there is no
  `None` to unwrap, no missing-key surprise.
- No silent coercion: an `i64` field reads as an integer; a JSON `1.5`
  for an `i64` field was already rejected at the 422 boundary, so the read
  never truncates a float.

Phase-1 ships `i64` + `str` field reads; `f64`/`bool` and nested-class /
list fields are later phases. Field reads work **only** on a body parameter
your handler received from `route_validated` — a hand-constructed
`CreateScore()` value does not yet carry field storage (that is the
native-struct follow-up). The compiler tracks which is which, so you never
get a surprise.

## String refinements: length + pattern (ADR-0080 Phase 2)

A `str` field can carry two more kinds of `where`-constraint — a **length
bound** and a **regex pattern**:

```python
import pit

class SignupBody:
    # Length bound: 1..=20 characters (inclusive). `len(self)` is the
    # field's length; the one-sided `len(self) <= 20` / `len(self) >= 1`
    # forms work too.
    username: str where 1 <= len(self) and len(self) <= 20
    # Pattern: the value must match this regex (a literal string).
    email: str where pattern(self, ".+@.+")

fn signup(req: pit.Request, body: SignupBody) -> pit.Response:
    return pit.text_response(201, "created")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/signup", signup)
    let _ = app.serve_openapi("/openapi.json")
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

At the boundary:

- `{"username":"bob","email":"b@x.com"}` → **201**, the handler runs.
- a 21-character username → **422** (over the max of 20), handler **not
  entered**.
- an empty username → **422** (under the min of 1).
- `"email":"notanemail"` → **422** (fails the `.+@.+` pattern).

Two notes that follow from the elegance-law:

- **A bad regex is a compile error, not a runtime surprise.** If you write
  `pattern(self, "[")` (an unclosed character class), the compiler rejects
  it with the fix — you never ship a server that panics on every request.
- **The OpenAPI schema stays in lockstep.** A length bound shows up as
  `minLength`/`maxLength`, a pattern as `pattern` — read from the same
  source the validator checks (see the next section), so they cannot drift.

## Float value-range refinement (ADR-0080 Phase 3a)

An `f64` field can carry a `where` **value-range** constraint — the exact
**mirror** of the integer range (`i64 where 0 <= self and self <= 100`) on an
`f64` field:

```python
import pit

class Reading:
    name: str
    # Inclusive value range: 0.0 ..= 1.0. The two-sided and one-sided
    # (`0.5 <= self` / `self <= 100.0`) forms both work. An integer literal
    # is accepted as a float bound (`0 <= self` means `0.0` — the natural
    # spelling).
    ratio: f64 where 0.0 <= self and self <= 1.0

fn submit(req: pit.Request, body: Reading) -> pit.Response:
    return pit.text_response(201, "created")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/readings", submit)
    let _ = app.serve_openapi("/openapi.json")
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

At the boundary:

- `{"name":"a","ratio":0.5}` → **201**, the handler runs; an integer `ratio:1`
  → **201** too (an integer is a valid f64).
- `ratio:1.5` (over the max of 1.0) and `ratio:-0.5` (under the min of 0.0) →
  **422**, handler **not entered**; `ratio:"x"` (not a number) → **422** (wrong
  type).

Three notes that follow from the elegance-law:

- **Only inclusive `<=`/`>=` are accepted.** A strict `<`/`>` bound is a
  **compile error** with the fix: the integer grammar rewrites `S < N` to
  `<= N-1`, but the reals are dense, so a float strict bound has no clean `±1`
  inclusive rewrite — rather than silently inventing an epsilon (a footgun), the
  fix steers you to the inclusive spelling.
- **The 422 error prints the fix** (§2.5):
  ``field `ratio` value 1.5 must be in [0, 1]`` — not just a diagnosis, but the
  accepted range.
- **The OpenAPI schema stays in lockstep.** A float range shows up as
  `{type:number, minimum, maximum}` — read from the same source the validator
  checks, so they cannot drift.

## Auto OpenAPI (`serve_openapi`, ADR-0080 Phase-1b-iii)

FastAPI's other defining feature is the free `/docs` — an OpenAPI schema
derived from your model. Cobrust does the same, with one key property: the
schema is derived from **the same source the validator reads**, so it
**cannot drift** from what the server actually enforces.

```python
fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    # Explicitly opt in to serving the OpenAPI doc. NOT a magic auto-route:
    # you write this line, so doc-serving is visible at the call site.
    let _ = app.serve_openapi("/openapi.json")
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

`GET /openapi.json` then returns an OpenAPI 3.1 document. For the
`CreateScore` body above:

```json
{
  "openapi": "3.1.0",
  "components": {
    "schemas": {
      "CreateScore": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "rank": { "type": "integer", "minimum": 0, "maximum": 100 }
        }
      }
    }
  }
}
```

The `rank.maximum` of `100` is the EXACT same bound the validator enforces
(it rejects `rank: 200` with a 422) — both are read from one field table +
refinement side-table. There is no second, hand-kept schema declaration to
fall out of sync (the utoipa/drf-spectacular drift footgun, dropped).

`serve_openapi` is an **explicit opt-in** (the elegance-law: no import-time
side effect, no hidden global). Call it AFTER the `route_validated`
registrations it should document. The mapping:
`str → {type:string}`, `i64 → {type:integer}`, `f64 → {type:number}`,
`bool → {type:boolean}`; an int-range refinement adds `minimum`/`maximum`, a
str-length refinement adds `minLength`/`maxLength`, and a pattern adds
`pattern`. For the `SignupBody` above the doc shows
`username: {type:string, minLength:1, maxLength:20}` and
`email: {type:string, pattern:".+@.+"}` — the same bounds the validator
enforces. The array-length `maxItems` form for list fields is a later phase.

## Nested object bodies (ADR-0080 Phase 4, #156)

A body field can be **another validated class** — the pydantic nested-model
idiom. pit validates the nested object **recursively** and the OpenAPI doc
references it with a `$ref`, all from the same single source (so the nested
bounds cannot drift either).

```python
class Address:                        # the nested validated class
    city: str
    zip: i64 where 0 <= self and self <= 99999

class CreateUser:                     # the root: a flat field + a nested-class field
    name: str
    address: Address

fn create_user(req: pit.Request, body: CreateUser) -> pit.Response:
    # Reached only if BOTH the root fields AND the nested address validated.
    return pit.text_response(201, "ok")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/users", create_user)
    let _ = app.serve_openapi("/openapi.json")
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

What you get:

- **The nested object is validated recursively.** Each of these is a `422`
  (and the handler is never entered):
  - `{"name":"a","address":{"city":"NYC","zip":100000}}` — nested `zip` over
    its `99999` bound (the nested refinement is enforced too);
  - `{"name":"a","address":{"zip":10001}}` — nested object missing `city`;
  - `{"name":"a","address":{"city":"NYC","zip":"x"}}` — nested `zip` wrong type;
  - `{"name":"a","address":"oops"}` — `address` is not an object at all
    (``field `address` must be of type object``);
  - `{"name":"a","address":{"city":"NYC","zip":10001,"country":"US"}}` — a
    nested **extra** key (the same unknown-key rejection applies at every level).
  Only `{"name":"a","address":{"city":"NYC","zip":10001}}` returns `201`.
- **Nesting goes as deep as your model.** A class field that is a class whose
  field is a class … validates all the way down (a defensive depth cap of 64
  levels guards a pathological cyclic model with a clear error).
- **The OpenAPI doc uses a `$ref` + a separate component.** `GET /openapi.json`
  shows the root with the nested field as a reference, plus a component for the
  nested class — each carrying the same bounds the validator enforces:

```json
{
  "components": {
    "schemas": {
      "CreateUser": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "address": { "$ref": "#/components/schemas/Address" }
        }
      },
      "Address": {
        "type": "object",
        "properties": {
          "city": { "type": "string" },
          "zip": { "type": "integer", "minimum": 0, "maximum": 99999 }
        }
      }
    }
  }
}
```

The nested `Address.zip.maximum` of `99999` is the **exact** bound the recursive
validator enforces (it rejects a nested `zip: 100000`) — both read from one
source, so the nested schema cannot drift either. Scope today is nested
**objects**; `list[T]` fields and optional/`None` nested fields are later phases.

## Full worked example

`examples/fastapi_real_demo/` is a complete, runnable REST API that exercises
the whole validated-body surface **together**: one `class CreateUser` carrying
all three refinement kinds (int range on `age`, string length on `name`,
string pattern on `email`), a handler that **reads** `body.age` and branches on
it (201 for adults, 403 for minors), `json_response` echoing the validated
body, and `serve_openapi` deriving the schema. See its `README.md` for the
endpoints, the curl session, and the live E2E
(`crates/cobrust-cli/tests/fastapi_real_demo_e2e.rs`).

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
- **Middleware is canned presets only** (ADR-0078 Phase 1):
  `use_cors()`/`use_trace()`/`use_compression()` take no arguments.
  Configurable CORS origins and custom `.cb` middleware are ADR-0078
  Phases 2/3. (Auto OpenAPI now ships — see `serve_openapi` above.)
- **Validated bodies** (`route_validated`, ADR-0080): the fixed int-range
  refinement on an `i64` field plus the string-length (`len(self)`) and
  pattern (`pattern(self, "…")`) refinements on a `str` field ship now.
  Echoing the validated body back in the response (`json_response(status,
  body)`) and reading individual `i64` / `str` fields off the body
  (`body.rank`, `body.name`) now ship too (ADR-0081). **Nested-class bodies**
  (a field typed as another validated class, recursively validated, one or more
  levels deep) now ship too (ADR-0080 Phase 4, #156 — see "Nested object
  bodies" above). `f64` / `bool` field reads and `list[T]` / optional nested
  fields are later phases.
- **OpenAPI** (`serve_openapi`, ADR-0080): the doc covers the body schema of
  each validated route — type plus int-range `minimum`/`maximum`,
  str-length `minLength`/`maxLength`, and `pattern`. The list-field
  `maxItems` form follows the validator's later phase; the served doc is a
  Rust-assembled JSON string (not yet a `.cb`-struct serialization).
- **`pit.Request` accessors not yet wired**: the handler must construct
  the Response without reading the Request's path/method/body. A paired
  follow-up adds the borrow shims.
- **Single-threaded handlers**: axum dispatches concurrently, but each
  handler invocation is one tokio task; the .cb handler must be
  re-entrant under Send + Sync (which it is by construction — extern
  fn pointers are unconditionally Send + Sync + Copy).
