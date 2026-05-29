---
doc_kind: module
module_id: mod:pit
crate: cobrust-pit
last_verified_commit: pending
dependencies: [mod:translator]
---

# Module: pit

## Purpose

Cobrust translation of Flask 3.0's sync web-server surface over the
Rust `axum`/`tokio` stack. The v0.7.0 Stream Z.1.a deliverable: the one
MUST-ship HTTP server (roadmap §5). The Cobrust module name is `pit`
(ADR-0071: "a snake pit handles many callers"); the *source* library is
`flask` (kept distinct per the rebrand provenance rule).

Surface-translates the Flask request lifecycle —
`app = Flask(__name__)` → register `@app.route("/users/<id>")` →
`return jsonify(...)` → `app.run(host, port)` — onto an axum router,
keeping the public API SYNC. Python's Flask is itself sync (WSGI);
`App::run` drives axum under a singleton tokio runtime via
`Runtime::block_on` (ADR-0028 §A precedent), so there is no async/sync
coloring at the user layer (constitution §2.2).

LLM-first (constitution §2.5, per ADR-0071 §3): the API SHAPE mirrors
Flask so an LLM agent writes it correctly on the first try
(maximize-overlap-with-training-data), and errors are a closed,
exhaustively-matchable `Result` taxonomy (compile-time-catch-errors).

## Status

- **Z.1.a — delivered.** Flask web-server surface translated via the
  synthetic-LLM pattern (hand-authored to the shape; real-LLM pipeline
  rerun pending — same posture as `mod:strike` / `mod:den`). Backend
  bound to `axum = "0.7"` over `tokio`. The L3 downstream gate spins the
  real axum server on an ephemeral port (`127.0.0.1:0`) and exercises it
  with an in-process `reqwest::blocking` client. The `.cb`-source
  `@pit.route` decorator + `import pit` wiring (so Cobrust source can
  serve) is a deferred follow-on — see Non-goals.

## Public surface (Z.1.a)

```rust
pub type Handler = Arc<dyn Fn(Request) -> Response + Send + Sync>;

/// Max request-body bytes buffered for a handler (16 MiB; B5 hardening).
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct App { /* private: Vec<Route> */ }

impl App {
    pub fn new() -> Self;
    pub fn route<F>(&mut self, method: &str, path: &str, handler: F) -> Result<(), PitError>
        where F: Fn(Request) -> Response + Send + Sync + 'static;
    pub fn get<F>(&mut self, path: &str, handler: F) -> Result<(), PitError> /* + post/put/delete */;
    pub fn run(self, host: &str, port: u16) -> Result<(), PitError>;          // blocking
    pub fn serve_in_background(self, host: &str, port: u16) -> Result<ServerHandle, PitError>;
}

pub struct ServerHandle { /* aborts the server task on Drop */ }
impl ServerHandle { pub fn local_addr(&self) -> SocketAddr; }

#[derive(Clone, Debug)]
pub struct Request { /* private: method/path/path_params/query/headers/body */ }
impl Request {
    pub fn from_parts(method, path, path_params, query, headers, body) -> Self;
    pub fn method(&self) -> &str;
    pub fn path(&self) -> &str;
    pub fn path_param(&self, name: &str) -> Option<&str>;
    pub fn query(&self, name: &str) -> Option<&str>;
    pub fn header(&self, name: &str) -> Option<&str>;   // case-insensitive
    pub fn body(&self) -> &[u8];
    pub fn text(&self) -> Result<String, PitError>;
    pub fn json(&self) -> Result<serde_json::Value, PitError>;
}

#[derive(Clone, Debug)]
pub struct Response { /* private: status/headers/body */ }
impl Response {
    pub fn text(body: impl Into<String>) -> Self;          // 200, text/html
    pub fn json(value: &serde_json::Value) -> Self;        // 200, application/json (== jsonify)
    pub fn from_parts(status: u16, headers, body: Vec<u8>) -> Self;
    pub fn with_status(self, status: u16) -> Self;         // builder
    pub fn with_header(self, name, value) -> Self;         // builder
    pub fn status_code(&self) -> u16;
    pub fn headers(&self) -> &HashMap<String, String>;
    pub fn body(&self) -> &[u8];
}

#[derive(Clone, Debug)]
pub struct PitError { pub kind: PitErrorKind, pub message: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitErrorKind { Bind, DuplicateRoute, InvalidRoute, Runtime }
```

## Scope window (Z.1.a)

In scope:

- `App::new` → `app = Flask(__name__)`.
- Route registration: `route(method, path, handler)` + `get` / `post` /
  `put` / `delete` shorthands. `handler: Fn(Request) -> Response`.
- Flask-style route patterns: literal segments + `<name>` capture
  segments (`/users/<id>`).
- `Request`: method, path, path params (`view_args`), query string
  (`args`), headers (case-insensitive), raw body, `text()`, `json()`.
- `Response`: `text(body)`, `json(value)` (== `jsonify`), explicit
  `from_parts`, `with_status` / `with_header` builders.
- `run(host, port)` — blocking serve over axum + singleton tokio.
- `serve_in_background` — ephemeral-port bind for embedding / tests.
- 404 for an unmatched route; status-code propagation.

Out of scope (deferred):

- The `@app.route` decorator + `import pit` on the `.cb` surface
  (codegen layer).
- Werkzeug converters (`<int:id>`, `<path:p>`), regex rules,
  optional-trailing-slash redirects.
- 405 Method Not Allowed (a known path with a wrong method 404s here).
- Blueprints, before/after-request hooks, sessions, cookies, templates
  (Jinja), static files, streaming responses.
- WSGI/ASGI app protocol, `app.test_client()`.
- The Z.8 REST demo (a `.cb` program using `pit`).

## Invariants

- **No panic on a request.** A bind failure, malformed route, duplicate
  route, or bad body all route to a `PitError` `Result::Err` (for setup)
  or a graceful error response (for a handler fault); the server path
  never panics (constitution §5.1).
- **Closed error taxonomy.** Four `PitErrorKind` variants; opaque
  `Box<dyn Error>` is forbidden (constitution §2.2).
- **Sync surface.** The public API never exposes `Future` / `async fn`
  (constitution §2.2; roadmap §4.1). axum/tokio run under a `block_on`
  bridge.
- **Path-param round-trip is lossless.** A captured `<name>` segment is
  returned byte-for-byte to the handler (fuzz-verified ≥ 450 inputs).

## @py_compat tier

`semantic`. The surface preserves Flask's routing / request / response
SHAPE and observable behaviour for the common REST path, but is not
`strict` byte-for-byte WSGI parity. Documented divergences (also in
`PROVENANCE.toml [verification] divergences`):

- **Method API, not the decorator.** Routes register via
  `app.route(method, path, handler)` / `app.get` / `app.post` / … —
  NOT `@app.route`. The decorator + `import pit` on the `.cb` surface is
  a deferred follow-on (codegen layer), like `mod:den`'s `.cb` wiring.
  Same shape, minus the decorator sugar.
- **Sync-only** (constitution §2.2). `App::run` blocks and drives axum
  under a singleton tokio runtime via `block_on` (ADR-0028 §A). Matches
  Flask's own sync (WSGI) model.
- **Errors are `Result`, not exceptions** (constitution §2.2).
  `PitErrorKind`: `Bind` (≈ `OSError` at `app.run`), `DuplicateRoute`
  (≈ Flask's endpoint-overwrite `AssertionError`), `InvalidRoute`
  (≈ Werkzeug rule-compile error), `Runtime` (internal).
- **Literal + `<name>` capture only** — no converters / regex /
  trailing-slash redirect; `/a/` and `/a` are distinct.
- **Narrowed return coercion.** A bare string → 200 text response; a
  JSON value → `jsonify` response; explicit `from_parts` for everything
  else. Flask's full return protocol (tuples-with-headers, Response
  subclasses, generators) is out of scope.
- **404, not 405, on method mismatch**, and a minimal `404 Not Found`
  body rather than Flask's HTML error page.

## Gates (Z.1.a — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L0 | spec produced | Flask web-server surface shape + ephemeral-port oracle | ✅ |
| L1 | code emitted | every file has provenance header | ✅ |
| L2.build | `cargo build -p cobrust-pit` | zero warnings | ✅ |
| L2.behavior | downstream + fuzz | real axum server on `127.0.0.1:0` exercised by an in-process `reqwest::blocking` client (routing, path params, GET/POST, JSON round-trip, 404, status codes) + seeded fuzz ≥ 450 inputs/fn × 3 seeds | ✅ |
| L2.perf | binding-overhead | surface-translate / Rust-binding tier per ADR-0022 §6 (axum/hyper is the perf reference) | ✅ |
| L3.pyo3 | PyO3-shaped wrapper | `--features pyo3` compiles per ADR-0011 | ✅ |
| L3.dependents | (deferred) | Z.8 REST demo + web-framework dependents widen after `.cb` `@pit.route` wiring | deferred |

## Done means (Z.1.a — DONE)

- [x] `App` (`new` / `route` / `get` / `post` / `put` / `delete` /
      `run` / `serve_in_background`), `Request`, `Response`, `PitError`
      translated.
- [x] Flask `<name>` path-param capture + query + headers + JSON
      request→response round-trip.
- [x] Real ephemeral-port axum server downstream gate: root, path
      params, query, POST echo, JSON sum, 404, method-mismatch 404,
      malformed-JSON 400, JSON content-type.
- [x] Seeded fuzz: ≥ 450 path-param round-trips/fn × 3 seeds + 404
      classification + panic-free dispatch.
- [x] PROVENANCE.toml with `[source] library = "flask"` + axum backend
      note + `@py_compat = semantic` + divergences.
- [x] PyO3 wrapper + `--features pyo3` build path per ADR-0011.

## Done means (deferred — open)

- [ ] `.cb`-source `@pit.route` decorator + `import pit` intrinsic/extern
      wiring (codegen layer, CTO serial follow-on).
- [ ] The Z.8 REST demo (a `.cb` program using `pit`).
- [ ] Werkzeug converters / regex rules / 405 / blueprints / sessions.
- [ ] Downstream web-framework dependents.

## Non-goals

- **Not** a complete Flask implementation — see "Scope window".
- **Not** async on its public surface (constitution §2.2; roadmap
  §4.1). axum/tokio run under a `block_on` bridge; Flask is itself sync.
- **Not** the `.cb`-language surface wiring — Z.1.a stops at the Rust
  crate + PyO3 + tests + docs layer to avoid `crates/cobrust-codegen/`
  cross-sprint contention; the codegen `@pit.route` + `import pit`
  wiring is a deferred serial follow-on.

## ADR-0080 Phase-1b-ii — `route_validated` (type-driven body validation + 422)

`app.route_validated(method, path, handler)` is the type-driven
request-validation route — the FastAPI-defining capability #156, the
elegance-law PRIME target. SIBLING of `route`; the only differences are
the runtime symbol (`__cobrust_pit_app_route_validated`) and a 2-arg
handler shape.

Surface (`.cb`):

```python
class CreateScore:                 # a validated request body = a typed class
    name: str                      # field presence + base-type → compile-time (footgun #1)
    rank: i64 where 0 <= self and self <= 100   # value range → runtime guard (Q3)

fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    return pit.text_response(201, "ok")   # body is ALREADY validated here

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

Mechanism (the layered pipeline):

- **Parse.** A bare class-body field `name: type` (no `let`/`=`) parses to
  a synthetic `let` (so Phase-1a field-tracking records it); an optional
  postfix `where <pred>` is captured per field. `where` is a soft keyword
  (no lexer change) admitted only in the field-annotation position.
- **Side-table (Q2).** `check_class` interprets each `where`-predicate into
  a `(AdtId, field) → Refinement` side-table (`cobrust-types`), NOT into
  `Ty`. Phase-1b-ii admits only the FIXED int-range grammar on an `i64`
  field (`lo <= self`, `self <= hi`, `lo <= self and self <= hi`, `>=`
  mirror, strict `<`/`>` ±1-shifted to inclusive). Anything else →
  `TypeError::UnsupportedRefinement` with a §2.5-B FIX (the compile error).
- **Callback gate (Q5).** The manifest callback `FnTy` is
  `fn(pit.Request, <Body>) -> pit.Response` with a sentinel 2nd-param
  (`PIT_VALIDATED_BODY_SENTINEL_ADT`); `check_callback_arg` accepts any
  field-tracked user class there and rejects a 1-arg handler or a non-class
  2nd param with `CallbackSignatureMismatch` + a FIX.
- **Schema synthesis (MIR).** The retarget injects a 4th `Str` arg — the
  validated-body SCHEMA descriptor (`field<TAB>kind[:lo:hi]` lines)
  synthesised from the body class's field table + refinement side-table on
  `TypedModule` (the SAME source the checker used — footgun #4, cannot
  drift).
- **Codegen.** `__cobrust_pit_app_route_validated(app, method, path,
  handler, schema)` — 5 params, the FIFTH is the schema `Str`.
- **Trampoline + 422 (the core, `cabi.rs`).** Per request the closure
  parses `req.json()`, runs `validation::validate_against_schema` (TOTAL
  boundary deserialization — missing/extra key, wrong type, out-of-range →
  `Err`). On `Ok` it boxes BOTH the Request and the validated
  `serde_json::Value` (both Rust-owned, dual-box, `handle_drop_symbol →
  None`), calls the handler with both raw pointers, frees BOTH exactly once,
  `catch_unwind`s across the C ABI. On `Err(ve)` it synthesises a typed
  **422** `Response` from the `ValidationError` WITHOUT entering the handler
  (footgun #2 — the Result-error path stays in Rust as a Response).

Scope (Phase-1b-ii): the validation + 422 engine ONLY. The OpenAPI emit is
Phase-1b-iii (below — it walks the SAME schema descriptor + side-table this
phase carries). Body re-serialization (`json_response(201, body)`) is the
deferred `.cb`↔serde bridge (ADR-0080 §9); the success handler returns a
fixed response. `len`/`pattern` refinements are Phase-2/3.

## ADR-0080 Phase-1b-iii — `serve_openapi` (OpenAPI emission, cannot drift)

`app.serve_openapi(doc_path: str) -> None` is the EXPLICIT opt-in that
registers a `GET <doc_path>` route serving an OpenAPI 3.1 doc DERIVED from
the validated routes' body-schema descriptors (ADR-0080 §2 Q4, §5.3). The
load-bearing property is footgun #4 (cannot drift): the schema is a second
projection of the ONE source the validator reads.

Surface (`.cb`):

```python
fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _ = app.serve_openapi("/openapi.json")   # EXPLICIT — no magic auto-route
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

Mechanism (the chain, sibling of `route_validated` / `use_cors`):

- **Body name (MIR + types).** `TypedModule.adt_names` (the inverse of the
  checker's `class_names`) lets MIR prepend a `# <BodyName>` header line to
  the schema descriptor `validated_body_schema_for_handler` synthesises. The
  validator skips it for free (no TAB → `parse_schema`'s `split_once('\t')`
  is `None`); the OpenAPI emitter reads it to key
  `components/schemas/<BodyName>`. One descriptor string, both consumers.
- **Manifest.** `(PIT_APP_ADT, "serve_openapi")` →
  `__cobrust_pit_app_serve_openapi`, `[Value(Str)] → None`,
  `PyCompatTier::Semantic`. `Ty::None` return mirrors `route`/`use_cors`'s
  in-place-effect discard (no second drop-eligible App handle).
- **MIR.** No special-case — a plain value-arg method through the generic
  eco-call path (the doc path is the one `Str` arg).
- **Codegen.** `__cobrust_pit_app_serve_openapi(app, path) -> *mut u8 = null`
  (2 ptr args, ptr return — same shape as `request_path_param`).
- **CLI.** Matched by the `__cobrust_pit_*` prefix recognizer for free.
- **App accumulation (`app.rs`).** The `route_validated` trampoline calls
  `App::register_validated_meta(method, path, schema)` (with the SAME schema
  string it hands the validator), pushing a `ValidatedRouteMeta` into the
  App's `validated_routes` (NOT a hidden global — it lives inside the `App`,
  read only by an explicit `serve_openapi`). `App::serve_openapi(doc_path)`
  snapshots `validated_routes` into a `GET` handler closure that returns
  `Response::json(build_openapi_doc(&routes))`.
- **Emitter (`openapi.rs`, the cannot-drift core).** `build_openapi_doc`
  walks each `ValidatedRouteMeta.schema` through
  `validation::parse_schema` — the EXACT same parse the validator
  range-checks — and projects each `FieldSpec` to OpenAPI:
  `str→{type:string}`, `i64→{type:integer}`, `f64→{type:number}`,
  `bool→{type:boolean}`; `FieldSpec.lo→minimum`, `FieldSpec.hi→maximum`.
  The advertised `maximum` IS the `hi` the validator enforces — two
  projections of one `parse_schema`, provably cannot diverge.

Done-means (verified): `GET /openapi.json` → 200 + `components/schemas/CreateScore`
shows `name:{type:string}`, `rank:{type:integer,minimum:0,maximum:100}`;
the cannot-drift cross-check — `POST /scores {"rank":200}` → 422 (validator
rejects, enforcing max 100) AND the doc advertises `maximum:100`, both from
one source.

Scope (Phase-1b-iii): int-range schema bounds only (`minimum`/`maximum`).
`minLength`/`maxLength` + `pattern` follow the validator's Phase-2/3. The
doc is a Rust-assembled JSON string (`Response::json`), not a `.cb`-struct
serialization (the deferred §9 bridge).

## Cross-references

- `mod:strike` — sister ecosystem crate (HTTP-client precedent +
  layout template).
- `mod:den` — sister ecosystem crate (the most recent layout template +
  the F62 `ignore`-doctest precedent).
- `mod:translator` — pipeline that emits ecosystem crates.
- [adr:0011](../adr/0011-pyo3-build-path.md) — PyO3 build path.
- [adr:0022](../adr/0022-translation-ecosystem-batch.md) — ecosystem
  surface-translate methodology.
- [adr:0028](../adr/0028-m13-concurrency-runtime.md) — the
  `block_on` sync↔tokio bridge precedent.
- [adr:0071](../adr/0071-ecosystem-library-cobra-rebrand.md) —
  flask → `pit` rebrand.
- roadmap — `docs/agent/strategy/v0.7.0-network-backend-libraries-roadmap.md`
  §4.1 (flask row) + §5 (MUST-ship HTTP server).
- Flask — https://flask.palletsprojects.com/.
- axum crate — https://crates.io/crates/axum.
