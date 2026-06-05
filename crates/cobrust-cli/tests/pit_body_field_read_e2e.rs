//! ADR-0081 validated-body `body.field` RUNTIME READ, end-to-end.
//!
//! STATUS (Phase-1b SHIPPED + Phase-2 SHIPPED + Phase-3 SHIPPED): the runtime
//! read is LIVE. `body.<i64|str>` (Phase-1b), `body.<f64|bool>` (Phase-2),
//! the NESTED chain `body.inner.x` / `body.mid.leaf.v` (Phase-2 nested), AND
//! `body.<list[T]>` for T ∈ {str, i64, f64, bool} (Phase-3) all read the REAL
//! validated value, OBSERVABLE on the wire (the handler RESPONSE flips with
//! the read value). This corpus, written TEST-FIRST and now GREEN, pins that
//! behaviour against regression.
//!
//! ## ADR-0081 Phase-3 (list fields + body-as-fn-arg) — what this adds
//!
//!   * `body.<list[T]-field>` (T ∈ str/i64/f64/bool) MINTS a fresh `.cb`
//!     `list[T]` from the validated JSON array via one accessor per element
//!     type (`__cobrust_pit_body_get_list_{str,i64,f64,bool}`, `cabi.rs`),
//!     the redis-`lrange` / coil-`shape` `__cobrust_list_new(8,len)` +
//!     per-slot `__cobrust_list_set` mint recipe. The handler READS + ITERATES
//!     the real array (`xs.len()`, `for s in body.tags:`), so the response
//!     reflects the genuine element values. The minted list is `.cb`-owned +
//!     drops EXACTLY ONCE (`Ty::List(Str)` → `__cobrust_list_drop_elems`, else
//!     `__cobrust_list_drop`) — pinned by a 200-read hammer-loop that proves
//!     the server survives (no leak / no double-free). Element-type VALIDATION
//!     already shipped in ADR-0080 Phase-4(c) (`pit_collection_body_e2e.rs`):
//!     a type-mismatched array (`{"tags":["a",42]}`) is a 422 BEFORE the
//!     handler, so the accessor is a pure typed read (§2.2 — no coercion).
//!   * **body-as-fn-arg**: passing a READ FIELD VALUE (an `i64`/`str`/`list`)
//!     to another fn is DELIVERED (it is an ordinary value arg — pinned by
//!     `test_e2e_body_arg_read_field_value_to_fn`). Passing the WHOLE
//!     validated `body` to another fn is DEFERRED: the `validated_body_of`
//!     mark does NOT propagate across a call boundary, so a `b.field` read in
//!     the CALLEE hits the `Field(0)` stub (a wrong value, NOT UB — the
//!     registration gate still prevents the serde cast). The ignored
//!     `test_e2e_body_arg_whole_body_to_fn_DEFERRED` documents the gap +
//!     proves no-UB (no accessor symbol emitted, clean run).
//!
//! ## What "real read" means (the load-bearing assertion shape)
//!
//! Each read test drives ONE handler/route whose RESPONSE DEPENDS on the
//! read value, then issues the SAME request to that route differing ONLY in
//! the value of the field under test, and asserts the response FLIPS. A
//! stub-load (the pre-impl `Projection::Field(0)` that discards the field
//! name) makes the branch CONSTANT, so it CANNOT produce both arms — the
//! flip is the proof the value was genuinely read at runtime.
//!
//! HISTORICAL RED (captured at HEAD `984872e`, before Phase-1b landed — kept
//! for provenance; the assertions below are now GREEN):
//!   POST {name:a,rank:50}  -> 200 "high"   (correct + observed)
//!   POST {name:a,rank:10}  -> 200 "high"   (WRONG then: a real read says "low")
//!   POST {name:hello,...}  -> 200 ""        (WRONG then: a real read echoes "hello")
//! i.e. the branch was CONSTANT and the str read empty — the stub. The
//! shipped impl makes the branch flip ("high"/"low") and echoes "hello".
//!
//! The feature this corpus pins (ADR-0081 §2 Q2/Q4/Q5, §5.2, §6 Phase-1 +
//! Phase-2):
//!   * the typed accessor shims `__cobrust_pit_body_get_{i64,str}`
//!     (Phase-1b) + `__cobrust_pit_body_get_{f64,bool,nested}` (Phase-2),
//!     cloned from the `(ptr,ptr)->ret` `path_param` template (`cabi.rs`):
//!     borrow the boxed `serde_json::Value` the validator left, do a typed
//!     `v.get(name).and_then(as_{i64,str,f64,bool})` (NO coercion — the i64
//!     shim deliberately does NOT widen via `as_f64`, footgun #3; the f64
//!     shim reads `as_f64` of a DECLARED-f64 field; the bool shim is strict
//!     `as_bool`). The NESTED shim returns the BORROWED interior `&Value`
//!     for the nested object (no alloc/free — it lives in the parent box the
//!     trampoline frees once, `cabi.rs:530`);
//!   * the checker->MIR registration channel — `TypedModule
//!     .validated_handlers: HashMap<DefId, (usize, AdtId)>` populated in
//!     `check_eco_sig` + the `LocalDecl.validated_body_of: Option<AdtId>`
//!     mark set in MIR when lowering a registered handler's body param;
//!   * the REGISTRATION-DRIVEN MIR `Attr` sub-arm (Q4): the serde-accessor
//!     retarget fires ONLY when the base resolves (RECURSIVELY for nesting,
//!     via `resolve_validated_body_base`) to a local carrying
//!     `validated_body_of == Some(id)` AND the field is in that class's
//!     `adt_fields` — NEVER on `Ty::Adt`-with-a-field-table alone. A nested
//!     read re-marks its result temp `validated_body_of = Some(nested_adt)`
//!     so a further `.field` recurses. The field-name `Str` is
//!     COMPILER-SYNTHESISED (footgun #1), and MIR names a SYMBOL + a `Ty`,
//!     never serde / a JSON key (the §2-Q5 swappable seam).
//!
//! Harness: mirrors `pit_json_response_e2e.rs` / `pit_validated_body_e2e.rs`
//! EXACTLY — compile a `.cb` source to an exe, pick an ephemeral free port
//! (bind-and-drop a `TcpListener`), spawn the binary, poll the port until
//! the server binds, issue real HTTP via `reqwest::blocking`, assert
//! status/body, and an RAII `ChildGuard` kills the process on Drop so a
//! failing assertion never leaks the spawned `.cb` server. The keep-alive
//! is `app.run(host, port)` (blocks until killed, the z8 demo's shape).
//!
//! ```text
//! `import pit` + a body `class` + a validated handler that READS body.field
//! + branches/echoes + `app.route_validated(...)` + `app.run(...)`
//!   → cobrust-frontend parse (`class` typed-field body + `where`-clause + `body.field`)
//!   → cobrust-types check (body.field typed against adt_fields; validated_handlers registry — NEW)
//!   → cobrust-mir (LocalDecl.validated_body_of mark; registration-gated Attr sub-arm; recursive base resolve for nesting)
//!   → cobrust-codegen (body_get externs `(ptr,ptr)->{i64|f64|i1|ptr}`)
//!   → cobrust-pit C-ABI shims `__cobrust_pit_body_get_{i64,str,f64,bool,nested}` (typed serde get over the boxed Value)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client POSTs bodies; the RESPONSE depends on the READ value
//! ```

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Long live-server assertion tests (each drives multiple routes + asserts the
// flip on every read) exceed the pedantic 100-line cap — the established
// pattern in the sibling pit e2e files (`pit_collection_body_e2e.rs`,
// `pit_nested_body_e2e.rs`).
#![allow(clippy::too_many_lines)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// =====================================================================
// Harness — copied verbatim from pit_json_response_e2e.rs (itself from
// pit_validated_body_e2e.rs / pit_pong_e2e.rs) so the live-server E2Es
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

/// Compile a `.cb` source to a RELOCATABLE OBJECT (`--emit obj`) and return
/// its path. Unlike the linked exe, a `.o` references an external symbol
/// IFF codegen emitted a call to it — its undefined-symbol table + its
/// `bl`/`call` relocations are a direct, link-time-stable window onto which
/// runtime shims the codegen actually called. Used by the disassembly
/// tripwire below: a `.cb`-constructed `b.field` read that (wrongly) hit
/// the serde accessor would leave an undefined `__cobrust_pit_body_get_*`
/// reference here; the registration gate leaves none.
fn compile_object(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let obj = dir.path().join("prog.o");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&obj)
        .arg("--emit")
        .arg("obj")
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "object build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, obj)
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

// =====================================================================
// (1) MUST-HAVE: the live RUNTIME FIELD READ, OBSERVABLE over HTTP.
//
// The ADR-0081 §5.1 Phase-1 handler, now READING the validated body. The
// handler's RESPONSE DEPENDS on the read value, so a correct runtime read
// is observable on the wire and a stub-load read is caught:
//
//   class CreateScore: name: str ; rank: i64 where 0 <= self <= 100
//   fn create_score(req, body) -> Response:
//       let r: i64 = body.rank          # the i64 runtime read under test
//       if r >= 50: return text_response(200, "high")
//       return text_response(200, "low")
//   fn read_name(req, body) -> Response:
//       let n: str = body.name          # the str runtime read under test
//       return text_response(200, n)
//
// One server, two routes:
//   POST /scores {name:a,rank:50}  -> 200 "high"   (r==50, >=50)
//   POST /scores {name:a,rank:10}  -> 200 "low"    (r==10, <50  — PROVES the read:
//                                                   a constant branch can't flip here)
//   POST /scores {name:a,rank:200} -> 422          (out-of-range, handler not entered,
//                                                   unchanged from ADR-0080)
//   POST /name   {name:hello,rank:1} -> 200 "hello" (PROVES the str read)
// =====================================================================

/// The two-route field-read program. `create_score` BRANCHES on
/// `body.rank` (the i64 read); `read_name` ECHOES `body.name` (the str
/// read). Both classes/handlers are declared BEFORE `main` (a
/// signature-position forward ref to a LATER class is a known limitation,
/// so order matters).
fn field_read_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            "class CreateScore:\n",
            "    name: str\n",
            "    rank: i64 where 0 <= self and self <= 100\n",
            "\n",
            // The i64 read under test: the response DEPENDS on `body.rank`.
            // A correct read returns "high" for rank>=50 and "low" for
            // rank<50; a stub-load (Field(0), name discarded) returns a
            // CONSTANT branch regardless of the input rank.
            "fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:\n",
            "    let r: i64 = body.rank\n",
            "    if r >= 50:\n",
            "        return pit.text_response(200, \"high\")\n",
            "    return pit.text_response(200, \"low\")\n",
            "\n",
            // The str read under test: the response ECHOES `body.name`. A
            // correct read returns "hello"; a stub-load returns "".
            "fn read_name(req: pit.Request, body: CreateScore) -> pit.Response:\n",
            "    let n: str = body.name\n",
            "    return pit.text_response(200, n)\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/scores\", create_score)\n",
            "    let _ = app.route_validated(\"POST\", \"/name\", read_name)\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

#[test]
fn test_e2e_body_field_read_branches_and_echoes() {
    let port = pick_free_port();
    let source = field_read_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit body-field-read server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- i64 read, branch HIGH: rank=50 (>= 50) -> "high". ---
    let high_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":50}"#)
        .send()
        .expect("POST /scores rank=50");
    let high_status = high_resp.status().as_u16();
    let high_body = high_resp.text().unwrap();
    assert_eq!(
        high_status, 200,
        "rank=50 (valid) must be 200; got {high_status}, body={high_body:?}"
    );
    assert_eq!(
        high_body, "high",
        "rank=50 (>= 50) must branch to \"high\" (a correct `body.rank` read); \
         got {high_body:?}"
    );

    // --- i64 read, branch LOW: rank=10 (< 50) -> "low". THE PROOF: this
    // is the SAME handler, the SAME route, differing ONLY in the value of
    // `body.rank`. If the branch flips with the input, `body.rank` was
    // genuinely READ at runtime. A stub-load (Field(0), name discarded)
    // makes the branch CONSTANT, so it cannot produce BOTH "high" (rank=50)
    // AND "low" (rank=10) — exactly the RED at HEAD `984872e` (rank=10
    // observed returning "high"). ---
    let low_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":10}"#)
        .send()
        .expect("POST /scores rank=10");
    let low_status = low_resp.status().as_u16();
    let low_body = low_resp.text().unwrap();
    assert_eq!(
        low_status, 200,
        "rank=10 (valid) must be 200; got {low_status}, body={low_body:?}"
    );
    assert_eq!(
        low_body, "low",
        "rank=10 (< 50) must branch to \"low\" — this PROVES `body.rank` is read at \
         RUNTIME (the branch flips with the input value). At HEAD `984872e` this is \
         RED: `body.rank` lowers to the Field(0) stub (name discarded, lower.rs:1476), \
         so the branch is CONSTANT and rank=10 wrongly returns \"high\". got {low_body:?}"
    );

    // --- str read: name="hello" -> echoed body "hello". PROVES the str
    // read. At HEAD the Field(0) stub returns an empty Str. ---
    let name_resp = client
        .post(format!("{base}/name"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"hello","rank":1}"#)
        .send()
        .expect("POST /name name=hello");
    let name_status = name_resp.status().as_u16();
    let name_body = name_resp.text().unwrap();
    assert_eq!(
        name_status, 200,
        "name=hello (valid) must be 200; got {name_status}, body={name_body:?}"
    );
    assert_eq!(
        name_body, "hello",
        "the handler echoes `body.name` — a correct str read returns \"hello\". \
         At HEAD `984872e` this is RED: the Field(0) stub returns an empty Str. \
         got {name_body:?}"
    );

    // --- out-of-range still 422 (unchanged from ADR-0080): the read work
    // adds reads ON TOP, touches neither the validator nor the 422 path. ---
    let oor_resp = client
        .post(format!("{base}/scores"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","rank":200}"#)
        .send()
        .expect("POST /scores rank=200");
    let oor_status = oor_resp.status().as_u16();
    let oor_body = oor_resp.text().unwrap();
    assert_eq!(
        oor_status, 422,
        "rank=200 (> 100) must still be 422, handler NOT entered (unchanged from \
         ADR-0080 — field reads must not break the validation path); got {oor_status}, \
         body={oor_body:?}"
    );
    assert_ne!(
        oor_body, "high",
        "the 422 path must NOT enter the handler (so it can never carry the handler's \
         \"high\"/\"low\" branch output); got {oor_body:?}"
    );
    assert_ne!(
        oor_body, "low",
        "the 422 path must NOT enter the handler; got {oor_body:?}"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// (2) THE no-UB RUNTIME-SURVIVAL probe (ADR-0081 §2-Q4 / §5.2 / §10).
//
// A NON-registered tracked-body param must NOT fire the serde accessor
// shim. The serde shim is REGISTRATION-gated (`validated_body_of ==
// Some(id)`), NOT type-gated. A `.cb`-CONSTRUCTED `CreateScore()` has the
// SAME `Ty::Adt(real-id)` + the SAME `adt_fields` table as a validated
// body, but its `*mut u8` is a null/opaque pointer
// (`AggregateKind::Adt(_,_) => opaque_ptr_ty.const_null()`,
// `llvm_backend.rs:5037`), NOT a boxed `serde_json::Value`.
//
// THE PROGRAM (no server — a plain `.cb` exe):
//   class CreateScore: name: str ; rank: i64 where 0 <= self <= 100
//   fn helper(b: CreateScore) -> i64:        # NOT route_validated-registered
//       return b.rank
//   fn main() -> i64:
//       let s = CreateScore()                # .cb-constructed: null/opaque ptr
//       let v: i64 = helper(s)               # b.rank read inside a non-registered fn
//       print(v)
//       return 0
//
// WHAT THIS RUNTIME TEST PROVES — AND, HONESTLY, WHAT IT DOES NOT:
//   * What it PROVES: the program BUILDS + RUNS to a CLEAN exit. A
//     `b.rank` read on a `.cb`-constructed instance, inside a
//     non-registered fn, does not crash/abort/UB the process. This is
//     CONSISTENT with the §5.2 registration gate holding (the read hit the
//     pre-existing `Field(0)` no-field-storage stub, not the serde shim).
//   * What it DOES NOT prove (the load-bearing honesty fix — see the
//     ADSD audit, GO_WITH_FINDINGS, 2026-05-30): this runtime survival
//     ALONE does NOT distinguish a REGISTRATION gate from a TYPE-ONLY
//     gate. Two facts make a type-only-gate regression ALSO exit cleanly
//     here, so a clean exit is NOT a sufficient standalone UB tripwire:
//       (a) every `.cb`-constructed Adt is a NULL pointer today
//           (`AggregateKind::Adt(_,_) => const_null()`,
//           `llvm_backend.rs:5037`), so even if a type-only gate emitted
//           `bl __cobrust_pit_body_get_i64` on `helper`'s `b`, the arg
//           passed is null — NOT a wild/garbage pointer; and
//       (b) BOTH shims null-guard on entry (`cabi.rs:862`/`887`:
//           `if body.is_null() { return 0 / empty }`), so the shim
//           returns cleanly instead of dereferencing the null Value.
//     EMPIRICAL CONFIRMATION (audit mutation, reproduced by this agent
//     2026-05-30): replacing the `lower.rs:741` gate with a TYPE-ONLY gate
//     (`let Ty::Adt(body_adt, _) = decl.ty else { return None };`) STILL
//     made this runtime test PASS — disassembly of the helper's `.o`
//     showed `bl ... ARM64_RELOC_BRANCH26 ___cobrust_pit_body_get_i64`
//     WAS emitted, yet the program ran to a clean exit because of (a)+(b).
//     The clean-exit assertion held whether the gate was registration-
//     driven OR type-only — i.e. by itself it is FALSE COMFORT.
//
// Therefore the ACTUAL gate-kind tripwire is the SEPARATE disassembly
// test below (`test_no_ub_..._does_not_emit_accessor_call`): it asserts
// the CODEGEN PROPERTY (no `__cobrust_pit_body_get_*` call site is emitted
// on the non-registered helper) that the null-guard masks at runtime. THAT
// test goes RED under a type-only gate; THIS one does not. This runtime
// probe is retained only as a coarse "the non-registered path at least
// does not crash" smoke check, not as the registration-gate guard.
// =====================================================================

/// The no-UB negative program: a non-registered `helper(b: CreateScore)`
/// reads `b.rank` on a `.cb`-constructed instance.
const NO_UB_PROGRAM: &str = concat!(
    "import pit\n",
    "\n",
    "class CreateScore:\n",
    "    name: str\n",
    "    rank: i64 where 0 <= self and self <= 100\n",
    "\n",
    // helper is NOT route_validated-registered -> its `b` param has
    // validated_body_of == None -> `b.rank` must NOT serde-cast.
    "fn helper(b: CreateScore) -> i64:\n",
    "    return b.rank\n",
    "\n",
    "fn main() -> i64:\n",
    // .cb-constructed instance: a null/opaque pointer (llvm_backend.rs:5016),
    // NOT a boxed serde_json::Value.
    "    let s = CreateScore()\n",
    "    let v: i64 = helper(s)\n",
    // Observable: prints the read value (stub garbage today, but MUST NOT crash).
    "    print(v)\n",
    "    return 0\n",
);

/// Runtime-SURVIVAL probe for the no-UB negative. PROVES: a non-registered
/// `fn helper(b: CreateScore): return b.rank` on a `.cb`-constructed
/// instance builds + runs to a CLEAN exit (no crash/abort/UB) — consistent
/// with the §5.2 registration gate holding.
///
/// HONEST SCOPE (do not overclaim): a clean exit here does NOT, by itself,
/// distinguish a REGISTRATION gate from a TYPE-ONLY gate. A type-only-gate
/// regression ALSO exits cleanly, because (a) every `.cb`-constructed Adt
/// is a NULL pointer today (`llvm_backend.rs:5037` `const_null`) and (b)
/// both shims null-guard on entry (`cabi.rs:862`/`887`). This agent
/// reproduced the audit mutation: a type-only gate emitted the accessor
/// call yet THIS test still PASSED. The gate-KIND guard is the separate
/// disassembly test `test_no_ub_non_registered_body_field_read_does_not_\
/// emit_accessor_call`, which DOES go RED under a type-only gate. This test
/// is only a coarse "the non-registered path does not crash" smoke check.
#[test]
fn test_no_ub_non_registered_body_field_read_runs_clean() {
    // Build the standalone exe (no server). The build is expected to
    // SUCCEED — the no-UB invariant is a RUNTIME guarantee (no crash on a
    // `.cb`-constructed `b.rank`), not a compile gate. (If a future impl
    // instead rejects this at compile time — the "clean error" arm of the
    // §5.2 no-UB invariant — the build assert in `compile_source` fires and
    // the sibling `test_no_ub_non_registered_param_check_is_well_defined`
    // documents that the OTHER acceptable arm is a CLEAN error. Either way
    // the forbidden outcome is a crash/UB, which neither arm produces.)
    let (_dir, exe) = compile_source(NO_UB_PROGRAM);

    // Run it. Assertion: a CLEAN exit (status success, no killing signal).
    // NOTE — this proves the process SURVIVES the `b.rank` read; it does
    // NOT prove the gate is registration-driven (a type-only gate would
    // ALSO survive here: the `.cb`-constructed pointer is null and both
    // shims null-guard, so even an erroneously-emitted serde shim returns
    // cleanly rather than dereferencing a wild pointer). The gate-kind
    // discrimination lives in the disassembly tripwire below.
    let out = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run the no-UB negative exe");

    assert!(
        out.status.success(),
        "the no-UB negative MUST run to a CLEAN exit — a non-registered \
         `fn helper(b: CreateScore): return b.rank` reading a `.cb`-constructed \
         `CreateScore()` MUST NOT crash/abort (no UB) on the non-registered path. \
         (A clean exit is necessary but NOT sufficient to prove the registration \
         gate — see this fn's docstring + the disassembly tripwire.) exit={:?}, \
         stdout={:?}, stderr={:?}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Belt-and-suspenders: the process produced SOME stdout line (the
    // `print(v)`), confirming it reached and passed the `b.rank` read site
    // rather than aborting before it. (We do NOT assert the VALUE — at
    // HEAD it is the Field(0) stub garbage; the only contract here is
    // no-crash. After the impl, `helper`'s `b.rank` is still the deferred
    // no-field-storage stub — §5.2 — so the value remains unspecified;
    // the registration gate is what the disassembly tripwire pins, not the
    // helper's read value.)
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "the no-UB negative must reach `print(v)` (proving it passed the `b.rank` \
         read site without crashing); stdout was empty. stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );
}

// =====================================================================
// (2b) THE REAL no-UB TRIPWIRE — a CODEGEN-PROPERTY assertion that goes
// RED under a type-only gate (the audit's GO_WITH_FINDINGS recommendation,
// 2026-05-30).
//
// WHY a separate test: the runtime-survival probe (2) above cannot
// distinguish a registration gate from a type-only gate, because the
// `.cb`-constructed body pointer is NULL (`llvm_backend.rs:5037`
// `const_null`) and both accessor shims null-guard on entry
// (`cabi.rs:862`/`887`) — so even an erroneously-emitted serde shim
// returns cleanly. The runtime crash the old test asserted on is MASKED
// by the null-guard. We must instead assert the CODEGEN PROPERTY that the
// null-guard masks: the non-registered helper must NOT EMIT a call to any
// `__cobrust_pit_body_get_*` shim at all.
//
// HOW: compile the helper-only program (NO `route_validated` handler that
// reads a field — so the ONLY possible `__cobrust_pit_body_get_*` call
// site in the whole program is the non-registered `fn helper(b): b.rank`)
// to a RELOCATABLE `.o`. A `.o` references an external symbol IFF codegen
// emitted a `bl`/`call` to it; the linker has not yet had a chance to
// dead-strip or resolve it. We scan the object's symbol table (`nm`) for
// any reference to `__cobrust_pit_body_get_`:
//   * SHIPPED registration gate (`lower.rs:741` `decl.validated_body_of?`):
//     `helper`'s `b` is unmarked -> the read falls to the `Field(0)` stub
//     -> NO accessor call emitted -> NO such symbol -> GREEN.
//   * TYPE-ONLY gate regression (`let Ty::Adt(id,_) = decl.ty`): `helper`'s
//     `b` is `Ty::Adt`-with-fields -> the serde accessor IS retargeted ->
//     `bl __cobrust_pit_body_get_i64` emitted -> the symbol appears -> RED.
//
// VERIFIED by this agent (2026-05-30): under the shipped gate the helper's
// `.o` has NO `pit_body_get` symbol (`nm` empty); under the type-only
// mutation `nm` shows `U ___cobrust_pit_body_get_i64` and `objdump -dr`
// shows `bl ... ARM64_RELOC_BRANCH26 ___cobrust_pit_body_get_i64` inside
// `_helper`. So this test is GREEN on the real code and RED under the
// regression — the gold-standard gate-kind tripwire.
//
// NOTE on the symbol form: codegen names the shim `__cobrust_pit_body_get_*`;
// Mach-O `nm` prints it with an extra leading underscore (`___cobrust_...`);
// ELF `nm` would print `__cobrust_...`. We match the toolchain-independent
// substring `cobrust_pit_body_get_` to be robust across both. A bare
// `declare`/extern with no call site cannot appear in a `.o` symbol table
// as a referenced undefined symbol unless something actually calls it, so
// substring-presence == an emitted call site (the regression), absence ==
// the gate held.
// =====================================================================

/// Locate a usable `nm` (the object-symbol reader). Prefer the LLVM
/// toolchain's `llvm-nm` next to `LLVM_SYS_181_PREFIX` (matches the codegen
/// backend), then the env `NM`, then a bare `nm` on `PATH` (system /usr/bin
/// on macOS, binutils on Linux). Returns `None` if none is runnable so the
/// tripwire can SKIP with a clear message rather than flaky-fail on a host
/// without object tooling.
fn find_nm() -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(prefix) = std::env::var("LLVM_SYS_181_PREFIX") {
        candidates.push(format!("{prefix}/bin/llvm-nm"));
    }
    if let Ok(nm) = std::env::var("NM") {
        candidates.push(nm);
    }
    candidates.push("nm".to_string());
    // `--version` is understood by both binutils `nm` and `llvm-nm`; the
    // first candidate that runs it successfully is our object-symbol reader.
    candidates.into_iter().find(|cand| {
        Command::new(cand)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// THE gate-kind tripwire. The non-registered helper's compiled object MUST
/// NOT reference any `__cobrust_pit_body_get_*` accessor shim. GREEN under
/// the shipped registration gate; RED under a type-only gate.
#[test]
fn test_no_ub_non_registered_body_field_read_does_not_emit_accessor_call() {
    let Some(nm) = find_nm() else {
        // No object-symbol reader on this host: SKIP cleanly (do NOT
        // flaky-fail). The runtime-survival probe + check-only sibling
        // still run; only the codegen-property tripwire needs `nm`.
        eprintln!(
            "SKIP test_no_ub_non_registered_body_field_read_does_not_emit_accessor_call: \
             no runnable `nm`/`llvm-nm` found (set $NM or $LLVM_SYS_181_PREFIX). The \
             registration-gate codegen property is unverified on this host."
        );
        return;
    };

    let (_dir, obj) = compile_object(NO_UB_PROGRAM);

    let nm_out = Command::new(&nm)
        .arg(&obj)
        .output()
        .expect("run nm on the no-UB object");
    assert!(
        nm_out.status.success(),
        "`{nm}` failed on {obj:?}: status={:?}, stderr={:?}",
        nm_out.status,
        String::from_utf8_lossy(&nm_out.stderr),
    );
    let symbols = String::from_utf8_lossy(&nm_out.stdout);

    // The CRITICAL assertion: NO reference to the body-accessor shim. The
    // ONLY possible call site in this program is the non-registered
    // `helper`'s `b.rank`; under the registration gate it is the `Field(0)`
    // stub (no external call), so the symbol must be ABSENT. Its PRESENCE
    // means codegen emitted `bl __cobrust_pit_body_get_*` on an unmarked
    // local — the type-only-gate serde-cast UB regression the §5.2 no-UB
    // invariant forbids. (Substring `cobrust_pit_body_get_` matches both
    // Mach-O `___cobrust_...` and ELF `__cobrust_...`.)
    let offending: Vec<&str> = symbols
        .lines()
        .filter(|l| l.contains("cobrust_pit_body_get_"))
        .collect();
    assert!(
        offending.is_empty(),
        "REGRESSION (type-only gate / serde-cast UB): the non-registered \
         `fn helper(b: CreateScore): return b.rank` emitted a call to a \
         `__cobrust_pit_body_get_*` accessor shim. The serde accessor must fire \
         ONLY on a `validated_body_of`-MARKED local (ADR-0081 §5.2 Q4 gate, \
         `lower.rs:741`), NEVER on a `.cb`-constructed / non-registered \
         `Ty::Adt`-with-fields local. (This is the codegen property the runtime \
         null-guard masks — see test (2)'s docstring.) Offending `nm` symbol \
         line(s):\n{}\nfull `{nm}` output:\n{symbols}",
        offending.join("\n"),
    );
}

/// Sibling assertion (the OTHER acceptable arm of the §5.2 no-UB
/// invariant): a non-registered `b.rank` read is permitted to be a CLEAN
/// compile-time error INSTEAD of a deferred-stub runtime read — but it
/// must NEVER be a serde-cast (UB). This `cobrust check` probe (no codegen,
/// runs without a C toolchain — the `error_ux_corpus.rs` idiom) asserts
/// that whichever arm the impl picks, the outcome is well-defined: the
/// check either SUCCEEDS (the deferred-stub arm — the program is accepted)
/// OR FAILS with a clean diagnostic (NOT a panic / ICE). It must not crash
/// the compiler.
#[test]
fn test_no_ub_non_registered_param_check_is_well_defined() {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, NO_UB_PROGRAM).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    let combined = {
        let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&out.stderr));
        s
    };
    // The compiler must terminate normally (a clean exit code), whether it
    // accepts (the deferred-stub arm) or rejects (the clean-error arm). A
    // panic / ICE (signalled exit, or a `panicked at`/`internal compiler`
    // banner) is the forbidden outcome.
    assert!(
        out.status.code().is_some(),
        "`cobrust check` on the no-UB negative must terminate normally (not be \
         killed by a signal / panic-abort); combined output:\n{combined}"
    );
    assert!(
        !combined.contains("panicked at") && !combined.to_lowercase().contains("internal compiler"),
        "`cobrust check` on the no-UB negative must NOT panic / ICE — a \
         non-registered `b.rank` is either accepted (deferred stub) or a CLEAN \
         diagnostic, never an unhandled crash; combined output:\n{combined}"
    );
}

// =====================================================================
// (3) ADR-0081 Phase-2 — `f64` + `bool` validated-body field READs, live.
//
// Same "real read = the branch flips with the input" shape as (1), now for
// the Phase-2 scalar types:
//   * `body.<f64-field>` reads via `__cobrust_pit_body_get_f64` (serde
//     `as_f64`). ratio=0.7 -> "high-ratio", ratio=0.2 -> "low-ratio".
//   * `body.<bool-field>` reads via `__cobrust_pit_body_get_bool` (serde
//     `as_bool`, the LLVM `i1` ABI — the `re.match` precedent). active=true
//     -> "active", active=false -> "inactive".
// A stub-load makes EACH branch constant, so it cannot produce BOTH arms.
// =====================================================================

/// One server, two routes: `/ratio` BRANCHES on the f64 `body.ratio`, and
/// `/active` BRANCHES on the bool `body.active`. The body carries BOTH a
/// refined f64 (`ratio: f64 where 0.0 <= self <= 1.0`, mirroring
/// `pit_float_refinement_e2e.rs`) and a bool field, so the program also
/// proves the f64+bool fields COEXIST in one validated body.
fn scalar_read_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            "class Reading:\n",
            "    name: str\n",
            // FLOAT VALUE-RANGE refinement (the Phase-3a float-refinement
            // shape), READ at runtime by `/ratio`.
            "    ratio: f64 where 0.0 <= self and self <= 1.0\n",
            // BOOL field, READ at runtime by `/active`.
            "    active: bool\n",
            "\n",
            // The f64 read under test: response DEPENDS on `body.ratio`.
            "fn check_ratio(req: pit.Request, body: Reading) -> pit.Response:\n",
            "    let r: f64 = body.ratio\n",
            "    if r >= 0.5:\n",
            "        return pit.text_response(200, \"high-ratio\")\n",
            "    return pit.text_response(200, \"low-ratio\")\n",
            "\n",
            // The bool read under test: response DEPENDS on `body.active`.
            // `if a:` requires `a: bool` (CLAUDE.md §2.2 — no implicit
            // truthiness), so this also pins that the bool read lands in a
            // real `Bool` local usable as a condition.
            "fn check_active(req: pit.Request, body: Reading) -> pit.Response:\n",
            "    let a: bool = body.active\n",
            "    if a:\n",
            "        return pit.text_response(200, \"active\")\n",
            "    return pit.text_response(200, \"inactive\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/ratio\", check_ratio)\n",
            "    let _ = app.route_validated(\"POST\", \"/active\", check_active)\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

#[test]
fn test_e2e_body_field_read_f64_and_bool_branches() {
    let port = pick_free_port();
    let source = scalar_read_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit f64/bool field-read server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- f64 read, branch HIGH: ratio=0.7 (>= 0.5) -> "high-ratio". ---
    let hi = client
        .post(format!("{base}/ratio"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":0.7,"active":true}"#)
        .send()
        .expect("POST /ratio ratio=0.7");
    assert_eq!(hi.status().as_u16(), 200, "ratio=0.7 must be 200");
    assert_eq!(
        hi.text().unwrap(),
        "high-ratio",
        "ratio=0.7 (>= 0.5) must branch to \"high-ratio\" — a real `body.ratio` f64 read"
    );

    // --- f64 read, branch LOW: ratio=0.2 (< 0.5) -> "low-ratio". THE PROOF:
    // same route, only the value differs — a stub-load cannot flip here. ---
    let lo = client
        .post(format!("{base}/ratio"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":0.2,"active":true}"#)
        .send()
        .expect("POST /ratio ratio=0.2");
    assert_eq!(lo.status().as_u16(), 200, "ratio=0.2 must be 200");
    assert_eq!(
        lo.text().unwrap(),
        "low-ratio",
        "ratio=0.2 (< 0.5) must branch to \"low-ratio\" — this PROVES `body.ratio` is \
         read at RUNTIME via `__cobrust_pit_body_get_f64` (the branch flips with the \
         input). A stub-load Field(0) makes the branch CONSTANT."
    );

    // --- bool read, branch TRUE: active=true -> "active". ---
    let on = client
        .post(format!("{base}/active"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":0.5,"active":true}"#)
        .send()
        .expect("POST /active active=true");
    assert_eq!(on.status().as_u16(), 200, "active=true must be 200");
    assert_eq!(
        on.text().unwrap(),
        "active",
        "active=true must branch to \"active\" — a real `body.active` bool read"
    );

    // --- bool read, branch FALSE: active=false -> "inactive". THE PROOF:
    // the bool branch flips with the value. ---
    let off = client
        .post(format!("{base}/active"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"a","ratio":0.5,"active":false}"#)
        .send()
        .expect("POST /active active=false");
    assert_eq!(off.status().as_u16(), 200, "active=false must be 200");
    assert_eq!(
        off.text().unwrap(),
        "inactive",
        "active=false must branch to \"inactive\" — this PROVES `body.active` is read at \
         RUNTIME via `__cobrust_pit_body_get_bool` (the i1 ABI; the branch flips with the \
         input value)."
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// (4) ADR-0081 Phase-2 (nested) — `body.inner.x` recursive READ, live.
//
// A body field whose type is ANOTHER field-tracked validated class. The
// nested-OBJECT VALIDATION already shipped (ADR-0080 Phase-4(b),
// `pit_nested_body_e2e.rs`); Phase-2 adds the RECURSIVE READ:
//   * `body.inner` reads the BORROWED interior nested object via
//     `__cobrust_pit_body_get_nested`, its result temp re-marked
//     `validated_body_of = Some(Inner)`;
//   * `.x` on THAT recurses into `__cobrust_pit_body_get_i64`.
// Tested at TWO depths (one level: `body.inner.x`; three levels:
// `body.mid.leaf.v`) so the recursion's generality is pinned, and across a
// nested SCALAR-TYPE mix (i64 + bool leaf fields).
// =====================================================================

/// One server. `/one` reads the one-level-deep i64 `body.inner.x`; `/deep`
/// reads the three-level-deep i64 `body.mid.leaf.v`; `/deepflag` reads the
/// three-level-deep bool `body.mid.leaf.flag` — each BRANCHING on the read.
fn nested_read_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            // Leaf carries an i64 (range-refined) + a bool — both READ at
            // depth 3.
            "class Leaf:\n",
            "    v: i64 where 0 <= self and self <= 100\n",
            "    flag: bool\n",
            "\n",
            "class Mid:\n",
            "    leaf: Leaf\n",
            "\n",
            // Inner is the one-level-deep nested body (separate from the
            // 3-level Mid/Leaf chain so both depths live in one program).
            "class Inner:\n",
            "    x: i64 where 0 <= self and self <= 100\n",
            "\n",
            "class Root:\n",
            "    name: str\n",
            "    inner: Inner\n",
            "    mid: Mid\n",
            "\n",
            // One level: body.inner.x
            "fn one(req: pit.Request, body: Root) -> pit.Response:\n",
            "    let v: i64 = body.inner.x\n",
            "    if v >= 50:\n",
            "        return pit.text_response(200, \"one-high\")\n",
            "    return pit.text_response(200, \"one-low\")\n",
            "\n",
            // Three levels: body.mid.leaf.v
            "fn deep(req: pit.Request, body: Root) -> pit.Response:\n",
            "    let v: i64 = body.mid.leaf.v\n",
            "    if v >= 50:\n",
            "        return pit.text_response(200, \"deep-high\")\n",
            "    return pit.text_response(200, \"deep-low\")\n",
            "\n",
            // Three levels, nested BOOL: body.mid.leaf.flag
            "fn deepflag(req: pit.Request, body: Root) -> pit.Response:\n",
            "    let f: bool = body.mid.leaf.flag\n",
            "    if f:\n",
            "        return pit.text_response(200, \"flag-on\")\n",
            "    return pit.text_response(200, \"flag-off\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/one\", one)\n",
            "    let _ = app.route_validated(\"POST\", \"/deep\", deep)\n",
            "    let _ = app.route_validated(\"POST\", \"/deepflag\", deepflag)\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

#[test]
fn test_e2e_body_field_read_nested_recurses() {
    let port = pick_free_port();
    let source = nested_read_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit nested field-read server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // Two full bodies differing only in the nested values, so each route's
    // branch must flip between them.
    let hi_body = r#"{"name":"a","inner":{"x":70},"mid":{"leaf":{"v":80,"flag":true}}}"#;
    let lo_body = r#"{"name":"a","inner":{"x":10},"mid":{"leaf":{"v":20,"flag":false}}}"#;

    // --- one level: body.inner.x. x=70 -> "one-high", x=10 -> "one-low". ---
    let r1 = client
        .post(format!("{base}/one"))
        .header("Content-Type", "application/json")
        .body(hi_body)
        .send()
        .expect("POST /one x=70");
    assert_eq!(r1.status().as_u16(), 200, "nested x=70 must be 200");
    assert_eq!(
        r1.text().unwrap(),
        "one-high",
        "body.inner.x=70 (>= 50) must branch to \"one-high\" — a real ONE-level nested read"
    );
    let r2 = client
        .post(format!("{base}/one"))
        .header("Content-Type", "application/json")
        .body(lo_body)
        .send()
        .expect("POST /one x=10");
    assert_eq!(
        r2.text().unwrap(),
        "one-low",
        "body.inner.x=10 (< 50) must branch to \"one-low\" — this PROVES `body.inner.x` \
         recurses at RUNTIME (the nested accessor returns the borrowed inner object, then \
         `.x` reads it; the branch flips with the nested value)."
    );

    // --- three levels: body.mid.leaf.v. v=80 -> "deep-high", v=20 -> "deep-low". ---
    let r3 = client
        .post(format!("{base}/deep"))
        .header("Content-Type", "application/json")
        .body(hi_body)
        .send()
        .expect("POST /deep v=80");
    assert_eq!(
        r3.text().unwrap(),
        "deep-high",
        "body.mid.leaf.v=80 (>= 50) must branch to \"deep-high\" — a real THREE-level read"
    );
    let r4 = client
        .post(format!("{base}/deep"))
        .header("Content-Type", "application/json")
        .body(lo_body)
        .send()
        .expect("POST /deep v=20");
    assert_eq!(
        r4.text().unwrap(),
        "deep-low",
        "body.mid.leaf.v=20 (< 50) must branch to \"deep-low\" — PROVES the read recurses \
         to depth 3 (two nested-object borrows, then the terminal i64 read)."
    );

    // --- three levels, nested bool: body.mid.leaf.flag. ---
    let r5 = client
        .post(format!("{base}/deepflag"))
        .header("Content-Type", "application/json")
        .body(hi_body)
        .send()
        .expect("POST /deepflag flag=true");
    assert_eq!(
        r5.text().unwrap(),
        "flag-on",
        "body.mid.leaf.flag=true must branch to \"flag-on\" — a real nested BOOL read at depth 3"
    );
    let r6 = client
        .post(format!("{base}/deepflag"))
        .header("Content-Type", "application/json")
        .body(lo_body)
        .send()
        .expect("POST /deepflag flag=false");
    assert_eq!(
        r6.text().unwrap(),
        "flag-off",
        "body.mid.leaf.flag=false must branch to \"flag-off\" — PROVES the nested bool read \
         flips at depth 3 (the recursion delivers a real `bool` to the `if` condition)."
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// (5) ADR-0081 Phase-2 — the no-UB CODEGEN-PROPERTY tripwire EXTENDED to
// the f64 / bool / nested accessors (the gold-standard gate-kind guard, the
// sibling of (2b) above). A NON-registered `fn helper(b: <Body>): return
// b.<field>` (or `b.inner.x`) on a `.cb`-constructed instance must NOT emit
// a call to ANY `__cobrust_pit_body_get_*` shim — the serde accessor is
// REGISTRATION-gated (`validated_body_of == Some`), not type-gated. Under
// the shipped gate the read falls to the `Field(0)` stub (no external call);
// a type-only-gate regression would emit `bl __cobrust_pit_body_get_{f64,
// bool,nested}` and this goes RED.
//
// One `.o` per field-type so the ONLY possible accessor call site in each
// program is the single non-registered helper read. (Separate single-helper
// programs sidestep the move-checker rejecting one `.cb` instance consumed
// by two helper calls — a borrow-check artefact unrelated to the gate.)
// =====================================================================

/// A non-registered f64 read. `helper(b: Reading): return b.ratio`.
const NO_UB_F64_PROGRAM: &str = concat!(
    "import pit\n",
    "\n",
    "class Reading:\n",
    "    name: str\n",
    "    ratio: f64 where 0.0 <= self and self <= 1.0\n",
    "\n",
    "fn helper(b: Reading) -> f64:\n",
    "    return b.ratio\n",
    "\n",
    "fn main() -> i64:\n",
    "    let s = Reading()\n",
    "    let r: f64 = helper(s)\n",
    "    print(r)\n",
    "    return 0\n",
);

/// A non-registered bool read. `helper(b: Reading): return b.active`.
const NO_UB_BOOL_PROGRAM: &str = concat!(
    "import pit\n",
    "\n",
    "class Reading:\n",
    "    name: str\n",
    "    active: bool\n",
    "\n",
    "fn helper(b: Reading) -> bool:\n",
    "    return b.active\n",
    "\n",
    "fn main() -> i64:\n",
    "    let s = Reading()\n",
    "    let a: bool = helper(s)\n",
    "    return 0\n",
);

/// A non-registered NESTED read. `helper(b: Outer): return b.inner.x`. The
/// nested accessor is registration-gated through the WHOLE chain: the
/// recursive base resolver only succeeds when the chain bottoms out at a
/// marked param, so `b.inner.x` on an unmarked `b` emits NEITHER
/// `__cobrust_pit_body_get_nested` NOR `__cobrust_pit_body_get_i64`.
const NO_UB_NESTED_PROGRAM: &str = concat!(
    "import pit\n",
    "\n",
    "class Inner:\n",
    "    x: i64 where 0 <= self and self <= 100\n",
    "\n",
    "class Outer:\n",
    "    name: str\n",
    "    inner: Inner\n",
    "\n",
    "fn helper(b: Outer) -> i64:\n",
    "    return b.inner.x\n",
    "\n",
    "fn main() -> i64:\n",
    "    let s = Outer()\n",
    "    let v: i64 = helper(s)\n",
    "    print(v)\n",
    "    return 0\n",
);

/// Shared body for the Phase-2 no-UB codegen-property tripwires: compile the
/// given non-registered-helper program to a `.o` and assert no
/// `__cobrust_pit_body_get_*` symbol is referenced. SKIPs cleanly when no
/// `nm`/`llvm-nm` is available (mirrors `(2b)`).
fn assert_no_accessor_symbol(program: &str, label: &str) {
    let Some(nm) = find_nm() else {
        eprintln!(
            "SKIP {label}: no runnable `nm`/`llvm-nm` found (set $NM or \
             $LLVM_SYS_181_PREFIX). The Phase-2 registration-gate codegen property is \
             unverified on this host."
        );
        return;
    };
    let (_dir, obj) = compile_object(program);
    let nm_out = Command::new(&nm)
        .arg(&obj)
        .output()
        .expect("run nm on the Phase-2 no-UB object");
    assert!(
        nm_out.status.success(),
        "`{nm}` failed on {}: status={:?}, stderr={:?}",
        obj.display(),
        nm_out.status,
        String::from_utf8_lossy(&nm_out.stderr),
    );
    let symbols = String::from_utf8_lossy(&nm_out.stdout);
    let offending: Vec<&str> = symbols
        .lines()
        .filter(|l| l.contains("cobrust_pit_body_get_"))
        .collect();
    assert!(
        offending.is_empty(),
        "REGRESSION ({label}, type-only gate / serde-cast UB): a non-registered helper \
         emitted a call to a `__cobrust_pit_body_get_*` accessor shim. The Phase-2 \
         accessors (f64/bool/nested) are registration-gated EXACTLY like i64/str \
         (ADR-0081 §5.2 Q4); a `.cb`-constructed / non-registered body must take the \
         `Field(0)` stub. Offending `nm` symbol line(s):\n{}\nfull `{nm}` output:\n{symbols}",
        offending.join("\n"),
    );
}

#[test]
fn test_no_ub_non_registered_f64_read_does_not_emit_accessor_call() {
    assert_no_accessor_symbol(
        NO_UB_F64_PROGRAM,
        "test_no_ub_non_registered_f64_read_does_not_emit_accessor_call",
    );
}

#[test]
fn test_no_ub_non_registered_bool_read_does_not_emit_accessor_call() {
    assert_no_accessor_symbol(
        NO_UB_BOOL_PROGRAM,
        "test_no_ub_non_registered_bool_read_does_not_emit_accessor_call",
    );
}

#[test]
fn test_no_ub_non_registered_nested_read_does_not_emit_accessor_call() {
    assert_no_accessor_symbol(
        NO_UB_NESTED_PROGRAM,
        "test_no_ub_non_registered_nested_read_does_not_emit_accessor_call",
    );
}

// =====================================================================
// (6) ADR-0081 Phase-3 — `body.<list[T]-field>` READ + ITERATE, live.
//
// A body field whose declared `Ty` is `list[T]` (T ∈ str/i64/f64/bool). The
// element-type VALIDATION already shipped (ADR-0080 Phase-4(c),
// `pit_collection_body_e2e.rs`); Phase-3 adds the READ: the accessor
// (`__cobrust_pit_body_get_list_{str,i64,f64,bool}`) BORROWS the parent body
// box, reads the JSON array, and MINTS a fresh `.cb` `list[T]` from it (the
// redis-`lrange` recipe). The handler then READS + ITERATES the minted list
// (`xs.len()`, `for s in body.tags:`), so the RESPONSE reflects the REAL
// element values — a stub-load (empty/garbage list) cannot produce the
// element-dependent responses below.
//
// Same "real read = the response tracks the input" shape as (1):
//   * list[str] `/join`  — concatenates the tag strings; ["a","b","c"] ->
//     "abc", ["foo","bar"] -> "foobar" (each STRING element genuinely read).
//   * list[str] `/count` — `body.tags.len()`; >=3 -> "many", else "few".
//   * list[i64] `/sum`   — sums the score ints; [60,50] (110) -> "big",
//     [3,4] (7) -> "small", [] (0) -> "small" (each INT element read).
// =====================================================================

/// One server. `/join` + `/count` read & iterate the list[str] `body.tags`;
/// `/sum` reads & iterates the list[i64] `body.scores`. The accumulation is
/// the canonical Cobrust `let acc = ...` outside + `acc = acc + x` rebind
/// inside the loop (no `mut` keyword — `builtins_abs_range_e2e.rs` idiom).
fn list_read_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            "class TagBody:\n",
            "    tags: list[str]\n",
            "    scores: list[i64]\n",
            "\n",
            // list[str] ITERATE: concatenate the real tag strings. A
            // stub-load (empty minted list) yields "" regardless of input.
            "fn join_tags(req: pit.Request, body: TagBody) -> pit.Response:\n",
            "    let xs: list[str] = body.tags\n",
            "    let acc: str = \"\"\n",
            "    for s in xs:\n",
            "        acc = acc + s\n",
            "    return pit.text_response(200, acc)\n",
            "\n",
            // list[str] LEN: branch on the real element count.
            "fn count_tags(req: pit.Request, body: TagBody) -> pit.Response:\n",
            "    let xs: list[str] = body.tags\n",
            "    let n: i64 = xs.len()\n",
            "    if n >= 3:\n",
            "        return pit.text_response(200, \"many\")\n",
            "    return pit.text_response(200, \"few\")\n",
            "\n",
            // list[i64] ITERATE: sum the real score ints + branch on the sum.
            "fn sum_scores(req: pit.Request, body: TagBody) -> pit.Response:\n",
            "    let xs: list[i64] = body.scores\n",
            "    let total: i64 = 0\n",
            "    for v in xs:\n",
            "        total = total + v\n",
            "    if total >= 100:\n",
            "        return pit.text_response(200, \"big\")\n",
            "    return pit.text_response(200, \"small\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/join\", join_tags)\n",
            "    let _ = app.route_validated(\"POST\", \"/count\", count_tags)\n",
            "    let _ = app.route_validated(\"POST\", \"/sum\", sum_scores)\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

#[test]
fn test_e2e_body_field_read_list_str_and_i64_iterates() {
    let port = pick_free_port();
    let source = list_read_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit list field-read server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // --- list[str] ITERATE: tags=["a","b","c"] -> "abc". THE PROOF: the
    // response is the CONCATENATION of the real array elements; a stub-load
    // (empty minted list) would return "". ---
    let j1 = client
        .post(format!("{base}/join"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["a","b","c"],"scores":[1]}"#)
        .send()
        .expect("POST /join tags=[a,b,c]");
    assert_eq!(j1.status().as_u16(), 200, "list[str] join must be 200");
    assert_eq!(
        j1.text().unwrap(),
        "abc",
        "body.tags=[\"a\",\"b\",\"c\"] iterated + concatenated must be \"abc\" — PROVES the \
         list[str] field is READ and ITERATED at runtime (a fresh `.cb` list minted from the \
         JSON array via `__cobrust_pit_body_get_list_str`, then `for s in xs:`). A stub-load \
         empty list would return \"\"."
    );

    // --- list[str] ITERATE, DIFFERENT values: ["foo","bar"] -> "foobar".
    // The SAME route, only the array differs — the response tracks the real
    // elements (a constant/stub cannot produce both "abc" and "foobar"). ---
    let j2 = client
        .post(format!("{base}/join"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["foo","bar"],"scores":[1]}"#)
        .send()
        .expect("POST /join tags=[foo,bar]");
    assert_eq!(
        j2.text().unwrap(),
        "foobar",
        "body.tags=[\"foo\",\"bar\"] -> \"foobar\" — the iteration reads the ACTUAL element \
         strings (the response flips with the array contents)."
    );

    // --- list[str] LEN: len 3 -> "many", len 1 -> "few". The count tracks
    // the real array length. ---
    let c3 = client
        .post(format!("{base}/count"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["a","b","c"],"scores":[1]}"#)
        .send()
        .expect("POST /count len=3");
    assert_eq!(
        c3.text().unwrap(),
        "many",
        "body.tags.len()==3 (>=3) -> \"many\" — the minted list's length is the REAL array length"
    );
    let c1 = client
        .post(format!("{base}/count"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["x"],"scores":[1]}"#)
        .send()
        .expect("POST /count len=1");
    assert_eq!(
        c1.text().unwrap(),
        "few",
        "body.tags.len()==1 (<3) -> \"few\" — PROVES `.len()` reads the real minted-list length \
         (the branch flips with the array size)."
    );

    // --- list[i64] ITERATE: scores=[60,50] (sum 110) -> "big". Each INT
    // element is read and summed. ---
    let s_big = client
        .post(format!("{base}/sum"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["x"],"scores":[60,50]}"#)
        .send()
        .expect("POST /sum [60,50]");
    assert_eq!(
        s_big.text().unwrap(),
        "big",
        "body.scores=[60,50] summed (110 >= 100) -> \"big\" — PROVES the list[i64] field is read \
         + iterated (each int element genuinely summed via `__cobrust_pit_body_get_list_i64`)."
    );

    // --- list[i64] ITERATE, smaller: [3,4] (sum 7) -> "small". The branch
    // flips with the real element values. ---
    let s_small = client
        .post(format!("{base}/sum"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["x"],"scores":[3,4]}"#)
        .send()
        .expect("POST /sum [3,4]");
    assert_eq!(
        s_small.text().unwrap(),
        "small",
        "body.scores=[3,4] summed (7 < 100) -> \"small\" — the sum tracks the ACTUAL int elements."
    );

    // --- list[i64] EMPTY: [] (sum 0) -> "small". An empty JSON array mints a
    // valid empty list (the fail-clean shape), iterates zero times. ---
    let s_empty = client
        .post(format!("{base}/sum"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["x"],"scores":[]}"#)
        .send()
        .expect("POST /sum []");
    assert_eq!(
        s_empty.text().unwrap(),
        "small",
        "body.scores=[] (empty array) -> sum 0 -> \"small\" — an EMPTY array mints a valid empty \
         `.cb` list (len 0), iterates zero times (no panic, no stub garbage)."
    );

    // --- type-mismatched array still 422 (unchanged from ADR-0080 Phase-4(c)):
    // the read work adds reads ON TOP, touches neither validator nor 422 path.
    // A number in a list[str] is rejected BEFORE the handler. ---
    let bad = client
        .post(format!("{base}/join"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["a",42],"scores":[1]}"#)
        .send()
        .expect("POST /join tags=[a,42]");
    assert_eq!(
        bad.status().as_u16(),
        422,
        "a number in a list[str] (`[\"a\",42]`) must still be 422, handler NOT entered \
         (ADR-0080 Phase-4(c) element validation — the read accessor is a pure typed read of an \
         ALREADY-validated array, §2.2). got {}",
        bad.status().as_u16(),
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// (7) ADR-0081 Phase-3 — the list[str] DROP-DISCIPLINE hammer-loop.
//
// Each `body.tags` read MINTS a fresh `.cb` `list[str]` (one `Str` buffer per
// element + the container) that the handler scope drops EXACTLY ONCE (the
// `Ty::List(Str)` schedule → `__cobrust_list_drop_elems(list,
// __cobrust_str_drop)` frees each element `Str` then the container). A leak
// would balloon RSS; a double-free would crash the server (mimalloc free-list
// corruption, the ADR-0050c Phase-4 failure mode). We hammer the route 200×
// and assert every read returns the correct iterated value AND the server is
// still alive + serving after the loop — the server-survival proof of
// drop-once for the minted list + its element strings.
// =====================================================================

#[test]
fn test_e2e_body_field_read_list_str_drops_once_under_hammer() {
    let port = pick_free_port();
    let source = list_read_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit list hammer server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // 200 reads of a 3-element list[str] (mint + iterate + drop each time).
    // Every response must be the correct concatenation; a drop bug surfaces
    // either as a wrong body or (more likely) a server crash mid-loop (the
    // next request then fails to connect).
    for i in 0..200 {
        let resp = client
            .post(format!("{base}/join"))
            .header("Content-Type", "application/json")
            .body(r#"{"tags":["alpha","beta","gamma"],"scores":[1]}"#)
            .send()
            .unwrap_or_else(|e| {
                panic!(
                    "POST /join iteration {i} failed to connect — the server likely CRASHED \
                     (a double-free of the minted list[str] / its element Str buffers): {e}"
                )
            });
        assert_eq!(
            resp.status().as_u16(),
            200,
            "list[str] hammer iteration {i}: status must stay 200 (server alive)"
        );
        assert_eq!(
            resp.text().unwrap(),
            "alphabetagamma",
            "list[str] hammer iteration {i}: every read must mint + iterate a FRESH correct list \
             (no cross-request corruption from a mis-scheduled drop)"
        );
    }

    // The server is still alive + serving correctly AFTER 200 mint/drop
    // cycles — the server-survival proof that the minted list[str] (container
    // + element Str buffers) drops EXACTLY ONCE per read (no leak, no
    // double-free).
    let after = client
        .post(format!("{base}/join"))
        .header("Content-Type", "application/json")
        .body(r#"{"tags":["x","y"],"scores":[1]}"#)
        .send()
        .expect("server must still serve after the 200-read hammer (drop-once held)");
    assert_eq!(
        after.text().unwrap(),
        "xy",
        "after 200 mint/drop cycles the server still mints a correct list[str] — drop-once held \
         across the whole loop (no accumulated leak, no double-free)"
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// (8) ADR-0081 Phase-3 — the list-field no-UB CODEGEN-PROPERTY tripwire (the
// gold-standard gate-kind guard, the sibling of (2b)/(5)). A NON-registered
// `fn helper(b: <Body>): return b.<list-field>.len()` on a `.cb`-constructed
// instance must NOT emit a call to ANY `__cobrust_pit_body_get_*` shim — the
// list accessors are REGISTRATION-gated EXACTLY like the scalar/nested ones
// (`validated_body_of == Some`), not type-gated. Under the shipped gate the
// read falls to the `Field(0)` stub (no external call); a type-only-gate
// regression would emit `bl __cobrust_pit_body_get_list_str` and this goes
// RED. (Reuses `assert_no_accessor_symbol` from (5).)
// =====================================================================

/// A non-registered list[str] read. `helper(b: TagBody): return b.tags.len()`.
const NO_UB_LIST_PROGRAM: &str = concat!(
    "import pit\n",
    "\n",
    "class TagBody:\n",
    "    tags: list[str]\n",
    "\n",
    "fn helper(b: TagBody) -> i64:\n",
    "    let xs: list[str] = b.tags\n",
    "    return xs.len()\n",
    "\n",
    "fn main() -> i64:\n",
    // .cb-constructed instance: a null/opaque pointer, NOT a boxed
    // serde_json::Value -> b.tags must NOT serde-cast it.
    "    let s = TagBody()\n",
    "    let v: i64 = helper(s)\n",
    "    print(v)\n",
    "    return 0\n",
);

#[test]
fn test_no_ub_non_registered_list_read_does_not_emit_accessor_call() {
    assert_no_accessor_symbol(
        NO_UB_LIST_PROGRAM,
        "test_no_ub_non_registered_list_read_does_not_emit_accessor_call",
    );
}

/// Runtime-survival companion for the list no-UB negative (the coarse "does
/// not crash" smoke, the sibling of (2)): a non-registered `helper(b):
/// b.tags.len()` on a `.cb`-constructed instance builds + runs to a CLEAN
/// exit. (The gate-KIND discrimination is the disassembly tripwire above;
/// this only proves the non-registered list-read path does not abort/UB.)
#[test]
fn test_no_ub_non_registered_list_read_runs_clean() {
    let (_dir, exe) = compile_source(NO_UB_LIST_PROGRAM);
    let out = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run the list no-UB negative exe");
    assert!(
        out.status.success(),
        "the list no-UB negative MUST run to a CLEAN exit — a non-registered \
         `fn helper(b: TagBody): return b.tags.len()` reading a `.cb`-constructed `TagBody()` \
         MUST NOT crash/abort (the `Field(0)` stub path, not a serde cast on a null/opaque ptr). \
         exit={:?}, stdout={:?}, stderr={:?}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

// =====================================================================
// (9) ADR-0081 Phase-3 — body-as-fn-arg, the DELIVERED half: passing a READ
// FIELD VALUE (an `i64` / a `list[str]`) to another fn.
//
// Once `body.field` is READ into a `.cb` local, that local is an ordinary
// value — passing it to a fn is plain argument-passing, NO body-machinery
// involved. This is the trivial-and-must-work case the work order names:
//   * `double(body.rank)`        — an i64 read passed to a fn.
//   * `first_or_empty(body.tags)`— a list[str] read passed to a fn that
//     iterates it.
// The response tracks the real read values through the call, so the reads
// genuinely happened before the hand-off.
// =====================================================================

fn body_arg_fieldval_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            "class TagBody:\n",
            "    name: str\n",
            "    tags: list[str]\n",
            "    rank: i64 where 0 <= self and self <= 100\n",
            "\n",
            "fn double(n: i64) -> i64:\n",
            "    return n + n\n",
            "\n",
            // Takes a list[str] VALUE arg (the read field), iterates it.
            "fn first_or_empty(xs: list[str]) -> str:\n",
            "    for s in xs:\n",
            "        return s\n",
            "    return \"empty\"\n",
            "\n",
            "fn handle(req: pit.Request, body: TagBody) -> pit.Response:\n",
            "    let r: i64 = body.rank\n",
            "    let doubled: i64 = double(r)\n",
            "    let tags: list[str] = body.tags\n",
            "    let first: str = first_or_empty(tags)\n",
            "    if doubled >= 100:\n",
            "        return pit.text_response(200, first)\n",
            "    return pit.text_response(200, \"low\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/h\", handle)\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

#[test]
fn test_e2e_body_arg_read_field_value_to_fn() {
    let port = pick_free_port();
    let source = body_arg_fieldval_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit body-arg field-value server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // rank=60 -> double(60)=120 (>=100) -> return first_or_empty(tags) =
    // "zoo". PROVES both: the i64 read flows through `double`, AND the
    // list[str] read flows through `first_or_empty` (which iterates it).
    let r1 = client
        .post(format!("{base}/h"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"n","tags":["zoo","bar"],"rank":60}"#)
        .send()
        .expect("POST /h rank=60");
    assert_eq!(r1.status().as_u16(), 200, "rank=60 must be 200");
    assert_eq!(
        r1.text().unwrap(),
        "zoo",
        "double(body.rank=60)=120>=100 -> first_or_empty(body.tags)=\"zoo\" — PROVES a READ FIELD \
         VALUE (both the i64 `rank` and the list[str] `tags`) passes correctly to another fn \
         (ordinary value-arg passing; the reads happened before the hand-off)."
    );

    // rank=10 -> double=20 (<100) -> "low". The i64 read flips the branch
    // through the `double` call.
    let r2 = client
        .post(format!("{base}/h"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"n","tags":["zoo"],"rank":10}"#)
        .send()
        .expect("POST /h rank=10");
    assert_eq!(
        r2.text().unwrap(),
        "low",
        "double(body.rank=10)=20<100 -> \"low\" — the i64 read value flips the branch THROUGH the \
         `double` call (a stub-load constant could not flip here)."
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

// =====================================================================
// (10) ADR-0081 Phase-3 — body-as-fn-arg, the DEFERRED half: passing the
// WHOLE validated `body` to another fn.
//
// HONEST DEFERRAL (F37 — an `#[ignore]` with a specific reason, NOT a failing
// un-ignored test). Passing the WHOLE body to `read_rank(body)` COMPILES and
// RUNS CLEAN, but the `b.rank` read INSIDE the callee hits the `Field(0)`
// stub — because the `validated_body_of` mark is set ONLY on the registered
// handler's body param (`lower.rs` `lower_fn`), and it does NOT propagate
// across the `read_rank(body)` call boundary. So the callee's `b` is unmarked
// and its `b.rank` is the deferred no-field-storage stub (a WRONG value), NOT
// a serde cast (NOT UB — the registration gate still holds: see the no-UB
// proof in the sibling test below).
//
// EMPIRICALLY (this agent, build at this HEAD): rank=70 AND rank=10 BOTH
// returned "high" — the branch did NOT flip, confirming `read_rank`'s
// `b.rank` is the stub, not the real read. To DELIVER this would require
// propagating `validated_body_of` (or the boxed Value itself) through the
// call's arg → the callee's param local — a deep inter-procedural change
// (the body box is MOVED into the call; the callee would need to know its
// param IS a validated-body view). That is a Phase-4+ inter-procedural
// concern (it interacts with the §7 native-struct ABI — once a body is a real
// struct, passing it to a fn is ordinary), DEFERRED here.
//
// This test is `#[ignore]`d (it would FAIL as a flip-asserting test today);
// the SEPARATE always-on `test_no_ub_whole_body_arg_emits_no_accessor` below
// PROVES the no-UB invariant holds for this deferred shape.
// =====================================================================

fn whole_body_arg_program(port: u16) -> String {
    format!(
        concat!(
            "import pit\n",
            "\n",
            "class TagBody:\n",
            "    name: str\n",
            "    rank: i64 where 0 <= self and self <= 100\n",
            "\n",
            // Reads `b.rank` on a param that is the WHOLE forwarded body.
            "fn read_rank(b: TagBody) -> i64:\n",
            "    return b.rank\n",
            "\n",
            "fn handle(req: pit.Request, body: TagBody) -> pit.Response:\n",
            "    let r: i64 = read_rank(body)\n",
            "    if r >= 50:\n",
            "        return pit.text_response(200, \"high\")\n",
            "    return pit.text_response(200, \"low\")\n",
            "\n",
            "fn main() -> i64:\n",
            "    let app = pit.App()\n",
            "    let _ = app.route_validated(\"POST\", \"/h\", handle)\n",
            "    let _exit = app.run(\"127.0.0.1\", {port})\n",
            "    return 0\n",
        ),
        port = port,
    )
}

#[ignore = "ADR-0081 Phase-3 DEFERRED: passing the WHOLE validated body to another fn \
            does not propagate `validated_body_of` across the call boundary, so the callee's \
            `b.rank` is the Field(0) stub (a wrong value, NOT UB — the registration gate holds, \
            see test_no_ub_whole_body_arg_emits_no_accessor). Delivering it needs deep \
            inter-procedural propagation (or the §7 native-struct ABI); deferred to Phase-4+."]
#[test]
fn test_e2e_body_arg_whole_body_to_fn_deferred() {
    let port = pick_free_port();
    let source = whole_body_arg_program(port);
    let (_dir, exe) = compile_source(&source);

    let child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = ChildGuard(child);

    wait_for_port(port, Duration::from_secs(8)).expect("pit whole-body-arg server bind");

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();

    // THE GAP (would FAIL today, hence #[ignore]): the branch should flip with
    // the real rank read through `read_rank(body)`, but `read_rank`'s `b.rank`
    // is the Field(0) stub, so rank=70 and rank=10 both return the same arm.
    let hi = client
        .post(format!("{base}/h"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"n","rank":70}"#)
        .send()
        .expect("POST /h rank=70");
    assert_eq!(
        hi.text().unwrap(),
        "high",
        "rank=70 (>=50) should branch \"high\" via read_rank(body).rank"
    );
    let lo = client
        .post(format!("{base}/h"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"n","rank":10}"#)
        .send()
        .expect("POST /h rank=10");
    assert_eq!(
        lo.text().unwrap(),
        "low",
        "rank=10 (<50) should branch \"low\" — THIS is the deferred gap: read_rank's `b.rank` is \
         the Field(0) stub (the mark does not cross the call boundary), so the branch does NOT \
         flip and this fails. Deferred to Phase-4+ inter-procedural propagation."
    );

    drop(guard.0.kill());
    let _ = guard.0.wait();
}

/// The ALWAYS-ON no-UB proof for the DEFERRED whole-body-as-arg shape: even
/// though `read_rank`'s `b.rank` is the (wrong-value) stub, it must NEVER be a
/// serde cast on the moved body pointer — the registration gate holds across
/// the call boundary. The codegen-property assertion (sibling of (8)): the
/// whole-body-forwarding program emits NO `__cobrust_pit_body_get_*` accessor
/// symbol (neither `handle` — which only MOVES the body into the call, no
/// field read — nor the unmarked callee `read_rank`). GREEN under the shipped
/// registration gate; the wrong VALUE is a deferred-feature gap, but the
/// no-UB invariant is UNCONDITIONAL and pinned here.
#[test]
fn test_no_ub_whole_body_arg_emits_no_accessor() {
    // Reuse the deferred program's source (port is irrelevant for an object
    // compile — no server is spawned).
    let program = whole_body_arg_program(0);
    let Some(nm) = find_nm() else {
        eprintln!(
            "SKIP test_no_ub_whole_body_arg_emits_no_accessor: no runnable `nm`/`llvm-nm` \
             found. The whole-body-as-arg registration-gate codegen property is unverified \
             on this host."
        );
        return;
    };
    let (_dir, obj) = compile_object(&program);
    let nm_out = Command::new(&nm)
        .arg(&obj)
        .output()
        .expect("run nm on the whole-body-arg object");
    assert!(
        nm_out.status.success(),
        "`{nm}` failed on {}: status={:?}, stderr={:?}",
        obj.display(),
        nm_out.status,
        String::from_utf8_lossy(&nm_out.stderr),
    );
    let symbols = String::from_utf8_lossy(&nm_out.stdout);
    let offending: Vec<&str> = symbols
        .lines()
        .filter(|l| l.contains("cobrust_pit_body_get_"))
        .collect();
    assert!(
        offending.is_empty(),
        "REGRESSION (no-UB, whole-body-as-arg): forwarding the WHOLE validated body to \
         `read_rank(body)` emitted a `__cobrust_pit_body_get_*` accessor on an UNMARKED callee \
         param (or on the moved body in `handle`). The body read must fire ONLY on a \
         `validated_body_of`-MARKED local (ADR-0081 §5.2 Q4); the mark does NOT cross a call \
         boundary, so the callee's `b.rank` is the Field(0) stub (a deferred-feature wrong \
         value), NEVER a serde cast (UB). Offending `nm` line(s):\n{}\nfull `{nm}` output:\n{symbols}",
        offending.join("\n"),
    );
}
