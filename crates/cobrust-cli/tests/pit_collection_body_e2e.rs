//! #156 COLLECTION validated request bodies — the SECOND pydantic-killer
//! (ADR-0080 §6 Phase-4 item (c) `list[T]` fields; the CTO-LOCKED D1-D4
//! design), end-to-end. Builds DIRECTLY on the just-landed nested-OBJECT
//! work (commit edcb027 / ADR-0080 Phase-4 (b)): the MULTI-BLOCK descriptor +
//! `FieldKind::Obj` + recursive `validate_block` + OpenAPI `$ref`.
//!
//! TEST-FIRST (ADSD): this corpus is written RED, BEFORE the impl. The
//! feature it exercises does not exist yet — at the TEST-phase HEAD a body
//! field whose type is `list[T]` is lowered to the descriptor kind `any`
//! (the `_ => "any"` arm at `crates/cobrust-mir/src/lower.rs` —
//! `emit_class_block` handles the nested-OBJECT class case via `obj:<Name>`,
//! but a `list[T]` field still falls through to `any`), which is
//! PRESENCE-ONLY (`FieldKind::Any` → `validation.rs` accepts ANY JSON value
//! with no element recursion) and emits an EMPTY OpenAPI schema `{}`
//! (`openapi.rs` `Any` arm — no `array`, no `items`, no element component).
//!
//! ## TEST-PHASE EVIDENCE (verified by running a probe at this HEAD)
//!
//! A `.cb` body with a `list[str]` field AND a `list[Class]` field compiles,
//! links, and RUNS today (the type-check prerequisite — a no-value `list[T]`
//! class field — was already CLEARED by the nested-object work's
//! `check_class` field-`let` skip, `check.rs` ~982-986: "this skip
//! incidentally admits their no-value form too"). So — UNLIKE
//! `pit_nested_body_e2e.rs`, whose first RED was a `build failed`
//! type-check wall — there is NO type-check prerequisite test here; the
//! program builds + serves, and the RED is PURELY BEHAVIORAL. Captured at
//! the TEST-phase HEAD against a running list-body server:
//!
//! ```text
//!   POST valid body                                   → 201 (correct, stays)
//!   POST {tags:["a",42]}   (number in a list[str])    → 201  ← WRONG (want 422)
//!   POST {lines:[{qty:9999}]} (oob nested list elem)  → 201  ← WRONG (want 422)
//!   POST {tags:"oops"}     (a bare string, no array)  → 201  ← WRONG (want 422)
//!   POST {tags:[],scores:[],lines:[]} (empty lists)   → 201 (correct, stays)
//!   GET  /openapi.json  → tags  = {}   (want {type:array,items:{type:string}})
//!                         lines = {}   (want {type:array,items:{$ref:…/OrderLine}})
//!                         OrderLine component ABSENT  (want present)
//! ```
//!
//! Every "WRONG" line above is a RED assertion below — a body a correct impl
//! REJECTS with 422 today returns 201 (the handler entered), and the served
//! OpenAPI carries an empty `{}` for each list field with no element type and
//! no element component.
//!
//! ## The feature (ADR-0080 §6 Phase-4 (c); the CTO-LOCKED D1-D4 design)
//!
//!   * (D1) DESCRIPTOR: a `list[T]` field's payload is the NEW token
//!     `list:<elem-payload>` where elem-payload is T's OWN payload — a scalar
//!     kind (`str` / `i64:lo:hi` / `f64:lo:hi` / `bool` / `pat:<re>`) OR
//!     `obj:<ClassName>` for `list[SomeClass]` (and that class's block is
//!     emitted into the multi-block descriptor exactly like a direct nested
//!     field — REUSING the BFS collector). Examples: `tags\tlist:str`;
//!     `scores\tlist:i64:0:100`; `lines\tlist:obj:OrderLine` + an `# OrderLine`
//!     block. A flat / scalar / nested-OBJECT body with NO list field stays
//!     BYTE-IDENTICAL.
//!   * (D2) DECODE: `parse_schema_blocks` / `FieldKind` gain
//!     `FieldKind::List(Box<FieldKind>)` (carrying the elem spec) — `list:<rest>`
//!     parses by RECURSIVELY parsing `<rest>` as an element kind (scalar or
//!     `obj:<Name>`). ENCODE (MIR) and DECODE are mirror inverses (footgun #4).
//!   * (D3) VALIDATE: a `List` field's JSON value MUST be a JSON ARRAY (else a
//!     `WrongType` 422 with `expected: "array"`); validate EACH element against
//!     the elem kind — a scalar elem via the existing scalar `check_field`
//!     path, an `obj` elem by recursing into the named block (reuse
//!     `validate_block` + the depth cap). An EMPTY array is VALID. Reuse the
//!     existing Missing/Unknown/range policies for object elements.
//!   * (D4) OpenAPI: a `List` field → `{"type":"array","items":<elem-schema>}`
//!     where elem-schema is the element's `field_schema` (a scalar
//!     `{type:…}` OR a `$ref` for an obj element). The referenced element
//!     class still registers as its own `components/schemas/<name>`. Derived
//!     from the SAME `parse_schema_blocks` parse (footgun #4 — cannot drift).
//!
//! SCOPE: `list[<scalar>]` + `list[<Class>]`. DEFERRED (NOT pinned here):
//! list-of-list (`list[list[T]]`), `dict[K,V]`, Optional/None list,
//! element-level refinements beyond what the class/scalar already carries.
//! The scalar / nested-OBJECT / flat behaviour stays byte-identical (pinned
//! by the existing `pit_validated_body_e2e.rs` / `pit_openapi_e2e.rs` /
//! `pit_nested_body_e2e.rs`).
//!
//! ## Harness
//!
//! Mirrors `pit_nested_body_e2e.rs` (itself a verbatim copy of
//! `pit_validated_body_e2e.rs`) EXACTLY: compile a `.cb` source to an exe,
//! pick an ephemeral free port (bind-and-drop a `TcpListener`), spawn the
//! binary, poll the port until the server binds, issue real HTTP via
//! `reqwest::blocking`, assert status/body, and an RAII `ChildGuard` kills
//! the process on Drop so a failing assertion never leaks the spawned `.cb`
//! server. The keep-alive is `app.run(host, port)` (blocks until killed, the
//! z8 demo's shape).
//!
//! ```text
//! `import pit` + a COLLECTION body `class` (a `list[str]` field + a
//! `list[<Class>]` field) + a 2-arg validated handler + `app.route_validated(…)`
//! + `app.serve_openapi("/openapi.json")` + `app.run(…)`
//!   → cobrust-frontend parse (`class` with `list[T]`-typed fields)
//!   → cobrust-types check (the list field is a `Ty::List`; no-value admitted)
//!   → cobrust-mir (MULTI-BLOCK schema descriptor: root + `list:str` /
//!     `list:obj:OrderLine` + the `# OrderLine` block)
//!   → cobrust-pit C-ABI (recursive per-element validate → Ok dispatch /
//!     Err 422; OpenAPI array+items / element-$ref + element component)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client POSTs valid + invalid collection bodies; GETs
//!     /openapi.json and inspects the array/items + element-$ref + component
//! ```

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Test-helper naming + nested-if patterns mirror the sibling pit E2E
// corpora's module-level test-lint allows.
#![allow(clippy::similar_names)]
#![allow(clippy::collapsible_if)]
// The element-recursion test drives several POST cases against one server
// (D3's element-invalidity classes), exceeding the 100-line cap. This is the
// SAME module-level test-lint fence the sibling live-server e2e corpora carry
// (pit_nested_body_e2e, fastapi_real_demo_e2e, …) — a lint-only allow, not a
// behavioral change to the test-owned assertions.
#![allow(clippy::too_many_lines)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Harness — copied verbatim from pit_nested_body_e2e.rs so the live E2Es
// drive a `.cb` pit binary identically.
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

/// The collection-body program. `CreateOrder` has a flat `note: str` field, a
/// `list[str]` field (`tags`), a `list[i64]` field with NO element refinement
/// (`scores` — proves a plain scalar element list works), AND a
/// `list[OrderLine]` field (`lines`), where `OrderLine` is itself a validated
/// `class` with a flat `sku: str` field + an int-range-refined
/// `qty: i64 where 1 <= self and self <= 999` field.
///
/// `OrderLine` (the element class) is declared BEFORE the root `CreateOrder`
/// so the `list[OrderLine]` annotation resolves (signature-position forward
/// refs to a LATER class are a known limit, mirrored by the existing corpora).
///
/// The success handler returns a FIXED marker; the 422 path is synthesised in
/// Rust WITHOUT entering the handler, so a 422 body provably cannot carry the
/// marker (the handler-NOT-entered assertion). Body re-serialization is a
/// deferred §9 sub-ADR (see the `pit_validated_body_e2e.rs` module header).
const HANDLER_MARKER: &str = "entered-create-order-handler";

fn collection_body_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // The ELEMENT validated class — its own field table + an
            // int-range refinement (so we can prove the per-element nested
            // refinement is enforced, not just per-element object presence).
            "class OrderLine:\n",
            "    sku: str\n",
            "    qty: i64 where 1 <= self and self <= 999\n",
            "\n",
            // The ROOT validated class: a flat `note` field PLUS a
            // `list[str]` field, a `list[i64]` (no elem refinement) field, and
            // a `list[OrderLine]` field (the #156 collection surface).
            "class CreateOrder:\n",
            "    note: str\n",
            "    tags: list[str]\n",
            "    scores: list[i64]\n",
            "    lines: list[OrderLine]\n",
            "\n",
            // 2-arg validated handler. pit deserializes + validates EACH list
            // element BEFORE this runs, so reaching here proves every element
            // (scalar AND nested-object) validated.
            "fn create_order(req: pit.Request, body: CreateOrder) -> pit.Response:\n",
            "    return pit.text_response(201, \"{marker}\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/orders\", create_order)\n",
            // EXPLICIT OpenAPI opt-in (the surface the sibling corpus assumes;
            // the DEV may rename — see pit_openapi_e2e.rs header).
            "    let _ = app.serve_openapi(\"/openapi.json\")\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        marker = HANDLER_MARKER,
        port = port,
    )
}

// =====================================================================
// MUST-HAVE 1 (D3 — list[scalar] element validation): one server, several
// POSTs against the `tags: list[str]` + `scores: list[i64]` fields.
//
// A fully-valid collection body passes (201, handler entered). FOUR distinct
// list-scalar-invalidity classes each short-circuit a 422 WITHOUT entering the
// handler — each of which TODAY (the list field = `any`, presence-only) is
// silently accepted (verified 201 by the TEST-phase probe), so each
// assertion is RED until the per-element validator lands.
//
//   1. valid                {tags:["a","b"], scores:[1,2]}        → 201, entered
//   2. list[str] number elem tags = ["a", 42] (a number)         → 422, NOT entered
//   3. list[i64] string elem scores = [1, "x"] (a string)        → 422, NOT entered
//   4. list[str] field is NOT an array  tags = "oops"            → 422, NOT entered
//   5. empty lists           tags:[], scores:[]                  → 201, entered (valid)
// =====================================================================

#[test]
fn test_e2e_collection_body_scalar_element_validation() {
    let port = pick_free_port();
    let source = collection_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit collection-body server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: fully-valid collection body → 201, handler entered. ---
    let ok_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a","b"],"scores":[1,2],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders valid collection");
    let ok_status = ok_resp.status().as_u16();
    let ok_body = ok_resp.text().unwrap();
    assert_eq!(
        ok_status, 201,
        "a fully-valid collection body must be 201; got {ok_status}, body={ok_body:?}"
    );
    assert!(
        ok_body.contains(HANDLER_MARKER),
        "valid collection request MUST enter the handler (marker {HANDLER_MARKER:?} present); body={ok_body:?}"
    );

    // --- Case 2: list[str] element is a number → 422, handler NOT entered. ---
    // tags = ["a", 42]; the second element is a JSON number, not a string.
    // TODAY the `any` field does not element-type-check → silently 201 → RED.
    let str_elem_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a",42],"scores":[1,2],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders list[str] number element");
    let str_elem_status = str_elem_resp.status().as_u16();
    let str_elem_body = str_elem_resp.text().unwrap();
    assert_eq!(
        str_elem_status, 422,
        "a number element in a list[str] (`tags`) must be 422 (per-element type-check); \
         got {str_elem_status}, body={str_elem_body:?}"
    );
    assert!(
        !str_elem_body.contains(HANDLER_MARKER),
        "list[str]-wrong-element 422 MUST NOT enter the handler; body={str_elem_body:?}"
    );

    // --- Case 3: list[i64] element is a string → 422, handler NOT entered. ---
    // scores = [1, "x"]; the second element is a JSON string, not an integer.
    // A list[i64] with NO element refinement still enforces the ELEMENT base
    // type (NumPy-of-the-web semantics — the element type is enforced even
    // with no `where`). TODAY the `any` field accepts it silently → RED.
    let int_elem_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1,"x"],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders list[i64] string element");
    let int_elem_status = int_elem_resp.status().as_u16();
    let int_elem_body = int_elem_resp.text().unwrap();
    assert_eq!(
        int_elem_status, 422,
        "a string element in a list[i64] (`scores`) must be 422 (per-element type-check, \
         even with no element refinement); got {int_elem_status}, body={int_elem_body:?}"
    );
    assert!(
        !int_elem_body.contains(HANDLER_MARKER),
        "list[i64]-wrong-element 422 MUST NOT enter the handler; body={int_elem_body:?}"
    );

    // --- Case 4: the list field is NOT a JSON array → 422, NOT entered. ---
    // D3: a `List` field's JSON value MUST be a JSON array; a bare string is a
    // WrongType 422 (expected `array`). TODAY `any` accepts the string
    // silently (verified 201 by the probe) → RED.
    let notarr_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":"oops","scores":[1],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders list field not-an-array");
    let notarr_status = notarr_resp.status().as_u16();
    let notarr_body = notarr_resp.text().unwrap();
    assert_eq!(
        notarr_status, 422,
        "a non-array value for the `tags` list field must be 422 \
         (D3: a list field requires a JSON array); got {notarr_status}, body={notarr_body:?}"
    );
    assert!(
        !notarr_body.contains(HANDLER_MARKER),
        "list-field-not-an-array 422 MUST NOT enter the handler; body={notarr_body:?}"
    );

    // --- Case 5: EMPTY lists → 201, handler entered (an empty array is VALID). ---
    // D3: an empty array is valid (no elements to reject; the field IS an
    // array). This already returns 201 today (the `any` field accepts it), but
    // it must STAY 201 once the array+element check lands — an empty list is
    // not "missing" and not "wrong type".
    let empty_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":[],"scores":[],"lines":[]}"#)
        .send()
        .expect("POST /orders empty lists");
    let empty_status = empty_resp.status().as_u16();
    let empty_body = empty_resp.text().unwrap();
    assert_eq!(
        empty_status, 201,
        "empty lists must be 201 (an empty array is valid — D3); got {empty_status}, body={empty_body:?}"
    );
    assert!(
        empty_body.contains(HANDLER_MARKER),
        "an empty-lists body is valid and MUST enter the handler; body={empty_body:?}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 2 (D3 — list[Class] per-element RECURSIVE validation): one
// server, several POSTs against the `lines: list[OrderLine]` field. EACH
// element of the list is recursively validated against the `OrderLine` block
// (reusing the nested-object `validate_block` + depth cap).
//
// A valid list of objects passes (201). FOUR distinct element-object
// invalidity classes each 422 WITHOUT entering the handler — each silently
// accepted TODAY (the `lines` field = `any`, no per-element recursion;
// verified 201 by the probe) → RED.
//
//   1. valid object element  lines:[{sku,qty:in-range}]            → 201, entered
//   2. element refinement     lines:[{sku, qty:9999}] (> 999)      → 422, NOT entered
//   3. element missing field  lines:[{qty:5}] (no `sku`)           → 422, NOT entered
//   4. element NOT an object  lines:["oops"] (a string, not obj)   → 422, NOT entered
//   5. element extra field    lines:[{sku,qty,color:"red"}]        → 422, NOT entered
// =====================================================================

#[test]
fn test_e2e_collection_body_object_element_recursive_validation() {
    let port = pick_free_port();
    let source = collection_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8))
        .expect("pit collection-body object-elem server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- Case 1: a valid list of objects → 201, handler entered. ---
    let ok_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":5},{"sku":"s2","qty":10}]}"#)
        .send()
        .expect("POST /orders valid object list");
    let ok_status = ok_resp.status().as_u16();
    let ok_body = ok_resp.text().unwrap();
    assert_eq!(
        ok_status, 201,
        "a list[OrderLine] with all-valid elements must be 201; got {ok_status}, body={ok_body:?}"
    );
    assert!(
        ok_body.contains(HANDLER_MARKER),
        "a valid object-list request MUST enter the handler; body={ok_body:?}"
    );

    // --- Case 2: an element violates the class refinement (qty=9999 > 999). ---
    // The list element {sku:"s1", qty:9999} fails OrderLine's
    // `qty <= 999` int-range refinement. TODAY the `any` `lines` field does
    // not recurse into the element → silently 201 (verified by the probe) →
    // RED. This is the load-bearing assertion: the per-element refinement is
    // enforced recursively, not just element-object-presence.
    let oor_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":9999}]}"#)
        .send()
        .expect("POST /orders element out-of-range qty");
    let oor_status = oor_resp.status().as_u16();
    let oor_body = oor_resp.text().unwrap();
    assert_eq!(
        oor_status, 422,
        "a list[OrderLine] element with qty=9999 (> 999) must be 422 (per-element \
         recursive range-check); got {oor_status}, body={oor_body:?}"
    );
    assert!(
        !oor_body.contains(HANDLER_MARKER),
        "element-out-of-range 422 MUST NOT enter the handler; body={oor_body:?}"
    );

    // --- Case 3: an element MISSING a field (no `sku`) → 422, NOT entered. ---
    let missing_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":[{"qty":5}]}"#)
        .send()
        .expect("POST /orders element missing sku");
    let missing_status = missing_resp.status().as_u16();
    let missing_body = missing_resp.text().unwrap();
    assert_eq!(
        missing_status, 422,
        "a list[OrderLine] element missing `sku` must be 422 (per-element recursive total \
         deser, SAME MissingField policy as the flat validator); got {missing_status}, body={missing_body:?}"
    );
    assert!(
        !missing_body.contains(HANDLER_MARKER),
        "element-missing-field 422 MUST NOT enter the handler; body={missing_body:?}"
    );

    // --- Case 4: an element is NOT a JSON object (a string) → 422, NOT entered. ---
    // D3: an `obj` element's JSON value MUST be a JSON object; a string is a
    // WrongType 422. TODAY `any` accepts it silently → RED.
    let notobj_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":["oops"]}"#)
        .send()
        .expect("POST /orders element not-an-object");
    let notobj_status = notobj_resp.status().as_u16();
    let notobj_body = notobj_resp.text().unwrap();
    assert_eq!(
        notobj_status, 422,
        "a non-object element in a list[OrderLine] must be 422 (D3: an obj element requires \
         a JSON object); got {notobj_status}, body={notobj_body:?}"
    );
    assert!(
        !notobj_body.contains(HANDLER_MARKER),
        "element-not-an-object 422 MUST NOT enter the handler; body={notobj_body:?}"
    );

    // --- Case 5: an element has an EXTRA undeclared key → 422, NOT entered. ---
    // D3: missing/extra element fields follow the SAME policy the flat
    // validator uses; the flat validator rejects unknown keys (`UnknownField`),
    // so an element extra key is 422.
    let extra_resp = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":5,"color":"red"}]}"#)
        .send()
        .expect("POST /orders element extra key");
    let extra_status = extra_resp.status().as_u16();
    let extra_body = extra_resp.text().unwrap();
    assert_eq!(
        extra_status, 422,
        "an undeclared element key (`color`) must be 422 (per-element recursive total deser, \
         SAME unknown-key policy as the flat validator); got {extra_status}, body={extra_body:?}"
    );
    assert!(
        !extra_body.contains(HANDLER_MARKER),
        "element-extra-key 422 MUST NOT enter the handler; body={extra_body:?}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 3 (D1 — flat/scalar fields stay byte-identical under a collection
// schema): against the SAME collection-body program, the ROOT flat `note`
// field still validates (missing + wrong-typed → 422), and a valid body still
// 201s. This guards the LOCKED constraint that adding list fields does NOT
// regress the flat-field validation of the ROOT block.
// =====================================================================

#[test]
fn test_e2e_collection_body_root_flat_field_still_validated() {
    let port = pick_free_port();
    let source = collection_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8))
        .expect("pit collection-body flat-field server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- ROOT `note` missing → 422 (the root block's flat field). ---
    let missing_note = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders missing root note");
    assert_eq!(
        missing_note.status().as_u16(),
        422,
        "the ROOT `note` field must still be required under a collection schema (D1 \
         flat-byte-identical); body={:?}",
        missing_note.text().unwrap()
    );

    // --- ROOT `note` wrong type (a number) → 422. ---
    let wrong_note = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":42,"tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders wrong-typed root note");
    assert_eq!(
        wrong_note.status().as_u16(),
        422,
        "the ROOT `note` must still type-check (str) under a collection schema; body={:?}",
        wrong_note.text().unwrap()
    );

    // --- A fully-valid body still 201s (the collection schema did not break
    //     the happy path). ---
    let ok = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":5}]}"#)
        .send()
        .expect("POST /orders valid");
    assert_eq!(
        ok.status().as_u16(),
        201,
        "a fully-valid collection body must still be 201; body={:?}",
        ok.text().unwrap()
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 4 (D4 — OpenAPI array+items + element-$ref + element component):
// GET /openapi.json must show the ROOT `CreateOrder` schema with:
//   * `tags`   = {type:array, items:{type:string}}        (list[str])
//   * `scores` = {type:array, items:{type:integer}}       (list[i64], no bound)
//   * `lines`  = {type:array, items:{$ref:#/.../OrderLine}} (list[Class])
// AND a SEPARATE `components/schemas/OrderLine` object schema carrying the
// element class's fields (sku:{string}, qty:{integer, minimum:1, maximum:999}).
//
// TODAY each list field emits an EMPTY schema `{}` (the `Any` arm) with NO
// `type:array`, NO `items`, and the `OrderLine` element component is ABSENT
// (verified by the TEST-phase probe) → every assertion below is RED.
// =====================================================================

#[test]
fn test_e2e_collection_body_openapi_array_items_and_element_component() {
    let port = pick_free_port();
    let source = collection_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit collection-body openapi server bind");

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

    // --- The ROOT CreateOrder component exists. ---
    let schemas = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .unwrap_or_else(|| panic!("doc must carry components/schemas; got:\n{body}"));
    let root = schemas
        .get("CreateOrder")
        .unwrap_or_else(|| panic!("components/schemas/CreateOrder missing; got:\n{body}"));
    let props = root
        .get("properties")
        .unwrap_or_else(|| panic!("CreateOrder must have properties; got root={root}"));

    // --- ROOT.note is still a plain string (flat field byte-identical). ---
    assert_eq!(
        props
            .get("note")
            .and_then(|n| n.get("type"))
            .and_then(|t| t.as_str()),
        Some("string"),
        "ROOT note must be {{type:string}}; got root={root}"
    );

    // --- tags = {type:array, items:{type:string}} (list[str], D4). ---
    // TODAY the list field is `any` → an empty `{}` schema (no array, no items).
    let tags = props
        .get("tags")
        .unwrap_or_else(|| panic!("ROOT must declare property `tags`; got root={root}"));
    assert_eq!(
        tags.get("type").and_then(|t| t.as_str()),
        Some("array"),
        "the `tags` list[str] field MUST be {{type:array}} (D4 — not an empty/any schema); \
         got tags={tags}"
    );
    assert_eq!(
        tags.get("items")
            .and_then(|i| i.get("type"))
            .and_then(|t| t.as_str()),
        Some("string"),
        "the `tags` list[str] field's items MUST be {{type:string}} (D4 element schema); \
         got tags={tags}"
    );

    // --- scores = {type:array, items:{type:integer}} (list[i64], no bound). ---
    // A list[i64] with NO element refinement still advertises the element base
    // type `integer` (no minimum/maximum). Proves the element-schema path is
    // not refinement-gated.
    let scores = props
        .get("scores")
        .unwrap_or_else(|| panic!("ROOT must declare property `scores`; got root={root}"));
    assert_eq!(
        scores.get("type").and_then(|t| t.as_str()),
        Some("array"),
        "the `scores` list[i64] field MUST be {{type:array}}; got scores={scores}"
    );
    assert_eq!(
        scores
            .get("items")
            .and_then(|i| i.get("type"))
            .and_then(|t| t.as_str()),
        Some("integer"),
        "the `scores` list[i64] field's items MUST be {{type:integer}} (element base type, \
         even with no element refinement); got scores={scores}"
    );

    // --- lines = {type:array, items:{$ref:#/components/schemas/OrderLine}}. ---
    // D4: a list[Class] element renders as a $ref to the element component.
    let lines = props
        .get("lines")
        .unwrap_or_else(|| panic!("ROOT must declare property `lines`; got root={root}"));
    assert_eq!(
        lines.get("type").and_then(|t| t.as_str()),
        Some("array"),
        "the `lines` list[OrderLine] field MUST be {{type:array}}; got lines={lines}"
    );
    assert_eq!(
        lines
            .get("items")
            .and_then(|i| i.get("$ref"))
            .and_then(|r| r.as_str()),
        Some("#/components/schemas/OrderLine"),
        "the `lines` list[OrderLine] field's items MUST be a $ref to the OrderLine component \
         (D4 — element-class $ref, not an inline/empty schema); got lines={lines}"
    );

    // --- A SEPARATE components/schemas/OrderLine object schema exists (D4). ---
    let order_line = schemas.get("OrderLine").unwrap_or_else(|| {
        panic!(
            "the list element class MUST be registered as its own \
             components/schemas/OrderLine (D4 — derived from the same parse via the BFS \
             collector); got:\n{body}"
        )
    });
    assert_eq!(
        order_line.get("type").and_then(|t| t.as_str()),
        Some("object"),
        "the OrderLine component must be an object schema; got order_line={order_line}"
    );
    // sku: {type:string}
    assert_eq!(
        order_line
            .get("properties")
            .and_then(|p| p.get("sku"))
            .and_then(|c| c.get("type"))
            .and_then(|t| t.as_str()),
        Some("string"),
        "OrderLine.sku must be {{type:string}}; got order_line={order_line}"
    );
    // qty: {type:integer, minimum:1, maximum:999} — the element refinement
    // bounds, advertised on the element component (the SAME bounds the
    // per-element recursive validator enforces — cannot drift).
    let qty = order_line
        .get("properties")
        .and_then(|p| p.get("qty"))
        .unwrap_or_else(|| {
            panic!("OrderLine must declare property `qty`; got order_line={order_line}")
        });
    assert_eq!(
        qty.get("type").and_then(|t| t.as_str()),
        Some("integer"),
        "OrderLine.qty must be {{type:integer}}; got qty={qty}"
    );
    assert_eq!(
        qty.get("minimum").and_then(serde_json::Value::as_i64),
        Some(1),
        "OrderLine.qty.minimum must be 1 (the element refinement lower bound); got qty={qty}"
    );
    assert_eq!(
        qty.get("maximum").and_then(serde_json::Value::as_i64),
        Some(999),
        "OrderLine.qty.maximum must be 999 (the element refinement upper bound); got qty={qty}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// MUST-HAVE 5 (D3 + D4 cannot-drift, the load-bearing #156 property):
// against the SAME running server, the bound the per-element recursive
// VALIDATOR enforces on a list[Class] element and the bound the OpenAPI doc
// ADVERTISES on the element component come from ONE source, so they agree.
//   * POST {lines:[{sku,qty:9999}]} → 422 (validator rejects 9999 > 999); AND
//   * GET /openapi.json shows components/schemas/OrderLine.qty.maximum == 999.
// If a future change moved one without the other (e.g. the OpenAPI element
// component derived from a SECOND source), this equality would break —
// exactly the drift D4's single-source design forbids (footgun #4).
// =====================================================================

#[test]
fn test_e2e_collection_body_validator_and_openapi_cannot_drift() {
    let port = pick_free_port();
    let source = collection_body_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8))
        .expect("pit collection-body cannot-drift server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- (a) The per-element recursive validator REJECTS the element qty=9999. ---
    let post = client
        .post(format!("{base}/orders"))
        .header("Content-Type", "application/json")
        .body(r#"{"note":"x","tags":["a"],"scores":[1],"lines":[{"sku":"s1","qty":9999}]}"#)
        .send()
        .expect("POST /orders element qty=9999");
    let post_status = post.status().as_u16();
    let post_body = post.text().unwrap();
    assert_eq!(
        post_status, 422,
        "a list element with qty=9999 (> the enforced element max 999) must be 422 — the \
         per-element recursive validator's behaviour the element schema must match; \
         got {post_status}, body={post_body:?}"
    );
    assert!(
        !post_body.contains(HANDLER_MARKER),
        "the element-422 path must NOT enter the handler; body={post_body:?}"
    );

    // --- (b) The served element component ADVERTISES maximum == 999. ---
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
        .and_then(|s| s.get("OrderLine"))
        .and_then(|a| a.get("properties"))
        .and_then(|p| p.get("qty"))
        .and_then(|q| q.get("maximum"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| {
            panic!(
                "components/schemas/OrderLine.qty.maximum must be present (D4 element \
                 component); doc=\n{doc}"
            )
        });

    // --- The cannot-drift property: ONE source, consistent. ---
    assert_eq!(
        advertised_max, 999,
        "the element component's advertised OrderLine.qty.maximum ({advertised_max}) must \
         equal the bound the per-element recursive validator enforces (999, proven by the 422 \
         on element qty=9999) — ONE source, cannot drift (D4 / ADR-0080 footgun #4)"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}
