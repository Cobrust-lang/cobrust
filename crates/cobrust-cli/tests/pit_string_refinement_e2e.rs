//! ADR-0080 Phase-2 — STRING refinements (`len(self)` LENGTH bounds +
//! `pattern(self, "<re>")` PATTERN), end-to-end. Mirrors the int-range
//! Phase-1 chain (`pit_validated_body_e2e.rs` + `pit_openapi_e2e.rs`) on a
//! `str` field instead of an `i64` field.
//!
//! TEST-FIRST (ADSD): this corpus is written RED, BEFORE the impl. At HEAD
//! `ecf9298` (ADR-0080 Phase-1 complete: int-range validation + OpenAPI
//! live) it FAILS because the str-refinement surface does not exist yet.
//! Confirmed RED probes (recorded in the dispatch report):
//!
//!   * a LENGTH bound on a `str` field —
//!     `username: str where 1 <= len(self) and len(self) <= 20` —
//!     does not type-check: `interpret_refinement` admits ONLY the fixed
//!     int-range grammar on an `i64` field, so a `len(self)` bound on a
//!     `str` field is rejected with
//!     `error[Type]: unsupported refinement `where`-predicate on field
//!     `username`: only the fixed int-range grammar is accepted in v1`
//!     (its hint literally reads "`len(self) <= n` / `pattern(self, "…")`
//!     are later phases"). The `.cb` FAILS TO BUILD, so the live server
//!     never binds.
//!   * a PATTERN refinement — `email: str where pattern(self, ".+@.+")` —
//!     fails even earlier in HIR name-resolution:
//!     `error[Type]: unknown name `pattern`` (Phase-2 has not yet taught
//!     the refinement parser that `pattern` is a fixed-form keyword, not a
//!     user binding).
//!
//! The feature (ADR-0080 §6 Phase-2 + Phase-3, §5.3, the Q6 fixed
//! grammar): TWO new fixed `where`-clause refinement kinds on a `str`
//! field, alongside the Phase-1 `Refinement::IntRange`:
//!
//!   * `Refinement::StrLen` — `1 <= len(self) and len(self) <= 20` (and the
//!     one-sided forms) → the runtime validator length-checks the
//!     deserialized string; the OpenAPI emitter projects it to
//!     `minLength` / `maxLength` (ADR-0080 §5.3 line 331:
//!     `title: str where 1 <= len(self) <= 255 →
//!      {"type":"string","minLength":1,"maxLength":255}`).
//!   * `Refinement::Pattern` — `pattern(self, ".+@.+")` (a LITERAL regex) →
//!     the validator regex-checks the string; the emitter projects it to
//!     `pattern` (ADR-0080 §5.3 line 339: `pattern(self, re) → pattern`).
//!
//! Both share the cannot-drift single source (ADR-0080 §3 footgun #4): the
//! bound the validator enforces and the bound the schema advertises are
//! TWO projections of the ONE field table + refinement side-table — there
//! is no second declaration to drift from.
//!
//! ## The str-refinement SURFACE assumed by this corpus (DEV may rename)
//!
//! Per the dispatch SURFACE NOTE, this corpus assumes the FIXED forms:
//!
//! ```text
//! username: str where 1 <= len(self) and len(self) <= 20   # LENGTH
//! email:    str where pattern(self, ".+@.+")                # PATTERN (literal regex)
//! ```
//!
//! mirroring the Phase-1 int-range `where` shape verbatim with `self`
//! swapped for `len(self)` (LENGTH) or wrapped in `pattern(self, "<re>")`
//! (PATTERN). The DEV OWNS THE FINAL SURFACE: if the impl spells `len` /
//! `pattern` differently (e.g. `self.len()`, or a different regex-call
//! spelling), the `.cb` source strings below should be renamed to match.
//! The LOAD-BEARING assertions stay surface-agnostic — they pin the HTTP
//! BEHAVIOR (201 valid / 422 on each violation + handler-not-entered) and
//! the OpenAPI BOUNDS (`minLength` / `maxLength` / `pattern`), and the
//! cannot-drift pairing (a 21-char username 422 AND `maxLength:20`).
//!
//! The MUST-HAVE is the LENGTH chain (Phase-2 proper). The PATTERN chain
//! (Phase-3) is exercised by the same live server when the impl lands it;
//! if the DEV ships LENGTH only, the email field should be demoted to a
//! plain `str` and the pattern-specific cases/asserts dropped — but as
//! dispatched, both string forms land together as "Phase-2 STRING
//! refinements", so this corpus drives both.
//!
//! ## Body-serialization form (the Phase-1 SCOPE NOTE, carried forward)
//!
//! As in `pit_validated_body_e2e.rs`, the success handler returns a FIXED
//! `pit.text_response(201, "<marker>")` rather than re-serializing the body
//! (the `.cb`↔serde bridge is a deferred ADR-0080 §9 sub-ADR). The marker
//! gives the handler-NOT-entered assertion for free: the 422 path is
//! synthesised in Rust without entering the handler (§5.4 step 4), so a 422
//! body provably cannot carry the marker.
//!
//! ## Harness
//!
//! Mirrors `pit_validated_body_e2e.rs` / `pit_openapi_e2e.rs` EXACTLY:
//! compile a `.cb` source to an exe, pick an ephemeral free port
//! (bind-and-drop a `TcpListener`), spawn the binary, poll the port until
//! the server binds, issue real HTTP via `reqwest::blocking`, assert
//! status/body, and an RAII `ChildGuard` kills the process on Drop so a
//! failing assertion never leaks the spawned `.cb` server. The keep-alive
//! is `app.run(host, port)` (blocks until killed).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Test-helper naming + nested-if patterns (the live-server harness uses
// `port` / `post` bindings; the schema-locator helper nests two `if let`s
// for clarity) — mirrors the module-level test-lint allows the sibling pit
// E2E corpora carry.
#![allow(clippy::similar_names)]
#![allow(clippy::collapsible_if)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Harness — copied verbatim from pit_validated_body_e2e.rs so the live
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
/// TOCTOU window before the `.cb` server claims it; the `wait_for_port`
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

// =====================================================================
// The Phase-2 program: a body `class` with TWO string-refinement fields —
// `username: str where 1 <= len(self) and len(self) <= 20` (LENGTH) and
// `email: str where pattern(self, ".+@.+")` (PATTERN) — declared BEFORE
// the handler, a 2-arg validated handler, `app.route_validated`, plus the
// explicit OpenAPI opt-in `app.serve_openapi`.
//
// The fixed bounds are the SAME source the validator AND the OpenAPI
// schema project from (ADR-0080 §3 footgun #4):
//   username → minLength:1, maxLength:20
//   email    → pattern:".+@.+"
// =====================================================================

/// What the 201 success path returns. The 422 path (synthesised in Rust
/// without entering the handler, §5.4) can never produce it, giving us the
/// handler-NOT-entered assertion.
const HANDLER_MARKER: &str = "entered-signup-handler";

/// The username LENGTH bounds (the SAME numbers asserted against the
/// validator's 422 behavior AND the OpenAPI `minLength`/`maxLength`).
const USERNAME_MIN_LEN: i64 = 1;
const USERNAME_MAX_LEN: i64 = 20;
/// The email PATTERN (a literal regex; the SAME string asserted against
/// the OpenAPI `pattern`). Deliberately simple ("has an @") so the
/// matching/non-matching test inputs are unambiguous.
const EMAIL_PATTERN: &str = ".+@.+";

fn signup_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The validated request body — the ONE source the validator
            // AND the OpenAPI schema are both projected from. Declared
            // BEFORE the handler (signature-position forward refs to a
            // LATER class are a known limit).
            "class SignupBody:\n",
            // LENGTH refinement (Phase-2): a two-sided `len(self)` bound,
            // mirroring the int-range `0 <= self and self <= 100` shape
            // with `self` swapped for `len(self)`.
            "    username: str where {umin} <= len(self) and len(self) <= {umax}\n",
            // PATTERN refinement (Phase-3): a literal-regex `pattern(...)`
            // call. DEV owns the exact spelling (see module header).
            "    email: str where pattern(self, \"{pat}\")\n",
            "\n",
            // 2-arg handler: the body is a TYPED second parameter. pit
            // validates the JSON body into `body: SignupBody` BEFORE this
            // runs, so reaching here proves validation passed. Returns a
            // FIXED marker (body re-serialization is a deferred §9 bridge).
            "fn signup(req: pit.Request, body: SignupBody) -> pit.Response:\n",
            "    return pit.text_response(201, \"{marker}\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/signup\", signup)\n",
            // The explicit OpenAPI opt-in (Phase-1b-iii surface).
            "    let _ = app.serve_openapi(\"/openapi.json\")\n",
            // app.run(host, port) blocks until the process is killed.
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        umin = USERNAME_MIN_LEN,
        umax = USERNAME_MAX_LEN,
        pat = EMAIL_PATTERN,
        marker = HANDLER_MARKER,
        port = port,
    )
}

// =====================================================================
// MUST-HAVE 1: the live string-refinement validation E2E.
//
// One server, four POSTs:
//   1. valid          {"username":"bob","email":"b@x.com"}  → 201, entered
//   2. too-long user  ("a"×21, > max 20)                    → 422, NOT entered
//   3. empty user     ("", len 0, < min 1)                  → 422, NOT entered
//   4. bad email      ("notanemail", no @)                  → 422, NOT entered
//
// Cases 2+3 pin the LENGTH bound (Phase-2 MUST-HAVE); case 4 pins the
// PATTERN bound (Phase-3). All three violations short-circuit a 422 in
// Rust WITHOUT entering the handler (ADR-0080 §5.4 step 4).
// =====================================================================

#[test]
fn test_e2e_string_refinement_full_round_trip() {
    let port = pick_free_port();
    let source = signup_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit string-refinement server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: valid body → 201, handler entered (marker present). ---
    // "bob" is 3 chars (in [1, 20]); "b@x.com" matches ".+@.+".
    let ok_resp = client
        .post(format!("{base}/signup"))
        .header("Content-Type", "application/json")
        .body(r#"{"username":"bob","email":"b@x.com"}"#)
        .send()
        .expect("POST /signup valid");
    let ok_status = ok_resp.status().as_u16();
    let ok_body = ok_resp.text().unwrap();
    assert_eq!(
        ok_status, 201,
        "valid {{username:bob,email:b@x.com}} must be 201; got {ok_status}, body={ok_body:?}"
    );
    assert!(
        ok_body.contains(HANDLER_MARKER),
        "valid request MUST enter the handler (marker {HANDLER_MARKER:?} present); body={ok_body:?}"
    );

    // --- Case 2: too-long username (21 chars > max 20) → 422, NOT entered. ---
    // This is the Phase-2 LENGTH MUST-HAVE: the upper bound is enforced.
    let long_username = "a".repeat(21);
    let toolong_body = format!(r#"{{"username":"{long_username}","email":"b@x.com"}}"#);
    let toolong_resp = client
        .post(format!("{base}/signup"))
        .header("Content-Type", "application/json")
        .body(toolong_body)
        .send()
        .expect("POST /signup too-long username");
    let toolong_status = toolong_resp.status().as_u16();
    let toolong_text = toolong_resp.text().unwrap();
    assert_eq!(
        toolong_status, 422,
        "a 21-char username (> max {USERNAME_MAX_LEN}) must be 422; got {toolong_status}, body={toolong_text:?}"
    );
    assert!(
        !toolong_text.contains(HANDLER_MARKER),
        "the too-long-username 422 path MUST NOT enter the handler (marker {HANDLER_MARKER:?} ABSENT); body={toolong_text:?}"
    );

    // --- Case 3: empty username (len 0 < min 1) → 422, NOT entered. ---
    // The Phase-2 LENGTH lower bound is enforced.
    let empty_resp = client
        .post(format!("{base}/signup"))
        .header("Content-Type", "application/json")
        .body(r#"{"username":"","email":"b@x.com"}"#)
        .send()
        .expect("POST /signup empty username");
    let empty_status = empty_resp.status().as_u16();
    let empty_text = empty_resp.text().unwrap();
    assert_eq!(
        empty_status, 422,
        "an empty username (len 0 < min {USERNAME_MIN_LEN}) must be 422; got {empty_status}, body={empty_text:?}"
    );
    assert!(
        !empty_text.contains(HANDLER_MARKER),
        "the empty-username 422 path MUST NOT enter the handler; body={empty_text:?}"
    );

    // --- Case 4: non-matching email ("notanemail", no @) → 422, NOT entered. ---
    // The Phase-3 PATTERN case: a string that fails ".+@.+" is rejected.
    // (username "bob" is in-bounds, so this 422 is solely the pattern miss.)
    let bademail_resp = client
        .post(format!("{base}/signup"))
        .header("Content-Type", "application/json")
        .body(r#"{"username":"bob","email":"notanemail"}"#)
        .send()
        .expect("POST /signup non-matching email");
    let bademail_status = bademail_resp.status().as_u16();
    let bademail_text = bademail_resp.text().unwrap();
    assert_eq!(
        bademail_status, 422,
        "a non-matching email (no @, fails {EMAIL_PATTERN:?}) must be 422; got {bademail_status}, body={bademail_text:?}"
    );
    assert!(
        !bademail_text.contains(HANDLER_MARKER),
        "the bad-email 422 path MUST NOT enter the handler; body={bademail_text:?}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 2: GET /openapi.json shows the string-refinement bounds —
//   username → {type:string, minLength:1, maxLength:20}
//   email    → {type:string, pattern:".+@.+"}
// (ADR-0080 §5.3 lines 331/339.)
// =====================================================================

#[test]
fn test_e2e_openapi_shows_string_refinement_bounds() {
    let port = pick_free_port();
    let source = signup_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit string-refinement openapi bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    let resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    let status = resp.status().as_u16();
    let body = resp.text().unwrap();
    assert_eq!(
        status, 200,
        "GET /openapi.json must be 200 (served via serve_openapi); got {status}, body={body:?}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&body).expect("/openapi.json body must be valid JSON");

    let schema = locate_signup_schema(&doc).unwrap_or_else(|| {
        panic!(
            "could not locate the SignupBody body schema in the OpenAPI doc \
             (expected components/schemas/SignupBody per ADR-0080 §5.3); got:\n{body}"
        )
    });

    // --- username: {type:string, minLength:1, maxLength:20} ---
    let username_schema = schema
        .get("properties")
        .and_then(|p| p.get("username"))
        .unwrap_or_else(|| panic!("schema must declare property `username`; schema={schema}"));
    assert_eq!(
        username_schema.get("type").and_then(|v| v.as_str()),
        Some("string"),
        "username must be {{type:string}}; got username_schema={username_schema}"
    );
    assert_eq!(
        username_schema
            .get("minLength")
            .and_then(serde_json::Value::as_i64),
        Some(USERNAME_MIN_LEN),
        "username.minLength must be {USERNAME_MIN_LEN} (the LENGTH lower bound, \
         ADR-0080 §5.3); got username_schema={username_schema}"
    );
    assert_eq!(
        username_schema
            .get("maxLength")
            .and_then(serde_json::Value::as_i64),
        Some(USERNAME_MAX_LEN),
        "username.maxLength must be {USERNAME_MAX_LEN} (the LENGTH upper bound, \
         ADR-0080 §5.3); got username_schema={username_schema}"
    );

    // --- email: {type:string, pattern:".+@.+"} ---
    let email_schema = schema
        .get("properties")
        .and_then(|p| p.get("email"))
        .unwrap_or_else(|| panic!("schema must declare property `email`; schema={schema}"));
    assert_eq!(
        email_schema.get("type").and_then(|v| v.as_str()),
        Some("string"),
        "email must be {{type:string}}; got email_schema={email_schema}"
    );
    assert_eq!(
        email_schema.get("pattern").and_then(|v| v.as_str()),
        Some(EMAIL_PATTERN),
        "email.pattern must be {EMAIL_PATTERN:?} (the PATTERN refinement, \
         ADR-0080 §5.3 line 339); got email_schema={email_schema}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 3 (cannot-drift, ADR-0080 §5.3 / footgun #4): the served
// string-refinement bounds MATCH the validator's behavior — ONE source.
// Against the SAME running server, assert BOTH in one test:
//   * POST /signup with a 21-char username → 422 (the validator enforces
//     maxLength 20); AND
//   * GET /openapi.json shows username.maxLength == 20.
// The validator's enforced bound (20) and the schema's advertised bound
// (20) come from the SAME field-table + side-table, so they cannot drift.
// The same pairing is asserted for the PATTERN: a non-matching email is
// 422 AND the schema advertises the matching pattern.
// =====================================================================

#[test]
fn test_e2e_string_refinement_matches_validator_cannot_drift() {
    let port = pick_free_port();
    let source = signup_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit string-refinement drift bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- (a) The validator REJECTS a 21-char username with 422. ---
    // 21 > 20, so the runtime length-check fails. This is the BEHAVIOR the
    // schema's maxLength must agree with.
    let long_username = "a".repeat(21);
    let toolong_body = format!(r#"{{"username":"{long_username}","email":"b@x.com"}}"#);
    let post = client
        .post(format!("{base}/signup"))
        .header("Content-Type", "application/json")
        .body(toolong_body)
        .send()
        .expect("POST /signup 21-char username");
    let post_status = post.status().as_u16();
    let post_body = post.text().unwrap();
    assert_eq!(
        post_status, 422,
        "a 21-char username (> the enforced max {USERNAME_MAX_LEN}) must be 422 — the \
         validator's behavior the schema must match; got {post_status}, body={post_body:?}"
    );
    assert!(
        !post_body.contains(HANDLER_MARKER),
        "the length-violation 422 must NOT enter the handler; body={post_body:?}"
    );

    // --- (a') The validator REJECTS a non-matching email with 422. ---
    let bademail_post = client
        .post(format!("{base}/signup"))
        .header("Content-Type", "application/json")
        .body(r#"{"username":"bob","email":"notanemail"}"#)
        .send()
        .expect("POST /signup non-matching email");
    assert_eq!(
        bademail_post.status().as_u16(),
        422,
        "a non-matching email (fails {EMAIL_PATTERN:?}) must be 422 — the validator's \
         behavior the schema's pattern must match"
    );

    // --- (b) The served schema ADVERTISES the same bounds. ---
    let resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    assert_eq!(resp.status().as_u16(), 200, "GET /openapi.json must be 200");
    let doc: serde_json::Value =
        serde_json::from_str(&resp.text().unwrap()).expect("/openapi.json is valid JSON");
    let schema = locate_signup_schema(&doc).expect("SignupBody schema present in the OpenAPI doc");

    let advertised_max_len = schema
        .get("properties")
        .and_then(|p| p.get("username"))
        .and_then(|u| u.get("maxLength"))
        .and_then(serde_json::Value::as_i64)
        .expect("username.maxLength present in the schema");
    let advertised_pattern = schema
        .get("properties")
        .and_then(|p| p.get("email"))
        .and_then(|e| e.get("pattern"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .expect("email.pattern present in the schema");

    // --- The cannot-drift property: ONE source, consistent. ---
    // The validator REJECTED a 21-char username (enforcing maxLength 20);
    // the schema ADVERTISES maxLength 20. They agree because both read the
    // SAME field-table + side-table (ADR-0080 §3 footgun #4). Likewise the
    // pattern the validator enforced (proven by the bad-email 422) is the
    // pattern the schema advertises. If a future change moved one without
    // the other, these equalities would break — exactly the drift the
    // single-source design forbids.
    assert_eq!(
        advertised_max_len, USERNAME_MAX_LEN,
        "the schema's advertised username.maxLength ({advertised_max_len}) must equal the \
         bound the validator enforces ({USERNAME_MAX_LEN}, proven by the 422 on a 21-char \
         username) — ONE source, cannot drift (ADR-0080 §5.3 / footgun #4)"
    );
    assert_eq!(
        advertised_pattern, EMAIL_PATTERN,
        "the schema's advertised email.pattern ({advertised_pattern:?}) must equal the \
         pattern the validator enforces ({EMAIL_PATTERN:?}, proven by the 422 on a \
         non-matching email) — ONE source, cannot drift (ADR-0080 §5.3 / footgun #4)"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// COMPILE-TIME NEGATIVE (ADR-0080 Q6): Phase-2 only ADDS `len(self)` /
// `pattern(self, "<re>")` as recognized FIXED forms — it does NOT open the
// `where`-clause to arbitrary expressions. An arbitrary user-fn call in a
// `where` predicate must STILL be rejected with UnsupportedRefinement +
// a §2.5-B FIX suggestion.
// =====================================================================

/// Compile-only helper — `cobrust check` (no codegen). Returns the
/// combined stdout+stderr (what the user sees) along with `success`.
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

/// Negative: a `str` field with a NON-FIXED `where` predicate (an
/// arbitrary user-fn call rather than the fixed `len(self)` / `pattern`
/// grammar). Phase-2 recognizes `len`/`pattern` ONLY — anything else is
/// still `TypeError::UnsupportedRefinement` with a FIX (§2.5-B: errors
/// print the fix, not just the diagnosis).
#[test]
fn test_neg_string_field_rejects_non_fixed_where_predicate() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "fn weird(s: str) -> bool:\n",
        "    return true\n",
        "\n",
        "class SignupBody:\n",
        // Non-fixed predicate: an arbitrary user-fn call, NOT `len(self)`
        // or `pattern(self, …)`. Q6 mandates rejection with a FIX.
        "    username: str where weird(self)\n",
        "\n",
        "fn main() -> i64:\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "a non-fixed `where` predicate on a str field must be rejected; output=\n{out}"
    );
    assert!(
        out.contains("Type") || out.contains("refinement") || out.contains("where"),
        "must be a type/refinement error on the `where` predicate; got:\n{out}"
    );
    // §2.5-B: the diagnostic must print the FIX, not just the diagnosis.
    // Accept any canonical phrasing the impl chooses, but REQUIRE a
    // fix/hint surface naming the recognized fixed forms.
    assert!(
        out.contains("hint")
            || out.contains("suggestion")
            || out.contains("fixed")
            || out.contains("len(self)")
            || out.contains("pattern(self")
            || out.contains("0 <= self"),
        "the refinement error MUST print a FIX suggestion naming the recognized \
         fixed forms (§2.5-B); got:\n{out}"
    );
}

// =====================================================================
// Helper: locate the SignupBody body schema in an OpenAPI doc.
//
// Primary: the canonical `components/schemas/SignupBody` path
// (ADR-0080 §5.3). Fallback: a recursive search for any JSON object that
// has a `properties.username` carrying a `maxLength` — robust to the DEV's
// exact component key / placement without weakening the bound assertions.
// =====================================================================

fn locate_signup_schema(doc: &serde_json::Value) -> Option<serde_json::Value> {
    // Canonical path first.
    if let Some(s) = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.get("SignupBody"))
    {
        return Some(s.clone());
    }
    // Fallback: recursively find an object schema declaring `username` with
    // a `maxLength` (the body schema this corpus's program produces).
    find_schema_with_username_maxlength(doc)
}

fn find_schema_with_username_maxlength(v: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(props) = v.get("properties").and_then(|p| p.as_object()) {
        if props
            .get("username")
            .and_then(|u| u.get("maxLength"))
            .is_some()
        {
            return Some(v.clone());
        }
    }
    match v {
        serde_json::Value::Object(map) => {
            for child in map.values() {
                if let Some(found) = find_schema_with_username_maxlength(child) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for child in arr {
                if let Some(found) = find_schema_with_username_maxlength(child) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}
