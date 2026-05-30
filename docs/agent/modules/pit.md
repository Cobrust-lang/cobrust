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
phase carries). Body re-serialization (`json_response(201, body)`) lands in
ADR-0081 Phase-1a (below). `len`/`pattern` refinements land in Phase-2 (below).

## ADR-0081 Phase-1a — `json_response` (re-serialise the validated body)

`pit.json_response(status, body) -> Response` is the SIBLING of
`text_response` — the only delta is the 2nd param. Instead of a `Str`
body it takes the VALIDATED-BODY class the `route_validated` handler holds,
re-serialising it to a JSON response. This is the body PASS-THROUGH the
Phase-1b-ii harness explicitly deferred (the §6 Phase-1 handler now
round-trips). NO field reads, NO dispatch gate, NO object-model change —
`json_response` takes the WHOLE body, not a field (the field-READ work is
ADR-0081 §5.2, a separate increment).

Surface (`.cb`):

```python
fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    return pit.json_response(201, body)   # re-serialises the validated body
```

Mechanism (the layered pipeline — every layer a sibling of `text_response`):

- **Manifest (`ecosystem.rs`).** `("pit", "json_response")` →
  `__cobrust_pit_json_response`, params `[Ty::Int, Ty::Adt(PIT_VALIDATED_BODY_SENTINEL_ADT)]`,
  ret `pit.Response`, `PyCompatTier::Semantic`. The 2nd param is the SAME
  sentinel `route_validated`'s callback body slot uses — the manifest cannot
  name the user's body class.
- **Checker (`check.rs`, `check_eco_sig` `EcoParam::Value` arm).** When the
  expected param is the `PIT_VALIDATED_BODY_SENTINEL_ADT`, the arm accepts any
  field-tracked user `Ty::Adt` (id outside the handle range AND in
  `adt_fields`) — the SAME `is_tracked_body` rule `check_callback_arg` uses.
  So `json_response(201, body)` type-checks where `body: CreateScore` is the
  handler's tracked-body param; a non-class body arg falls through to the
  normal `unify_call_arg` (which fails against the sentinel id).
- **MIR (`lower.rs`).** `json_response(...)` is a NORMAL free-fn call (Case 1
  in `try_lower_ecosystem_call`) — NO new mechanism. It passes the status
  `i64` + the body local's `*mut u8` (the body, a non-handle user `Ty::Adt`,
  is NOT borrow-upgraded, but it carries no drop schedule, so Move vs Copy is
  immaterial — the trampoline owns the box).
- **Codegen (`llvm_backend.rs`).** `__cobrust_pit_json_response` declared with
  the IDENTICAL `[i64, ptr] -> ptr` shape as `text_response`.
- **CLI prefix (`intrinsics.rs`).** `__cobrust_pit_json_response` matches the
  existing `__cobrust_pit_*` arm for free.
- **Trampoline + cabi shim (`cabi.rs`).** `__cobrust_pit_json_response(status,
  body)` reads the body `*mut u8` as `&serde_json::Value` (the box the
  `route_validated` trampoline owns), builds `Response::json(&*body)`
  (content-type `application/json` + `serde_json::to_vec`) `.with_status(status)`.

Ownership (no double-free, no leak, no use-after-free): `json_response`
**BORROWS** the body box — `Response::json` copies the bytes into an owned
`Vec<u8>` (`response.rs:50`), so the box is never moved-from or freed by the
shim. The `route_validated` trampoline retains sole ownership and frees the
box exactly once as a `serde_json::Value` AFTER the handler returns
(`cabi.rs` ~479). The returned `Response` box is reclaimed once by the
trampoline (`cabi.rs` ~494), the same discipline `text_response` follows.
`catch_unwind` across the C ABI is preserved (the handler invocation is
unchanged). Footgun #4 dropped: re-serialising the SAME validated Value means
the response body cannot drift from the validated body.

## ADR-0081 Phase-1b — `body.field` RUNTIME READ (the registration-gated serde accessor)

`body.field` — where `body` is a `route_validated`-registered handler's
validated-body param — now EXECUTES at runtime: it reads the field off the
boxed `serde_json::Value` the validator left, via a TYPED accessor shim keyed
on the field's declared `Ty`. At HEAD (Phase-1a) `body.field` type-checked
(against `adt_fields`, ADR-0080) but lowered to the `Field(0)` no-op stub; this
makes the read real.

Surface (`.cb`):

```python
fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    let r: i64 = body.rank        # __cobrust_pit_body_get_i64(body, "rank")
    let n: str = body.name        # __cobrust_pit_body_get_str(body, "name")
    if r >= 50:
        return pit.text_response(200, "high")
    return pit.json_response(201, body)
```

**The dispatch gate is REGISTRATION-DRIVEN, NOT type-driven (the load-bearing
correctness invariant, ADR-0081 §2-Q4 / §5.2).** The serde accessor fires ONLY
for a base local the checker recorded as a `route_validated` body-param —
NEVER for any `Ty::Adt`-with-a-field-table. A `.cb`-constructed `let s =
Score()` (or a NON-registered fn's `b: CreateScore` param) has the SAME
`Ty::Adt(real-id)` + the SAME field table, but its `*mut u8` is a null/opaque
pointer (`AggregateKind::Adt → opaque_ptr_ty.const_null()`), NOT a boxed
`serde_json::Value` — a serde cast over it would be UB. Gating on the type is
that UB bug; gating on the registration mark makes the cast structurally
unreachable for any unmarked local.

Mechanism (the layered pipeline, BOTTOM-UP — the NEW channel is the Q4 gate's
substrate):

- **Checker channel (`check.rs`, NEW).** `TypedModule.validated_handlers:
  HashMap<DefId, (usize, AdtId)>` — sibling of `adt_fields`. Populated in
  `check_callback_arg`'s validated-body sentinel branch: as each accepted
  `app.route_validated(_, _, handler)` callback arg is checked, record the
  handler `DefId` → (body-param positional index, body class `AdtId`). The ONLY
  source of "this param is a validated body" (route-shape validation is
  otherwise call-site-only, recorded nowhere a fn body can read). Carried out of
  `check()` exactly like `adt_fields`/`adt_names`.
- **MIR mark (`tree.rs` + `lower.rs`, NEW).** `LocalDecl.validated_body_of:
  Option<AdtId>`. `lower_fn`, lowering a fn whose `DefId` is in
  `validated_handlers`, sets the body-param local's `validated_body_of =
  Some(body_adt)`. Every OTHER local — a non-registered fn's param, a `let s =
  Score()` binding — keeps the `declare_local` default `None`.
- **Gated `Attr` sub-arm (`lower.rs`).** In the rvalue `ExprKind::Attr` arm,
  BEFORE the `Field(0)` stub fallthrough: `lookup_validated_body_field_accessor`
  fires ONLY when the base resolves to a local with `validated_body_of ==
  Some(id)` AND the field is in that class's `adt_fields`. It reads the field's
  declared `Ty`, picks the shim via `lookup_validated_body_accessor`, and lowers
  through the existing borrowed-receiver `emit_ecosystem_call` (the
  `coil.Buffer.shape` Move→Copy discipline), passing `(recv,
  Constant::Str(field_name))`. The field-name `Str` is COMPILER-SYNTHESISED
  (footgun #1 — never author-written). A base WITHOUT the mark takes the
  pre-existing `Field(0)` stub path UNCHANGED (no serde cast — the no-UB
  invariant).
- **The seam (`ecosystem.rs`, §2-Q5).** `lookup_validated_body_accessor(field_ty)
  -> Option<EcoSig>` names **a symbol + a `Ty`**, NEVER serde / a JSON key:
  `Ty::Int → __cobrust_pit_body_get_i64`, `Ty::Str → __cobrust_pit_body_get_str`.
  A future native-struct ABI (ADR-0081 §7) swaps the backing behind the SAME
  symbol — zero `.cb`-source churn. `f64`/`bool` are Phase-2 (`None` here until
  then → a `body.<f64-field>` read falls to the deferred stub, NOT a mis-read).
- **Codegen (`llvm_backend.rs`).** `__cobrust_pit_body_get_i64`
  (`[ptr, ptr] -> i64`) + `__cobrust_pit_body_get_str` (`[ptr, ptr] -> ptr`,
  type-identical to `request_path_param`) declared in the pit extern block.
- **CLI prefix (`intrinsics.rs`).** Both match the existing `__cobrust_pit_*`
  arm for free.
- **Accessor shims (`cabi.rs`).** Cloned from the `(ptr, ptr) -> <ret>`
  `request_path_param` template: borrow `&serde_json::Value`, `read_str_buf`
  the name, `v.get(name).and_then(as_i64 | as_str)`, `alloc_str_buffer`
  strings. The i64 shim uses `serde_json::Value::as_i64` — **integer-only,
  NEVER `as_f64`-then-truncate** (footgun #3; CLAUDE.md §2.2 no-silent-coercion).

Totality + ownership: validation already proved presence + type + range BEFORE
the handler ran (`validate_against_schema`), so each read is TOTAL — the
`unwrap_or` fail-clean sentinel (`0` / empty `Str`) is UNREACHABLE on the
validated path (a defense, mirroring `path_param`'s `unwrap_or("")`, NOT a
`KeyError` surface — footgun #2 dropped). The shims BORROW the body box; the
`route_validated` trampoline retains sole ownership and frees it exactly once
as a `serde_json::Value` after the handler returns. The str shim's return is a
fresh `.cb`-owned `Str` dropped once by the `.cb` scope.

The no-UB invariant (the paired-ADSD-audit's primary focus): a tracked-body
class used as anything OTHER than a registered handler's validated-body param —
(a) a NON-registered fn param `fn helper(b: CreateScore): return b.rank`, or
(b) a `let s = Score()` binding — has `validated_body_of == None`, so the serde
shim NEVER fires and the base is NEVER `cast::<Value>()`-ed. It hits the
pre-existing no-field-storage stub instead. The worst case degrades to the
already-documented "no field storage yet" limitation — a stub read, NOT
undefined behavior. (Test: `pit_body_field_read_e2e.rs` — the observable read
+ the no-UB negative, both green.)

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

Scope (Phase-1b-iii): int-range schema bounds (`minimum`/`maximum`).
`minLength`/`maxLength` + `pattern` are the Phase-2 addition (below). The
doc is a Rust-assembled JSON string (`Response::json`), not a `.cb`-struct
serialization (the deferred §9 bridge).

## ADR-0080 Phase-2 — STRING refinements (str length + pattern)

Two new fixed `where`-clause refinement kinds on a `str` field, alongside
the Phase-1 int range. Same side-table, same descriptor, same single-source
discipline — only new variants at each layer (a MIRROR of the int-range
chain, no new mechanism).

Surface (`.cb`):

```python
class SignupBody:
    username: str where 1 <= len(self) and len(self) <= 20   # LENGTH bound
    email:    str where pattern(self, ".+@.+")                # PATTERN (literal regex)

fn signup(req: pit.Request, body: SignupBody) -> pit.Response:
    return pit.text_response(201, "ok")
```

The fixed str forms (ADR-0080 Q6):

- LENGTH — `lo <= len(self) and len(self) <= hi`, and the one-sided
  `len(self) <= n` / `len(self) >= n`. The subject is `len(self)` (vs the
  bare `self` of the int range); the same `±1`-saturating strict→inclusive
  shift applies.
- PATTERN — `pattern(self, "<literal-regex>")`. The regex is a STRING
  LITERAL (a non-literal cannot be embedded in the descriptor).

Mechanism (the layered MIRROR of the int-range chain):

- **HIR name-resolution.** `len` and `pattern` are fixed refinement
  KEYWORDS, recognised structurally — bound to synthetic `DefId`s in the
  refinement-predicate lowering scope (alongside `self`) so the predicate
  resolves SELF-CONTAINED, independent of the prelude (which also defines a
  runtime `len`). Scoped to the predicate only.
- **Side-table (`cobrust-types`).** `interpret_refinement` keys on the
  field's BASE TYPE: `i64` → int range; `str` → `interpret_str_refinement`,
  which recognises `pattern(self, "…")` → `Refinement::Pattern { regex }`
  else a `len(self)` bound → `Refinement::StrLen { lo, hi }`. The regex is
  COMPILE-CHECKED here (`regex::Regex::new`) — a malformed pattern is a
  BUILD-time `TypeError::UnsupportedRefinement` with a FIX (§2.5-B), NOT a
  per-request runtime panic. A `len`/`pattern` form on a non-`str` field, or
  a bare-`self` int bound on a `str` field, is rejected with the FIX.
- **Descriptor encoding (the ONE encoder).**
  `Refinement::descriptor_payload(base_kind)` renders the payload after
  `field<TAB>`: `StrLen` → `str:<lo>:<hi>` (reuses the int-range numeric
  suffix; the `str` kind discriminates LENGTH from value range); `Pattern` →
  `pat:<regex>` (replaces the kind token; the regex is everything after the
  first `:`, so a `:` inside it is safe).
- **Decoder (the ONE reader, `validation::parse_schema`).** Splits the kind
  token off the FIRST `:`; a `pat` token takes the remainder as the raw
  regex, every other token parses the `:lo:hi` numeric suffix.
- **Validator (`validation::check_field`).** A `Str` field length-checks
  `s.chars().count()` (Unicode scalar count = Python `len()`; `None` bound =
  unbounded) → `LengthOutOfRange`. A `Pat` field re-compiles the (already
  compile-checked) regex and matches → `PatternMismatch`. Both render a
  typed 422 WITHOUT entering the handler.
- **OpenAPI emitter (`openapi::field_schema`, cannot-drift).** Kind-aware:
  `Str` field's `lo`/`hi` → `minLength`/`maxLength`; `Pat` field's regex →
  `pattern` (the raw string). Read from the SAME `parse_schema` output the
  validator checks — two projections of one source.

Done-means (verified, the live string-refinement E2E):
`POST /signup {"username":"bob","email":"b@x.com"}` → 201 + handler entered;
a 21-char username → 422 (maxLength 20) NOT entered; an empty username →
422 (minLength 1) NOT entered; `email:"notanemail"` → 422 (pattern miss)
NOT entered. `GET /openapi.json` → `username:{type:string,minLength:1,
maxLength:20}`, `email:{type:string,pattern:".+@.+"}`. Cannot-drift
cross-check: the 21-char-username 422 AND the advertised `maxLength:20`, and
the bad-email 422 AND the advertised `pattern:".+@.+"`, both from one source.

Scope (Phase-2): str LENGTH + PATTERN. The array-length `maxItems` form for
list fields stays Phase-4 (ADR-0080 §6). Per-request regex re-compile (tiny
patterns, schema already re-parsed per request) is the accepted
simplicity-over-micro-opt tradeoff; a process-wide compiled-regex cache is a
future optimisation, not a correctness concern.

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
