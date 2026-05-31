//! #156 nested-object validated request bodies — the FastAPI-real
//! pydantic-killer's nested-model slice (ADR-0080 §6 Phase-4 (b) "a body
//! field that is itself a field-tracked class → nested `$ref` in the
//! schema"), end-to-end.
//!
//! TEST-FIRST (ADSD): this corpus is written RED, BEFORE the impl. The
//! feature it exercises does not exist yet — at HEAD a body field whose
//! type is ANOTHER validated `class` is lowered to the descriptor kind
//! `any` (the `_ => "any"` arm at `crates/cobrust-mir/src/lower.rs:2315`),
//! which is PRESENCE-ONLY (`FieldKind::Any` →
//! `validation.rs:513` accepts any JSON value with no recursion) and emits
//! an EMPTY OpenAPI schema `{}` (`openapi.rs:148` `Any` arm, no `$ref`, no
//! nested component). So today:
//!   * an out-of-range / missing / wrong-typed / non-object NESTED field is
//!     SILENTLY ACCEPTED — the nested object is not validated (the
//!     validation assertions below FAIL: a body a correct impl rejects with
//!     422 currently returns 201, the handler entered);
//!   * the served OpenAPI doc carries NO `$ref` for the nested field and NO
//!     `components/schemas/<NestedName>` component (the OpenAPI assertions
//!     below FAIL).
//!
//! Confirmed-absent surfaces (verified by grep at the TEST-phase HEAD — the
//! impl introduces all four):
//!   * MIR `obj:<NestedClassName>` descriptor token (no occurrence of
//!     `obj:` in `cobrust-mir`/`cobrust-pit`);
//!   * pit `FieldKind::Obj(String)` + the multi-block `parse_schema`
//!     decode (no `Obj`/multi-block in `validation.rs`);
//!   * the recursive `validate_against_schema` for an `Obj` field
//!     (no recursion / depth-cap in `validation.rs`);
//!   * the OpenAPI `$ref` + per-nested-class component emission
//!     (no `$ref` in `openapi.rs`).
//!
//! ## The feature (ADR-0080 §6 Phase-4 (b); the CTO-LOCKED D1-D4 design)
//!
//!   * (D1) the MIR schema descriptor becomes MULTI-BLOCK: the ROOT class
//!     block first (`# Root` header + its `field<TAB>payload` lines), then
//!     one `# Nested`-headed block per transitively-referenced validated
//!     class. A class-typed field's payload is the NEW kind token
//!     `obj:<NestedClassName>` (replacing `_ => "any"`); a truly-unknown
//!     type still maps to `any`. A FLAT-only body (no nested field) stays
//!     BYTE-IDENTICAL to today (single block).
//!   * (D2) pit `parse_schema` decodes the multi-block string into a map
//!     `ClassName -> Vec<FieldSpec>` (ROOT = first block); a new
//!     `FieldKind::Obj(String)` carries the nested class name. ENCODE
//!     (MIR) and DECODE (`parse_schema`) are mirror inverses (footgun #4 —
//!     cannot drift).
//!   * (D3) `validate_against_schema` validates the ROOT block; an `Obj`
//!     field's JSON value MUST be a JSON object (else 422) and is
//!     RECURSIVELY validated against the named class's specs. A bounded
//!     depth cap guards a pathological cyclic schema. Missing/extra nested
//!     fields follow the SAME policy the flat validator uses.
//!   * (D4) the served OpenAPI doc emits an `Obj` field as
//!     `{"$ref":"#/components/schemas/<NestedName>"}` and registers EACH
//!     referenced class as its own `components/schemas/<Name>` object
//!     (derived from the SAME parse — no second source).
//!
//! SCOPE: nested OBJECT (a class-typed field, one OR more levels deep —
//! recursion handles depth). DEFERRED (NOT pinned here): collection fields
//! `list[Item]`, `dict`, and Optional/None nested fields. The i64/str/f64/
//! bool flat fields + all refinement behaviour stay byte-identical (pinned
//! by the existing `pit_validated_body_e2e.rs` / `pit_openapi_e2e.rs`).
//!
//! ## TEST-PHASE FINDING — the type-check prerequisite wall (the FIRST RED)
//!
//! Captured by the TEST author at the TEST-phase HEAD: a body class with a
//! field typed as ANOTHER class but NO initializer (`address: Address`)
//! does NOT type-check today. `cobrust check` on the nested-body program
//! fails BEFORE any runtime behaviour:
//!
//! ```text
//! error[Type]: type mismatch: expected `Adt#68`, found `None`
//! ```
//!
//! This is a PRE-EXISTING limitation in `check_class`
//! (`crates/cobrust-types/src/check.rs:904`): a class-body field is also
//! recursed as an `ItemKind::Let` member (`check.rs:962` `check_item`), and
//! a `let address: Address` with no value unifies the declared `Ty::Adt`
//! against the implicit `None` value → `TypeMismatch`. It is NOT
//! class-specific — `items: list[i64]` (no value) fails identically; only
//! `str`/`i64`/`f64`/`bool` (the existing Phase-1 corpora's fields) survive
//! the member-recursion because their no-value `let` is admitted. So #156
//! cannot reach the D1-D4 surface (MIR descriptor / recursive validate /
//! OpenAPI `$ref`) until a class field MAY be typed as another class
//! WITHOUT an initializer. That is the load-bearing FIRST target of the
//! impl phase (mirrors ADR-0080 §8's honesty: #156's elegance is contingent
//! on the class-field-tracking gate — here, on letting a class-typed field
//! with no default type-check). The behavioural assertions in MUST-HAVE
//! 1-4 below are the END-STATE RED behind this gate; `test_nested_body_
//! class_type_checks` isolates the gate itself as a focused first RED.
//!
//! NOTE for the impl phase: the locked D1-D4 design is the MIR/pit/OpenAPI
//! surface; admitting a no-initializer class-typed field is the type-check
//! PREREQUISITE the impl must clear first for any of D1-D4 to be reachable.
//!
//! ## Harness
//!
//! Mirrors `pit_validated_body_e2e.rs` + `pit_openapi_e2e.rs` EXACTLY:
//! compile a `.cb` source to an exe, pick an ephemeral free port
//! (bind-and-drop a `TcpListener`), spawn the binary, poll the port until
//! the server binds, issue real HTTP via `reqwest::blocking`, assert
//! status/body, and an RAII `ChildGuard` kills the process on Drop so a
//! failing assertion never leaks the spawned `.cb` server. The keep-alive
//! is `app.run(host, port)` (blocks until killed, the z8 demo's shape).
//!
//! ```text
//! `import pit` + a NESTED body `class` (a field typed as another validated
//! `class`) + a 2-arg validated handler + `app.route_validated(...)` +
//! `app.serve_openapi("/openapi.json")` + `app.run(...)`
//!   → cobrust-frontend parse (`class` typed-field body + `where`-clause)
//!   → cobrust-types check (the nested field is a `Ty::Adt`; adt_fields)
//!   → cobrust-mir (MULTI-BLOCK schema descriptor: root + `obj:Address`)
//!   → cobrust-pit C-ABI (recursive validate → Ok dispatch / Err 422;
//!     OpenAPI `$ref` + nested component)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client POSTs valid + invalid nested bodies; GETs
//!     /openapi.json and inspects the `$ref` + nested component
//! ```

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Test-helper naming + nested-if patterns mirror the sibling pit E2E
// corpora's module-level test-lint allows.
#![allow(clippy::similar_names)]
#![allow(clippy::collapsible_if)]
// The recursive-validation test drives six POST cases against one server
// (D3's six nested-invalidity classes), exceeding the 100-line cap. This is
// the SAME module-level test-lint fence the sibling live-server e2e corpora
// carry (cli_break_continue_e2e, fastapi_real_demo_e2e, …) — a lint-only
// allow, not a behavioral change to the test-owned assertions.
#![allow(clippy::too_many_lines)]

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

/// The nested-body program. `CreateUser` has a flat `name: str` field AND a
/// NESTED field `address: Address`, where `Address` is itself a validated
/// `class` with a flat `city: str` field + an int-range-refined
/// `zip: i64 where 0 <= self and self <= 99999` field.
///
/// Both `Address` (the nested class) and `CreateUser` (the root) are
/// declared BEFORE the handler (signature-position forward refs to a LATER
/// class are a known limit, mirrored by the existing corpora). The nested
/// class is declared FIRST so the root's `address: Address` annotation
/// resolves.
///
/// The success handler returns a FIXED marker (body re-serialization is a
/// deferred §9 sub-ADR — see the `pit_validated_body_e2e.rs` module
/// header); the 422 path is synthesised in Rust without entering the
/// handler, so a 422 body provably cannot carry the marker (the
/// handler-NOT-entered assertion).
const HANDLER_MARKER: &str = "entered-create-user-handler";

fn nested_body_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The NESTED validated class — its own field table + an
            // int-range refinement (so we can prove the nested refinement
            // is enforced recursively, not just nested presence).
            "class Address:\n",
            "    city: str\n",
            "    zip: i64 where 0 <= self and self <= 99999\n",
            "\n",
            // The ROOT validated class: a flat `name` field PLUS a field
            // whose type is the nested `Address` class (the #156 surface).
            "class CreateUser:\n",
            "    name: str\n",
            "    address: Address\n",
            "\n",
            // 2-arg validated handler. pit deserializes + RECURSIVELY
            // validates the JSON body into `body: CreateUser` BEFORE this
            // runs, so reaching here proves the nested object validated.
            "fn create_user(req: pit.Request, body: CreateUser) -> pit.Response:\n",
            "    return pit.text_response(201, \"{marker}\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/users\", create_user)\n",
            // EXPLICIT OpenAPI opt-in (the surface the sibling corpus
            // assumes; the DEV may rename — see pit_openapi_e2e.rs header).
            "    let _ = app.serve_openapi(\"/openapi.json\")\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        marker = HANDLER_MARKER,
        port = port,
    )
}

// =====================================================================
// MUST-HAVE 1 (D3 — recursive validation): one server, six POSTs.
//
// A fully-valid nested body passes (201, handler entered). FOUR distinct
// nested-invalidity classes each short-circuit a 422 WITHOUT entering the
// handler — each of which TODAY (nested field = `any`, presence-only) is
// silently accepted, so each assertion is RED until the recursive
// validator lands.
//
//   1. valid                {name, address:{city,zip:in-range}} → 201, entered
//   2. nested out-of-range  address.zip = 100000 (> 99999)      → 422, NOT entered
//   3. nested missing field address omits `city`                → 422, NOT entered
//   4. nested wrong type    address.zip = "x" (str, not i64)    → 422, NOT entered
//   5. nested NOT an object address = "oops" (a string)         → 422, NOT entered
//   6. nested extra field   address has an undeclared key       → 422, NOT entered
// =====================================================================

#[test]
fn test_e2e_nested_body_recursive_validation() {
    let port = pick_free_port();
    let source = nested_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit nested-body server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: fully-valid nested body → 201, handler entered. ---
    let ok_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"city":"NYC","zip":10001}}"#)
        .send()
        .expect("POST /users valid nested");
    let ok_status = ok_resp.status().as_u16();
    let ok_body = ok_resp.text().unwrap();
    assert_eq!(
        ok_status, 201,
        "a fully-valid nested body must be 201; got {ok_status}, body={ok_body:?}"
    );
    assert!(
        ok_body.contains(HANDLER_MARKER),
        "valid nested request MUST enter the handler (marker {HANDLER_MARKER:?} present); body={ok_body:?}"
    );

    // --- Case 2: nested out-of-range zip → 422, handler NOT entered. ---
    // address.zip = 100000 > 99999. TODAY this is silently accepted (the
    // `any` field does not range-check the nested object) → RED.
    let oor_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"city":"NYC","zip":100000}}"#)
        .send()
        .expect("POST /users nested out-of-range");
    let oor_status = oor_resp.status().as_u16();
    let oor_body = oor_resp.text().unwrap();
    assert_eq!(
        oor_status, 422,
        "nested address.zip=100000 (> 99999) must be 422 (recursive range-check); \
         got {oor_status}, body={oor_body:?}"
    );
    assert!(
        !oor_body.contains(HANDLER_MARKER),
        "nested-out-of-range 422 MUST NOT enter the handler; body={oor_body:?}"
    );

    // --- Case 3: nested missing field (no `city`) → 422, NOT entered. ---
    let missing_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"zip":10001}}"#)
        .send()
        .expect("POST /users nested missing city");
    let missing_status = missing_resp.status().as_u16();
    let missing_body = missing_resp.text().unwrap();
    assert_eq!(
        missing_status, 422,
        "nested address missing `city` must be 422 (recursive total deser); \
         got {missing_status}, body={missing_body:?}"
    );
    assert!(
        !missing_body.contains(HANDLER_MARKER),
        "nested-missing-field 422 MUST NOT enter the handler; body={missing_body:?}"
    );

    // --- Case 4: nested wrong type (zip as a string) → 422, NOT entered. ---
    let wrongtype_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"city":"NYC","zip":"x"}}"#)
        .send()
        .expect("POST /users nested wrong type");
    let wrongtype_status = wrongtype_resp.status().as_u16();
    let wrongtype_body = wrongtype_resp.text().unwrap();
    assert_eq!(
        wrongtype_status, 422,
        "nested address.zip as a string must be 422 (recursive type-check); \
         got {wrongtype_status}, body={wrongtype_body:?}"
    );
    assert!(
        !wrongtype_body.contains(HANDLER_MARKER),
        "nested-wrong-type 422 MUST NOT enter the handler; body={wrongtype_body:?}"
    );

    // --- Case 5: the nested field is NOT a JSON object → 422, NOT entered. ---
    // D3: an `Obj(name)` field's JSON value MUST be a JSON object; a string
    // is a WrongType 422. TODAY `any` accepts the string silently → RED.
    let notobj_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":"oops"}"#)
        .send()
        .expect("POST /users nested not-an-object");
    let notobj_status = notobj_resp.status().as_u16();
    let notobj_body = notobj_resp.text().unwrap();
    assert_eq!(
        notobj_status, 422,
        "a non-object value for the nested `address` field must be 422 \
         (D3: an obj field requires a JSON object); got {notobj_status}, body={notobj_body:?}"
    );
    assert!(
        !notobj_body.contains(HANDLER_MARKER),
        "nested-not-an-object 422 MUST NOT enter the handler; body={notobj_body:?}"
    );

    // --- Case 6: nested EXTRA undeclared key → 422, NOT entered. ---
    // D3: missing/extra nested fields follow the SAME policy the flat
    // validator uses; the flat validator rejects unknown keys
    // (`UnknownField`, validation.rs:442), so a nested extra key is 422.
    let extra_resp = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"city":"NYC","zip":10001,"country":"US"}}"#)
        .send()
        .expect("POST /users nested extra key");
    let extra_status = extra_resp.status().as_u16();
    let extra_body = extra_resp.text().unwrap();
    assert_eq!(
        extra_status, 422,
        "an undeclared nested key (`country`) must be 422 (recursive total deser, \
         SAME unknown-key policy as the flat validator); got {extra_status}, body={extra_body:?}"
    );
    assert!(
        !extra_body.contains(HANDLER_MARKER),
        "nested-extra-key 422 MUST NOT enter the handler; body={extra_body:?}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 2 (D1/D4 — flat fields stay byte-identical + the ROOT block's
// flat refinement still enforces under the nested schema): against the
// SAME nested-body program, a flat-field violation (the ROOT `name`
// missing, and a wrong-typed ROOT `name`) still 422s, and a valid body
// still 201s. This guards the LOCKED constraint that adding a nested field
// does NOT regress the flat-field validation of the ROOT block.
// =====================================================================

#[test]
fn test_e2e_nested_body_root_flat_fields_still_validated() {
    let port = pick_free_port();
    let source = nested_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit nested-body server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- ROOT `name` missing → 422 (the root block's flat field). ---
    let missing_name = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"address":{"city":"NYC","zip":10001}}"#)
        .send()
        .expect("POST /users missing root name");
    assert_eq!(
        missing_name.status().as_u16(),
        422,
        "the ROOT `name` field must still be required under a nested schema (D1 \
         flat-byte-identical); body={:?}",
        missing_name.text().unwrap()
    );

    // --- ROOT `name` wrong type (a number) → 422. ---
    let wrong_name = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":42,"address":{"city":"NYC","zip":10001}}"#)
        .send()
        .expect("POST /users wrong-typed root name");
    assert_eq!(
        wrong_name.status().as_u16(),
        422,
        "the ROOT `name` must still type-check (str) under a nested schema; body={:?}",
        wrong_name.text().unwrap()
    );

    // --- A fully-valid body still 201s (the nested schema did not break
    //     the happy path). ---
    let ok = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"city":"NYC","zip":10001}}"#)
        .send()
        .expect("POST /users valid");
    assert_eq!(
        ok.status().as_u16(),
        201,
        "a fully-valid nested body must still be 201; body={:?}",
        ok.text().unwrap()
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 3 (D4 — OpenAPI `$ref` + per-nested-class component): GET
// /openapi.json must show the ROOT `CreateUser` schema with `address` as a
// `$ref` to `#/components/schemas/Address`, AND a SEPARATE
// `components/schemas/Address` object schema carrying the nested fields
// (city:{string}, zip:{integer, minimum:0, maximum:99999}).
//
// TODAY the nested field emits an EMPTY schema `{}` (the `Any` arm) with NO
// `$ref` and NO `Address` component → every assertion below is RED.
// =====================================================================

#[test]
fn test_e2e_nested_body_openapi_ref_and_component() {
    let port = pick_free_port();
    let source = nested_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit nested-body openapi server bind");

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
        "GET /openapi.json must be 200; got {status}, body={body:?}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&body).expect("/openapi.json body must be valid JSON");

    // --- The ROOT CreateUser component exists. ---
    let schemas = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .unwrap_or_else(|| panic!("doc must carry components/schemas; got:\n{body}"));
    let root = schemas
        .get("CreateUser")
        .unwrap_or_else(|| panic!("components/schemas/CreateUser missing; got:\n{body}"));

    // --- ROOT.name is still a plain string (flat field byte-identical). ---
    assert_eq!(
        root.get("properties")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.get("type"))
            .and_then(|t| t.as_str()),
        Some("string"),
        "ROOT name must be {{type:string}}; got root={root}"
    );

    // --- ROOT.address is a `$ref` to #/components/schemas/Address (D4). ---
    // TODAY the nested field is `any` → an empty `{}` schema with NO $ref.
    let addr_prop = root
        .get("properties")
        .and_then(|p| p.get("address"))
        .unwrap_or_else(|| panic!("ROOT must declare property `address`; got root={root}"));
    assert_eq!(
        addr_prop.get("$ref").and_then(|r| r.as_str()),
        Some("#/components/schemas/Address"),
        "the nested `address` field MUST be a $ref to the Address component \
         (D4 — not an inline/empty schema); got address={addr_prop}"
    );

    // --- A SEPARATE components/schemas/Address object schema exists (D4). ---
    let addr = schemas.get("Address").unwrap_or_else(|| {
        panic!(
            "the nested class MUST be registered as its own \
             components/schemas/Address (D4 — derived from the same parse); got:\n{body}"
        )
    });
    assert_eq!(
        addr.get("type").and_then(|t| t.as_str()),
        Some("object"),
        "the Address component must be an object schema; got addr={addr}"
    );
    // city: {type:string}
    assert_eq!(
        addr.get("properties")
            .and_then(|p| p.get("city"))
            .and_then(|c| c.get("type"))
            .and_then(|t| t.as_str()),
        Some("string"),
        "Address.city must be {{type:string}}; got addr={addr}"
    );
    // zip: {type:integer, minimum:0, maximum:99999} — the nested
    // refinement bounds, advertised on the nested component (the SAME
    // bounds the recursive validator enforces — cannot drift).
    let zip = addr
        .get("properties")
        .and_then(|p| p.get("zip"))
        .unwrap_or_else(|| panic!("Address must declare property `zip`; got addr={addr}"));
    assert_eq!(
        zip.get("type").and_then(|t| t.as_str()),
        Some("integer"),
        "Address.zip must be {{type:integer}}; got zip={zip}"
    );
    assert_eq!(
        zip.get("minimum").and_then(serde_json::Value::as_i64),
        Some(0),
        "Address.zip.minimum must be 0 (the nested refinement lower bound); got zip={zip}"
    );
    assert_eq!(
        zip.get("maximum").and_then(serde_json::Value::as_i64),
        Some(99999),
        "Address.zip.maximum must be 99999 (the nested refinement upper bound); got zip={zip}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 4 (D3 + D4 cannot-drift, the load-bearing #156 property):
// against the SAME running server, the bound the recursive VALIDATOR
// enforces on the nested field and the bound the OpenAPI doc ADVERTISES on
// the nested component come from ONE source, so they agree.
//   * POST {address.zip:100000} → 422 (validator rejects 100000 > 99999); AND
//   * GET /openapi.json shows components/schemas/Address.zip.maximum == 99999.
// If a future change moved one without the other (e.g. the OpenAPI nested
// component derived from a SECOND source), this equality would break —
// exactly the drift D4's single-source design forbids.
// =====================================================================

#[test]
fn test_e2e_nested_body_validator_and_openapi_cannot_drift() {
    let port = pick_free_port();
    let source = nested_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit nested-body cannot-drift server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- (a) The recursive validator REJECTS the nested zip=100000. ---
    let post = client
        .post(format!("{base}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","address":{"city":"NYC","zip":100000}}"#)
        .send()
        .expect("POST /users nested zip=100000");
    let post_status = post.status().as_u16();
    let post_body = post.text().unwrap();
    assert_eq!(
        post_status, 422,
        "nested zip=100000 (> the enforced nested max 99999) must be 422 — the \
         recursive validator's behaviour the nested schema must match; \
         got {post_status}, body={post_body:?}"
    );
    assert!(
        !post_body.contains(HANDLER_MARKER),
        "the nested 422 path must NOT enter the handler; body={post_body:?}"
    );

    // --- (b) The served nested component ADVERTISES maximum == 99999. ---
    let resp = client
        .get(format!("{base}/openapi.json"))
        .send()
        .expect("GET /openapi.json");
    assert_eq!(resp.status().as_u16(), 200, "GET /openapi.json must be 200");
    let doc: serde_json::Value =
        serde_json::from_str(&resp.text().unwrap()).expect("/openapi.json is valid JSON");
    let advertised_max = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.get("Address"))
        .and_then(|a| a.get("properties"))
        .and_then(|p| p.get("zip"))
        .and_then(|z| z.get("maximum"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| {
            panic!(
                "components/schemas/Address.zip.maximum must be present (D4 nested \
                 component); doc=\n{doc}"
            )
        });

    // --- The cannot-drift property: ONE source, consistent. ---
    assert_eq!(
        advertised_max, 99999,
        "the nested component's advertised Address.zip.maximum ({advertised_max}) must \
         equal the bound the recursive validator enforces (99999, proven by the 422 on \
         nested zip=100000) — ONE source, cannot drift (D4 / ADR-0080 footgun #4)"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// COMPILE-TIME PREREQUISITE (the FIRST RED — the type-check gate, see the
// module header's TEST-PHASE FINDING): a `cobrust check` of a body class
// with a field typed as ANOTHER class but NO initializer
// (`address: Address`) must SUCCEED. The nested field is a `Ty::Adt`; for
// #156 to reach the D1-D4 MIR/pit/OpenAPI surface at all, the type checker
// must admit a no-default class-typed field.
//
// This is RED at the TEST-phase HEAD — it fails with
// `error[Type]: type mismatch: expected Adt#NN, found None` from
// `check_class`'s member-recursion (`check.rs:962`), the load-bearing
// prerequisite the impl phase clears FIRST. (Every behavioural assertion
// in MUST-HAVE 1-4 above is masked behind this same wall today — they
// `build failed` on it; this test isolates the wall as a focused,
// surface-agnostic first target so the impl knows the precise gate.)
// =====================================================================

/// Compile-only helper — `cobrust check` (no codegen). Returns combined
/// stdout+stderr + `success`. Mirrors the sibling corpora's `try_check`.
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

#[test]
fn test_nested_body_class_field_type_checks_prerequisite() {
    let (ok, out) = try_check(concat!(
        "import pit\n",
        "\n",
        "class Address:\n",
        "    city: str\n",
        "    zip: i64 where 0 <= self and self <= 99999\n",
        "\n",
        "class CreateUser:\n",
        "    name: str\n",
        "    address: Address\n",
        "\n",
        "fn create_user(req: pit.Request, body: CreateUser) -> pit.Response:\n",
        "    return pit.text_response(201, \"ok\")\n",
        "\n",
        "fn main() -> i64:\n",
        "    let app = pit.App()\n",
        "    let _ = app.route_validated(\"POST\", \"/users\", create_user)\n",
        "    let _ = app.serve_openapi(\"/openapi.json\")\n",
        "    return 0\n",
    ));
    assert!(
        ok,
        "PREREQUISITE (first RED): a class field typed as another class with no \
         initializer (`address: Address`) must type-check. Today it fails in \
         check_class's member-recursion (`expected Adt#NN, found None`); #156 \
         cannot reach the D1-D4 MIR/pit/OpenAPI surface until this gate clears. \
         output=\n{out}"
    );
}
