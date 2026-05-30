//! ADR-0076 Phase 2 — MULTI-IO end-to-end `.cb` source → compile → link →
//! run → stdout-assert for the `dora` ecosystem-import wiring.
//!
//! This is the TEST-FIRST (ADSD) corpus for the Phase 2 multi-IO scope
//! (ADR-0076 §5 + §6 "Phase 2 — Multi-IO, ... decorator sugar"). It is
//! RED at HEAD `8020f22`: the Phase 1 synthetic trampoline in
//! `crates/cobrust-dora/src/cabi.rs` injects exactly ONE canned event
//! (`id="camera"`, `data_str="frame_001"`) and there is NO `send_output`
//! surface anywhere in the compiler/runtime src (only in ADR/strategy
//! docs). The DEV closes the loop by (a) growing the trampoline to a
//! per-input event QUEUE so the handler fires once per declared input,
//! and (b) wiring a `send_output` surface that the trampoline captures
//! and surfaces to stdout for assertion.
//!
//! ## What Phase 1 ships today (the RED baseline)
//!
//! - `__cobrust_dora_node_run` (cabi.rs ~L271) allocates ONE
//!   `DoraEventHandle { id: "camera", data_str: "frame_001" }`, invokes
//!   the registered handler ONCE, frees the Event, returns 0. There is
//!   no event queue and no notion of "tick" as an input id.
//! - The `@dora.node(inputs=[...], outputs=[...])` decorator (HIR
//!   `validate_module_node_decorator_shape`, lower.rs ~L2273) VALIDATES
//!   the `inputs=`/`outputs=` lists as list-of-str literals then DROPS
//!   them (F68 resolution) — the synthesised `dora.node(<handler>)` is
//!   single-arg, the IO metadata never reaches the runtime. So a node
//!   declaring `inputs=["tick", "camera"]` still only ever sees the one
//!   canned "camera" event.
//! - The dora manifest (`cobrust-types/src/ecosystem.rs`) has rows for
//!   `(DORA_NODE_ADT, "run"|"shutdown")` + `(DORA_EVENT_ADT,
//!   "id"|"data_str")` — but NO `send_output` row on either handle.
//!   ADR-0076-y §4 (lines 226-231) sketches a `(DORA_NODE_ADT,
//!   "send_output")` row + `__cobrust_dora_node_send_output` shim; it is
//!   NOT yet in src.
//!
//! ## The Phase 2 behavior these tests pin
//!
//! 1. MULTI-INPUT DISPATCH: a handler declaring ≥2 inputs is invoked
//!    once per input — the trampoline injects a small canned QUEUE
//!    (one event per declared input id) instead of a single event. The
//!    handler dispatches on `event.id()` and prints a line per input;
//!    the test asserts BOTH input ids appear in stdout (proving the
//!    handler saw two distinct events, not the single P1 canned tick).
//! 2. SEND_OUTPUT CAPTURE: the handler emits on a declared output; the
//!    trampoline captures sent outputs and prints a marker line
//!    (`output[reading]=<payload>`); the test asserts that line.
//!
//! ## Send surface assumed (DEV owns the final surface)
//!
//! Primary assumption: **`event.send_output(output_id, payload)`** — an
//! Event-handle method. Rationale: in the decorator/callback form the
//! handler signature is `fn on_event(event: dora.Event) -> i64`; the
//! `node` handle is a LOCAL of `main` and is NOT in the handler's scope,
//! so a `node.send_output(...)` call (as the ADR-0076-y §5 prose sketch
//! shows) cannot type-check inside the handler today without a separate
//! "ambient node" mechanism. The Event is the ONE handle in scope, and
//! it already carries borrow-shim methods (`event.id()`,
//! `event.data_str()` — `DORA_EVENT_ADT` rows). Adding
//! `(DORA_EVENT_ADT, "send_output")` mirrors that shape with ZERO new
//! scoping machinery. If the DEV instead lands the ADR-0076-y
//! `(DORA_NODE_ADT, "send_output")` shape (via an ambient/captured node
//! handle), update ONLY the `send_output(...)` call site in the two
//! source fixtures below — the load-bearing assertions (two inputs
//! dispatched + an output captured on stdout) are surface-agnostic.
//!
//! Payload is a plain `str` (Phase 1 Arrow surface is `i64`+`str` scalar
//! only per ADR-0076 §4 risk 3; `pa.array_i64(...)` is Phase 2+/0076c).
//!
//! ## Dispatch primitive
//!
//! `if str_eq_lit(event.id(), "camera") == 1:` is the Phase-1-blessed
//! str==literal dispatch form (F68 §2; `examples/leetcode-stress/020-
//! twoptr-backspace-compare/solution.cb`). The natural `==` on `str` is
//! a separate Phase G+ language surface and is deliberately NOT used here
//! so the only RED is the multi-IO gap, not an incidental operator gap.
//!
//! ## Print primitive
//!
//! `print_no_nl(prefix)` + `print(value)` is the proven green dora_hello
//! pattern (avoids `+` str-concat / f-string chain links F68 §2 flagged).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable and return its path. The
/// caller spawns + asserts. Mirrors `dora_hello_e2e::compile_source`.
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

/// Build-and-run: returns `(build_ok, build_stderr, run_stdout,
/// run_stderr, exit_code)`. Unlike `compile_source` this does NOT assert
/// the build succeeds — Phase-2-RED programs may legitimately fail to
/// build (e.g. `send_output` is an unknown method at HEAD). The caller
/// inspects the tuple. The run fields are empty strings if the build
/// failed.
fn build_then_run(source: &str) -> (bool, String, String, String, Option<i32>) {
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
        .output()
        .unwrap();
    let build_ok = build.status.success();
    let build_stderr = String::from_utf8_lossy(&build.stderr).into_owned();
    if !build_ok {
        return (false, build_stderr, String::new(), String::new(), None);
    }
    let run = Command::new(&exe)
        .output()
        .expect("spawn dora multi-io prog");
    (
        true,
        build_stderr,
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
        run.status.code(),
    )
}

/// The canonical Phase 2 multi-IO `.cb` source. A node declaring TWO
/// inputs (`tick`, `camera`) + ONE output (`reading`). The handler
/// dispatches on `event.id()`:
///   - on `camera`: emit the frame payload on the `reading` output.
///   - always: print `saw input: <id>`.
///
/// Phase 2 runtime contract: the trampoline injects a canned event on
/// EACH declared input, so the handler fires twice (once per input).
const MULTI_IO_SRC: &str = concat!(
    "import dora\n",
    "\n",
    "@dora.node(inputs=[\"tick\", \"camera\"], outputs=[\"reading\"])\n",
    "fn on_event(event: dora.Event) -> i64:\n",
    "    if str_eq_lit(event.id(), \"camera\") == 1:\n",
    "        let payload: str = event.data_str()\n",
    "        let _ = event.send_output(\"reading\", payload)\n",
    "    print_no_nl(\"saw input: \")\n",
    "    print(event.id())\n",
    "    return 0\n",
    "\n",
    "fn main() -> i64:\n",
    "    let node = dora.Node(\"sensor\")\n",
    "    let _ = node.run()\n",
    "    return 0\n",
);

/// PRIMARY PHASE-2 TEST — multi-input dispatch + send_output capture.
///
/// Asserts (all RED at HEAD `8020f22`):
///   1. the handler SAW `tick`   (`saw input: tick`)   — 2nd input,
///   2. the handler SAW `camera` (`saw input: camera`) — proving the
///      trampoline injected BOTH declared inputs, not the single P1
///      canned event,
///   3. the `reading` output was captured (`output[reading]=...`) —
///      proving `send_output` reached the runtime + was emitted.
///
/// At HEAD this either FAILS TO BUILD (`send_output` is an unknown
/// `dora.Event` method — no manifest row) OR, if a future partial state
/// makes it build, runs but only ever prints `saw input: camera` (the
/// single P1 canned tick) — so assertion (1) and/or (3) fail. Either way
/// RED. See the module doc for which the actual HEAD exhibits.
#[test]
fn test_e2e_dora_multi_input_dispatch_and_send_output() {
    let (build_ok, build_stderr, stdout, stderr, code) = build_then_run(MULTI_IO_SRC);

    // If the program failed to build, that IS the RED evidence at HEAD
    // (send_output unknown). Surface the diagnostic so the DEV sees the
    // exact gap; the post-DEV GREEN state must build + run + satisfy the
    // three stdout assertions below.
    assert!(
        build_ok,
        "multi-io program must BUILD (Phase 2). At HEAD this is RED: \
         `event.send_output` is an unknown `dora.Event` method (no manifest \
         row). build stderr:\n{build_stderr}"
    );

    assert_eq!(
        code,
        Some(0),
        "exit code must be 0; got {code:?}\nstdout=\n{stdout}\nstderr=\n{stderr}"
    );

    // (1) + (2) MULTI-INPUT DISPATCH — both declared inputs reached the
    // handler. The `tick` line is the load-bearing one: P1 only ever
    // injects "camera", so seeing "tick" proves the per-input queue.
    assert!(
        stdout.contains("saw input: tick"),
        "handler must see the `tick` input (multi-input dispatch); P1 only \
         injects the single canned `camera` event. got stdout:\n{stdout}\n\
         stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("saw input: camera"),
        "handler must see the `camera` input. got stdout:\n{stdout}\n\
         stderr:\n{stderr}"
    );

    // (3) SEND_OUTPUT CAPTURE — the trampoline captured the output the
    // handler emitted on the `reading` port. The exact payload is the
    // canned camera event's data_str (Phase 2 trampoline chooses the
    // canned payloads; the DEV may pick any non-empty literal — assert
    // the marker + that SOMETHING followed it). The surface-agnostic
    // load-bearing fact is "an output named `reading` was captured".
    assert!(
        stdout.contains("output[reading]="),
        "the `reading` send_output must be captured + surfaced by the \
         trampoline (`output[reading]=<payload>`). At HEAD there is no \
         send_output surface at all. got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// NO-REGRESSION — the P1 single-input shape still works under whatever
/// Phase 2 lands. A node declaring ONE input (`camera`), no output, the
/// dora_hello handler body. This is the SAME behavioral contract as
/// `dora_hello_e2e::test_e2e_dora_hello_synthetic_runtime_round_trip`
/// (which stays untouched + green — that is the real regression check);
/// duplicated here so a Phase 2 trampoline rewrite that breaks the
/// single-input path is caught in THIS file too.
///
/// GREEN at HEAD `8020f22` (this is exactly the proven Phase 1 surface).
/// Stays GREEN post-DEV.
#[test]
fn test_e2e_dora_single_input_no_regression() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"detections\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let frame: str = event.data_str()\n",
        "    print_no_nl(\"got frame: \")\n",
        "    print(frame)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe)
        .output()
        .expect("spawn dora single-input");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "single-input binary exit non-zero ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("got frame: frame_001"),
        "single-input stdout must contain `got frame: frame_001`; got:\n{stdout}\nstderr:\n{stderr}",
    );
}

/// FOCUSED RED PROBE — isolate the `send_output` surface gap from the
/// multi-input gap. A SINGLE-input node that ONLY calls `send_output`.
/// This builds + runs today IFF `send_output` exists; at HEAD it does
/// not, so this is the cleanest single-axis RED for the send surface.
///
/// Kept separate from the primary test so the DEV can land the
/// `send_output` manifest row + shim and turn THIS green first, then the
/// multi-input queue work turns the primary test green — two
/// independently-bisectable RED axes.
#[test]
fn test_e2e_dora_send_output_surface_exists() {
    let source = concat!(
        "import dora\n",
        "\n",
        "@dora.node(inputs=[\"camera\"], outputs=[\"reading\"])\n",
        "fn detect(event: dora.Event) -> i64:\n",
        "    let payload: str = event.data_str()\n",
        "    let _ = event.send_output(\"reading\", payload)\n",
        "    print(\"emitted\")\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"detector\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (build_ok, build_stderr, stdout, stderr, code) = build_then_run(source);

    assert!(
        build_ok,
        "a node calling `event.send_output(...)` must BUILD (Phase 2). At \
         HEAD `8020f22` this is RED: no `send_output` manifest row on \
         `dora.Event` (nor `dora.Node`). build stderr:\n{build_stderr}"
    );
    assert_eq!(code, Some(0), "exit must be 0; stderr:\n{stderr}");
    assert!(
        stdout.contains("output[reading]="),
        "the `reading` output must be captured by the trampoline \
         (`output[reading]=<payload>`). got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("emitted"),
        "handler body must still run to completion. got stdout:\n{stdout}"
    );
}
