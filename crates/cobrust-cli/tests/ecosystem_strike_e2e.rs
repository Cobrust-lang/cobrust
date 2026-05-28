//! ADR-0072 third-module proof — end-to-end `.cb` source → compile →
//! link → run for the `strike` ecosystem-import wiring (HTTP client,
//! rebrand of `requests`).
//!
//! Twin of `ecosystem_den_e2e.rs` and `ecosystem_nest_e2e.rs`. Generalizes
//! the proven flat-intrinsic chain to a THIRD module that pairs the
//! handle pattern (Response, like `den.Connection`/`Cursor`) with
//! free-function entrypoints (`get`/`post`, like `den.connect`).
//!
//! ```text
//! `import strike` + `strike.get(url)` + `resp.text()` + `resp.status_code()`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_strike_*)
//!   → cobrust-codegen externs + Response handle drop schedule
//!   → cobrust-strike C-ABI shims (libstrike.a)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → real HTTP socket to a loopback axum server spun by `pit::App`
//!   → stdout
//! ```
//!
//! The loopback server pattern mirrors `cobrust-pit/tests/pit_downstream.rs`
//! — `pit::App` (the cobrust-pit Flask-shaped server, a workspace member)
//! exposes `serve_in_background(host, port)` which binds an ephemeral
//! port; we capture the bound address and weave it into the `.cb` source
//! string. No external network is touched.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::process::Command;

use pit::{App, Request, Response};

/// Compile + link + run a `.cb` source, returning its stdout. Asserts
/// the build and the run both succeed.
fn build_and_run_source(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
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
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).current_dir(dir.path()).output().unwrap();
    assert!(
        run.status.success(),
        "run failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Build the canonical loopback test app: a `/ping` text route + a
/// `/json` JSON route. Mirrors how `cobrust-pit/tests/pit_downstream.rs`
/// builds its test app.
fn build_strike_test_app() -> App {
    let mut app = App::new();
    app.get("/ping", |_req: Request| Response::text("pong"))
        .expect("register /ping");
    app.get("/json", |_req: Request| {
        Response::json(&serde_json::json!({"x": 42}))
    })
    .expect("register /json");
    app
}

/// ADR-0072 third-module proof — the minimal HTTP `GET /ping` round
/// trip. Spins a loopback `pit::App` on an ephemeral port, has the
/// compiled `.cb` binary fetch the URL through `strike.get(...)`, calls
/// `.text()` and `.status_code()`, and asserts the body + status print
/// the expected canonical lines.
#[test]
fn test_e2e_strike_get_prints_text_and_status() {
    let handle = build_strike_test_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind ephemeral");
    let addr = handle.local_addr();
    let port = addr.port();

    let source = format!(
        concat!(
            "import strike\n",
            "\n",
            "fn main() -> i64:\n",
            "    let resp = strike.get(\"http://127.0.0.1:{port}/ping\")\n",
            "    let body: str = resp.text()\n",
            "    let code: i64 = resp.status_code()\n",
            "    print(body)\n",
            "    print(code)\n",
            "    return 0\n",
        ),
        port = port,
    );

    let stdout = build_and_run_source(&source);
    assert_eq!(stdout, "pong\n200\n");
}

/// `GET /json` then `.json()` — the canonical-JSON rendering path
/// (mirrors `den.fetchall() -> str` first-proof rendering shape).
#[test]
fn test_e2e_strike_get_renders_canonical_json() {
    let handle = build_strike_test_app()
        .serve_in_background("127.0.0.1", 0)
        .expect("bind ephemeral");
    let port = handle.local_addr().port();

    let source = format!(
        concat!(
            "import strike\n",
            "\n",
            "fn main() -> i64:\n",
            "    let resp = strike.get(\"http://127.0.0.1:{port}/json\")\n",
            "    let body: str = resp.json()\n",
            "    let code: i64 = resp.status_code()\n",
            "    print(body)\n",
            "    print(code)\n",
            "    return 0\n",
        ),
        port = port,
    );

    let stdout = build_and_run_source(&source);
    assert_eq!(stdout, "{\"x\":42}\n200\n");
}

/// Error-path E2E — an unreachable / invalid URL must NOT crash; the
/// `.cb` source surface sees the fail-clean sentinel (status 0, empty
/// text) and prints it cleanly. Proves the shim's no-panic guarantee
/// at the C ABI boundary survives the full compile→link→run path.
///
/// We use a literal empty URL string — reqwest's URL parser rejects it
/// at the URL-parse layer, surfacing the `HttpErrorKind::InvalidUrl`
/// path → fail_clean_response(). Choosing an URL-parse failure (rather
/// than e.g. `127.0.0.1:1` unreachable port) keeps the test
/// independent of any locally-configured HTTP proxy.
#[test]
fn test_e2e_strike_unreachable_url_yields_fail_clean_sentinel() {
    let stdout = build_and_run_source(concat!(
        "import strike\n",
        "\n",
        "fn main() -> i64:\n",
        // Empty URL string → invalid URL → fail-clean sentinel.
        "    let resp = strike.get(\"\")\n",
        "    let body: str = resp.text()\n",
        "    let code: i64 = resp.status_code()\n",
        "    print(body)\n",
        "    print(code)\n",
        "    return 0\n",
    ));
    // Empty body line then status 0 line.
    assert_eq!(stdout, "\n0\n");
}
