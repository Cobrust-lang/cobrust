//! ADR-0080 Phase-1b-ii — the runtime validation engine, end-to-end.
//!
//! TEST-FIRST (ADSD): this corpus is written RED, BEFORE the impl. At
//! HEAD `7c58bd5` it FAILS TO BUILD because the surface it exercises does
//! not exist yet:
//!   * the `class CreateScore:` typed-field body does not parse
//!     (`error[Syntax]: expected end of statement, found `:``);
//!   * the per-field refinement `rank: i64 where 0 <= self and self <= 100`
//!     does not parse (same syntax gate);
//!   * `app.route_validated("POST", "/scores", create_score)` is an
//!     unknown method (`error[Type]: method `route_validated` not found`).
//!
//! Confirmed RED probes are recorded in the dispatch report.
//!
//! The feature (ADR-0080 §2 Q1/Q3/Q5/Q7, §5.1-§5.4, §6 Phase-1):
//!   * a body `class` whose fields carry types + an optional per-field
//!     refinement `field: ty where <fixed-pred>` (Approach B);
//!   * a handler declares the body as a TYPED second parameter
//!     `fn create(req: pit.Request, body: CreateScore) -> pit.Response`;
//!   * `app.route_validated(method, path, handler)` deserializes +
//!     validates the JSON body into `<Body>` at the trampoline BEFORE the
//!     handler runs. On `Ok` it calls the handler; on `Err` it
//!     short-circuits a typed **422** and NEVER enters the handler
//!     (footgun #1 + #2 dropped — total boundary deserialization +
//!     Result-error-as-Response).
//!
//! Harness: mirrors `pit_pong_e2e.rs` EXACTLY — compile a `.cb` source to
//! an exe, pick an ephemeral free port (bind-and-drop a `TcpListener`),
//! spawn the binary, poll the port until the server binds, issue real
//! HTTP via `reqwest::blocking`, assert status/body, and an RAII
//! `ChildGuard` kills the process on Drop so a failing assertion never
//! leaks the spawned `.cb` server. The keep-alive is `app.run(host, port)`
//! (blocks until killed, the z8 demo's shape) rather than the pong
//! busy-wait.
//!
//! ```text
//! `import pit` + a body `class` + a 2-arg validated handler +
//! `app.route_validated(...)` + `app.run(...)`
//!   → cobrust-frontend parse (`class` typed-field body + `where`-clause)
//!   → cobrust-types ecosystem manifest (route_validated, 2-arg Callback FnTy)
//!   → cobrust-types check (class field table; body.field typed)
//!   → cobrust-mir / cobrust-codegen (validate_<Body> shim + dual-box ABI)
//!   → cobrust-pit C-ABI trampoline (deserialize+validate → Ok dispatch / Err 422)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client POSTs valid + 3 invalid bodies
//! ```
//!
//! ## Body-serialization form chosen (the dispatch SCOPE NOTE)
//!
//! ADR-0080 §5.1's example handler returns `pit.json_response(201, body)`,
//! which RE-SERIALIZES the validated body. But (verified at HEAD) **(a)
//! `pit.json_response` is NOT in the ecosystem manifest** (only
//! `pit.text_response(i64, str)` exists, `ecosystem.rs:480`) and **(b) the
//! `.cb`-struct → serde re-serialization is an explicitly-DEFERRED §9
//! sub-ADR** ("The `.cb`-value ↔ serde bridge", ADR-0080 §9). Per the
//! dispatch SCOPE NOTE, the MUST-HAVE assertions are the STATUS CODES
//! (201 valid / 422 invalid + handler-not-entered), so this corpus has the
//! success handler return a FIXED `pit.text_response(201, "<marker>")`
//! instead of re-serializing the body. This pins the VALIDATION + 422 path
//! WITHOUT depending on body re-serialization. The marker string also
//! gives us the handler-NOT-entered assertion for free: the 422 path is
//! synthesised in Rust without entering the handler (ADR-0080 §5.4 step 4),
//! so a 422 response body provably cannot carry the handler's marker.
//!
//! When the `.cb`↔serde bridge lands (§9 sub-ADR), a follow-up E2E should
//! re-assert the body PASS-THROUGH (`json_response(201, body)` echoes the
//! validated fields). That is OUT of Phase-1b-ii scope.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Harness — copied verbatim from pit_pong_e2e.rs so the two live-server
// E2Es drive a `.cb` pit binary identically.
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

/// The Phase-1 program (ADR-0080 §6 Phase-1 verbatim, with the
/// body-serialization scope adaptation documented in the module header):
/// a body `class` with a `name: str` field + a `rank: i64 where 0 <= self
/// and self <= 100` refinement, a 2-arg validated handler, and
/// `app.route_validated`. The success handler returns a fixed marker
/// string (NOT `json_response(body)`) — see the module header.
///
/// `HANDLER_MARKER` is what the 201 success path returns; the 422 path
/// (synthesised in Rust without entering the handler, §5.4) can never
/// produce it, giving us the handler-NOT-entered assertion.
const HANDLER_MARKER: &str = "entered-create-score-handler";

fn validated_body_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The validated request body. ADR-0080 §6 Phase-1: typed
            // fields + ONE int-range refinement via a `where`-clause.
            // Declared BEFORE the handler (the ADR notes a
            // signature-position forward ref to a LATER class is a known
            // limitation, so order matters).
            "class CreateScore:\n",
            "    name: str\n",
            "    rank: i64 where 0 <= self and self <= 100\n",
            "\n",
            // 2-arg handler: the body is a TYPED second parameter (Q1/Q5).
            // pit validates the JSON body into `body: CreateScore` BEFORE
            // this runs, so reaching here proves validation passed. We
            // return a FIXED marker (body re-serialization is a deferred
            // §9 sub-ADR — module header).
            "fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:\n",
            "    return pit.text_response(201, \"{marker}\")\n",
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
        marker = HANDLER_MARKER,
        port = port,
    )
}

// =====================================================================
// MUST-HAVE: the live validation E2E (ADR-0080 §6 Phase-1 done-means).
//
// One server, four POSTs:
//   1. valid          {"name":"a","rank":50}    → 201, handler entered
//   2. out-of-range   {"name":"a","rank":200}   → 422, handler NOT entered
//   3. missing field  {"rank":50}               → 422 (total deser)
//   4. wrong type     {"name":"a","rank":"x"}   → 422 (total deser)
// =====================================================================

#[test]
fn test_e2e_validated_body_full_round_trip() {
    let port = pick_free_port();
    let source = validated_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit validated server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: valid body → 201, handler entered (marker present). ---
    let ok_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":50}"#)
        .send()
        .expect("POST /scores valid");
    let ok_status = ok_resp.status().as_u16();
    let ok_body = ok_resp.text().unwrap();
    assert_eq!(
        ok_status, 201,
        "valid {{name:a,rank:50}} must be 201; got {ok_status}, body={ok_body:?}"
    );
    assert!(
        ok_body.contains(HANDLER_MARKER),
        "valid request MUST enter the handler (marker {HANDLER_MARKER:?} present); body={ok_body:?}"
    );

    // --- Case 2: out-of-range rank → 422, handler NOT entered. ---
    // The §5.4 trampoline synthesises a 422 in Rust without calling the
    // handler, so the response body must NOT carry the handler's marker.
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
        "rank=200 (> 100) must be 422; got {oor_status}, body={oor_body:?}"
    );
    assert!(
        !oor_body.contains(HANDLER_MARKER),
        "422 path MUST NOT enter the handler (marker {HANDLER_MARKER:?} must be ABSENT); body={oor_body:?}"
    );

    // --- Case 3: missing required field → 422 (total deserialization). ---
    let missing_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"rank":50}"#)
        .send()
        .expect("POST /scores missing name");
    let missing_status = missing_resp.status().as_u16();
    let missing_body = missing_resp.text().unwrap();
    assert_eq!(
        missing_status, 422,
        "missing `name` must be 422 (total boundary deser); got {missing_status}, body={missing_body:?}"
    );
    assert!(
        !missing_body.contains(HANDLER_MARKER),
        "missing-field 422 MUST NOT enter the handler; body={missing_body:?}"
    );

    // --- Case 4: wrong JSON type for rank → 422 (total deserialization). ---
    let wrongtype_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":"x"}"#)
        .send()
        .expect("POST /scores wrong type");
    let wrongtype_status = wrongtype_resp.status().as_u16();
    let wrongtype_body = wrongtype_resp.text().unwrap();
    assert_eq!(
        wrongtype_status, 422,
        "rank as a string must be 422 (total boundary deser); got {wrongtype_status}, body={wrongtype_body:?}"
    );
    assert!(
        !wrongtype_body.contains(HANDLER_MARKER),
        "wrong-type 422 MUST NOT enter the handler; body={wrongtype_body:?}"
    );

    // Cleanup: kill the child. Guard's Drop handles it too.
    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// COMPILE-TIME NEGATIVES (ADR-0080 §6 Phase-1 done-means ≥3 negatives).
//
// Co-located in this E2E file, mirroring pit_pong_e2e.rs's inline
// negatives. We use `cobrust check` (type-check only, no codegen — the
// error_ux_corpus.rs idiom) so these run without a C toolchain: each fed
// a malformed `.cb` source, asserts the check FAILS with the canonical
// diagnostic phrasing.
//
//   (a) route_validated with a 1-ARG handler → CallbackSignatureMismatch
//   (b) a class field with a NON-FIXED `where` predicate (an arbitrary
//       fn call) → a TypeError carrying a FIX suggestion
//   (c) [bonus] a 2nd param that is NOT a field-tracked class →
//       CallbackSignatureMismatch (ADR-0080 §5.2 / §6 done-means)
// =====================================================================

/// Compile-only helper — `cobrust check` (no codegen). Returns the
/// combined stdout+stderr (lossy UTF-8, what the user sees) along with
/// `success`. Use for negatives that expect a non-zero exit.
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

/// Negative (a): `app.route_validated("POST", "/s", h)` where `h` is a
/// ONE-arg handler (only `req: pit.Request`, missing the `body` param).
/// The validated-route callback `FnTy` is the 2-arg shape
/// `fn(pit.Request, <Body>) -> pit.Response` (ADR-0080 Q5), so the
/// existing `EcoParam::Callback(FnTy)` gate rejects with
/// `CallbackSignatureMismatch` (`error.rs:259`).
#[test]
fn test_neg_route_validated_rejects_one_arg_handler() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "class CreateScore:\n",
        "    name: str\n",
        "    rank: i64 where 0 <= self and self <= 100\n",
        "\n",
        // 1-arg handler — missing the required `body: CreateScore` 2nd param.
        "fn create_score(req: pit.Request) -> pit.Response:\n",
        "    return pit.text_response(201, \"ok\")\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route_validated(\"POST\", \"/s\", create_score)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "1-arg handler on route_validated must be rejected; output=\n{out}"
    );
    assert!(
        out.contains("CallbackSignatureMismatch")
            || out.contains("callback")
            || out.contains("signature"),
        "must mention the callback signature mismatch; got:\n{out}"
    );
}

/// Negative (b): a class field with a NON-FIXED `where` predicate (an
/// arbitrary fn call rather than the fixed `lo <= self <= hi` /
/// `len(self) <= n` / `pattern(self, …)` grammar of ADR-0080 Q6). The
/// checker must reject with a `TypeError` that prints a FIX suggestion
/// (§2.5-B — errors print the fix; every `TypeError` variant carries
/// `suggestion: Option<&'static str>`, `error.rs:8`).
#[test]
fn test_neg_class_field_rejects_non_fixed_where_predicate() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "fn weird(x: i64) -> bool:\n",
        "    return true\n",
        "\n",
        "class CreateScore:\n",
        "    name: str\n",
        // Non-fixed predicate: an arbitrary user-fn call, NOT the fixed
        // `lo <= self <= hi` grammar. Q6 mandates rejection with a FIX.
        "    rank: i64 where weird(self)\n",
        "\n",
        "fn main() -> i64:\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "a non-fixed `where` predicate must be rejected; output=\n{out}"
    );
    // §2.5-B: the diagnostic must print the FIX, not just the diagnosis.
    // The fixed grammar names are the hint surface; accept any of the
    // canonical phrasings the impl may choose, but REQUIRE that a
    // fix/hint is present (the `hint:` line the renderer prints from
    // `suggestion`), so this test pins the error-UX contract, not just
    // the rejection.
    assert!(
        out.contains("Type") || out.contains("refinement") || out.contains("where"),
        "must be a type/refinement error on the `where` predicate; got:\n{out}"
    );
    assert!(
        out.contains("hint")
            || out.contains("suggestion")
            || out.contains("fixed")
            || out.contains("0 <= self")
            || out.contains("len(self)")
            || out.contains("pattern(self"),
        "the refinement error MUST print a FIX suggestion (§2.5-B); got:\n{out}"
    );
}

/// Negative (c) [bonus, ADR-0080 §6 done-means third negative]: a 2nd
/// param that is NOT a field-tracked body class (here a bare `i64`).
/// `route_validated`'s callback `FnTy` requires the 2nd param to be a
/// body class; a non-class 2nd param is a `CallbackSignatureMismatch`.
#[test]
fn test_neg_route_validated_rejects_non_class_body_param() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        // 2nd param is `i64`, not a body class.
        "fn create_score(req: pit.Request, body: i64) -> pit.Response:\n",
        "    return pit.text_response(201, \"ok\")\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route_validated(\"POST\", \"/s\", create_score)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "a non-class body param on route_validated must be rejected; output=\n{out}"
    );
    assert!(
        out.contains("CallbackSignatureMismatch")
            || out.contains("callback")
            || out.contains("signature")
            || out.contains("type mismatch"),
        "must mention the callback signature / type mismatch; got:\n{out}"
    );
}
