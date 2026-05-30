//! ADR-0080 Phase-3a — f64 value-range refinement (`FloatRange`), end-to-end.
//! The precise MIRROR of the Phase-1 int-range chain
//! (`pit_validated_body_e2e.rs` + `pit_openapi_e2e.rs`) on an `f64` field
//! instead of an `i64` field.
//!
//! The feature (ADR-0080 §Phase-3a, §5.3, the Q6 fixed grammar): ONE new
//! fixed `where`-clause refinement kind on an `f64` field, the exact dual of
//! `Refinement::IntRange`:
//!
//!   * `Refinement::FloatRange` — `0.0 <= self and self <= 1.0` (and the
//!     one-sided forms) → the runtime validator value-range-checks the
//!     deserialized JSON number; the OpenAPI emitter projects it to
//!     `minimum` / `maximum` on a `{"type":"number"}` schema (ADR-0080 §5.3:
//!     `ratio: f64 where 0 <= self <= 100 →
//!      {"type":"number","minimum":0,"maximum":100}`).
//!
//! Shares the cannot-drift single source (ADR-0080 §3 footgun #4): the bound
//! the validator enforces and the bound the schema advertises are TWO
//! projections of the ONE field table + refinement side-table — there is no
//! second declaration to drift from. The descriptor encode (`float_suffix` in
//! cobrust-types) and decode (`parse_float_suffix` in cobrust-pit) round-trip
//! exactly through `f64` `Display` ↔ `parse::<f64>()`.
//!
//! ## The f64-refinement SURFACE
//!
//! ```text
//! ratio: f64 where 0.0 <= self and self <= 1.0   # VALUE RANGE (inclusive)
//! ```
//!
//! mirroring the Phase-1 int-range `where` shape verbatim with `i64` swapped
//! for `f64` and integer bounds swapped for float bounds. The LOAD-BEARING
//! assertions pin the HTTP BEHAVIOR (201 valid / 422 on each violation +
//! handler-not-entered) and the OpenAPI BOUNDS (`type:number` + `minimum` /
//! `maximum`), and the cannot-drift pairing (an out-of-range value 422 AND
//! `maximum:1`).
//!
//! ## Body-serialization form (the Phase-1 SCOPE NOTE, carried forward)
//!
//! As in the sibling E2Es, the success handler returns a FIXED
//! `pit.text_response(201, "<marker>")` rather than re-serializing the body
//! (the `.cb`↔serde bridge is a deferred ADR-0080 §9 sub-ADR). The marker
//! gives the handler-NOT-entered assertion for free: the 422 path is
//! synthesised in Rust without entering the handler (§5.4 step 4), so a 422
//! body provably cannot carry the marker.
//!
//! ## Harness
//!
//! Mirrors `pit_string_refinement_e2e.rs` EXACTLY: compile a `.cb` source to
//! an exe, pick an ephemeral free port (bind-and-drop a `TcpListener`), spawn
//! the binary, poll the port until the server binds, issue real HTTP via
//! `reqwest::blocking`, assert status/body, and an RAII `ChildGuard` kills the
//! process on Drop so a failing assertion never leaks the spawned `.cb`
//! server. The keep-alive is `app.run(host, port)` (blocks until killed).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::similar_names)]
#![allow(clippy::collapsible_if)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Harness — copied verbatim from pit_string_refinement_e2e.rs so the live
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
// The Phase-3a program: a body `class` with one f64-refinement field —
// `ratio: f64 where 0.0 <= self and self <= 1.0` (VALUE RANGE) — declared
// BEFORE the handler, a 2-arg validated handler, `app.route_validated`, plus
// the explicit OpenAPI opt-in `app.serve_openapi`.
//
// The fixed bounds are the SAME source the validator AND the OpenAPI schema
// project from (ADR-0080 §3 footgun #4):
//   ratio → {type:number, minimum:0, maximum:1}
// =====================================================================

/// What the 201 success path returns. The 422 path (synthesised in Rust
/// without entering the handler, §5.4) can never produce it, giving us the
/// handler-NOT-entered assertion.
const HANDLER_MARKER: &str = "entered-reading-handler";

/// The ratio VALUE bounds (the SAME numbers asserted against the validator's
/// 422 behavior AND the OpenAPI `minimum`/`maximum`). Whole values so the
/// JSON-number assertions are unambiguous; the FRACTIONAL-bound round-trip
/// is covered by the unit tests in `cobrust-pit` (`f64:0.5:99.9`).
const RATIO_MIN: f64 = 0.0;
const RATIO_MAX: f64 = 1.0;

fn reading_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The validated request body — the ONE source the validator AND
            // the OpenAPI schema are both projected from. Declared BEFORE the
            // handler (signature-position forward refs to a LATER class are a
            // known limit).
            "class Reading:\n",
            "    name: str\n",
            // FLOAT VALUE-RANGE refinement (Phase-3a): a two-sided inclusive
            // bound, mirroring the int-range `0 <= self and self <= 100`
            // shape with `i64` swapped for `f64` and float bounds.
            "    ratio: f64 where {rmin} <= self and self <= {rmax}\n",
            "\n",
            // 2-arg handler: the body is a TYPED second parameter. pit
            // validates the JSON body into `body: Reading` BEFORE this runs,
            // so reaching here proves validation passed. Returns a FIXED
            // marker (body re-serialization is a deferred §9 bridge).
            "fn submit(req: pit.Request, body: Reading) -> pit.Response:\n",
            "    return pit.text_response(201, \"{marker}\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/readings\", submit)\n",
            // The explicit OpenAPI opt-in (Phase-1b-iii surface).
            "    let _ = app.serve_openapi(\"/openapi.json\")\n",
            // app.run(host, port) blocks until the process is killed.
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        rmin = RATIO_MIN,
        rmax = RATIO_MAX,
        marker = HANDLER_MARKER,
        port = port,
    )
}

// =====================================================================
// MUST-HAVE 1: the live f64-refinement validation E2E.
//
// One server, four POSTs:
//   1. valid          {"name":"a","ratio":0.5}   → 201, entered
//   2. above-max      ({ratio:1.5}, > max 1.0)    → 422, NOT entered
//   3. below-min      ({ratio:-0.5}, < min 0.0)   → 422, NOT entered
//   4. wrong type     ({ratio:"x"}, not a number) → 422, NOT entered
//
// Cases 2+3 pin the VALUE RANGE (the Phase-3a MUST-HAVE); case 4 the total
// boundary deserialization. All violations short-circuit a 422 in Rust
// WITHOUT entering the handler (ADR-0080 §5.4 step 4).
// =====================================================================

#[test]
fn test_e2e_float_refinement_full_round_trip() {
    let port = pick_free_port();
    let source = reading_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit float-refinement server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: valid body (ratio 0.5 ∈ [0, 1]) → 201, handler entered. ---
    let ok_resp = client
        .post(format!("{base}/readings"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":0.5}"#)
        .send()
        .expect("POST /readings valid");
    let ok_status = ok_resp.status().as_u16();
    let ok_body = ok_resp.text().unwrap();
    assert_eq!(
        ok_status, 201,
        "valid {{name:a,ratio:0.5}} must be 201; got {ok_status}, body={ok_body:?}"
    );
    assert!(
        ok_body.contains(HANDLER_MARKER),
        "valid request MUST enter the handler (marker {HANDLER_MARKER:?} present); body={ok_body:?}"
    );

    // --- Case 2: above-max ratio (1.5 > max 1.0) → 422, NOT entered. ---
    let above_resp = client
        .post(format!("{base}/readings"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":1.5}"#)
        .send()
        .expect("POST /readings above-max ratio");
    let above_status = above_resp.status().as_u16();
    let above_text = above_resp.text().unwrap();
    assert_eq!(
        above_status, 422,
        "ratio 1.5 (> max {RATIO_MAX}) must be 422; got {above_status}, body={above_text:?}"
    );
    assert!(
        !above_text.contains(HANDLER_MARKER),
        "the above-max 422 path MUST NOT enter the handler (marker ABSENT); body={above_text:?}"
    );

    // --- Case 3: below-min ratio (-0.5 < min 0.0) → 422, NOT entered. ---
    let below_resp = client
        .post(format!("{base}/readings"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":-0.5}"#)
        .send()
        .expect("POST /readings below-min ratio");
    let below_status = below_resp.status().as_u16();
    let below_text = below_resp.text().unwrap();
    assert_eq!(
        below_status, 422,
        "ratio -0.5 (< min {RATIO_MIN}) must be 422; got {below_status}, body={below_text:?}"
    );
    assert!(
        !below_text.contains(HANDLER_MARKER),
        "the below-min 422 path MUST NOT enter the handler; body={below_text:?}"
    );

    // --- Case 4: wrong type (ratio "x", not a number) → 422, NOT entered. ---
    let badtype_resp = client
        .post(format!("{base}/readings"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":"x"}"#)
        .send()
        .expect("POST /readings wrong-type ratio");
    let badtype_status = badtype_resp.status().as_u16();
    let badtype_text = badtype_resp.text().unwrap();
    assert_eq!(
        badtype_status, 422,
        "ratio \"x\" (not a number) must be 422 at the boundary; got {badtype_status}, body={badtype_text:?}"
    );
    assert!(
        !badtype_text.contains(HANDLER_MARKER),
        "the wrong-type 422 path MUST NOT enter the handler; body={badtype_text:?}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 2: GET /openapi.json shows the f64-refinement bounds —
//   ratio → {type:number, minimum:0, maximum:1}
// (ADR-0080 §5.3 / Phase-3a D4.)
// =====================================================================

#[test]
fn test_e2e_openapi_shows_float_refinement_bounds() {
    let port = pick_free_port();
    let source = reading_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit float-refinement openapi bind");

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

    let schema = locate_reading_schema(&doc).unwrap_or_else(|| {
        panic!(
            "could not locate the Reading body schema in the OpenAPI doc \
             (expected components/schemas/Reading per ADR-0080 §5.3); got:\n{body}"
        )
    });

    // --- ratio: {type:number, minimum:0, maximum:1} ---
    let ratio_schema = schema
        .get("properties")
        .and_then(|p| p.get("ratio"))
        .unwrap_or_else(|| panic!("schema must declare property `ratio`; schema={schema}"));
    assert_eq!(
        ratio_schema.get("type").and_then(|v| v.as_str()),
        Some("number"),
        "ratio must be {{type:number}} (NOT integer); got ratio_schema={ratio_schema}"
    );
    let adv_min = ratio_schema
        .get("minimum")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| {
            panic!("ratio.minimum must be present; got ratio_schema={ratio_schema}")
        });
    let adv_max = ratio_schema
        .get("maximum")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| {
            panic!("ratio.maximum must be present; got ratio_schema={ratio_schema}")
        });
    assert!(
        (adv_min - RATIO_MIN).abs() < f64::EPSILON,
        "ratio.minimum must be {RATIO_MIN} (the lower bound, ADR-0080 §5.3); got {adv_min}"
    );
    assert!(
        (adv_max - RATIO_MAX).abs() < f64::EPSILON,
        "ratio.maximum must be {RATIO_MAX} (the upper bound, ADR-0080 §5.3); got {adv_max}"
    );
    // A `number` value range must NOT carry the str-length keywords.
    assert!(
        ratio_schema.get("minLength").is_none() && ratio_schema.get("maxLength").is_none(),
        "an f64 value range must NOT emit minLength/maxLength; got ratio_schema={ratio_schema}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 3 (cannot-drift, ADR-0080 §5.3 / footgun #4): the served
// f64-refinement bound MATCHES the validator's behavior — ONE source.
// Against the SAME running server, assert BOTH in one test:
//   * POST /readings with ratio 1.5 → 422 (the validator enforces
//     maximum 1.0); AND
//   * GET /openapi.json shows ratio.maximum == 1.0.
// The validator's enforced bound and the schema's advertised bound come from
// the SAME field-table + side-table (the encode `float_suffix` ↔ decode
// `parse_float_suffix` round-trip), so they cannot drift.
// =====================================================================

#[test]
fn test_e2e_float_refinement_matches_validator_cannot_drift() {
    let port = pick_free_port();
    let source = reading_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit float-refinement drift bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- (a) The validator REJECTS ratio 1.5 with 422. ---
    // 1.5 > 1.0, so the runtime range-check fails. This is the BEHAVIOR the
    // schema's maximum must agree with.
    let post = client
        .post(format!("{base}/readings"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":1.5}"#)
        .send()
        .expect("POST /readings ratio 1.5");
    let post_status = post.status().as_u16();
    let post_body = post.text().unwrap();
    assert_eq!(
        post_status, 422,
        "ratio 1.5 (> the enforced max {RATIO_MAX}) must be 422 — the validator's behavior \
         the schema must match; got {post_status}, body={post_body:?}"
    );
    assert!(
        !post_body.contains(HANDLER_MARKER),
        "the range-violation 422 must NOT enter the handler; body={post_body:?}"
    );

    // --- (b) The served schema ADVERTISES the same bound. ---
    let resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    assert_eq!(resp.status().as_u16(), 200, "GET /openapi.json must be 200");
    let doc: serde_json::Value =
        serde_json::from_str(&resp.text().unwrap()).expect("/openapi.json is valid JSON");
    let schema = locate_reading_schema(&doc).expect("Reading schema present in the OpenAPI doc");

    let advertised_max = schema
        .get("properties")
        .and_then(|p| p.get("ratio"))
        .and_then(|r| r.get("maximum"))
        .and_then(serde_json::Value::as_f64)
        .expect("ratio.maximum present in the schema");

    // --- The cannot-drift property: ONE source, consistent. ---
    // The validator REJECTED ratio 1.5 (enforcing maximum 1.0); the schema
    // ADVERTISES maximum 1.0. They agree because both read the SAME
    // field-table + side-table (ADR-0080 §3 footgun #4): the encode
    // (`float_suffix`) and decode (`parse_float_suffix`) round-trip exactly.
    // If a future change moved one without the other, this equality would
    // break — exactly the drift the single-source design forbids.
    assert!(
        (advertised_max - RATIO_MAX).abs() < f64::EPSILON,
        "the schema's advertised ratio.maximum ({advertised_max}) must equal the bound the \
         validator enforces ({RATIO_MAX}, proven by the 422 on ratio 1.5) — ONE source, \
         cannot drift (ADR-0080 §5.3 / footgun #4)"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// COMPILE-TIME NEGATIVE (ADR-0080 Q6 / Phase-3a D6): Phase-3a ADDS the
// float-range `lo <= self <= hi` form on an `f64` field — it does NOT open
// the `where`-clause to arbitrary expressions. A `len(self)` / `pattern`
// form (str-only) on an `f64` field, or an arbitrary user-fn call, must
// STILL be rejected with `UnsupportedRefinement` + a §2.5-B FIX suggestion.
// =====================================================================

/// Compile-only helper — `cobrust check` (no codegen). Returns the combined
/// stdout+stderr (what the user sees) along with `success`.
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

/// Negative: a `len(self)` LENGTH form (str-only) on an `f64` field. The
/// float-range grammar wants `lo <= self <= hi`, not `len(self)`; Phase-3a
/// keeps the base-type discrimination — anything off-shape is
/// `TypeError::UnsupportedRefinement` with a FIX (§2.5-B / D6: errors print
/// the fix, not just the diagnosis).
#[test]
fn test_neg_float_field_rejects_len_form() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "class Reading:\n",
        // `len(self)` is the str-LENGTH subject; on an f64 field it is NOT
        // the float-range grammar. Q6/D6 mandate rejection with a FIX.
        "    ratio: f64 where len(self) <= 10\n",
        "\n",
        "fn main() -> i64:\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "a `len(self)` form on an f64 field must be rejected; output=\n{out}"
    );
    assert!(
        out.contains("Type") || out.contains("refinement") || out.contains("where"),
        "must be a type/refinement error on the `where` predicate; got:\n{out}"
    );
    // §2.5-B / D6: the diagnostic must print the FIX naming the fixed forms.
    assert!(
        out.contains("hint")
            || out.contains("suggestion")
            || out.contains("fixed")
            || out.contains("0 <= self")
            || out.contains("len(self)")
            || out.contains("pattern(self"),
        "the refinement error MUST print a FIX suggestion naming the recognized fixed forms \
         (§2.5-B); got:\n{out}"
    );
}

/// Negative: a STRICT `<` bound on an `f64` field (ADR-0080 Phase-3a D2). The
/// integer grammar rewrites `S < N` to `<= N-1`, but a float strict bound has
/// no clean inclusive ±1 rewrite (the reals are dense), so the fixed grammar
/// admits ONLY inclusive `<=`/`>=` — a `<` is rejected with a FIX.
#[test]
fn test_neg_float_field_rejects_strict_lt_bound() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "class Reading:\n",
        "    ratio: f64 where 0.0 <= self and self < 1.0\n",
        "\n",
        "fn main() -> i64:\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "a strict `<` bound on an f64 field must be rejected (D2 — inclusive only); output=\n{out}"
    );
    assert!(
        out.contains("Type") || out.contains("refinement") || out.contains("where"),
        "must be a type/refinement error; got:\n{out}"
    );
}

// =====================================================================
// Helper: locate the Reading body schema in an OpenAPI doc.
//
// Primary: the canonical `components/schemas/Reading` path (ADR-0080 §5.3).
// Fallback: a recursive search for any JSON object that has a
// `properties.ratio` carrying a `maximum` — robust to the exact component
// key / placement without weakening the bound assertions.
// =====================================================================

fn locate_reading_schema(doc: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(s) = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.get("Reading"))
    {
        return Some(s.clone());
    }
    find_schema_with_ratio_maximum(doc)
}

fn find_schema_with_ratio_maximum(v: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(props) = v.get("properties").and_then(|p| p.as_object()) {
        if props.get("ratio").and_then(|r| r.get("maximum")).is_some() {
            return Some(v.clone());
        }
    }
    match v {
        serde_json::Value::Object(map) => {
            for child in map.values() {
                if let Some(found) = find_schema_with_ratio_maximum(child) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for child in arr {
                if let Some(found) = find_schema_with_ratio_maximum(child) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}
