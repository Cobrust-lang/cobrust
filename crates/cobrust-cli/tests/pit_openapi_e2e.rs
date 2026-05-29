//! ADR-0080 Phase-1b-iii — OpenAPI schema EMISSION, end-to-end (the LAST
//! slice of Phase-1).
//!
//! TEST-FIRST (ADSD): this corpus is written RED, BEFORE the impl. At HEAD
//! `a1c9d83` it FAILS because the OpenAPI-serving surface does not exist
//! yet:
//!   * `app.serve_openapi("/openapi.json")` is an unknown App method
//!     (`error[Type]: method `serve_openapi` not found`), so the `.cb`
//!     program FAILS TO BUILD; and (even if a different surface name is
//!     chosen) there is no route serving the OpenAPI doc, so
//!     `GET /openapi.json` 404s.
//!
//! Confirmed RED probes are recorded in the dispatch report.
//!
//! The feature (ADR-0080 §2 Q4, §5.3 — the cannot-drift property; §6
//! Phase-1 OpenAPI done-means):
//!
//!   Phase-1b-ii already (verified green at HEAD): a body `class` whose
//!   fields carry types + a per-field int-range refinement
//!   `rank: i64 where 0 <= self <= 100` is parsed into the
//!   `(AdtId, field)` refinement side-table; MIR
//!   (`validated_body_schema_for_handler`) renders a compact
//!   schema-descriptor (`name<TAB>str\nrank<TAB>i64:0:100`) from the SAME
//!   `adt_fields` field-table + `adt_refinements` side-table; the pit
//!   `route_validated` trampoline parses it (`validation::parse_schema`)
//!   for the runtime validator.
//!
//!   1b-iii surfaces THAT SAME parsed schema as an OpenAPI
//!   `components/schemas/<Body>` JSON object, served over HTTP. Because
//!   the validator AND the schema are two projections of the ONE field
//!   table + side-table (ADR-0080 §3 footgun #4), they CANNOT DRIFT — the
//!   bound the validator enforces (rank <= 100) and the bound the schema
//!   advertises (`maximum: 100`) come from a single source.
//!
//! ## The serve surface ASSUMED by this corpus (DEV may rename)
//!
//! Per the dispatch SURFACE NOTE + the elegance-law (an EXPLICIT opt-in,
//! NOT a magic auto-route — the `.cb` author explicitly enables doc
//! serving), this corpus assumes:
//!
//! ```text
//! app.serve_openapi("/openapi.json")   # register the GET route that
//!                                       # serves the derived OpenAPI doc
//! ```
//!
//! a sibling App manifest method of `route` / `use_cors` — `Ty::None`
//! return (discard, mirroring `route`'s in-place-effect discipline so a
//! `let _ = app.serve_openapi(...)` form does not alias a second
//! drop-eligible App handle), runtime symbol of the
//! `__cobrust_pit_app_*` family (so the existing `intrinsics.rs:1385`
//! pit-prefix recognizer matches it for free). The DEV OWNS THE FINAL
//! SURFACE: if the impl spells it differently (e.g.
//! `app.openapi("/openapi.json")`, or a fixed implicit path, or a
//! builder), this corpus's `serve_openapi(...)` line + its build
//! expectation should be renamed to match — the LOAD-BEARING assertions
//! (a valid OpenAPI doc at `GET /openapi.json` showing
//! `rank.maximum == 100`, and the same-source cannot-drift pairing with
//! the 422) are surface-agnostic.
//!
//! ## Harness
//!
//! Mirrors `pit_validated_body_e2e.rs` (which mirrors `pit_pong_e2e.rs`)
//! EXACTLY: compile a `.cb` source to an exe, pick an ephemeral free port
//! (bind-and-drop a `TcpListener`), spawn the binary, poll the port until
//! the server binds, issue real HTTP via `reqwest::blocking`, assert
//! status/body, and an RAII `ChildGuard` kills the process on Drop so a
//! failing assertion never leaks the spawned `.cb` server. The keep-alive
//! is `app.run(host, port)` (blocks until killed, the z8 demo's shape).
//!
//! ```text
//! `import pit` + a body `class` + a 2-arg validated handler +
//! `app.route_validated("POST", "/scores", ...)` +
//! `app.serve_openapi("/openapi.json")` + `app.run(...)`
//!   → cobrust-frontend parse (`class` typed-field body + `where`-clause)
//!   → cobrust-types ecosystem manifest (route_validated + serve_openapi)
//!   → cobrust-types check (class field table; the ONE source)
//!   → cobrust-mir / cobrust-codegen (schema descriptor + the OpenAPI doc)
//!   → cobrust-pit C-ABI (route_validated trampoline + the openapi GET route)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client GETs /openapi.json + POSTs an out-of-range body
//! ```

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

/// The Phase-1b-iii program: the SAME validated body `class` as 1b-ii
/// (a `name: str` field + a `rank: i64 where 0 <= self and self <= 100`
/// refinement, declared BEFORE the handler), a 2-arg validated handler,
/// `app.route_validated("POST", "/scores", ...)`, PLUS the explicit
/// OpenAPI-serving opt-in `app.serve_openapi("/openapi.json")`.
///
/// The success handler returns a fixed marker (body re-serialization is a
/// deferred §9 sub-ADR — see the 1b-ii module header); we don't assert on
/// the 201 body here. 1b-iii's load-bearing surface is the OpenAPI doc.
const HANDLER_MARKER: &str = "entered-create-score-handler";

fn openapi_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The validated request body — the ONE source the validator
            // AND the OpenAPI schema are both projected from (ADR-0080
            // §3 footgun #4). Declared BEFORE the handler (signature-
            // position forward refs to a LATER class are a known limit).
            "class CreateScore:\n",
            "    name: str\n",
            "    rank: i64 where 0 <= self and self <= 100\n",
            "\n",
            "fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:\n",
            "    return pit.text_response(201, \"{marker}\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/scores\", create_score)\n",
            // The EXPLICIT OpenAPI opt-in (the surface this corpus
            // assumes; the DEV may rename — see the module header). It
            // registers a GET route that serves the OpenAPI doc derived
            // from the body class's field table + refinement side-table.
            // `Ty::None` return (sibling of `route` / `use_cors`).
            "    let _ = app.serve_openapi(\"/openapi.json\")\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        marker = HANDLER_MARKER,
        port = port,
    )
}

// =====================================================================
// MUST-HAVE 1: GET /openapi.json → 200 + a VALID OpenAPI doc whose body
// schema shows `name:{type:string}` and `rank:{type:integer, minimum:0,
// maximum:100}` (ADR-0080 §5.3 / §6 Phase-1 OpenAPI done-means).
// =====================================================================

#[test]
fn test_e2e_openapi_doc_served_with_refinement_bounds() {
    let port = pick_free_port();
    let source = openapi_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit openapi server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- GET /openapi.json → 200 + a JSON body that parses as JSON. ---
    let resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    let status = resp.status().as_u16();
    let body = resp.text().unwrap();
    assert_eq!(
        status, 200,
        "GET /openapi.json must be 200 (the doc is explicitly served via \
         serve_openapi); got {status}, body={body:?}"
    );

    let doc: serde_json::Value =
        serde_json::from_str(&body).expect("/openapi.json body must be valid JSON");

    // --- It must be a recognisable OpenAPI document. ---
    // The OpenAPI/Swagger version marker — accept either the 3.x
    // `openapi` field or the 2.0 `swagger` field (the DEV picks the
    // emitter's version; 3.x is the §5.3 reference, OpenAPI 3.1).
    let has_version_marker = doc.get("openapi").and_then(|v| v.as_str()).is_some()
        || doc.get("swagger").and_then(|v| v.as_str()).is_some();
    assert!(
        has_version_marker,
        "the doc must carry an OpenAPI/Swagger version marker \
         (`openapi` or `swagger`); got:\n{body}"
    );

    // --- Locate the body schema. ---
    // The §5.3 reference shape is `components/schemas/CreateScore`. To
    // stay robust to the DEV's exact placement (e.g. an inline request-
    // body schema, or a differently-cased component key), resolve the
    // CreateScore schema flexibly: prefer the canonical components path,
    // else search the whole doc for an object property `rank` carrying
    // `maximum`.
    let schema = locate_score_schema(&doc).unwrap_or_else(|| {
        panic!(
            "could not locate the CreateScore body schema in the OpenAPI doc \
             (expected components/schemas/CreateScore per ADR-0080 §5.3); got:\n{body}"
        )
    });

    // --- name: {type:string} ---
    let name_schema = schema
        .get("properties")
        .and_then(|p| p.get("name"))
        .unwrap_or_else(|| panic!("schema must declare property `name`; schema={schema}"));
    assert_eq!(
        name_schema.get("type").and_then(|v| v.as_str()),
        Some("string"),
        "name must be {{type:string}}; got name_schema={name_schema}"
    );

    // --- rank: {type:integer, minimum:0, maximum:100} ---
    let rank_schema = schema
        .get("properties")
        .and_then(|p| p.get("rank"))
        .unwrap_or_else(|| panic!("schema must declare property `rank`; schema={schema}"));
    assert_eq!(
        rank_schema.get("type").and_then(|v| v.as_str()),
        Some("integer"),
        "rank must be {{type:integer}}; got rank_schema={rank_schema}"
    );
    assert_eq!(
        rank_schema
            .get("minimum")
            .and_then(serde_json::Value::as_i64),
        Some(0),
        "rank.minimum must be 0 (the refinement lower bound, ADR-0080 §5.3); \
         got rank_schema={rank_schema}"
    );
    assert_eq!(
        rank_schema
            .get("maximum")
            .and_then(serde_json::Value::as_i64),
        Some(100),
        "rank.maximum must be 100 (the refinement upper bound, ADR-0080 §5.3); \
         got rank_schema={rank_schema}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 2 (the cannot-drift assertion, ADR-0080 §5.3 / footgun #4):
// the served schema's bounds MATCH the validator's behavior — ONE source,
// consistent. Against the SAME running server, assert BOTH in one test:
//   * POST /scores {"name":"a","rank":200} → 422 (the validator rejects
//     200 because 200 > 100, the 1b-ii runtime guard); AND
//   * GET /openapi.json shows rank.maximum == 100.
// The validator's enforced bound (100) and the schema's advertised bound
// (100) come from the SAME field-table + side-table, so they cannot drift.
// =====================================================================

#[test]
fn test_e2e_openapi_schema_matches_validator_cannot_drift() {
    let port = pick_free_port();
    let source = openapi_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit openapi server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- (a) The validator REJECTS rank:200 with 422 (1b-ii guard). ---
    // 200 > 100, so the runtime range-check fails and the handler is
    // never entered. This is the BEHAVIOR the schema must agree with.
    let post = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":200}"#)
        .send()
        .expect("POST /scores rank=200");
    let post_status = post.status().as_u16();
    let post_body = post.text().unwrap();
    assert_eq!(
        post_status, 422,
        "rank=200 (> the enforced max 100) must be 422 — the validator's \
         behavior the schema must match; got {post_status}, body={post_body:?}"
    );
    assert!(
        !post_body.contains(HANDLER_MARKER),
        "the 422 path must NOT enter the handler (marker {HANDLER_MARKER:?} \
         must be ABSENT); body={post_body:?}"
    );

    // --- (b) The served schema ADVERTISES maximum == 100. ---
    let resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    assert_eq!(resp.status().as_u16(), 200, "GET /openapi.json must be 200");
    let doc: serde_json::Value =
        serde_json::from_str(&resp.text().unwrap()).expect("/openapi.json is valid JSON");
    let schema = locate_score_schema(&doc).expect("CreateScore schema present in the OpenAPI doc");
    let advertised_max = schema
        .get("properties")
        .and_then(|p| p.get("rank"))
        .and_then(|r| r.get("maximum"))
        .and_then(serde_json::Value::as_i64)
        .expect("rank.maximum present in the schema");

    // --- The cannot-drift property: ONE source, consistent. ---
    // The validator REJECTED 200 (enforcing max 100); the schema
    // ADVERTISES maximum 100. They agree because both read the SAME
    // field-table + side-table (ADR-0080 §3 footgun #4). If a future
    // change moved one without the other, this equality would break —
    // which is exactly the drift the single-source design forbids.
    assert_eq!(
        advertised_max, 100,
        "the schema's advertised rank.maximum ({advertised_max}) must equal \
         the bound the validator enforces (100, proven by the 422 on \
         rank=200) — ONE source, cannot drift (ADR-0080 §5.3 / footgun #4)"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// Helper: locate the CreateScore body schema in an OpenAPI doc.
//
// Primary: the canonical `components/schemas/CreateScore` path
// (ADR-0080 §5.3). Fallback: a recursive search for any JSON object that
// has a `properties.rank` carrying a `maximum` — robust to the DEV's
// exact component key / placement without weakening the bound assertions
// (the caller still asserts type/minimum/maximum on whatever is found).
// =====================================================================

fn locate_score_schema(doc: &serde_json::Value) -> Option<serde_json::Value> {
    // Canonical path first.
    if let Some(s) = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.get("CreateScore"))
    {
        return Some(s.clone());
    }
    // Fallback: recursively find an object schema declaring `rank` with a
    // `maximum` (the body schema this corpus's program produces).
    find_schema_with_rank_maximum(doc)
}

fn find_schema_with_rank_maximum(v: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(props) = v.get("properties").and_then(|p| p.as_object()) {
        if props.get("rank").and_then(|r| r.get("maximum")).is_some() {
            return Some(v.clone());
        }
    }
    match v {
        serde_json::Value::Object(map) => {
            for child in map.values() {
                if let Some(found) = find_schema_with_rank_maximum(child) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for child in arr {
                if let Some(found) = find_schema_with_rank_maximum(child) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}
