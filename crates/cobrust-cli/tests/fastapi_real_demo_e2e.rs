//! FastAPI-real CAPSTONE demo E2E — the #156 Phase-1 COMPLETENESS check.
//!
//! The per-feature unit E2Es each pass in isolation:
//!   * `pit_validated_body_e2e.rs`  — int-range validation + 422 path;
//!   * `pit_string_refinement_e2e.rs` — str length + pattern refinements;
//!   * `pit_json_response_e2e.rs`   — `json_response(201, body)` echo;
//!   * `pit_body_field_read_e2e.rs` — `body.field` runtime read + branch;
//!   * `pit_openapi_e2e.rs`         — `serve_openapi` schema emission.
//!
//! This corpus is the COMPLETENESS-CRITIC check: it proves those features
//! COMPOSE in ONE running server, driven by the real
//! `examples/fastapi_real_demo/main.cb` capstone. A single validated
//! `class CreateUser` carries ALL THREE refinement kinds at once (string
//! length on `name`, int range on `age`, string pattern on `email`); the
//! handler READS a validated field (`body.age`) and branches on it (a
//! business rule: adults vs minors); the adult path echoes the body via
//! `json_response`; and the same server serves the derived OpenAPI doc.
//!
//! ```text
//! examples/fastapi_real_demo/main.cb
//!   → cobrust build (frontend → HIR → MIR → codegen → link)
//!   → spawn binary in background (binds 127.0.0.1:<ephemeral port from argv[1]>)
//!   → reqwest::blocking exercises EVERY Phase-1 feature against the ONE server:
//!       · valid adult           → 201 + echoed validated body JSON
//!       · valid minor (age 15)  → 403 (proves the runtime body.age READ drives the branch)
//!       · too-long name (51)    → 422 (str-length refinement, handler NOT entered)
//!       · age 200               → 422 (int-range refinement, handler NOT entered)
//!       · email "nope"          → 422 (str-pattern refinement, handler NOT entered)
//!       · GET /openapi.json     → 200 + schema with all three refinement kinds
//!   → ChildGuard kills the spawn on Drop
//! ```
//!
//! Harness: mirrors `z8_rest_blog_e2e.rs` (compile the REAL example file via
//! `compile_file`, spawn it, drive it over real HTTP) and the pit live-server
//! corpora (`pit_validated_body_e2e.rs` et al.) for the bind/poll/ChildGuard
//! plumbing. The demo accepts the port from argv[1] (z8 pattern) so this
//! harness picks an ephemeral port and never collides with the other pit
//! live-server tests running in parallel.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Test-helper naming + nested-if patterns — the schema-locator helper nests
// two `if let`s for clarity; mirrors the module-level test-lint allows the
// sibling pit E2E corpora carry (pit_openapi_e2e.rs / pit_string_refinement_e2e.rs).
#![allow(clippy::similar_names)]
#![allow(clippy::collapsible_if)]
// The completeness-critic E2E drives SIX probes against ONE running server in
// a single test (that is the point — proving the features compose live), which
// runs long; sibling live-server / E2E corpora carry the same module allow
// (cli_break_continue_e2e.rs / file_io_e2e.rs / string_stdlib_e2e.rs).
#![allow(clippy::too_many_lines)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Shared harness primitives — mirror z8_rest_blog_e2e.rs / pit_pong_e2e.rs.
// =====================================================================

/// Compile a `.cb` SOURCE FILE PATH into an executable and return its path.
/// Used to build the REAL `examples/fastapi_real_demo/main.cb` (mirrors
/// `z8_rest_blog_e2e.rs::compile_file`).
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
/// TOCTOU window before the `.cb` server claims it; the OS generally won't
/// immediately reassign the port in the gap, and `wait_for_port` tolerates a
/// missed bind by retrying.
fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Poll the port until a TCP connection succeeds (server up) or the timeout
/// elapses.
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

/// Locate `examples/fastapi_real_demo/main.cb` from the test binary's
/// working directory. Cargo runs integration tests with `CARGO_MANIFEST_DIR`
/// set to the test crate (`crates/cobrust-cli`); the demo lives at the repo
/// root's `examples/` tree, so we walk up two levels (mirrors
/// `z8_rest_blog_e2e.rs::z8_demo_path`).
fn demo_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // `crates/cobrust-cli` → `<repo>/examples/fastapi_real_demo/main.cb`
    manifest_dir
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // <repo>/
        .join("examples")
        .join("fastapi_real_demo")
        .join("main.cb")
}

// =====================================================================
// Floor smoke: the real capstone source compiles cleanly.
// =====================================================================

/// Compiles `examples/fastapi_real_demo/main.cb` and asserts a clean build.
/// Floor smoke — if the demo source itself rots (a removed/renamed surface),
/// this fires before the full round-trip and isolates a build break from a
/// behavioural one.
#[test]
fn test_e2e_fastapi_real_demo_compiles() {
    let src = demo_path();
    assert!(
        src.exists(),
        "capstone demo source missing at {} — the example must ship with the test",
        src.display()
    );
    let (_dir, exe) = compile_file(&src);
    assert!(
        exe.exists(),
        "demo built but exe path missing: {}",
        exe.display()
    );
}

// =====================================================================
// THE COMPLETENESS-CRITIC E2E: every Phase-1 feature, ONE running server.
//
// One server (the real capstone demo), six probes:
//   1. valid adult     {name:Ada,age:42,email:ada@x.com}  → 201 + echoed body JSON
//   2. valid minor     (age 15)                            → 403 "must be 18..."
//                                                            (proves body.age READ drives the branch)
//   3. too-long name   (51 chars)                          → 422 (str-length), NOT entered
//   4. age 200         (> 150)                             → 422 (int-range), NOT entered
//   5. email "nope"    (no @)                              → 422 (str-pattern), NOT entered
//   6. GET /openapi.json                                   → 200 + schema (all three kinds)
// =====================================================================

#[test]
fn test_e2e_fastapi_real_demo_all_features_compose() {
    let port = pick_free_port();

    let src = demo_path();
    let (_dir, exe) = compile_file(&src);
    // The demo reads the port from argv[1] (z8 pattern). Pass the ephemeral
    // port so parallel test runs never collide.
    let child = Command::new(&exe)
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("capstone demo server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // -----------------------------------------------------------------
    // (1) valid ADULT → 201 + the echoed validated body (json_response).
    //     ALL THREE refinements pass together: name "Ada" (3 chars, in
    //     1..=50), age 42 (in 0..=150), email "ada@x.com" (matches .+@.+).
    // -----------------------------------------------------------------
    let adult_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"Ada","age":42,"email":"ada@x.com"}"#)
        .send()
        .expect("POST /users valid adult");
    let adult_status = adult_resp.status().as_u16();
    let adult_ctype = adult_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let adult_body = adult_resp.text().unwrap();
    assert_eq!(
        adult_status, 201,
        "valid adult (all 3 refinements pass) must be 201 via json_response; \
         got {adult_status}, body={adult_body:?}"
    );
    // The echoed body re-serialises the SAME serde value validation produced
    // (json_response, ADR-0081 §5.3) — so it round-trips every field.
    let parsed: serde_json::Value = serde_json::from_str(&adult_body).unwrap_or_else(|e| {
        panic!("json_response body must be valid JSON, got {adult_body:?}: {e}")
    });
    assert_eq!(
        parsed.get("name").and_then(serde_json::Value::as_str),
        Some("Ada"),
        "echoed body must carry validated name == \"Ada\"; body={adult_body:?}"
    );
    assert_eq!(
        parsed.get("age").and_then(serde_json::Value::as_i64),
        Some(42),
        "echoed body must carry validated age == 42; body={adult_body:?}"
    );
    assert_eq!(
        parsed.get("email").and_then(serde_json::Value::as_str),
        Some("ada@x.com"),
        "echoed body must carry validated email == \"ada@x.com\"; body={adult_body:?}"
    );
    assert_eq!(
        adult_ctype.as_deref(),
        Some("application/json"),
        "json_response must set content-type application/json; got {adult_ctype:?}"
    );

    // -----------------------------------------------------------------
    // (2) valid MINOR (age 15) → 403 "must be 18 or older". THE PROOF that
    //     `body.age` is READ at runtime: this is the SAME route, the SAME
    //     handler, differing from the adult ONLY in the value of `body.age`
    //     (15 vs 42). Both bodies pass validation (15 is in 0..=150), so the
    //     422 path is NOT involved — the divergent status (403 vs 201) can
    //     ONLY come from the handler reading `body.age` and branching. A
    //     constant (un-read) branch could not produce BOTH 201 and 403.
    // -----------------------------------------------------------------
    let minor_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"Kid","age":15,"email":"kid@x.com"}"#)
        .send()
        .expect("POST /users valid minor");
    let minor_status = minor_resp.status().as_u16();
    let minor_body = minor_resp.text().unwrap();
    assert_eq!(
        minor_status, 403,
        "a VALID minor (age 15, passes validation) must hit the business-rule \
         branch → 403 — this PROVES `body.age` is read at runtime (the branch \
         flips with the field value; adult age 42 → 201, minor age 15 → 403); \
         got {minor_status}, body={minor_body:?}"
    );
    assert_eq!(
        minor_body, "must be 18 or older",
        "the minor branch must return the business-rule message; got {minor_body:?}"
    );

    // -----------------------------------------------------------------
    // (3) too-long name (51 chars > max 50) → 422 (STRING-LENGTH refinement),
    //     handler NOT entered. age + email are valid, so this 422 is solely
    //     the name-length violation.
    // -----------------------------------------------------------------
    let long_name = "a".repeat(51);
    let toolong_body = format!(r#"{{"name":"{long_name}","age":42,"email":"a@x.com"}}"#);
    let toolong_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(toolong_body)
        .send()
        .expect("POST /users too-long name");
    let toolong_status = toolong_resp.status().as_u16();
    let toolong_text = toolong_resp.text().unwrap();
    assert_eq!(
        toolong_status, 422,
        "a 51-char name (> max 50) must be 422 (str-length refinement); \
         got {toolong_status}, body={toolong_text:?}"
    );
    // The 422 is synthesised in Rust WITHOUT entering the handler (ADR-0080
    // §5.4), so it can never carry the handler's adult-echo or minor-message.
    assert_ne!(
        toolong_text, "must be 18 or older",
        "the str-length 422 must NOT enter the handler; body={toolong_text:?}"
    );
    assert!(
        !toolong_text.contains("\"name\":\"aaa"),
        "the str-length 422 must NOT echo the body (handler not entered); body={toolong_text:?}"
    );

    // -----------------------------------------------------------------
    // (4) age 200 (> max 150) → 422 (INT-RANGE refinement), handler NOT
    //     entered. name + email are valid, so this 422 is solely the age
    //     range violation.
    // -----------------------------------------------------------------
    let badage_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"Ada","age":200,"email":"a@x.com"}"#)
        .send()
        .expect("POST /users age 200");
    let badage_status = badage_resp.status().as_u16();
    let badage_text = badage_resp.text().unwrap();
    assert_eq!(
        badage_status, 422,
        "age 200 (> max 150) must be 422 (int-range refinement); \
         got {badage_status}, body={badage_text:?}"
    );
    assert_ne!(
        badage_text, "must be 18 or older",
        "the int-range 422 must NOT enter the handler; body={badage_text:?}"
    );

    // -----------------------------------------------------------------
    // (5) email "nope" (no @, fails .+@.+) → 422 (STRING-PATTERN refinement),
    //     handler NOT entered. name + age are valid, so this 422 is solely
    //     the email-pattern violation.
    // -----------------------------------------------------------------
    let bademail_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"Ada","age":42,"email":"nope"}"#)
        .send()
        .expect("POST /users email nope");
    let bademail_status = bademail_resp.status().as_u16();
    let bademail_text = bademail_resp.text().unwrap();
    assert_eq!(
        bademail_status, 422,
        "email \"nope\" (no @, fails .+@.+) must be 422 (str-pattern refinement); \
         got {bademail_status}, body={bademail_text:?}"
    );
    assert_ne!(
        bademail_text, "must be 18 or older",
        "the str-pattern 422 must NOT enter the handler; body={bademail_text:?}"
    );

    // -----------------------------------------------------------------
    // (6) GET /openapi.json → 200 + a valid OpenAPI doc whose CreateUser
    //     schema shows ALL THREE refinement kinds, derived from the SAME
    //     field table the validator reads (cannot drift):
    //       name  → {type:string, minLength:1, maxLength:50}
    //       age   → {type:integer, minimum:0, maximum:150}
    //       email → {type:string, pattern:".+@.+"}
    // -----------------------------------------------------------------
    let openapi_resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    let openapi_status = openapi_resp.status().as_u16();
    let openapi_body = openapi_resp.text().unwrap();
    assert_eq!(
        openapi_status, 200,
        "GET /openapi.json must be 200 (served via serve_openapi); \
         got {openapi_status}, body={openapi_body:?}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&openapi_body).expect("/openapi.json must be valid JSON");
    // OpenAPI/Swagger version marker.
    assert!(
        doc.get("openapi").and_then(|v| v.as_str()).is_some()
            || doc.get("swagger").and_then(|v| v.as_str()).is_some(),
        "the doc must carry an OpenAPI/Swagger version marker; got:\n{openapi_body}"
    );

    let schema = locate_user_schema(&doc).unwrap_or_else(|| {
        panic!(
            "could not locate the CreateUser body schema in the OpenAPI doc \
             (expected components/schemas/CreateUser); got:\n{openapi_body}"
        )
    });

    // name → {type:string, minLength:1, maxLength:50}
    let name_schema = schema
        .get("properties")
        .and_then(|p| p.get("name"))
        .unwrap_or_else(|| panic!("schema must declare property `name`; schema={schema}"));
    assert_eq!(
        name_schema.get("type").and_then(|v| v.as_str()),
        Some("string"),
        "name must be {{type:string}}; got name_schema={name_schema}"
    );
    assert_eq!(
        name_schema
            .get("minLength")
            .and_then(serde_json::Value::as_i64),
        Some(1),
        "name.minLength must be 1; got name_schema={name_schema}"
    );
    assert_eq!(
        name_schema
            .get("maxLength")
            .and_then(serde_json::Value::as_i64),
        Some(50),
        "name.maxLength must be 50; got name_schema={name_schema}"
    );

    // age → {type:integer, minimum:0, maximum:150}
    let age_schema = schema
        .get("properties")
        .and_then(|p| p.get("age"))
        .unwrap_or_else(|| panic!("schema must declare property `age`; schema={schema}"));
    assert_eq!(
        age_schema.get("type").and_then(|v| v.as_str()),
        Some("integer"),
        "age must be {{type:integer}}; got age_schema={age_schema}"
    );
    assert_eq!(
        age_schema
            .get("minimum")
            .and_then(serde_json::Value::as_i64),
        Some(0),
        "age.minimum must be 0; got age_schema={age_schema}"
    );
    assert_eq!(
        age_schema
            .get("maximum")
            .and_then(serde_json::Value::as_i64),
        Some(150),
        "age.maximum must be 150; got age_schema={age_schema}"
    );

    // email → {type:string, pattern:".+@.+"}
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
        Some(".+@.+"),
        "email.pattern must be \".+@.+\"; got email_schema={email_schema}"
    );

    // -----------------------------------------------------------------
    // The cannot-drift property, made concrete: the schema's advertised
    // age.maximum (150) is the EXACT bound the validator enforced (proven by
    // the 422 on age 200 in probe (4)). One source, cannot drift.
    // -----------------------------------------------------------------
    assert_eq!(
        age_schema
            .get("maximum")
            .and_then(serde_json::Value::as_i64),
        Some(150),
        "the schema's advertised age.maximum (150) must equal the bound the \
         validator enforces (proven by the 422 on age 200) — ONE source, cannot drift"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// Helper: locate the CreateUser body schema in an OpenAPI doc. Primary: the
// canonical `components/schemas/CreateUser` path. Fallback: a recursive
// search for any object schema declaring `age` with a `maximum` (robust to a
// future component-key change without weakening the bound assertions).
// =====================================================================

fn locate_user_schema(doc: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(s) = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.get("CreateUser"))
    {
        return Some(s.clone());
    }
    find_schema_with_age_maximum(doc)
}

fn find_schema_with_age_maximum(v: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(props) = v.get("properties").and_then(|p| p.as_object()) {
        if props.get("age").and_then(|a| a.get("maximum")).is_some() {
            return Some(v.clone());
        }
    }
    match v {
        serde_json::Value::Object(map) => {
            for child in map.values() {
                if let Some(found) = find_schema_with_age_maximum(child) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for child in arr {
                if let Some(found) = find_schema_with_age_maximum(child) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}
