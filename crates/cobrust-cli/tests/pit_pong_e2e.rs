//! ADR-0073 first proof — end-to-end `.cb` source → compile → link →
//! run → test client for the `pit` ecosystem-import wiring (Flask
//! web-server, the SIXTH ecosystem module — and the FIRST module
//! exercising the `.cb`↔Rust **callback marshalling** chain).
//!
//! Generalizes the proven flat-intrinsic chain to the LOAD-BEARING
//! callback case: the `.cb` source defines a top-level handler fn
//! whose pointer is materialised at codegen via `Constant::FnRef`,
//! crosses the C ABI as a raw fn pointer, and is invoked from Rust
//! through the trampoline in `cobrust-pit/src/cabi.rs`.
//!
//! ```text
//! `import pit` + `pit.App()` + `app.route(method, path, handler)` +
//! `app.serve_in_background(host, port)` + a top-level `fn handler(req) -> resp:`
//!   → cobrust-types ecosystem manifest (typecheck, EcoParam::Callback)
//!   → cobrust-mir lowering (Constant::FnRef for the callback arg)
//!   → cobrust-codegen fn-pointer materialisation via function_ids
//!   → cobrust-pit C-ABI shims (libpit.a) + trampoline closure
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client probes /ping + /missing
//! ```
//!
//! Pattern (mirrors `ecosystem_strike_e2e.rs`): the test compiles a
//! `.cb` program to an exe, picks a free port (bind-and-drop a
//! `TcpListener`), runs the binary with the port baked into the
//! source, polls the port until the server is up, issues real HTTP,
//! asserts response.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Compile a `.cb` source into an executable and return its path. The
/// caller is responsible for spawning + cleanup.
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

/// Find an ephemeral free port by binding-and-dropping. There is a
/// small TOCTOU window before the `.cb` server claims it; the OS
/// generally won't immediately reassign the port in the gap. The
/// `wait_for_port` poll loop tolerates a missed bind by retrying.
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

/// Compile the `.cb` "pong" program, spawn it, wait for it to bind,
/// then issue real HTTP through reqwest::blocking. The server side
/// uses `serve_in_background` which spawns onto the pit tokio runtime
/// and returns a `ServerHandle`; `main` blocks on `sleep` (busy-wait
/// via a counter so we don't depend on cobrust's std.time) and the
/// `ServerHandle` keeps the server alive until the binary exits.
///
/// The binary intentionally never returns from main (it sleeps in a
/// long-running loop); the test kills it after the assertions via
/// `ChildGuard`.
#[test]
fn test_e2e_pit_pong_full_round_trip() {
    let port = pick_free_port();
    let source = format!(
        concat!(
            "import pit\n",
            "\n",
            "fn handle_ping(req: pit.Request) -> pit.Response:\n",
            "    return pit.text_response(200, \"pong\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            // route() returns Ty::None — `let _ = …` discards.
            "    let _ = app.route(\"GET\", \"/ping\", handle_ping)\n",
            "    let _server = app.serve_in_background(\"127.0.0.1\", {port})\n",
            // Busy-wait keep-alive so the server stays bound. ADR-0073
            // first proof doesn't surface a sleep primitive on the .cb
            // side; a tight counter-loop is plenty for the test (the
            // test kills the child after the assertions). Cobrust
            // `let` shadows-assign in-block, so `i = i + 1` works
            // without a `mut` modifier — see `while` loop precedent
            // in den/strike E2E tests.
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

    wait_for_port(port, Duration::from_secs(8)).expect("pit server bind");

    // /ping → "pong" + 200
    let url = format!("http://127.0.0.1:{port}/ping");
    let resp = reqwest::blocking::get(&url).expect("GET /ping");
    let status = resp.status();
    let body = resp.text().unwrap();
    assert_eq!(status.as_u16(), 200, "expected 200, got {status}");
    assert_eq!(body, "pong", "expected `pong`, got {body:?}");

    // /missing → 404 (Flask's default for an unmatched URL).
    let url404 = format!("http://127.0.0.1:{port}/missing");
    let resp404 = reqwest::blocking::get(&url404).expect("GET /missing");
    assert_eq!(resp404.status().as_u16(), 404, "expected 404 on /missing");

    // Cleanup: kill the child. Guard's Drop handles it.
    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// Negative type-check cases (ADR-0073 §5 R4 — ≥3 required; we ship 5).
//
// Each case feeds a malformed `.cb` source to `cobrust build` and
// asserts the build FAILS with a diagnostic mentioning the callback /
// fn-name / signature constraint. The exit status is non-zero AND the
// stderr carries the canonical phrasing.
// =====================================================================

/// Compile-only helper — returns the build's stderr string (lossy
/// UTF-8) along with `success`. Use for negative cases that expect a
/// non-zero exit.
fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Case 1: lambda passed where a top-level `fn` name is required.
/// Cobrust's `lambda` keyword parses (HIR `ExprKind::Lambda`); the
/// typechecker callback gate then rejects via
/// `CallbackArgMustBeFnName` (not a bare `ExprKind::Name`).
#[test]
fn test_neg_callback_rejects_lambda() {
    let (ok, stderr) = try_build(concat!(
        "import pit\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route(\"GET\", \"/\", lambda r: r)\n",
        "    return 0\n",
    ));
    assert!(!ok, "lambda callback must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("CallbackArgMustBeFnName") || stderr.contains("callback"),
        "stderr must mention callback / fn name; got:\n{stderr}"
    );
}

/// Case 2: wrong-arity handler — takes 0 args, callback slot expects
/// 1 (a `pit.Request`).
#[test]
fn test_neg_callback_rejects_wrong_arity_fn() {
    let (ok, stderr) = try_build(concat!(
        "import pit\n",
        "\n",
        "fn bad_handler() -> pit.Response:\n",
        "    return pit.text_response(200, \"ignored\")\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route(\"GET\", \"/\", bad_handler)\n",
        "    return 0\n",
    ));
    assert!(!ok, "0-arg handler must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("callback") || stderr.contains("signature"),
        "stderr must mention callback signature mismatch; got:\n{stderr}"
    );
}

/// Case 3: wrong-return-type handler — returns `i64`, callback slot
/// expects `pit.Response`.
#[test]
fn test_neg_callback_rejects_wrong_return_type() {
    let (ok, stderr) = try_build(concat!(
        "import pit\n",
        "\n",
        "fn bad_return(req: pit.Request) -> i64:\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route(\"GET\", \"/\", bad_return)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "i64-returning handler must be rejected; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("callback") || stderr.contains("signature"),
        "stderr must mention callback signature mismatch; got:\n{stderr}"
    );
}

/// Case 4: name-bound non-fn (a let-binding) passed where a fn name is
/// required. The `app` name resolves to a `let` whose DefKind is
/// `LetBinding` (not `Fn`); the callback gate rejects.
#[test]
fn test_neg_callback_rejects_non_fn_name() {
    let (ok, stderr) = try_build(concat!(
        "import pit\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        // Pass `app` itself where a handler fn is expected.
        "    let _ = app.route(\"GET\", \"/\", app)\n",
        "    return 0\n",
    ));
    assert!(!ok, "non-fn name must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("callback")
            || stderr.contains("fn")
            || stderr.contains("type mismatch")
            || stderr.contains("signature"),
        "stderr must mention the callback / type mismatch; got:\n{stderr}"
    );
}

/// Case 5: a call-result (`make_handler()`) passed in the callback
/// slot. The expression is not a bare `Name` so the shape gate fires.
#[test]
fn test_neg_callback_rejects_call_result() {
    let (ok, stderr) = try_build(concat!(
        "import pit\n",
        "\n",
        "fn make_handler() -> pit.Response:\n",
        "    return pit.text_response(200, \"x\")\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route(\"GET\", \"/\", make_handler())\n",
        "    return 0\n",
    ));
    assert!(!ok, "call-result must be rejected; stderr=\n{stderr}");
    assert!(
        stderr.contains("callback")
            || stderr.contains("fn")
            || stderr.contains("type mismatch")
            || stderr.contains("signature"),
        "stderr must mention the callback / fn-name; got:\n{stderr}"
    );
}
