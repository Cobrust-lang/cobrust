//! #146 dora-cb Phase A — the **F36-honest** end-to-end proof that a
//! `.cb` dora node built `--features dora-real` is GENUINELY real (a live
//! `dora_node_api::DoraNode` + `events.recv()` loop), NOT the synthetic
//! canned-event trampoline that is the default build.
//!
//! # Why two parts (the F36 trap + how this escapes it)
//!
//! A test merely *named* `dora_real_*` that only checks the program
//! COMPILES is the F36 fixture-name-vs-behavior trap (memory:
//! f36-fixture-name-vs-behavior-drift): a green compile says nothing about
//! whether the REAL path or the synthetic trampoline ran. This file proves
//! "real" two independent, mutation-survivable ways:
//!
//! - **Part A — ALWAYS-ON hermetic link+symbol proof.** Build
//!   `cobrust-dora --features dora-real` → a REAL `libdora.a` (the dora /
//!   arrow / tokio stack), compile a `.cb` dora node against it (pointing
//!   `COBRUST_ECOSYSTEM_ARCHIVE_DORA` at the real archive), then `nm` the
//!   linked binary and assert it contains REAL `dora_node_api` + `arrow`
//!   symbols. The synthetic-default `libdora.a` has ZERO such symbols
//!   (asserted by the cabi crate's own check), so a binary carrying them
//!   PROVES the real path compiled + linked — not the trampoline. This is
//!   the dora-real-integration-plan §9 spike's "28,376 real symbols pulled
//!   in" reduced to a CI assertion. Mutation: revert the `cabi.rs` swap →
//!   the real bodies are gone → these symbols vanish → Part A fails.
//!
//! - **Part B — LIVE real `DoraNode` round-trip (hermetic, NO daemon).**
//!   dora 0.5.0 ships an `integration_testing` mode: setting
//!   `DORA_TEST_WITH_INPUTS` to a JSON events file makes the REAL
//!   `DoraNode::init_from_env()` construct a real node that feeds those
//!   events through the real `EventStream` (no coordinator/daemon needed).
//!   We drive the same `--features dora-real` binary with ONE real
//!   `Input{id:"tick", data:"<unique marker>"}` event then `Stop`, and
//!   assert the `.cb` handler printed the REAL marker payload it decoded
//!   from the live Arrow `ArrayRef`. Mutation: revert the swap → the
//!   synthetic trampoline ignores `DORA_TEST_WITH_INPUTS` and prints the
//!   canned `frame_tick` / `frame_001` instead of the marker → Part B
//!   fails. THIS is the load-bearing real-vs-synthetic delta — the handler
//!   observes data that ONLY a real `EventStream` could have delivered.
//!
//! # Skip discipline
//!
//! The `--features dora-real` archive is heavy (the dora tree, cold ~11
//! min). Both parts self-SKIP cleanly (an `eprintln!` + `return`, mirroring
//! the `redis_live_e2e` runtime-skip pattern) when the real archive cannot
//! be produced — UNLESS `COBRUST_DORA_REAL_E2E=1` is set, which makes a
//! build failure a HARD test failure (the CI lane that wants the real proof
//! sets it). So a fast local `cargo test` is not blocked on the 11-min
//! build, while the dedicated CI job enforces the real gate.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The `.cb` dora node both parts compile + run. Declares ONE input
/// `tick` (via the `@dora.node` decorator desugar) so the node has a real
/// input port; the handler prints the event id + the decoded payload. The
/// `REAL[...]` framing makes the assertion unambiguous + greppable.
const DORA_NODE_SRC: &str = concat!(
    "import dora\n",
    "\n",
    "@dora.node(inputs=[\"tick\"])\n",
    "fn on_tick(event: dora.Event) -> i64:\n",
    "    let id: str = event.id()\n",
    "    let payload: str = event.data_str()\n",
    "    print_no_nl(\"REAL[\")\n",
    "    print_no_nl(id)\n",
    "    print_no_nl(\"]=\")\n",
    "    print(payload)\n",
    "    return 0\n",
    "\n",
    "fn main() -> i64:\n",
    "    let node = dora.Node(\"cb_real_node\")\n",
    "    let _ = node.run()\n",
    "    return 0\n",
);

/// Whether a build failure should HARD-fail the test (CI real-gate lane)
/// rather than self-skip (fast local `cargo test`).
fn strict() -> bool {
    std::env::var("COBRUST_DORA_REAL_E2E").as_deref() == Ok("1")
}

/// Workspace root (two parents up from this crate's manifest dir).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("derive workspace root")
}

/// Build `cobrust-dora --features dora-real` and return the resulting
/// `libdora.a` path, or `None` if the build failed / the archive is absent
/// (→ self-skip unless strict). This is the REAL archive (dora + arrow +
/// tokio), distinct from the synthetic-default `target/debug/libdora.a`.
fn build_real_dora_archive() -> Option<PathBuf> {
    let ws = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .current_dir(&ws)
        .args(["build", "-p", "cobrust-dora", "--features", "dora-real"])
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    // The staticlib lands in the workspace target/debug (dev profile).
    let target_dir =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| ws.join("target"), PathBuf::from);
    let archive = target_dir.join("debug").join("libdora.a");
    archive.exists().then_some(archive)
}

/// Compile `DORA_NODE_SRC` to an executable, linking against the supplied
/// REAL `libdora.a` (via `COBRUST_ECOSYSTEM_ARCHIVE_DORA`). On macOS the
/// build.rs target-gated `-framework CoreFoundation` flag fires
/// automatically because `dora ∈ eco_modules` (#146 link fix). Returns the
/// exe path; asserts the build succeeded (a link failure here is a real
/// regression — the spike proved this links).
fn compile_against_real_archive(dir: &Path, real_archive: &Path) -> PathBuf {
    let src_path = dir.join("prog.cb");
    std::fs::write(&src_path, DORA_NODE_SRC).unwrap();
    let exe = dir.join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        // Force `cobrust build` to link the REAL dora archive instead of
        // the synthetic-default one cargo would otherwise (re)build.
        .env("COBRUST_ECOSYSTEM_ARCHIVE_DORA", real_archive)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "linking a .cb node against the REAL libdora.a failed (the §9 spike \
         proved this links — a failure is a real regression):\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    exe
}

/// Part A — ALWAYS-ON: a `.cb` node linked `--features dora-real` carries
/// REAL dora/arrow symbols in its binary (proving the real path linked, not
/// the synthetic trampoline). Self-skips (clean) when the heavy real
/// archive can't be built, unless `COBRUST_DORA_REAL_E2E=1`.
#[test]
fn dora_real_node_links_real_dora_symbols() {
    let Some(real_archive) = build_real_dora_archive() else {
        let msg = "dora_real_node_e2e (Part A): skipping cleanly — could not build \
                   `cobrust-dora --features dora-real` (the heavy real archive). \
                   Set COBRUST_DORA_REAL_E2E=1 to make this a hard failure.";
        assert!(!strict(), "{msg}");
        eprintln!("{msg}");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let exe = compile_against_real_archive(dir.path(), &real_archive);

    // `nm` the linked binary; the REAL path drags in dora_node_api + arrow
    // symbols. The synthetic-default libdora.a has ZERO of these (the cabi
    // crate asserts 0 in its own check), so their PRESENCE here proves the
    // real `DoraNode`/`EventStream` bodies compiled + linked into this exe.
    let nm = Command::new("nm")
        .arg(&exe)
        .output()
        .expect("run nm on the linked .cb dora node");
    assert!(
        nm.status.success(),
        "nm failed on {}: {}",
        exe.display(),
        String::from_utf8_lossy(&nm.stderr),
    );
    let symbols = String::from_utf8_lossy(&nm.stdout);
    let dora_syms = symbols.matches("dora_node_api").count();
    let arrow_syms = symbols.matches("arrow").count();
    assert!(
        dora_syms > 0,
        "the --features dora-real .cb binary must contain REAL `dora_node_api` \
         symbols (proving the real path linked, NOT the synthetic trampoline); \
         found {dora_syms}. If this is 0 the cabi swap reverted to synthetic.",
    );
    assert!(
        arrow_syms > 0,
        "the --features dora-real .cb binary must contain REAL `arrow` symbols \
         (the dora payload-marshalling stack); found {arrow_syms}.",
    );
    eprintln!(
        "dora_real_node_e2e (Part A): PASS — real binary carries {dora_syms} \
         dora_node_api + {arrow_syms} arrow symbols (real path linked, not synthetic)."
    );
}

/// Part B — LIVE: drive the SAME `--features dora-real` binary through dora's
/// hermetic `integration_testing` mode (NO daemon) with one REAL `Input`
/// event carrying a unique marker payload, and assert the `.cb` handler
/// printed that REAL marker (decoded from the live Arrow `ArrayRef`) — proof
/// a real `EventStream` delivered it. Self-skips (clean) when the real
/// archive can't be built, unless `COBRUST_DORA_REAL_E2E=1`.
#[test]
fn dora_real_node_drives_live_event_stream_round_trip() {
    let Some(real_archive) = build_real_dora_archive() else {
        let msg = "dora_real_node_e2e (Part B): skipping cleanly — could not build \
                   `cobrust-dora --features dora-real`. Set COBRUST_DORA_REAL_E2E=1 \
                   to make this a hard failure.";
        assert!(!strict(), "{msg}");
        eprintln!("{msg}");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let exe = compile_against_real_archive(dir.path(), &real_archive);

    // A process-unique marker so the assertion can't pass on a stale /
    // canned string — ONLY a real EventStream delivering THIS input could
    // make the handler print it.
    let marker = format!("hello_real_dora_{}", std::process::id());

    // dora `integration_testing` input file (JSON serialization of
    // `IntegrationTestInput`): one Input on port `tick` carrying the marker
    // (a bare JSON string → a length-1 Arrow Utf8 StringArray, which the
    // cabi `decode_arrow_payload` reads back losslessly), then Stop so the
    // real event loop terminates. `#[serde(tag = "type")]` on IncomingEvent
    // + `#[serde(flatten)]` on the data field define the wire shape below.
    let inputs_json = format!(
        r#"{{
  "id": "cb_real_node",
  "events": [
    {{ "time_offset_secs": 0.0, "type": "Input", "id": "tick", "data": "{marker}" }},
    {{ "time_offset_secs": 0.01, "type": "Stop" }}
  ]
}}"#,
    );
    let inputs_path = dir.path().join("inputs.json");
    std::fs::write(&inputs_path, inputs_json).unwrap();
    let outputs_path = dir.path().join("outputs.jsonl");

    let run = Command::new(&exe)
        .current_dir(dir.path())
        // REAL hermetic dora node: init_from_env() takes the testing path.
        .env("DORA_TEST_WITH_INPUTS", &inputs_path)
        .env("DORA_TEST_WRITE_OUTPUTS_TO", &outputs_path)
        .env("DORA_TEST_NO_OUTPUT_TIME_OFFSET", "1")
        .output()
        .expect("run the real-dora .cb node under integration_testing");

    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();

    assert!(
        run.status.success(),
        "real-dora node exited non-zero ({:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        run.status,
    );
    // The LOAD-BEARING assertion: the handler fired on the REAL `tick` input
    // and printed the REAL marker payload it decoded from the live Arrow
    // array. The synthetic trampoline would print `REAL[tick]=frame_tick`
    // (its canned per-input payload) and IGNORE the JSON marker entirely —
    // so matching the marker proves a real EventStream delivered the data.
    let expected = format!("REAL[tick]={marker}");
    assert!(
        stdout.contains(&expected),
        "the real-dora handler must print the marker payload delivered by the \
         live EventStream (`{expected}`) — if instead it printed a canned \
         `frame_tick`/`frame_001`, the cabi swap reverted to synthetic.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}",
    );
    eprintln!("dora_real_node_e2e (Part B): PASS — real EventStream delivered `{marker}`.");
}
