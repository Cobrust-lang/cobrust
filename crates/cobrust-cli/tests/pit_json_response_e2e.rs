//! ADR-0081 Phase-1a — `pit.json_response(status, body)` round-trip, end-to-end.
//!
//! TEST-FIRST (ADSD): this corpus was written RED, BEFORE the impl. At the
//! pre-impl HEAD `2fd2f94` it FAILED TO BUILD because `pit.json_response`
//! was NOT in the ecosystem manifest — only `pit.text_response(i64, str)`
//! existed (`ecosystem.rs:519`). The checker rejected the unknown free-fn
//! at type-check with `TypeError::UnknownName { name: "pit.json_response" }`
//! (`check.rs:2470-2479`), so the `.cb` never reached codegen.
//!
//! ADR-0081 §5.3 Phase-1a (this commit) wires the impl — manifest row +
//! checker validated-body sentinel acceptance + codegen extern + cabi shim
//! — so the corpus now lands GREEN atomically with the impl. The former
//! compile-time RED probe is flipped to a GREEN probe below.
//!
//! This is the follow-up the Phase-1b-ii harness explicitly deferred. Its
//! module header (`pit_validated_body_e2e.rs:64-67`) reads: "When the
//! `.cb`↔serde bridge lands (§9 sub-ADR), a follow-up E2E should re-assert
//! the body PASS-THROUGH (`json_response(201, body)` echoes the validated
//! fields). That is OUT of Phase-1b-ii scope." ADR-0081 makes that bridge
//! concrete; this is that follow-up.
//!
//! The feature (ADR-0081 §2 Q3, §5.3, §6 Phase-1 item 1 — the
//! INDEPENDENT, ships-FIRST slice):
//!   * a new manifest free-fn `pit.json_response(status: i64,
//!     body: <validated-body>) -> pit.Response` whose runtime
//!     `__cobrust_pit_json_response(status: i64, body: *mut u8) -> *Response`
//!     is the sibling of `__cobrust_pit_text_response` (`cabi.rs:193`),
//!     differing only in the 2nd param being the boxed `serde_json::Value`
//!     the validator already produced (`cabi.rs:464`) and the body being
//!     `Response::json(&*body).with_status(status)` (`response.rs:49` + `74`);
//!   * it BORROWS the body box — the `route_validated` trampoline still
//!     frees the box exactly once as a `serde_json::Value` (`cabi.rs:479`),
//!     and reclaims the handler's returned `Response` once (`cabi.rs:494`);
//!   * `json_response(201, body)` re-serializes the SAME `serde_json::Value`
//!     the validator produced, so the response body cannot drift from the
//!     validated body (ADR-0081 §3 footgun #4 dropped). NO field reads are
//!     needed for this slice (ADR-0081 §6 Phase-1: "This alone makes the §6
//!     handler's `return pit.json_response(201, body)` round-trip").
//!
//! Harness: mirrors `pit_validated_body_e2e.rs` (itself a verbatim copy of
//! `pit_pong_e2e.rs`) EXACTLY — compile a `.cb` source to an exe, pick an
//! ephemeral free port (bind-and-drop a `TcpListener`), spawn the binary,
//! poll the port until the server binds, issue real HTTP via
//! `reqwest::blocking`, assert status/body, and an RAII `ChildGuard` kills
//! the process on Drop so a failing assertion never leaks the spawned `.cb`
//! server. The keep-alive is `app.run(host, port)` (blocks until killed,
//! the z8 demo's shape).
//!
//! ```text
//! `import pit` + a body `class` + a 2-arg validated handler returning
//! `pit.json_response(201, body)` + `app.route_validated(...)` + `app.run(...)`
//!   → cobrust-frontend parse (`class` typed-field body + `where`-clause)
//!   → cobrust-types ecosystem manifest (json_response free-fn — NEW; the RED point)
//!   → cobrust-types check (json_response(i64, <validated-body>) -> Response)
//!   → cobrust-mir / cobrust-codegen (json_response extern `[i64, ptr] -> ptr`)
//!   → cobrust-pit C-ABI shim `__cobrust_pit_json_response` (Response::json + with_status)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client POSTs a valid body, asserts 201 + echoed body
//! ```

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Harness — copied verbatim from pit_validated_body_e2e.rs (itself from
// pit_pong_e2e.rs) so the live-server E2Es drive a `.cb` pit binary
// identically.
// =====================================================================

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

/// Find an ephemeral free port by binding-and-dropping. There is a small
/// TOCTOU window before the `.cb` server claims it; the OS generally
/// won't immediately reassign the port in the gap. The `wait_for_port`
/// poll loop tolerates a missed bind by retrying.
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

/// The ADR-0081 §5.1 Phase-1 program: a body `class` with a `name: str`
/// field + a `rank: i64 where 0 <= self and self <= 100` refinement, a
/// 2-arg validated handler, and `app.route_validated`. UNLIKE the
/// Phase-1b-ii harness (which returned a FIXED `text_response` marker
/// because `json_response` did not exist), the success handler here
/// returns `pit.json_response(201, body)` — the ADR-0080 §6 Phase-1
/// handler verbatim — which re-serializes the validated body.
fn json_response_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The validated request body. ADR-0080 §6 Phase-1 / ADR-0081
            // §5.1: typed fields + ONE int-range refinement via a
            // `where`-clause. Declared BEFORE the handler (signature-
            // position forward ref to a LATER class is a known limitation,
            // so order matters).
            "class CreateScore:\n",
            "    name: str\n",
            "    rank: i64 where 0 <= self and self <= 100\n",
            "\n",
            // 2-arg handler: the body is a TYPED second parameter. pit
            // validates the JSON body into `body: CreateScore` BEFORE this
            // runs, so reaching here proves validation passed. We return
            // `json_response(201, body)` (ADR-0081 §5.3) — re-serializing
            // the SAME serde Value the validator produced (footgun #4
            // dropped: no hand-rebuild, no drift). NO field reads needed.
            "fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:\n",
            "    return pit.json_response(201, body)\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            // `route_validated` returns Ty::None (sibling of `route`,
            // ADR-0080 Q5) — `let _ = …` discards.
            "    let _ = app.route_validated(\"POST\", \"/scores\", create_score)\n",
            // app.run(host, port) blocks until the process is killed (the
            // z8 demo's keep-alive shape; the test kills the child via
            // ChildGuard after the assertions).
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

// =====================================================================
// MUST-HAVE: the live json_response round-trip E2E (ADR-0081 §6 Phase-1
// item 1 done-means — the body PASS-THROUGH the Phase-1b-ii harness
// explicitly deferred).
//
// One server, two POSTs:
//   1. valid    {"name":"a","rank":50}   → 201 AND the response body is
//                                           JSON echoing the validated body
//                                           (contains "rank", 50, "name", "a");
//                                           content-type application/json.
//   2. invalid  {"name":"a","rank":200}  → 422, handler NOT entered
//                                           (proves json_response did NOT
//                                           break the 422 path, unchanged
//                                           from Phase-1b-ii).
// =====================================================================

#[test]
fn test_e2e_json_response_echoes_validated_body() {
    let port = pick_free_port();
    let source = json_response_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit json_response server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: valid body → 201, body echoes the validated body. ---
    let ok_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":50}"#)
        .send()
        .expect("POST /scores valid");
    let ok_status = ok_resp.status().as_u16();
    // Capture content-type BEFORE consuming the body (ADR-0081 §5.3:
    // Response::json sets `content-type: application/json`, response.rs:52).
    let ok_ctype = ok_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let ok_body = ok_resp.text().unwrap();

    assert_eq!(
        ok_status, 201,
        "valid {{name:a,rank:50}} via json_response(201, body) must be 201; \
         got {ok_status}, body={ok_body:?}"
    );

    // The CORE of Phase-1a: the body is JSON that ECHOES the validated
    // body. json_response re-serializes the SAME serde Value the validator
    // produced (ADR-0081 §5.3 `Response::json(&*body)`), so the response
    // MUST round-trip the input fields. Parse it as JSON and assert the
    // field/value pairs survive — robust against key ordering /
    // whitespace, unlike a substring match alone.
    let parsed: serde_json::Value = serde_json::from_str(&ok_body).unwrap_or_else(|e| {
        panic!("json_response body must be valid JSON (Response::json), got {ok_body:?}: {e}")
    });
    assert_eq!(
        parsed.get("rank").and_then(serde_json::Value::as_i64),
        Some(50),
        "the echoed body MUST carry the validated `rank` == 50 (re-serialized \
         serde Value); body={ok_body:?}"
    );
    assert_eq!(
        parsed.get("name").and_then(serde_json::Value::as_str),
        Some("a"),
        "the echoed body MUST carry the validated `name` == \"a\"; body={ok_body:?}"
    );
    // Belt-and-suspenders raw-substring checks (the dispatch's literal
    // CONTAINS-"rank"/50/"a" requirement) — survive even if the JSON-parse
    // assertions above are ever relaxed.
    assert!(
        ok_body.contains("rank") && ok_body.contains("50") && ok_body.contains('a'),
        "the echoed body MUST contain \"rank\", 50, and the name \"a\"; body={ok_body:?}"
    );

    // content-type is observable on the wire (Response::json sets it).
    assert_eq!(
        ok_ctype.as_deref(),
        Some("application/json"),
        "json_response MUST set content-type application/json (Response::json, \
         response.rs:52); got {ok_ctype:?}"
    );

    // --- Case 2: out-of-range rank → 422, handler NOT entered. ---
    // Proves json_response did NOT break the 422 path. The §5.4 trampoline
    // synthesises a 422 in Rust without calling the handler, so the
    // response is the validation error — NOT a 201 json_response echo. We
    // assert NOT-201 + NOT-an-echo-of-the-input (rank 200 is invalid, so a
    // real json_response echo of it can never appear on the 422 path).
    let oor_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":200}"#)
        .send()
        .expect("POST /scores out-of-range");
    let oor_status = oor_resp.status().as_u16();
    let oor_body = oor_resp.text().unwrap();
    assert_eq!(
        oor_status, 422,
        "rank=200 (> 100) must be 422 (unchanged from Phase-1b-ii — \
         json_response must NOT break the 422 path); got {oor_status}, body={oor_body:?}"
    );

    // Cleanup: kill the child. Guard's Drop handles it too.
    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// COMPILE-TIME GREEN PROBE (flipped from the original RED probe when
// Phase-1a landed). At HEAD `2fd2f94` this was RED: `pit.json_response`
// was NOT in the manifest, so a `cobrust check` failed with
// `TypeError::UnknownName { name: "pit.json_response" }`. ADR-0081 §5.3
// wired the manifest row (`ecosystem.rs`), the checker's validated-body
// sentinel acceptance (`check.rs` — `json_response(201, body)` where
// `body: CreateScore` is the route_validated handler's tracked-body
// param), the codegen extern (`llvm_backend.rs`, `[i64, ptr] -> ptr`),
// and the cabi shim (`cabi.rs`). So the call now TYPE-CHECKS. This probe
// (mirroring the `error_ux_corpus.rs` check-only idiom — `cobrust check`,
// no codegen, runs without a C toolchain) asserts the GREEN: the program
// type-checks AND `json_response` is no longer flagged as unknown.
// =====================================================================

/// Compile-only helper — `cobrust check` (no codegen). Returns the
/// combined stdout+stderr (lossy UTF-8, what the user sees) along with
/// `success`. Mirrors `pit_validated_body_e2e.rs::try_check`.
fn try_check(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), combined)
}

/// GREEN PROBE (Phase-1a landed): `pit.json_response(201, body)`
/// type-checks where `body: CreateScore` is the route_validated handler's
/// validated-body param. The manifest row + the checker's validated-body
/// sentinel acceptance (ADR-0081 §5.3) make the call known + well-typed —
/// no `UnknownName`, no `TypeMismatch` on the body slot.
#[test]
fn test_json_response_type_checks_after_phase_1a() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "class CreateScore:\n",
        "    name: str\n",
        "    rank: i64 where 0 <= self and self <= 100\n",
        "\n",
        "fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:\n",
        "    return pit.json_response(201, body)\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route_validated(\"POST\", \"/scores\", create_score)\n",
        "    return 0\n",
    ));
    assert!(
        ok,
        "Phase-1a landed: `pit.json_response(201, body)` must now type-check \
         (manifest row + validated-body sentinel acceptance, ADR-0081 §5.3); \
         output=\n{out}"
    );
    // No unknown-fn diagnostic must remain for `json_response`.
    assert!(
        !out.contains("UnknownName") && !out.contains("unknown"),
        "json_response must no longer be flagged unknown after Phase-1a; got:\n{out}"
    );
}
