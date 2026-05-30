//! ADR-0081 Phase-1b — validated-body `body.field` RUNTIME READ, end-to-end.
//!
//! TEST-FIRST (ADSD): this corpus is written RED, BEFORE the impl. At HEAD
//! `984872e` (ADR-0081 Phase-1a `json_response` just landed) the surface it
//! exercises *compiles* but the runtime read is a NO-OP STUB, so the
//! behavioural assertions FAIL:
//!
//!   * a handler that does `let r: i64 = body.rank` and branches on `r`
//!     returns the SAME branch for `rank:50` AND `rank:10` — because
//!     `body.rank` does NOT read the real validated value. The MIR `Attr`
//!     rvalue arm (`lower.rs:1445-1477`) routes a NON-handle base (a user
//!     body class id `< ECO_ADT_BASE`, so `lookup_handle_attr` returns
//!     `None`) into the placeholder `Projection::Field(0)` that DISCARDS
//!     the field name (`let _ = name;`, `lower.rs:1476`); codegen's
//!     `lower_place_load` has no `Projection::Field` arm at all
//!     (`llvm_backend.rs:4435`), so `Field(_)` falls into the bare-local
//!     stub-load `else` (`llvm_backend.rs:4564-4573`). The typed surface is
//!     real (`body.rank` type-checks against `adt_fields`, ADR-0080), but
//!     the runtime read loads the wrong slot.
//!   * `body.name` (str) reads an empty/garbage `Str`, not the validated
//!     `"hello"`.
//!
//! RED EVIDENCE captured at `984872e` (manual probe, recorded in the
//! dispatch report, reproduced by the assertions below):
//!   POST {name:a,rank:50}  -> 200 "high"   (a correct impl would ALSO say "high" — coincidence)
//!   POST {name:a,rank:10}  -> 200 "high"   (WRONG: a correct impl reads rank=10 -> "low")
//!   POST {name:hello,...}  -> 200 ""        (WRONG: a correct impl echoes "hello")
//! i.e. the branch is CONSTANT regardless of `body.rank`, and the str read
//! is empty — proving `body.rank` / `body.name` are not read at runtime.
//!
//! The feature this corpus pins (ADR-0081 §2 Q2/Q4/Q5, §5.2, §6 Phase-1
//! items 2+3):
//!   * 2 typed accessor shims `__cobrust_pit_body_get_i64` /
//!     `__cobrust_pit_body_get_str`, cloned bit-for-bit from the
//!     `(ptr,ptr)->ptr` `path_param` template (`cabi.rs:806`): borrow the
//!     boxed `serde_json::Value` the validator left (`cabi.rs:464`), do a
//!     typed get (`v.get(name).and_then(as_i64)` — NOT `as_f64`-truncate,
//!     footgun #3), `alloc_str_buffer` strings;
//!   * the NEW checker->MIR registration channel — `TypedModule
//!     .validated_handlers: HashMap<DefId, (usize, AdtId)>` populated in
//!     `check_eco_sig` + a NEW `LocalDecl.validated_body_of: Option<AdtId>`
//!     mark set in MIR when lowering a registered handler's body param;
//!   * the REGISTRATION-DRIVEN MIR `Attr` sub-arm (Q4): the serde-accessor
//!     retarget fires ONLY when the base resolves to a local carrying
//!     `validated_body_of == Some(id)` AND the field is in that class's
//!     `adt_fields` — NEVER on `Ty::Adt`-with-a-field-table alone. The
//!     field-name `Str` is COMPILER-SYNTHESISED (footgun #1), passed via
//!     the existing borrowed-receiver `emit_ecosystem_call`
//!     (`lower.rs:1457`), and MIR names a SYMBOL + a `Ty`, never serde / a
//!     JSON key (the §2-Q5 swappable seam).
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
//!   → cobrust-mir (LocalDecl.validated_body_of mark — NEW; registration-gated Attr sub-arm — the RED point)
//!   → cobrust-codegen (body_get externs `(ptr,ptr)->{i64|ptr}`)
//!   → cobrust-pit C-ABI shims `__cobrust_pit_body_get_{i64,str}` (typed serde get over the boxed Value)
//!   → real HTTP socket bound by the compiled .cb binary
//!   → reqwest::blocking client POSTs bodies; the RESPONSE depends on the READ value
//! ```

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

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
