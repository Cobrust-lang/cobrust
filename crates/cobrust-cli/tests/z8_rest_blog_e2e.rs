//! Z.8 REST blog demo E2E harness — v0.7.0 §5 网络 MUST-ship closure.
//!
//! Drives the `examples/z8_rest_blog/main.cb` demo end-to-end:
//!
//! ```text
//! examples/z8_rest_blog/main.cb
//!   → cobrust build (frontend → HIR → MIR → codegen → link)
//!   → spawn binary in background (binds 127.0.0.1:<port>)
//!   → reqwest::blocking probes REST endpoints
//!   → assert HTTP status + JSON body shapes
//!   → ChildGuard kills spawn on Drop
//! ```
//!
//! ## Status post-F65 resolution sprint (2026-05-29)
//!
//! All four tests in this file PASS LIVE — F65 closed G1-G5 in
//! `examples/z8_rest_blog/main.cb`:
//!
//! - **G1 closed** — `req.body() -> str` + `req.path_param(name) -> str`
//!   shipped in `cobrust-pit/src/cabi.rs` + `cobrust-types/src/ecosystem.rs`.
//! - **G2 closed** — `app.run(host, port) -> i64` blocking-serve shipped.
//! - **G3 + G4 closed** — demo opens file-backed SQLite at
//!   `/tmp/z8_blog.sqlite3`; `DROP TABLE IF EXISTS` + `CREATE TABLE` in
//!   `main()` before any route fires. Handlers reopen the same path —
//!   SQLite file-backed semantics make this work (Connection is `!Send`
//!   per ADR-0072 §5 risk 2, so we cannot share a single connection
//!   across handlers).
//! - **G5 closed** — `GET /posts/<id>` + `DELETE /posts/<id>` handlers
//!   using the new `req.path_param("id")` shim.
//!
//! The file ships four live tests (0 ignored):
//!
//! - **`test_e2e_z8_demo_compiles`** — floor smoke (`cobrust build` of
//!   the demo source passes).
//! - **`test_e2e_z8_demo_full_round_trip`** — full round-trip against
//!   the real `examples/z8_rest_blog/main.cb` binary: POST → GET-by-id
//!   → GET-list → DELETE → GET-by-id-404.
//! - **`test_e2e_z8_harness_pattern_proof_inline`** — pit-only minimal
//!   scaffolded harness (regression floor — if the harness pattern
//!   itself breaks, this fires first; the primary test then
//!   distinguishes harness-regression vs demo-regression).
//! - **`test_e2e_z8_harness_method_mismatch_returns_404`** — negative-
//!   sanity (GET-on-POST-only + POST-on-GET-only returns 404 / 405).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Shared harness primitives (mirror pit_pong_e2e.rs).
// =====================================================================

/// Compile a `.cb` SOURCE STRING into an executable and return its path.
/// Used by the inline scaffolded-variant tests.
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

/// Compile a `.cb` SOURCE FILE PATH into an executable and return its
/// path. Used by the primary "real demo" tests against
/// `examples/z8_rest_blog/main.cb`.
fn compile_file(src_path: &Path) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build of {} failed: {}\nstderr: {}",
        src_path.display(),
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

/// Find an ephemeral free port by binding-and-dropping. There is a small
/// TOCTOU window before the `.cb` server claims it; the OS generally
/// won't immediately reassign the port in the gap, and `wait_for_port`
/// tolerates a missed bind by retrying.
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

/// Locate `examples/z8_rest_blog/main.cb` from the test binary's working
/// directory. Cargo runs integration tests with `CARGO_MANIFEST_DIR` set
/// to the test crate (`crates/cobrust-cli`); the demo lives at the repo
/// root's `examples/` tree, so we walk up two levels.
fn z8_demo_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // `crates/cobrust-cli` → `<repo>/examples/z8_rest_blog/main.cb`
    manifest_dir
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // <repo>/
        .join("examples")
        .join("z8_rest_blog")
        .join("main.cb")
}

// =====================================================================
// Primary "real Z.8 demo" tests — LIVE post-F65 (2026-05-29). Both
// previously-ignored tests now exercise the repaired
// `examples/z8_rest_blog/main.cb` end-to-end.
// =====================================================================

/// Compiles `examples/z8_rest_blog/main.cb` and asserts a clean build.
/// Floor smoke test — the F65 resolution sprint closed G1 + G2, so
/// the demo now compiles cleanly.
#[test]
fn test_e2e_z8_demo_compiles() {
    let src = z8_demo_path();
    assert!(
        src.exists(),
        "demo source missing at {} — F65 follow-up should also \
         verify the file is present",
        src.display()
    );
    let (_dir, exe) = compile_file(&src);
    assert!(
        exe.exists(),
        "demo built but exe path missing: {}",
        exe.display()
    );
}

/// Primary E2E — full Z.8 demo round-trip:
///
/// - POST /posts {"title":"hello","body":"world"} → 201 + JSON with id
/// - GET /posts/<id> → 200 + matches POST body
/// - GET /posts → 200 + JSON array containing the post
/// - DELETE /posts/<id> → 204
/// - GET /posts/<id> → 404 (deleted)
///
/// F65 closed G1-G5: the demo compiles, persists state across handler
/// calls (file-backed SQLite), wires by-id GET + DELETE via the new
/// `req.path_param(name)` shim, and accepts a port from argv[1] so this
/// harness can pick an ephemeral port (avoiding collisions with other
/// pit tests running in parallel against fixed port 8080).
#[test]
fn test_e2e_z8_demo_full_round_trip() {
    let port = pick_free_port();

    let src = z8_demo_path();
    let (_dir, exe) = compile_file(&src);
    // F65 repair — demo accepts argv[1] for port (falls back to 8080
    // when absent). Pass the ephemeral port so parallel test runs
    // never collide.
    let child = Command::new(&exe)
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    // Wait up to 8s for the bind (mirrors pit_pong_e2e's tolerance).
    wait_for_port(port, Duration::from_secs(8)).expect("z8 demo server bind");

    let base = format!("http://127.0.0.1:{port}");

    // POST /posts {"title":"hello","body":"world"} → 201
    let client = reqwest::blocking::Client::new();
    let post_resp = client
        .post(format!("{base}/posts"))
        .header("Content-Type", "application/json")
        .body(r#"{"title":"hello","body":"world"}"#)
        .send()
        .expect("POST /posts");
    assert_eq!(
        post_resp.status().as_u16(),
        201,
        "POST /posts → expected 201"
    );
    let post_body: serde_json::Value = post_resp.json().expect("POST /posts JSON body");
    let id = post_body
        .get("id")
        .and_then(serde_json::Value::as_i64)
        .expect("POST response carries an `id`");

    // GET /posts/<id> → 200 + body matches.
    let get_url = format!("{base}/posts/{id}");
    let get_resp = reqwest::blocking::get(&get_url).expect("GET /posts/<id>");
    assert_eq!(get_resp.status().as_u16(), 200, "GET /posts/<id> → 200");
    let get_body: serde_json::Value = get_resp.json().expect("GET /posts/<id> JSON");
    assert_eq!(
        get_body.get("title").and_then(|v| v.as_str()),
        Some("hello")
    );
    assert_eq!(get_body.get("body").and_then(|v| v.as_str()), Some("world"));

    // GET /posts → 200 + array containing this post.
    let list_resp = reqwest::blocking::get(format!("{base}/posts")).expect("GET /posts");
    assert_eq!(list_resp.status().as_u16(), 200);
    let list_body: serde_json::Value = list_resp.json().expect("GET /posts JSON array");
    assert!(
        list_body.as_array().is_some_and(|arr| !arr.is_empty()),
        "GET /posts → expected non-empty array, got {list_body:?}"
    );

    // DELETE /posts/<id> → 204
    let delete_resp = client.delete(&get_url).send().expect("DELETE /posts/<id>");
    assert_eq!(
        delete_resp.status().as_u16(),
        204,
        "DELETE /posts/<id> → 204"
    );

    // GET /posts/<id> → 404 (deleted)
    let get_after_delete = reqwest::blocking::get(&get_url).expect("GET /posts/<id> after DELETE");
    assert_eq!(
        get_after_delete.status().as_u16(),
        404,
        "GET /posts/<id> after DELETE → 404"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// Inline "scaffolded harness pattern" tests — DO PASS today.
//
// These prove the harness shape works using a minimal pit-only `.cb`
// source (no den, no body parsing) that exercises the structural skeleton
// the repaired demo will inhabit:
//
// - multiple routes registered
// - background-served binary
// - real HTTP round-trip through reqwest::blocking
// - cleanup via ChildGuard
//
// When F65 closes, these tests become the regression floor for the
// harness — if the harness pattern breaks while the demo's gaps are still
// being closed, these tests fire BEFORE the primary tests can.
// =====================================================================

/// Inline scaffolded Z.8 harness: a 3-route pit-only `.cb` source that
/// proves the harness pattern (compile → spawn → bind → multi-route HTTP
/// round-trip + 404). Mirrors the structural shape the repaired Z.8 demo
/// will have once F65 closes (route registration form, background-serve
/// keep-alive, real HTTP probe).
///
/// Routes:
/// - `GET /posts` → 200 + `"[]"` (empty list sentinel — den-less stand-in)
/// - `GET /health` → 200 + `"ok"`
/// - `POST /posts` → 201 + `"created"`
/// - unmatched URL → 404 (Flask default, proven in pit_pong_e2e)
#[test]
fn test_e2e_z8_harness_pattern_proof_inline() {
    let port = pick_free_port();
    let source = format!(
        concat!(
            "import pit\n",
            "\n",
            // GET /posts — empty list stand-in. Real demo (once F65
            // closes G1+G3+G4) replaces this with a den `SELECT id,
            // title, body FROM posts` + JSON list rendering.
            "fn list_posts(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(200, \"[]\")\n",
            "\n",
            // GET /health — sanity endpoint, mirrors a "ping" liveness
            // probe.
            "fn health(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(200, \"ok\")\n",
            "\n",
            // POST /posts — canned 201 stand-in. Real demo (once F65
            // closes G1) parses req.body() + den-INSERTs + returns
            // 201 + {\"id\": N} JSON.
            "fn create_post(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(201, \"created\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route(\"GET\", \"/posts\", list_posts)\n",
            "    let _ = app.route(\"GET\", \"/health\", health)\n",
            "    let _ = app.route(\"POST\", \"/posts\", create_post)\n",
            "    let _server = app.serve_in_background(\"127.0.0.1\", {port})\n",
            // Busy-wait keep-alive (mirrors pit_pong_e2e).
            "    let i: i64 = 0\n",
            "    while i < 10000000000:\n",
            "        i = i + 1\n",
            "    return 0\n",
        ),
        port = port,
    );
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("z8 harness pit server bind");

    let client = reqwest::blocking::Client::new();

    // GET /health → "ok" + 200 (floor smoke).
    let resp_health = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .expect("GET /health");
    assert_eq!(resp_health.status().as_u16(), 200);
    assert_eq!(resp_health.text().unwrap(), "ok");

    // GET /posts → "[]" + 200 (empty-list stand-in).
    let resp_list = client
        .get(format!("http://127.0.0.1:{port}/posts"))
        .send()
        .expect("GET /posts");
    assert_eq!(resp_list.status().as_u16(), 200);
    assert_eq!(resp_list.text().unwrap(), "[]");

    // POST /posts → "created" + 201.
    let resp_create = client
        .post(format!("http://127.0.0.1:{port}/posts"))
        .body(r#"{"title":"hello","body":"world"}"#)
        .send()
        .expect("POST /posts");
    assert_eq!(resp_create.status().as_u16(), 201);
    assert_eq!(resp_create.text().unwrap(), "created");

    // Unmatched URL → 404 (Flask default; proven in pit_pong_e2e).
    let resp_missing = client
        .get(format!("http://127.0.0.1:{port}/nonexistent"))
        .send()
        .expect("GET /nonexistent");
    assert_eq!(resp_missing.status().as_u16(), 404);

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

/// Negative-sanity inline test: GET on a POST-only path (and vice
/// versa). Flask's routing returns 404 on a method+path miss in pit's
/// current first-proof implementation (a future tightening to 405
/// Method-Not-Allowed is a separate sprint per the pit manifest comments).
///
/// The harness done-means table calls for "≥2 sanity / negative" cases;
/// this is one. The other is the "/nonexistent → 404" probe inside
/// `test_e2e_z8_harness_pattern_proof_inline` above.
#[test]
fn test_e2e_z8_harness_method_mismatch_returns_404() {
    let port = pick_free_port();
    let source = format!(
        concat!(
            "import pit\n",
            "\n",
            "fn list_posts(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(200, \"[]\")\n",
            "\n",
            "fn create_post(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(201, \"created\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            // ONLY GET /posts registered.
            "    let _ = app.route(\"GET\", \"/posts\", list_posts)\n",
            // ONLY POST /items registered.
            "    let _ = app.route(\"POST\", \"/items\", create_post)\n",
            "    let _server = app.serve_in_background(\"127.0.0.1\", {port})\n",
            "    let i: i64 = 0\n",
            "    while i < 10000000000:\n",
            "        i = i + 1\n",
            "    return 0\n",
        ),
        port = port,
    );
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("z8 harness pit server bind");

    let client = reqwest::blocking::Client::new();

    // POST /posts → 404 (only GET is registered on /posts).
    let resp_wrong_method = client
        .post(format!("http://127.0.0.1:{port}/posts"))
        .body("")
        .send()
        .expect("POST /posts");
    assert!(
        resp_wrong_method.status().as_u16() == 404 || resp_wrong_method.status().as_u16() == 405,
        "POST on GET-only /posts → expected 404 or 405; got {}",
        resp_wrong_method.status()
    );

    // GET /items → 404 (only POST is registered on /items).
    let resp_wrong_method_2 = client
        .get(format!("http://127.0.0.1:{port}/items"))
        .send()
        .expect("GET /items");
    assert!(
        resp_wrong_method_2.status().as_u16() == 404
            || resp_wrong_method_2.status().as_u16() == 405,
        "GET on POST-only /items → expected 404 or 405; got {}",
        resp_wrong_method_2.status()
    );

    // Sanity: registered routes still work.
    let resp_get_posts = client
        .get(format!("http://127.0.0.1:{port}/posts"))
        .send()
        .expect("GET /posts");
    assert_eq!(resp_get_posts.status().as_u16(), 200);

    drop(guard.0.kill());
    let _ = guard.0.wait();
}
