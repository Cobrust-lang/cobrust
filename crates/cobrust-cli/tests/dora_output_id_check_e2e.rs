//! ADR-0092 — end-to-end `.cb` source → compile / check → assert for the
//! dora `event.send_output("<id>", payload)` COMPILE-TIME output-id check
//! (`TypeError::DoraUnknownOutputId`).
//!
//! This LIFTS the dora undeclared-output-id reject from RUNTIME (the
//! `cobrust-dora` `__cobrust_dora_event_send_output` shim's `eprintln!` +
//! `-1` return; ADR-0076 Phase 2) to COMPILE TIME (CLAUDE.md §2.5-A
//! compile-time-catch): a mistyped output id is now a `cobrust
//! check`/`cobrust build` error, not a silent runtime drop. It is the one
//! genuinely-remaining real-path dora compiler increment per ADR-0076c
//! §4.2 / ADR-0076 §6 Phase-2 done-means-2.
//!
//! How declared outputs flow: `@dora.node(outputs=["pose"])` desugars
//! (cobrust-hir `lower.rs`) into one `dora.declare_output("pose")`
//! register-call at `main`'s prologue. A module PRE-PASS in the
//! type-checker (`cobrust-types` `check.rs::collect_dora_declared_outputs`)
//! collects every such string-literal id into the declared-output set on
//! `Ctx`; the `event.send_output(...)` method-synth
//! (`try_synth_ecosystem_call` Case 2) then rejects a string-LITERAL id
//! that is NOT in that set.
//!
//! The FOUR cases (per the §2.5 compile-time-catch contract):
//!   (a) NEGATIVE  — `outputs=["pose"]` + `send_output("twist_typo", _)`
//!       FAILS at check/build with `DoraUnknownOutputId`; stderr names the
//!       offending id, the declared id `pose`, and a nearest-match.
//!   (b) POSITIVE  — `outputs=["pose"]` + `send_output("pose", _)`
//!       type-checks, builds, and runs (exit 0, emits the payload).
//!   (c) NON-LITERAL — `send_output(<str var>, _)` type-checks (the SKIP
//!       path — a computed id cannot be proven statically; the runtime
//!       backstop covers it). Proves NO false-positive.
//!   (d) BARE      — `@dora.node` (NO `outputs=`) + `send_output("x", _)`
//!       type-checks (the None-set SKIP path — nothing declared ⇒ the full
//!       set is unknown ⇒ inert). Proves NO false-positive.
//!
//! Pattern mirrors `decorator_dora_e2e.rs` (compile/check a `.cb`, assert
//! exit + stderr) + the `coil_compare_e2e.rs` `try_check` helper (for the
//! nice §2.5-B FIX-text assertion via `cobrust check`).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable and return its path. The
/// caller spawns + asserts. Panics with the build stderr on failure.
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

/// `cobrust build` compile-only helper — `(success?, stderr)`. The negative
/// case asserts on the BUILD stderr (per the §2.5 build-stderr contract;
/// `build` Debug-formats the type error, so the stderr carries the variant
/// name + the declared list + the nearest-match).
fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `cobrust check` helper — `(success?, stderr)`. Used for the nice §2.5-B
/// FIX-text assertion (the `error_ux` renderer prints the declared-output
/// list + the `did you mean` clause an LLM parses to fix in one step) and
/// for the SKIP-path positives (non-literal / bare → exit 0, no error).
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
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// (a) NEGATIVE — undeclared output id rejected at COMPILE TIME.
// =====================================================================

/// `@dora.node(outputs=["pose"])` whose handler calls
/// `event.send_output("twist_typo", _)` is REJECTED at `cobrust build`
/// with `DoraUnknownOutputId`. The stderr names the offending id
/// (`twist_typo`) and the declared id (`pose`) — proving the §2.5-A
/// compile-time-catch fires (NOT a runtime `-1` drop).
#[test]
fn test_neg_dora_send_output_undeclared_id_rejected_at_build() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"pose\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let _ = event.send_output(\"twist_typo\", \"payload\")\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_build(source);
    assert!(
        !ok,
        "undeclared `send_output` id must be REJECTED at compile time; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("DoraUnknownOutputId"),
        "build stderr must name the `DoraUnknownOutputId` variant; got:\n{stderr}"
    );
    assert!(
        stderr.contains("twist_typo"),
        "build stderr must name the offending id `twist_typo`; got:\n{stderr}"
    );
    assert!(
        stderr.contains("pose"),
        "build stderr must name the declared output `pose` (the §2.5-B FIX); got:\n{stderr}"
    );
}

/// The SAME program via `cobrust check` renders the nice §2.5-B FIX: the
/// declared-output list `[pose]` + a `did you mean` clause. An LLM reading
/// this stderr fixes the call in one step. The nearest-match fires here on
/// a CLOSE typo: `outputs=["pose", "twist"]` + `send_output("twst", _)` →
/// `did you mean `twist`?` (edit distance 1).
#[test]
fn test_neg_dora_send_output_check_prints_fix_with_nearest_match() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"pose\", \"twist\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let _ = event.send_output(\"twst\", \"payload\")\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_check(source);
    assert!(
        !ok,
        "undeclared `send_output` id must fail `cobrust check`; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("unknown dora output id `twst`"),
        "check stderr must name the offending id in the FIX message; got:\n{stderr}"
    );
    // §2.5-B: the FIX names the REAL declared ids (deterministic, sorted).
    assert!(
        stderr.contains("declared outputs: [pose, twist]"),
        "check stderr must list the declared outputs (the §2.5-B FIX); got:\n{stderr}"
    );
    // §2.5-B: the nearest-match `did you mean` clause (edit distance 1).
    assert!(
        stderr.contains("did you mean `twist`?"),
        "check stderr must suggest the nearest declared id; got:\n{stderr}"
    );
}

// =====================================================================
// (b) POSITIVE — a DECLARED output id type-checks, builds, and runs.
// =====================================================================

/// `@dora.node(outputs=["pose"])` whose handler calls
/// `event.send_output("pose", frame)` — the id IS declared, so it
/// type-checks, builds, and the synthetic runtime emits the payload on
/// `pose` (captured as `output[pose]=...`). Exit 0.
#[test]
fn test_pos_dora_send_output_declared_id_builds_and_runs() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"pose\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let frame: str = event.data_str()\n",
        "    let _ = event.send_output(\"pose\", frame)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe).output().expect("spawn dora positive");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "declared-id program must exit 0; stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert_eq!(out.status.code(), Some(0), "exit code must be 0");
    assert!(
        stdout.contains("output[pose]=frame_001"),
        "the `pose` send_output must be captured (synthetic build); got stdout:\n{stdout}",
    );
}

// =====================================================================
// (c) NON-LITERAL — a computed/variable id SKIPS the compile-time check
// (cannot prove statically). Proves NO false-positive.
// =====================================================================

/// `event.send_output(<str var>, _)` — the id is NOT a string literal, so
/// the compile-time check is SKIPPED (the runtime backstop covers the
/// dynamic case). The program type-checks even though `outputs=["pose"]`
/// declares only `pose` and the variable could hold anything. This proves
/// the check does NOT false-positive on the un-provable dynamic surface.
#[test]
fn test_nonliteral_dora_send_output_id_type_checks() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"pose\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let dynamic_id: str = event.data_str()\n",
        "    let _ = event.send_output(dynamic_id, \"payload\")\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_check(source);
    assert!(
        ok,
        "a non-literal `send_output` id must SKIP the compile-time check \
         (no false-positive); check stderr=\n{stderr}"
    );
}

// =====================================================================
// (d) BARE — `@dora.node` (NO outputs=) declares nothing → None-set SKIP.
// Proves NO false-positive on the un-typed bare surface.
// =====================================================================

/// A bare `@dora.node` (no `inputs=`/`outputs=`) declares NO outputs, so
/// the declared-output set is `None` → the compile-time check is INERT.
/// `event.send_output("anything", _)` type-checks (the full declared set is
/// unknown; the runtime backstop applies). Proves NO false-positive when
/// the node opts out of the typed-IO surface.
#[test]
fn test_bare_dora_node_send_output_literal_type_checks() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let _ = event.send_output(\"anything\", \"payload\")\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (ok, stderr) = try_check(source);
    assert!(
        ok,
        "a bare `@dora.node` (no outputs=) must leave the output-id check \
         INERT (no false-positive); check stderr=\n{stderr}"
    );
}
