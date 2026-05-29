//! ADR-0078 §6.1 Phase-1 first proof (TEST-FIRST / ADSD RED) —
//! end-to-end `.cb` source → compile → link → run → test client for
//! the **tower-http middleware** wiring on pit's `App`.
//!
//! The new surface (ADR-0078 §6.1 Implementation map): `pit.App` gains
//! zero-arg, `Ty::None`-returning methods `use_cors()` / `use_trace()` /
//! `use_compression()` that, **called BEFORE serve**, register the
//! corresponding `tower_http` `Layer` on the axum `Router` so the
//! served responses carry the middleware's effect.
//!
//! ```text
//! `import pit` + `pit.App()` + `app.use_cors()` (BEFORE serve) +
//! `app.route("GET", "/", handler)` + `app.serve_in_background(host, 0)`
//!   → cobrust-types ecosystem manifest (new App-method rows,
//!     receiver pit.App, zero value-args, ret Ty::None — ADR-0078 §6.1)
//!   → cobrust-mir handle-method retarget (rides ADR-0073's
//!     `emit_ecosystem_call`, NO new mechanism)
//!   → cobrust-codegen extern decls `__cobrust_pit_app_use_cors` etc.
//!   → cobrust-pit cabi shims flip a middleware-flag on `App`; `serve`
//!     conditionally `.layer(CorsLayer::permissive())` / `TraceLayer` /
//!     `CompressionLayer` when building the Router
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client asserts the middleware's HTTP effect
//! ```
//!
//! The **load-bearing RED obligation** is the with-CORS case: at HEAD
//! `app.use_cors()` is an unknown method on `pit.App`, so the `.cb`
//! program FAILS to typecheck/build. Once DEV ships the §6.1 wiring,
//! the served response carries `Access-Control-Allow-Origin` (present
//! WITH `use_cors`, ABSENT in the control app without it) — proving
//! `use_cors` is what adds it.
//!
//! Pattern mirrors `pit_pong_e2e.rs` / `decorator_pit_e2e.rs` verbatim:
//! pick a free port, bake it into the source, compile to an exe, spawn,
//! poll the port until bound, issue real HTTP via reqwest::blocking,
//! assert, `ChildGuard` cleanup.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Compile a `.cb` source into an executable and return its path. The
/// caller is responsible for spawning + cleanup. (Mirrors the
/// `pit_pong_e2e` helper — keep it in sync.)
fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

/// Find an ephemeral free port by binding-and-dropping. Small TOCTOU
/// window before the `.cb` server claims it; `wait_for_port` tolerates
/// a missed bind by retrying.
fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Poll the port until a TCP connection succeeds (server up) or the
/// timeout elapses.
fn wait_for_port(port: u16, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "server on port {port} did not come up in {timeout:?}"
    ))
}

/// RAII child-process guard — kills the process on Drop so a failing
/// assertion never leaks the spawned `.cb` binary.
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn a compiled `.cb` server exe and wait for it to bind `port`.
/// Returns the guard (kills on Drop). Panics with captured stderr-style
/// diagnostics on bind timeout.
fn spawn_and_wait(exe: &PathBuf, port: u16) -> ChildGuard {
    let child = Command::new(exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let guard = ChildGuard(child);
    wait_for_port(port, Duration::from_secs(8)).expect("pit server bind");
    guard
}

/// Build a `.cb` pit server program. `middleware_calls` is the block of
/// `let _ = app.use_*()` lines injected BEFORE `serve_in_background`
/// (ADR-0078 §6.1 before-serve contract). `body` is the text the single
/// `GET /` handler returns. The server busy-waits to stay bound (no
/// `.cb` sleep primitive; the test kills the child after asserting —
/// the `pit_pong_e2e` keep-alive precedent).
fn pit_program(port: u16, middleware_calls: &str, body: &str) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            "fn handle_root(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(200, \"{body}\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            // Middleware registration goes BEFORE serve (the §6.1
            // before-serve contract: the flag is read at serve time).
            "{middleware}",
            // route() returns Ty::None — `let _ = …` discards.
            "    let _ = app.route(\"GET\", \"/\", handle_root)\n",
            "    let _server = app.serve_in_background(\"127.0.0.1\", {port})\n",
            // Busy-wait keep-alive so the server stays bound. Mirrors
            // pit_pong_e2e (no `.cb` sleep primitive; test kills child).
            "    let i: i64 = 0\n",
            "    while i < 10000000000:\n",
            "        i = i + 1\n",
            "    return 0\n",
        ),
        body = body,
        middleware = middleware_calls,
        port = port,
    )
}

// =====================================================================
// PRIMARY (load-bearing) — with/without CORS header proof.
//
// `app.use_cors()` (permissive preset = `CorsLayer::permissive()`,
// ADR-0078 §6.1) makes the served response carry the CORS header
// `Access-Control-Allow-Origin`. The paired control app (NO `use_cors`)
// must NOT carry it — proving `use_cors` is exactly what adds it.
//
// RED AT HEAD: `app.use_cors()` is an unknown method on `pit.App`, so
// `compile_source` fails the build assertion. That is the correct
// failing proof obligation.
// =====================================================================

/// Case 1 (PRIMARY): a `.cb` pit app that calls `app.use_cors()` BEFORE
/// `serve_in_background`, registers `GET /`, then a reqwest::blocking
/// GET asserts the response carries `Access-Control-Allow-Origin`.
///
/// `CorsLayer::permissive()` sets `Access-Control-Allow-Origin: *` on
/// responses (and reflects/allows any origin), so a plain GET (no
/// preflight needed) already carries the header. The assertion only
/// requires the header's PRESENCE — robust to permissive's `*` vs
/// origin-reflection rendering.
#[test]
fn test_e2e_use_cors_adds_allow_origin_header() {
    let port = pick_free_port();
    // The middleware call under proof — BEFORE serve.
    let source = pit_program(port, "    let _ = app.use_cors()\n", "cors-on");
    let (_dir, exe) = compile_source(&source);
    let _guard = spawn_and_wait(&exe, port);

    let url = format!("http://127.0.0.1:{port}/");
    let resp = reqwest::blocking::get(&url).expect("GET / (cors-on)");
    let status = resp.status();
    let has_acao = resp.headers().contains_key("access-control-allow-origin");
    let acao = resp
        .headers()
        .get("access-control-allow-origin")
        .map(|v| v.to_str().unwrap_or("<binary>").to_string());
    let body = resp.text().unwrap();

    assert_eq!(status.as_u16(), 200, "expected 200 from cors-on app");
    assert_eq!(body, "cors-on", "expected handler body, got {body:?}");
    assert!(
        has_acao,
        "use_cors() MUST add the `Access-Control-Allow-Origin` header; \
         present headers did not include it (value seen: {acao:?})"
    );
}

/// Case 2 (PRIMARY control): the SAME app shape WITHOUT `use_cors()`
/// must NOT carry the CORS header. This is the differential half that
/// proves `use_cors` is the cause — a plain axum response has no
/// `Access-Control-Allow-Origin`.
#[test]
fn test_e2e_without_use_cors_has_no_allow_origin_header() {
    let port = pick_free_port();
    // Identical program, NO middleware call.
    let source = pit_program(port, "", "cors-off");
    let (_dir, exe) = compile_source(&source);
    let _guard = spawn_and_wait(&exe, port);

    let url = format!("http://127.0.0.1:{port}/");
    let resp = reqwest::blocking::get(&url).expect("GET / (cors-off)");
    let status = resp.status();
    let has_acao = resp.headers().contains_key("access-control-allow-origin");
    let body = resp.text().unwrap();

    assert_eq!(status.as_u16(), 200, "expected 200 from control app");
    assert_eq!(body, "cors-off", "expected handler body, got {body:?}");
    assert!(
        !has_acao,
        "control app WITHOUT use_cors() must NOT carry \
         `Access-Control-Allow-Origin` — its presence would mean the \
         header is added unconditionally, not by use_cors()"
    );
}

// =====================================================================
// use_compression — a large body served with the CompressionLayer on.
//
// ADR-0078 §6.1 maps `use_compression()` to `CompressionLayer`. The
// nice-to-have is a `Content-Encoding: gzip` assertion, but reqwest's
// default-feature build here is `default-features = false` (no `gzip`
// feature — see cobrust-cli Cargo.toml), so the client does not send
// `Accept-Encoding: gzip` and the layer passes the body through
// uncompressed. The LOAD-BEARING assertion is therefore: the server
// still builds, serves, and returns the full body intact with the
// compression layer registered (the layer must not corrupt/drop the
// response when the client doesn't negotiate gzip).
// =====================================================================

/// Case 3: `app.use_compression()` BEFORE serve; a route returning a
/// large (4 KiB-class) body. Assert the server builds + serves + the
/// body round-trips intact with the layer on.
///
/// `Content-Encoding` is asserted as a NICE-TO-HAVE only if present
/// (the test client does not negotiate gzip under
/// `reqwest default-features = false`); the hard requirement is an
/// intact 200 + full body.
#[test]
fn test_e2e_use_compression_serves_large_body() {
    let port = pick_free_port();
    // ~4 KiB body so a real CompressionLayer would have something to
    // act on (tower-http's default min-size threshold is small).
    let big = "abcdefghij".repeat(420); // 4200 bytes
    let source = pit_program(port, "    let _ = app.use_compression()\n", &big);
    let (_dir, exe) = compile_source(&source);
    let _guard = spawn_and_wait(&exe, port);

    let url = format!("http://127.0.0.1:{port}/");
    let resp = reqwest::blocking::get(&url).expect("GET / (compression)");
    let status = resp.status();
    // NICE-TO-HAVE: record Content-Encoding if the client negotiated it.
    let _content_encoding = resp
        .headers()
        .get("content-encoding")
        .map(|v| v.to_str().unwrap_or("<binary>").to_string());
    let body = resp.text().unwrap();

    assert_eq!(
        status.as_u16(),
        200,
        "server with CompressionLayer must still return 200"
    );
    assert_eq!(
        body,
        big,
        "CompressionLayer must not corrupt the body when the client \
         does not negotiate gzip; expected the full {} byte body",
        big.len()
    );
}

// =====================================================================
// use_trace — tracing is a logging side-effect, not observable via
// HTTP. The obligation is only that the layer does not break serving:
// the app builds + serves + returns 200.
// =====================================================================

/// Case 4: `app.use_trace()` BEFORE serve; assert the app builds,
/// serves, and returns 200 with the expected body. `TraceLayer`
/// (ADR-0078 §6.1 = `TraceLayer::new_for_http()`) only emits tracing
/// spans/events — there is no HTTP-observable header — so the proof is
/// "registering it does not break the server".
#[test]
fn test_e2e_use_trace_does_not_break_server() {
    let port = pick_free_port();
    let source = pit_program(port, "    let _ = app.use_trace()\n", "traced");
    let (_dir, exe) = compile_source(&source);
    let _guard = spawn_and_wait(&exe, port);

    let url = format!("http://127.0.0.1:{port}/");
    let resp = reqwest::blocking::get(&url).expect("GET / (trace)");
    let status = resp.status();
    let body = resp.text().unwrap();

    assert_eq!(
        status.as_u16(),
        200,
        "server with TraceLayer must still return 200"
    );
    assert_eq!(body, "traced", "expected handler body, got {body:?}");
}

// =====================================================================
// Stacking — multiple middlewares registered together (CORS + trace +
// compression) must compose without breaking the server, and the CORS
// effect (the only HTTP-observable one) must still be present. This
// guards against a DEV impl that handles one layer but mis-wires the
// `.layer(...)` chain when several are flagged.
// =====================================================================

/// Case 5: all three middlewares registered BEFORE serve. Assert the
/// server serves 200 AND the CORS header is present (the observable
/// proof survives stacking).
#[test]
fn test_e2e_stacked_middleware_composes_and_keeps_cors() {
    let port = pick_free_port();
    let middleware = "    let _ = app.use_cors()\n\
                      \x20\x20\x20\x20let _ = app.use_trace()\n\
                      \x20\x20\x20\x20let _ = app.use_compression()\n";
    let source = pit_program(port, middleware, "stacked");
    let (_dir, exe) = compile_source(&source);
    let _guard = spawn_and_wait(&exe, port);

    let url = format!("http://127.0.0.1:{port}/");
    let resp = reqwest::blocking::get(&url).expect("GET / (stacked)");
    let status = resp.status();
    let has_acao = resp.headers().contains_key("access-control-allow-origin");
    let body = resp.text().unwrap();

    assert_eq!(
        status.as_u16(),
        200,
        "stacked-middleware app must return 200"
    );
    assert_eq!(body, "stacked", "expected handler body, got {body:?}");
    assert!(
        has_acao,
        "CORS effect must survive stacking with trace + compression \
         (the .layer(...) chain must apply all flagged middlewares)"
    );
}

// =====================================================================
// ORDERING contract (ADR-0078 audit LOW finding, §6.1 before-serve
// rationale).
//
// EXPECTATION FOR DEV (documented, not executed): the middleware flag
// is read by pit's `App::serve`/`serve_in_background` at the moment the
// Router is constructed. A `use_cors()` call issued AFTER serve has
// already started is a **no-op** — the Router has already been built
// and bound, so flipping the flag afterward cannot retroactively add
// the layer.
//
// Therefore the DEV impl MUST preserve the before-serve contract:
//   - `use_cors()`/`use_trace()`/`use_compression()` set a flag on the
//     `App` (ADR-0078 §6.1 "App must hold a middleware-flag set");
//   - `serve`/`serve_in_background` reads those flags ONCE when building
//     the Router and applies `.layer(...)` accordingly.
//
// This case is intentionally NOT a runnable `#[test]`: `.cb`'s
// `serve_in_background` does not return control to the user program in
// a way that lets the `.cb` source issue a post-serve `use_cors()` and
// then observe a (correctly absent) effect over HTTP within this
// harness — the keep-alive busy-loop never re-enters user code, and the
// no-op is by definition unobservable as a header. It is recorded here
// as the binding before-serve EXPECTATION so DEV does not "helpfully"
// re-apply layers on every request (which would change the contract and
// could double-apply on a hot path). The PRIMARY with/without-CORS
// cases above already pin that the flag, set before serve, takes effect.
//
// (If a future surface lets `.cb` observe a post-serve call, this
// becomes a runnable case asserting the header is ABSENT when
// `use_cors()` runs after the server is already bound.)
#[test]
fn test_ordering_contract_documented_before_serve() {
    // Placeholder that encodes the contract as an executable invariant
    // on the harness side: the middleware-call snippet is injected
    // strictly BEFORE the `serve_in_background` line in `pit_program`,
    // so any program this suite builds applies middleware before serve.
    let port = 0u16;
    let prog = pit_program(port, "    let _ = app.use_cors()\n", "x");
    let use_idx = prog.find("app.use_cors()").expect("use_cors call present");
    let serve_idx = prog
        .find("app.serve_in_background")
        .expect("serve call present");
    assert!(
        use_idx < serve_idx,
        "the before-serve contract: every middleware call this suite \
         emits precedes serve_in_background (use@{use_idx} < serve@{serve_idx})"
    );
}
