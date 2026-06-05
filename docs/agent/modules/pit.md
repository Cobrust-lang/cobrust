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
  `Ty::Int → __cobrust_pit_body_get_i64`, `Ty::Str → __cobrust_pit_body_get_str`,
  and (ADR-0081 Phase-2) `Ty::Float → __cobrust_pit_body_get_f64`, `Ty::Bool →
  __cobrust_pit_body_get_bool`, plus a USER-class-typed field (`Ty::Adt(id, _)`
  with `id` outside the ecosystem-handle range) → `__cobrust_pit_body_get_nested`.
  A future native-struct ABI (ADR-0081 §7) swaps the backing behind the SAME
  symbols — zero `.cb`-source churn. (See the ADR-0081 Phase-2 section below.)
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

## ADR-0081 Phase-2 — `f64` / `bool` / nested `body.inner.x` field reads

Phase-2 widens the Phase-1b read seam to the remaining scalar types AND nested
objects. No new `.cb` syntax — `body.field` / `body.inner.x` are already what
the type checker accepts (ADR-0080 typed nested attr chains).

```python
class Inner:
    x: i64 where 0 <= self and self <= 100
class Payload:
    ratio: f64 where 0.0 <= self and self <= 1.0
    active: bool
    inner: Inner
fn h(req: pit.Request, body: Payload) -> pit.Response:
    let r: f64 = body.ratio          # __cobrust_pit_body_get_f64(body, "ratio")
    let a: bool = body.active        # __cobrust_pit_body_get_bool(body, "active")
    let v: i64 = body.inner.x        # nested(body,"inner") -> i64(.,"x") — recursive
    ...
```

- **`f64` / `bool` (mechanical, mirror i64/str).** New arms in
  `lookup_validated_body_accessor`: `Ty::Float → __cobrust_pit_body_get_f64`
  (serde `as_f64`), `Ty::Bool → __cobrust_pit_body_get_bool` (serde `as_bool`,
  STRICT — a JSON `true`/`false` only, no truthiness; §2.2). New shims in
  `cabi.rs` (BORROW the box, fail-clean `0.0`/`false` sentinel). New codegen
  externs: `f64` → LLVM `double` (the `math.sqrt` precedent), `bool` → LLVM `i1`
  via `bool_type()` (the `re.match` / `fang.verify_password` / `coil.any`
  precedent — the i1 lands in the `.cb` `_ecoret` Bool local, usable in
  `if body.flag:`). The `__cobrust_pit_*` prefix recognizer covers them for free.
- **Nested `body.inner.x` (recursive).** A field typed as ANOTHER field-tracked
  validated class (`Ty::Adt(nested_adt, _)`, id OUTSIDE `ECO_ADT_BASE`) resolves
  to `__cobrust_pit_body_get_nested`, which returns the **BORROWED interior**
  `&serde_json::Value` for the nested JSON object (no allocation, no free). MIR's
  `Attr` base resolution is now RECURSIVE (`resolve_validated_body_base`,
  `lower.rs`): it walks a `body.inner.…` chain down to the marked param, emitting
  a nested borrow at each hop and **re-marking each result temp**
  `validated_body_of = Some(nested_adt)`, so `.field` on it recurses through the
  SAME registration-gated arm. Verified at depth 1 (`body.inner.x`) AND depth 3
  (`body.mid.leaf.v`, `body.mid.leaf.flag`).
- **Soundness of the borrowed interior pointer.** It aliases the parent box the
  `route_validated` trampoline owns + frees EXACTLY ONCE *after* the handler
  returns (`cabi.rs`), so the borrow is valid for the whole handler. The
  `_ecoret` temp is typed `Ty::Adt(user_class)`, whose codegen drop is a NO-OP
  (`handle_drop_symbol(user_id) == None`), so even if the drop schedule
  enumerates the temp, NO free is emitted on the borrowed pointer (no
  double-free, no UB).
- **No-UB gate preserved.** The recursive resolver only succeeds when the chain
  bottoms out at a `validated_body_of`-marked param, so a non-registered helper
  reading `b.ratio` / `b.active` / `b.inner.x` (or a `.cb`-constructed instance)
  emits NEITHER the scalar NOR the nested accessor — pinned by three new
  `nm`-on-`.o` codegen-property tripwires. The gate is REGISTRATION-driven, not
  type-driven (§5.2 / §10). Tests: `pit_body_field_read_e2e.rs` (9 e2es) +
  `cabi.rs` `#[cfg(test)]` shim unit tests (f64 fractional / bool strict / nested
  borrow recursion) + `ecosystem.rs` accessor-lookup unit tests.

## ADR-0081 Phase-3 — `body.<list[T]>` field reads + body-as-fn-arg

Phase-3 adds LIST-field reads (`T ∈ {str, i64, f64, bool}`) and resolves the
body-as-fn-arg question. No new `.cb` syntax — `body.tags` is already what the
type checker accepts (ADR-0080 Phase-4(c) `list[T]` body fields).

```python
class TagBody:
    tags: list[str]
    scores: list[i64]
fn h(req: pit.Request, body: TagBody) -> pit.Response:
    let xs: list[str] = body.tags     # __cobrust_pit_body_get_list_str(body, "tags")
    let n: i64 = xs.len()             # iterate / index / len like any .cb list
    for s in body.tags:               # the minted list is a real .cb list[str]
        ...
    let sum: i64 = 0
    for v in body.scores:             # __cobrust_pit_body_get_list_i64(body, "scores")
        sum = sum + v
    ...
```

- **List accessors (ONE per element type).** New `Ty::List(elem)` arm in
  `lookup_validated_body_accessor`: `list[str] → __cobrust_pit_body_get_list_str`,
  `list[i64] → ..._list_i64`, `list[f64] → ..._list_f64`, `list[bool] →
  ..._list_bool` (codegen-extern clarity, mirroring the scalar shims). The `ret`
  carries `Ty::List(elem)` so the result temp's drop schedule selects the right
  list drop. A `list[<deferred-elem>]` (list-of-list, `list[<Class>]`, out of
  #156 read scope) returns `None` (the `Field(0)` stub, never a serde cast).
- **The mint (`cabi.rs`).** Each accessor BORROWS the parent body box
  (`&serde_json::Value`), reads the JSON array, and MINTS a fresh `.cb` `list[T]`
  via the redis-`lrange` / coil-`shape` recipe (`__cobrust_list_new(8, len)` +
  per-slot `__cobrust_list_set`). Slot conventions match how codegen consumes a
  `.cb` `list[T]`: a heap-`Str` pointer for `str` (one `alloc_str_buffer` per
  element), the raw `i64`, `0`/`1` for `bool`, and `f64::to_bits()` for `f64`
  (the `Constant::Float` slot convention — the `.cb` consumer `from_bits`-reads
  it). Each element uses the typed `as_str`/`as_i64`/`as_f64`/`as_bool` (§2.2 — no
  coercion; the validator already rejected a type-mismatched array with 422 BEFORE
  the handler, ADR-0080 Phase-4(c)). An empty / missing / non-array field mints a
  valid EMPTY list (fail-clean, NEVER null, NEVER a panic).
- **Codegen externs.** The four `__cobrust_pit_body_get_list_*` shims are
  `(ptr, ptr) -> ptr` (a list HANDLE is a raw pointer, type-identical to the
  Str/nested returns). The `__cobrust_pit_*` prefix recognizer covers them for
  free. **No MIR edit** — the `Attr` sub-arm is type-driven (it already names
  `accessor.runtime_symbol` + types the `_ecoret` `accessor.ret`), so the list
  arm flows through the SAME registration-gated retarget the scalars use.
- **Drop discipline.** The minted list is `.cb`-OWNED → its `_ecoret` temp's
  `Ty::List(elem)` drives the codegen drop: `list[str]` →
  `__cobrust_list_drop_elems(list, __cobrust_str_drop)` (frees each element `Str`
  then the container), else `__cobrust_list_drop` (container only). The accessor
  frees NOTHING (no aliasing — the list is a deep copy of the array). Proven by a
  200-read hammer-loop e2e (server survives + still serves correctly afterwards).
- **No-UB gate.** The list accessors are REGISTRATION-gated EXACTLY like the
  scalar/nested ones — a non-registered `fn helper(b): b.tags.len()` emits NO
  accessor symbol (pinned by a new `nm`-on-`.o` tripwire); the existing no-UB
  negatives stay green.
- **Body-as-fn-arg.** Passing a READ FIELD VALUE (an `i64`/`str`/`list`) to
  another fn is DELIVERED — ordinary value-arg passing (`double(body.rank)`,
  `first_or_empty(body.tags)`). Passing the WHOLE validated `body` to another fn
  is DEFERRED (an honest `#[ignore]`): the `validated_body_of` mark does NOT cross
  a call boundary, so a `b.field` read in the CALLEE is the `Field(0)` stub — a
  WRONG value, NOT UB (the gate holds; an always-on tripwire proves no accessor is
  emitted). Delivering it needs deep inter-procedural propagation (or the §7
  native-struct ABI). Tests: `pit_body_field_read_e2e.rs` (16 tests — 15 GREEN + 1
  honest-deferred ignore) + `cabi.rs` 4 list-mint unit tests + `ecosystem.rs`
  list-arm test.

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

## ADR-0080 Phase-3a — f64 value-range refinement (`FloatRange`)

ONE new fixed `where`-clause refinement kind on an `f64` field — the precise
MIRROR of the Phase-1 `Refinement::IntRange`. Same side-table, same descriptor
shape, same single-source discipline; only new variants at each layer (no new
mechanism). `bool` value-validation is OUT (a later 3b).

Surface (`.cb`):

```python
class Reading:
    name:  str
    ratio: f64 where 0.0 <= self and self <= 1.0   # VALUE RANGE (inclusive)

fn submit(req: pit.Request, body: Reading) -> pit.Response:
    return pit.text_response(201, "ok")            # reached only if 0 <= ratio <= 1
```

The fixed float form (ADR-0080 Q6 / Phase-3a D2):

- VALUE RANGE — `lo <= self and self <= hi`, and the one-sided `lo <= self` /
  `self <= hi`, with `lo`/`hi` FLOAT (or integer-widened) literals. The subject
  is the bare `self` (as the int range). **Only inclusive `<=`/`>=` are
  admitted** — a strict `<`/`>` is REJECTED with a FIX (the integer `±1`
  inclusive rewrite has no clean float analog; the reals are dense). `NaN`/`inf`
  are not producible by the grammar.

Mechanism (the layered MIRROR of the int-range chain):

- **Side-table (`cobrust-types`).** `interpret_refinement` keys on the field's
  BASE TYPE: a new `f64` → `interpret_float_range`, which threads
  `parse_bound_predicate_f64` (the `f64` dual of the int bound-parser, identical
  contradiction detection) over `parse_subject_bound_f64` + `literal_float_value`
  → `Refinement::FloatRange { lo, hi }`. An integer literal is accepted as a
  float bound (`0 <= self` → `0.0`, the natural spelling, §2.5). A
  `len`/`pattern`/strict-`<`/arbitrary-call form on an `f64` field is rejected
  with the §2.5-B FIX.
- **Descriptor encoding (the ONE encoder).** `descriptor_payload("f64",
  FloatRange{lo,hi})` → `f64:<lo>:<hi>` via `float_suffix` (dual to `int_suffix`),
  each bound rendered with `f64` `Display` (shortest round-trippable decimal):
  `f64:0:1`, one-sided `f64:0.5:` / `f64::100`, fractional `f64:0.5:99.9`.
- **Decoder (the ONE reader, `validation::parse_schema`).** The `f64` kind
  parses its `:lo:hi` suffix with `parse_float_suffix` (`str::parse::<f64>()`,
  accepts everything `f64` `Display` emits → the encode↔decode pair round-trips
  exactly) into a SEPARATE `FieldSpec` pair `lo_f`/`hi_f: Option<f64>` (a
  fractional bound is not an `i64`).
- **Validator (`validation::check_field`).** An `F64` field extracts the JSON
  number with `as_f64` (doubles as the type check + value), then range-checks
  against `lo_f`/`hi_f` → `ValidationError::FloatOutOfRange`, rendered as a typed
  422 WITHOUT entering the handler. The 422 detail PRINTS THE FIX (§2.5-D6):
  ``field `ratio` value 1.5 must be in [0, 1]``.
- **OpenAPI emitter (`openapi::field_schema`, cannot-drift).** The `F64` arm
  emits `minimum`/`maximum` (from `lo_f`/`hi_f`, via `serde_json::Number::from_f64`)
  on a `{type:number}` schema — read from the SAME `parse_schema` output the
  validator checks.
- **MIR — unchanged.** `lower.rs` already maps `Ty::Float → "f64"` and calls
  `descriptor_payload(kind)` generically, so the FloatRange suffix renders with
  no MIR edit.
- **`Eq`-drop (D1).** `Refinement` + `ValidationError` derive `PartialEq` only
  (an `f64` bound is `PartialEq`-not-`Eq`). SAFE — both are HashMap values /
  `==`-compared, never keys; no site bounds them `: Eq`.

Done-means (verified, the live float-refinement E2E):
`POST /readings {"name":"a","ratio":0.5}` → 201 + handler entered (an integer
`ratio:1` → 201, an integer is a valid f64); `ratio:1.5` (> max) and
`ratio:-0.5` (< min) → 422 NOT entered; `ratio:"x"` → 422 wrong type
(`must be of type number`). `GET /openapi.json` → `ratio:{type:number,
minimum:0,maximum:1}` (NOT integer, NOT minLength/maxLength). Cannot-drift
cross-check: the `ratio:1.5` 422 AND the advertised `maximum:1`, from one source.

Scope (Phase-3a): f64 VALUE RANGE only. `bool` value-validation, the §2.5-A
compile-time-checked-refinement upgrade, and cross-field constraints stay later
phases (ADR-0080 §6/§9).

## ADR-0080 §6 Phase-4 (b) / #156 — nested OBJECT bodies (the multi-block descriptor)

A body field whose type is ANOTHER validated `class` is now RECURSIVELY validated
and emitted as a nested OpenAPI `$ref` (previously: lowered to kind `any`,
presence-only + empty schema). Scope = nested OBJECT only (one OR more levels deep —
recursion handles depth); `list[T]` / `dict` / Optional nested fields stay DEFERRED.

```text
class Address:                       # the nested validated class
    city: str
    zip: i64 where 0 <= self and self <= 99999

class CreateUser:                    # the ROOT: a flat field + a nested-class field
    name: str
    address: Address                 # type is another validated class → obj field

fn create_user(req: pit.Request, body: CreateUser) -> pit.Response:
    return pit.text_response(201, "ok")   # reached only if the nested object validated
```

The CTO-locked design (D1-D4), each preserving cannot-drift (footgun #4):

- **D1 — MULTI-BLOCK descriptor (`mir::lower::validated_body_schema_for_handler` +
  the new `emit_class_block`).** The descriptor is now ROOT block first
  (`# CreateUser\naddress\tobj:Address\nname\tstr`), then one `# <Nested>`-headed block
  per transitively-referenced validated class (`# Address\ncity\tstr\nzip\ti64:0:99999`),
  collected by a deterministic BFS deduplicated by `AdtId`. A class-typed field's
  payload is the NEW token `obj:<NestedClassName>` (the nested class's source name);
  a truly-unknown type still maps to `any`. A FLAT-only body emits exactly ONE block —
  BYTE-IDENTICAL to the pre-Phase-4 descriptor.
- **D2 — multi-block decode (`validation::parse_schema_blocks` + `FieldKind::Obj(String)`).**
  Parses the descriptor into an ordered `Vec<(class_name, Vec<FieldSpec>)>` (ROOT =
  first block). The MIR ENCODE (`emit_class_block`) and this DECODE are mirror
  inverses — pinned by `obj_token_round_trips`. `FieldKind` dropped `Copy` (the `Obj`
  variant owns a `String`); every use site matches `&spec.kind`. `parse_schema` is a
  back-compat shim returning the ROOT block.
- **D3 — recursive `validate_against_schema` (`validation::validate_block`).** An
  `Obj(name)` field's JSON value MUST be a JSON object (else `WrongType` 422,
  `expected:"object"`); it is recursively validated against the named class's block.
  Missing/extra nested fields use the SAME `MissingField`/`UnknownField` policy as
  the flat validator. A depth cap (`MAX_NESTING_DEPTH = 64`) returns a clear
  `NestingTooDeep` 422 guarding a pathological cyclic schema; recursion otherwise
  terminates on any finite body.
- **D4 — OpenAPI `$ref` + per-class components (`openapi::field_schema` +
  `build_openapi_doc`).** An `Obj(name)` field renders as
  `{"$ref":"#/components/schemas/<name>"}`; `build_openapi_doc` registers EACH
  descriptor block (ROOT + every nested class) as its own `components/schemas/<Name>`
  object schema, from the SAME `parse_schema_blocks` parse the validator reads (no
  second source). The nested component advertises the SAME bound the recursive
  validator enforces.

The cleared PREREQUISITE: a class field typed as another class with no initializer
(`address: Address`) did NOT type-check (`check_class` re-checked the synthetic
`let address: Address = None` and unified `Ty::Adt` against `None`). The fix:
`check_class` skips re-checking a FIELD `let` (its type already came from the
annotation into `adt_fields`; field access resolves through `adt_fields`, not the
`let` binding). Flat scalar fields are observationally identical.

Done-means (verified, the live nested-body E2E `pit_nested_body_e2e.rs`):
`POST /users {"name":"a","address":{"city":"NYC","zip":10001}}` → 201 + handler
entered; four nested-invalidity classes (out-of-range zip 100000, missing nested
`city`, wrong-typed nested `zip:"x"`, non-object `address:"oops"`) + a nested extra
key → 422 NOT entered; `GET /openapi.json` shows `CreateUser.address` as a `$ref`
to `Address` + a separate `Address` component with `zip:{minimum:0,maximum:99999}`.
Cannot-drift cross-check: the nested-zip 422 AND the advertised nested `maximum:99999`,
from one source.

## ADR-0080 §6 Phase-4 (c) / #156 — list[T] COLLECTION bodies (the `list:` element token)

A body field typed `list[T]` is now array-checked + EACH element recursively validated,
and emitted as `{"type":"array","items":…}` (previously: lowered to kind `any`,
presence-only + empty schema). REUSES the Phase-4 (b) machinery for `list[<Class>]`
elements. Scope = `list[<scalar>]` (`str`/`i64`/`f64`/`bool`) + `list[<Class>]`;
`list[list[T]]` / `dict[K,V]` / Optional list / element-COUNT (`minItems`/`maxItems`)
stay DEFERRED.

```text
class OrderLine:                     # the element validated class
    sku: str
    qty: i64 where 1 <= self and self <= 999

class CreateOrder:                   # flat + scalar-list + object-list fields
    note: str
    tags: list[str]                  # list[scalar]  → list:str
    scores: list[i64]                # list[scalar]  → list:i64 (no elem refinement)
    lines: list[OrderLine]           # list[Class]   → list:obj:OrderLine + an # OrderLine block

fn create_order(req: pit.Request, body: CreateOrder) -> pit.Response:
    return pit.text_response(201, "ok")   # reached only if every element validated
```

The CTO-locked design (D1-D4), each preserving cannot-drift (footgun #4):

- **D1 — the `list:<elem-payload>` token (`mir::lower::emit_class_block` +
  the new `element_payload` / `obj_element_payload`).** A `list[T]` field's payload is
  `list:<elem-payload>`, where elem-payload is T's OWN payload: a scalar kind, OR
  `obj:<ClassName>` for `list[SomeClass]` (whose block is emitted by the SAME BFS — the
  shared `obj_element_payload` enqueues the element class, so the direct-nested and
  list-element `obj:` token + enqueue have ONE source). Examples: `tags\tlist:str`,
  `scores\tlist:i64`, `lines\tlist:obj:OrderLine` + an `# OrderLine` block. A list
  element carries NO refinement suffix (element-level refinement is DEFERRED; a
  `list[i64]` element is bare `i64`). A `list[<deferred-elem>]` (`list[list[T]]`) →
  `any` (`element_payload` returns `None`). A FLAT/scalar/nested-object body with NO
  list field is BYTE-IDENTICAL (the `Ty::List` arm never fires).
- **D2 — recursive decode (`validation::parse_field_payload` + `FieldKind::List(Box<FieldSpec>)`).**
  `list:<rest>` is parsed by RECURSIVELY parsing `<rest>` as the element spec (the
  element shares the parent field's `name` placeholder). The MIR ENCODE
  (`element_payload`) and this DECODE are mirror inverses — pinned by
  `list_scalar_descriptor_round_trips` + `list_obj_element_descriptor_round_trips`.
  `FieldKind` dropped `Eq` (the embedded `FieldSpec` carries `f64` bounds —
  `PartialEq`-not-`Eq`); `FieldSpec` gained `Clone` + `PartialEq`. Every use site
  matches `&spec.kind`; no `HashMap`/`HashSet` key bounds it `: Eq`.
- **D3 — array + per-element validation (`validation::check_field` `List` arm).** A
  `List(elem_spec)` field's JSON value MUST be a JSON ARRAY (else `WrongType` 422,
  `expected:"array"`); EACH element is validated against a CLONE of the element spec
  (name `<field>[<i>]`, §2.5-B) by RECURSING into `check_field` — a scalar element via
  the scalar path, an `obj:<Name>` element by recursing into the named block (REUSING
  `validate_block` + the SAME `MAX_NESTING_DEPTH` cap; `depth` is threaded unchanged
  because the array is not a JSON-object nesting level). An EMPTY array is VALID.
  Missing/extra/out-of-range object-element fields reuse the existing policies.
- **D4 — OpenAPI `array`+`items` + element component (`openapi::field_schema` +
  `build_openapi_doc`).** A `List(elem_spec)` field renders as
  `{"type":"array","items":<elem-schema>}`, where `<elem-schema>` is the element spec's
  own `field_schema` (a scalar `{type:…}` OR a `$ref` for a `list[SomeClass]` element).
  The element class registers as its OWN `components/schemas/<name>` via the SAME
  `parse_schema_blocks` BFS (no second source). The element component advertises the
  SAME bound the per-element validator enforces.

PREREQUISITE: already cleared by Phase-4 (b) — a no-value `list[T]` class field
type-checks today via the SAME `check_class` field-`let` skip (it admits the no-value
form for collection fields too). So there is NO type-check prerequisite for the
collection slice; the RED was PURELY behavioral.

Done-means (verified, the live collection-body E2E `pit_collection_body_e2e.rs`,
5 tests): `POST /orders {"note":"x","tags":["a","b"],"scores":[1,2],"lines":[{"sku":"s1","qty":5}]}`
→ 201 + handler entered; scalar-element classes (number in `list[str]` → `tags[1]`,
string in `list[i64]` → `scores[1]`, non-array `tags:"oops"`) + object-element classes
(element `qty:9999` > 999, element missing `sku`, non-object element, element extra
`color`) → 422 NOT entered; empty lists → 201; `GET /openapi.json` shows
`tags:{type:array,items:{type:string}}`, `scores:{type:array,items:{type:integer}}`,
`lines:{type:array,items:{$ref:…/OrderLine}}` + a separate `OrderLine` component with
`qty:{minimum:1,maximum:999}`. Cannot-drift cross-check: the element-qty 422 AND the
advertised element `maximum:999`, from one source.

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
