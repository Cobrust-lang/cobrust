// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: flask 3.0 (web-server surface)
// oracle: cpython 3.11 (module: flask)
// see PROVENANCE.toml for the full manifest.

//! The [`App`] — Flask's `Flask(__name__)` analogue.
//!
//! Construct an app, register routes (method-based API in this
//! increment; the `@pit.route` decorator on the `.cb` surface is a
//! deferred follow-on), then `run(host, port)` to serve. `run` is
//! SYNC and blocking: it drives an axum server under a singleton tokio
//! runtime via `Runtime::block_on` (ADR-0028 §A precedent), so the
//! user surface has no `async fn` (constitution §2.2 — no async/sync
//! coloring at the user layer).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::Response as AxumResponse;
use tokio::runtime::Runtime;

use crate::error::PitError;
use crate::request::Request;
use crate::response::Response;
use crate::routing::RoutePattern;

/// A boxed request handler. `handler(Request) -> Response`, mirroring a
/// Flask view function. `Send + Sync` so the route table can be shared
/// across axum's worker tasks.
pub type Handler = Arc<dyn Fn(Request) -> Response + Send + Sync>;

/// Maximum request-body size buffered for a handler (16 MiB). Prevents
/// OOM from an adversarially large / runaway request body (mirrors
/// cobrust-strike's `MAX_BODY_BYTES` B5 hardening). A larger body yields
/// an empty body to the handler rather than buffering unbounded.
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

/// One registered route: method + compiled pattern + handler.
struct Route {
    method: String,
    pattern: RoutePattern,
    handler: Handler,
}

/// The application object. Mirrors `flask.Flask`.
///
/// Constitution §5.1: 0 public fields. Routes are registered through
/// the method API and served by [`App::run`].
///
/// # Middleware flags (ADR-0078 §6.1 Phase-1)
///
/// `cors` / `trace` / `compress` are set by the `use_cors()` /
/// `use_trace()` / `use_compression()` surface (cabi shims flip the
/// flag on the live `App`). They are read ONCE in [`serve`] at the
/// moment the axum `Router` is constructed, applying the corresponding
/// `tower_http` `Layer` — the before-serve contract: a `use_cors()`
/// call after `serve`/`serve_in_background` has already built the
/// Router is a no-op (the Router is already bound). The `std::mem::take`
/// in the cabi serve shims swaps the WHOLE `App` (flags included) into
/// the value moved into `serve`, so flags set before serve survive the
/// take. Default `false` (no middleware) — `#[derive(Default)]` gives
/// `bool::default() == false` for free.
#[derive(Default)]
pub struct App {
    routes: Vec<Route>,
    /// `CorsLayer::permissive()` applied at serve when set (ADR-0078 §6.1).
    cors: bool,
    /// `TraceLayer::new_for_http()` applied at serve when set.
    trace: bool,
    /// `CompressionLayer::new()` applied at serve when set.
    compress: bool,
}

/// Lazy process-singleton tokio runtime backing the blocking `run`
/// bridge. Per ADR-0028 §A — multi-thread runtime, first-use init.
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Runtime::new().expect("pit tokio runtime initialization failed (ADR-0028 §A)")
    })
}

impl App {
    /// Construct an empty app. Mirrors `app = Flask(__name__)`.
    ///
    /// All middleware flags default `false` (no CORS/trace/compression
    /// until the corresponding `use_*` setter runs) — `Self::default()`
    /// gives `Vec::new()` routes + `false` flags.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for `(method, path)`. The general form behind
    /// the `get`/`post`/… shorthands. Mirrors
    /// `app.add_url_rule(path, view_func=handler, methods=[method])`.
    ///
    /// # Errors
    /// Returns [`PitError`] (`InvalidRoute`) for a malformed path, or
    /// (`DuplicateRoute`) if the same `(method, path)` is already
    /// registered.
    pub fn route<F>(&mut self, method: &str, path: &str, handler: F) -> Result<(), PitError>
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        let method_uc = method.to_ascii_uppercase();
        let pattern = RoutePattern::compile(path)?;
        let dup = self
            .routes
            .iter()
            .any(|r| r.method == method_uc && r.pattern.raw() == pattern.raw());
        if dup {
            return Err(PitError::duplicate_route(format!(
                "route already registered: {method_uc} {path}"
            )));
        }
        self.routes.push(Route {
            method: method_uc,
            pattern,
            handler: Arc::new(handler),
        });
        Ok(())
    }

    /// Register a `GET` handler. Mirrors `@app.get(path)`.
    ///
    /// # Errors
    /// See [`App::route`].
    pub fn get<F>(&mut self, path: &str, handler: F) -> Result<(), PitError>
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route("GET", path, handler)
    }

    /// Register a `POST` handler. Mirrors `@app.post(path)`.
    ///
    /// # Errors
    /// See [`App::route`].
    pub fn post<F>(&mut self, path: &str, handler: F) -> Result<(), PitError>
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route("POST", path, handler)
    }

    /// Register a `PUT` handler. Mirrors `@app.put(path)`.
    ///
    /// # Errors
    /// See [`App::route`].
    pub fn put<F>(&mut self, path: &str, handler: F) -> Result<(), PitError>
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route("PUT", path, handler)
    }

    /// Register a `DELETE` handler. Mirrors `@app.delete(path)`.
    ///
    /// # Errors
    /// See [`App::route`].
    pub fn delete<F>(&mut self, path: &str, handler: F) -> Result<(), PitError>
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route("DELETE", path, handler)
    }

    /// Enable the CORS middleware preset (ADR-0078 §6.1 Phase-1).
    ///
    /// Sets a flag read at serve time; [`serve`] then applies
    /// `tower_http::cors::CorsLayer::permissive()` to the axum `Router`,
    /// so served responses carry `Access-Control-Allow-Origin`. Mirrors
    /// FastAPI's `app.add_middleware(CORSMiddleware, …)` / Flask-CORS
    /// `CORS(app)` shape (constitution §2.5). MUST be called BEFORE
    /// `run`/`serve_in_background` (the flag is read once when the
    /// Router is built).
    pub fn use_cors(&mut self) {
        self.cors = true;
    }

    /// Enable the request-tracing middleware preset (ADR-0078 §6.1).
    ///
    /// Sets a flag read at serve time; [`serve`] then applies
    /// `tower_http::trace::TraceLayer::new_for_http()`. The effect is a
    /// logging side-effect (tracing spans/events), not an HTTP-observable
    /// header. MUST be called BEFORE serve.
    pub fn use_trace(&mut self) {
        self.trace = true;
    }

    /// Enable the response-compression middleware preset (ADR-0078 §6.1).
    ///
    /// Sets a flag read at serve time; [`serve`] then applies
    /// `tower_http::compression::CompressionLayer::new()`, which
    /// compresses the response body when the client negotiates an
    /// accepted encoding (e.g. `Accept-Encoding: gzip`) and passes the
    /// body through untouched otherwise. MUST be called BEFORE serve.
    pub fn use_compression(&mut self) {
        self.compress = true;
    }

    /// Match an incoming `(method, path)` against the route table.
    /// Returns the handler + captured path params, or `None` for a 404.
    fn dispatch(&self, method: &str, path: &str) -> Option<(Handler, HashMap<String, String>)> {
        for r in &self.routes {
            if r.method != method {
                continue;
            }
            if let Some(params) = r.pattern.match_path(path) {
                return Some((r.handler.clone(), params));
            }
        }
        None
    }

    /// Test/diagnostic hook: report whether `(method, path)` matches a
    /// registered route, returning the captured path params on a hit.
    /// Exposed for the integration/fuzz harness (which has no access to
    /// the private `dispatch`); not part of the Flask-shaped surface.
    #[doc(hidden)]
    #[must_use]
    pub fn dispatch_for_test(&self, method: &str, path: &str) -> Option<HashMap<String, String>> {
        self.dispatch(&method.to_ascii_uppercase(), path)
            .map(|(_, params)| params)
    }

    /// Test/diagnostic hook (ADR-0080 Phase-1b-ii): match `(method, path)`,
    /// build a `Request` carrying `body`, INVOKE the registered handler
    /// closure, and return its `Response`. Lets the `route_validated`
    /// trampoline test drive the validate-or-422 path (and assert
    /// handler-not-entered-on-422) without spinning a live server. Returns
    /// `None` for an unmatched route. Not part of the Flask-shaped surface.
    #[doc(hidden)]
    #[must_use]
    pub fn dispatch_and_invoke_for_test(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Option<Response> {
        let method_uc = method.to_ascii_uppercase();
        let (handler, params) = self.dispatch(&method_uc, path)?;
        let mut headers = HashMap::new();
        headers.insert("content-type".to_owned(), "application/json".to_owned());
        let req = Request::from_parts(
            &method_uc,
            path,
            params,
            HashMap::new(),
            headers,
            body.to_vec(),
        );
        Some(handler(req))
    }

    /// Run the server on `host:port`, blocking the calling thread until
    /// the process is killed. Mirrors `app.run(host, port)`.
    ///
    /// SYNC: bridges to the singleton tokio runtime via `block_on`
    /// (constitution §2.2 — the public surface has no `async fn`).
    ///
    /// # Errors
    /// Returns [`PitError`] (`Bind` kind) if the listen socket cannot be
    /// bound, or (`Runtime` kind) if the server task fails.
    pub fn run(self, host: &str, port: u16) -> Result<(), PitError> {
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| PitError::bind(format!("bad host:port {host}:{port}: {e}")))?;
        let shared = Arc::new(self);
        runtime().block_on(async move {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| PitError::bind(format!("bind {addr}: {e}")))?;
            serve(listener, shared).await
        })
    }

    /// Bind to `host:port` (port `0` = ephemeral), returning the actual
    /// bound address plus a future that serves until dropped. Used by
    /// the in-process test harness to spin the real axum server on a
    /// random port and learn which one it got.
    ///
    /// SYNC: the bind happens on the singleton runtime; the returned
    /// address lets a test dispatch a real HTTP client at the server.
    ///
    /// # Errors
    /// Returns [`PitError`] (`Bind` kind) on bind failure.
    pub fn serve_in_background(self, host: &str, port: u16) -> Result<ServerHandle, PitError> {
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| PitError::bind(format!("bad host:port {host}:{port}: {e}")))?;
        let shared = Arc::new(self);
        let rt = runtime();
        let (bound, server_fut) = rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| PitError::bind(format!("bind {addr}: {e}")))?;
            let bound = listener
                .local_addr()
                .map_err(|e| PitError::bind(format!("local_addr: {e}")))?;
            Ok::<_, PitError>((bound, serve(listener, shared)))
        })?;
        let join = rt.spawn(async move {
            let _ = server_fut.await;
        });
        Ok(ServerHandle {
            addr: bound,
            join: Some(join),
        })
    }
}

/// Handle to a backgrounded server (from [`App::serve_in_background`]).
/// Dropping it aborts the server task.
pub struct ServerHandle {
    addr: SocketAddr,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl ServerHandle {
    /// The actual bound address (resolves the ephemeral `:0` port).
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

/// The single axum service: every request flows through here, gets
/// matched against the route table, and is dispatched (or 404'd).
///
/// # Middleware (ADR-0078 §6.1 Phase-1)
///
/// The `App`'s `cors`/`trace`/`compress` flags are read ONCE here when
/// the `Router` is constructed (the before-serve contract — §6.1). Each
/// set flag layers the corresponding canned `tower_http` preset. Layers
/// are applied AFTER `with_state` so they wrap the whole service; the
/// flags live behind the `Arc<App>` so they are read, not moved.
async fn serve(listener: tokio::net::TcpListener, app: Arc<App>) -> Result<(), PitError> {
    let (cors, trace, compress) = (app.cors, app.trace, app.compress);
    let mut router = axum::Router::new().fallback(handle_any).with_state(app);
    // Apply the canned middleware presets for each set flag. The chain
    // order is fixed (cors → trace → compress); Phase-1 ships canned
    // presets only — configurable origins/levels are a follow-up
    // (ADR-0078 §9 "configurable middleware builders").
    if cors {
        router = router.layer(tower_http::cors::CorsLayer::permissive());
    }
    if trace {
        router = router.layer(tower_http::trace::TraceLayer::new_for_http());
    }
    if compress {
        router = router.layer(tower_http::compression::CompressionLayer::new());
    }
    axum::serve(listener, router)
        .await
        .map_err(|e| PitError::runtime(format!("server: {e}")))
}

/// Convert an axum request into our [`Request`], dispatch, and convert
/// the [`Response`] back to an axum response.
async fn handle_any(State(app): State<Arc<App>>, req: axum::extract::Request) -> AxumResponse {
    let method = req.method().as_str().to_owned();
    let uri = req.uri().clone();
    let path = uri.path().to_owned();

    let mut query = HashMap::new();
    if let Some(q) = uri.query() {
        for (k, v) in form_urlencoded_pairs(q) {
            query.insert(k, v);
        }
    }

    let mut headers = HashMap::new();
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            headers.insert(name.as_str().to_ascii_lowercase(), v.to_owned());
        }
    }

    // Cap the request body at MAX_BODY_BYTES to prevent OOM from an
    // adversarially large / runaway request (mirrors cobrust-strike's B5
    // hardening). A body over the cap (or a transport read error) yields
    // an empty body to the handler rather than buffering unbounded.
    let body_bytes: Bytes = match axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => Bytes::new(),
    };

    let Some((handler, path_params)) = app.dispatch(&method, &path) else {
        // 404 — mirrors Flask's default NotFound for an unmatched URL.
        return AxumResponse::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("404 Not Found"))
            .unwrap_or_else(|_| AxumResponse::new(Body::empty()));
    };

    let request = Request::from_parts(
        method,
        path,
        path_params,
        query,
        headers,
        body_bytes.to_vec(),
    );
    let response = handler(request);
    let (status, resp_headers, resp_body) = response.into_parts();

    let mut builder = AxumResponse::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
    for (k, v) in &resp_headers {
        if let (Ok(name), Ok(val)) = (
            HeaderName::try_from(k.as_str()),
            HeaderValue::try_from(v.as_str()),
        ) {
            builder = builder.header(name, val);
        }
    }
    builder
        .body(Body::from(resp_body))
        .unwrap_or_else(|_| AxumResponse::new(Body::empty()))
}

/// Minimal `application/x-www-form-urlencoded` query parser: split on
/// `&`, then `=`, percent-decoding `%XX` and `+`. Kept local so the
/// crate does not pull a urlencoding dep (roadmap §4.1: prefer not
/// adding heavy deps beyond axum).
fn form_urlencoded_pairs(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next().unwrap_or("");
            let v = it.next().unwrap_or("");
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

/// Decode a percent-encoded query component (`%XX` + `+` → space).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_val(bytes[i + 1]);
                let lo = hex_val(bytes[i + 2]);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi << 4) | lo);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_registration_and_dispatch() {
        let mut app = App::new();
        app.get("/", |_req| Response::text("root")).expect("reg /");
        app.get("/users/<id>", |req| {
            Response::text(format!("user {}", req.path_param("id").unwrap_or("?")))
        })
        .expect("reg /users/<id>");

        let (h, params) = app.dispatch("GET", "/users/7").expect("match");
        assert_eq!(params.get("id").map(String::as_str), Some("7"));
        let resp = h(Request::from_parts(
            "GET",
            "/users/7",
            params,
            HashMap::new(),
            HashMap::new(),
            Vec::new(),
        ));
        assert_eq!(resp.body(), b"user 7");

        // Unmatched path / wrong method -> no dispatch (404).
        assert!(app.dispatch("GET", "/nope").is_none());
        assert!(app.dispatch("POST", "/").is_none());
    }

    #[test]
    fn duplicate_route_is_rejected() {
        let mut app = App::new();
        app.get("/x", |_r| Response::text("a")).expect("first");
        let err = app.get("/x", |_r| Response::text("b")).expect_err("dup");
        assert_eq!(err.kind, crate::error::PitErrorKind::DuplicateRoute);
    }

    #[test]
    fn invalid_route_is_rejected() {
        let mut app = App::new();
        let err = app
            .get("no-slash", |_r| Response::text("x"))
            .expect_err("bad");
        assert_eq!(err.kind, crate::error::PitErrorKind::InvalidRoute);
    }

    #[test]
    fn same_path_different_method_coexist() {
        let mut app = App::new();
        app.get("/r", |_r| Response::text("get")).expect("get");
        app.post("/r", |_r| Response::text("post")).expect("post");
        assert!(app.dispatch("GET", "/r").is_some());
        assert!(app.dispatch("POST", "/r").is_some());
    }

    #[test]
    fn percent_decode_handles_escapes_and_plus() {
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("%41%42"), "AB");
        assert_eq!(percent_decode("plain"), "plain");
        // Malformed trailing percent is passed through, not panicked.
        assert_eq!(percent_decode("%4"), "%4");
    }
}
