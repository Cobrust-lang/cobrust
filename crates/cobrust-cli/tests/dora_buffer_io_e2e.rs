//! ADR-0076c (D)-B-1a — `.cb` end-to-end for the typed-numeric
//! Arrow↔coil.Buffer round-trip on the SYNTHETIC dora build: a node reads a
//! typed input payload via `event.data_buffer() -> coil.Buffer`, runs coil
//! math on it, and emits a typed output via
//! `event.send_output_buffer(output_id, buffer)`.
//!
//! # What this pins (the synthetic-build contract)
//!
//! The DEFAULT (synthetic) dora build hands a CANNED Float64 `[1.0, 2.0,
//! 3.0]` from `data_buffer()` (so the whole `.cb` build → link → run chain
//! resolves the two new shims + the coil bridge WITHOUT a live broker — the
//! same shape `event.data_str()` returns a canned `"frame_001"`). These
//! tests assert:
//!   1. the program type-checks + BUILDS (the two manifest rows + the
//!      codegen externs + the cobrust-dora↔cobrust-coil cross-crate link all
//!      resolve),
//!   2. `data_buffer()` flows into coil math (`coil.print_buffer` shows the
//!      canned `array([1, 2, 3], dtype=float64)`),
//!   3. `send_output_buffer("<declared>", buf)` is captured + surfaced as
//!      `output[<id>]=buffer[len=<n>]` (the synthetic-E2E marker),
//!   4. the `.cb` scope drops the `coil.Buffer` it owns exactly once (a
//!      double-free / leak would crash / leak — the program exits 0).
//!
//! The HERMETIC bit-faithfulness of the ndarray↔arrow bridge (all 5 dtypes,
//! empty, the 1000-event drop balance) is proven UNCONDITIONALLY by the
//! cabi crate's `arrow_bridge_tests` (under `--features dora-real`); the
//! LIVE real round-trip is `dora_real_node_e2e` Part C. THIS file is the
//! cheap, always-on `.cb`-chain proof on the default build (no heavy arch).
//!
//! # Why a node importing BOTH `dora` and `coil`
//!
//! `event.data_buffer()` returns a `coil.Buffer`, so the source must
//! `import coil` to name the type + (optionally) call coil ops on it —
//! exactly the ADR-0076c §3 "one array type spans the numeric + robotics
//! pillars" robot-policy shape. This also exercises the cross-crate link
//! fix (libdora.a embeds coil's `Array`/constructors but NOT coil's
//! `#[no_mangle]` cabi shims — cobrust-dora deps coil
//! `default-features = false` — so a program linking BOTH `libdora.a` AND
//! `libcoil.a` has no duplicate-symbol clash; ADR-0076c cross-crate note).
//!
//! # LINK note (ADR-0076c BLOCKER-A): coil is pulled by drop-glue, not the
//! `coil.Buffer` type name
//!
//! A `coil.Buffer` local — even one the handler never passes to a
//! `coil.<fn>()` free-fn — emits scope-exit DROP-GLUE
//! (`__cobrust_coil_buffer_drop`) that lives ONLY in `libcoil.a`. So
//! `libcoil.a` MUST be on the link line whenever a `coil.Buffer` is owned,
//! regardless of any explicit `coil.*` call. `data_buffer()` is the first
//! NON-`coil` module to hand out a `coil.Buffer`, so an echo node
//! (`data_buffer()` → `send_output_buffer()`, no other coil call) used to
//! link-fail: the build's `collect_ecosystem_modules` scanned only
//! `Terminator::Call` callees, missing the drop. The fix scans
//! `Terminator::Drop` on ecosystem-handle locals too. The
//! `test_e2e_dora_echo_buffer_no_explicit_coil_call_links` test below is
//! the drop-glue-only regression guard; the other tests here additionally
//! exercise the `coil.<fn>()`-call link path.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; assert the build succeeds and
/// return its path. Mirrors `dora_multi_io_e2e::compile_source`.
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

/// PRIMARY — a node reads `event.data_buffer()`, prints it via coil, and
/// emits it back via `event.send_output_buffer(...)`. On the synthetic
/// build the canned Float64 `[1.0, 2.0, 3.0]` flows through both shims;
/// the program exits 0 (the `coil.Buffer` it owns drops exactly once).
#[test]
fn test_e2e_dora_data_buffer_and_send_output_buffer_round_trip() {
    let source = concat!(
        "import dora\n",
        "import coil\n",
        "\n",
        "@dora.node(inputs=[\"state\"], outputs=[\"action\"])\n",
        "fn policy(event: dora.Event) -> i64:\n",
        "    let obs: coil.Buffer = event.data_buffer()\n",
        "    let _ = coil.print_buffer(obs)\n",
        "    let _ = event.send_output_buffer(\"action\", obs)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"policy_node\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe)
        .output()
        .expect("spawn dora buffer-io node");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "buffer-io node must exit 0; stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert_eq!(out.status.code(), Some(0), "exit code must be 0");

    // (2) data_buffer() flowed into coil math — the canned Float64 buffer
    // prints as numpy-shaped `array([1, 2, 3], dtype=float64)` (coil drops
    // trailing zeros in the float repr).
    assert!(
        stdout.contains("array([1, 2, 3], dtype=float64)"),
        "data_buffer() must yield the canned Float64 buffer + coil.print_buffer it; \
         got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // (3) send_output_buffer captured on the declared `action` port (the
    // synthetic marker; len=3 is the canned buffer's element count).
    assert!(
        stdout.contains("output[action]=buffer[len=3]"),
        "send_output_buffer must be captured + surfaced (`output[action]=buffer[len=3]`); \
         got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// ECHO (BLOCKER-A regression) — the MINIMAL / most natural buffer node:
/// `data_buffer()` → `send_output_buffer()` with NO other `coil.*` call in
/// the source. The handler names `coil.Buffer` (the return type) but invokes
/// ZERO `coil.<fn>()` free-fns, so the ONLY `coil` symbol the object file
/// references is the scope-exit DROP-GLUE (`__cobrust_coil_buffer_drop`).
///
/// This is the exact path that LINK-FAILED before the BLOCKER-A fix:
/// `collect_ecosystem_modules` scanned only `Terminator::Call` callees and
/// was BLIND to drop-glue, so `coil` was never added to the link set,
/// `libcoil.a` was never on the link line, and `cobrust build` died with
/// `ld: ___cobrust_coil_buffer_drop not found` (while `cobrust check`
/// PASSED — the symbol resolved in the manifest but not the linker). The
/// pre-fix tests all incidentally co-located a `coil.print_buffer`/`mean`/
/// `full` call that pulled `libcoil.a` as a side-effect, masking the bug
/// (F36/F37-class). This test omits ALL explicit `coil.*` calls so it
/// covers the drop-glue-only link path directly; a green build here proves
/// the scanner now registers `coil` from the `Terminator::Drop` on the
/// `coil.Buffer` local alone.
#[test]
fn test_e2e_dora_echo_buffer_no_explicit_coil_call_links() {
    // ONLY `data_buffer()` + `send_output_buffer()` — the `coil.Buffer`
    // type name is the sole `coil` surface; no `coil.<fn>()` is called, so
    // drop-glue is the only thing that pulls `libcoil.a`.
    let source = concat!(
        "import dora\n",
        "import coil\n",
        "\n",
        "@dora.node(inputs=[\"state\"], outputs=[\"action\"])\n",
        "fn echo(event: dora.Event) -> i64:\n",
        "    let buf: coil.Buffer = event.data_buffer()\n",
        "    let _ = event.send_output_buffer(\"action\", buf)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"echo_node\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    // `compile_source` asserts the BUILD succeeds (the link line now carries
    // `libcoil.a` from the drop-glue scan) — the regression is in the build,
    // not the run.
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe).output().expect("spawn dora echo node");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "echo node must exit 0 (drops the owned coil.Buffer exactly once); \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // The canned buffer (len 3) is echoed on `action` — proving the round
    // trip ran end-to-end through the drop-glue-only link path.
    assert!(
        stdout.contains("output[action]=buffer[len=3]"),
        "echo node must emit the data_buffer() payload back on `action`; \
         got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// CHAIN — `data_buffer()` feeds a coil reduction (`coil.mean`) whose scalar
/// result the handler prints, AND a coil scalar-op result is emitted. Proves
/// the received Buffer is a first-class coil value (not an opaque blob): the
/// SAME numpy-rebrand ops a robot policy runs (`buf.mean()`-style) work on a
/// dora-delivered Buffer. mean([1,2,3]) == 2 (printed as a float).
#[test]
fn test_e2e_dora_data_buffer_feeds_coil_reduction() {
    let source = concat!(
        "import dora\n",
        "import coil\n",
        "\n",
        "@dora.node(inputs=[\"state\"], outputs=[\"action\"])\n",
        "fn policy(event: dora.Event) -> i64:\n",
        "    let obs: coil.Buffer = event.data_buffer()\n",
        "    let m: f64 = coil.mean(obs)\n",
        "    print_no_nl(\"mean=\")\n",
        "    print(m)\n",
        "    let scaled: coil.Buffer = coil.full(3, m)\n",
        "    let _ = event.send_output_buffer(\"action\", scaled)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"policy_node\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);

    let out = Command::new(&exe)
        .output()
        .expect("spawn dora reduction node");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "reduction node must exit 0; stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // mean of the canned [1,2,3] is 2.
    assert!(
        stdout.contains("mean=2"),
        "coil.mean(data_buffer()) must be 2 (canned [1,2,3]); got stdout:\n{stdout}",
    );
    // The freshly-constructed `scaled` Buffer (len 3) is emitted.
    assert!(
        stdout.contains("output[action]=buffer[len=3]"),
        "the coil-derived Buffer must emit on `action`; got stdout:\n{stdout}",
    );
}

/// NEGATIVE — `event.send_output_buffer("<typo>", buf)` on an UNDECLARED
/// output id is REJECTED at compile time with `DoraUnknownOutputId` (the
/// §2.5-A compile-time-catch now fires for `send_output_buffer` too, NOT
/// just `send_output` — else a typo'd id in the BUFFER send would escape to
/// a runtime `-1`). The check is on arg0 (the id) for BOTH methods.
#[test]
fn test_neg_dora_send_output_buffer_undeclared_id_rejected() {
    let source = concat!(
        "import dora\n",
        "import coil\n",
        "\n",
        "@dora.node(inputs=[\"state\"], outputs=[\"action\"])\n",
        "fn policy(event: dora.Event) -> i64:\n",
        "    let obs: coil.Buffer = event.data_buffer()\n",
        "    let _ = event.send_output_buffer(\"acton_typo\", obs)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"policy_node\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));

    // `cobrust check` — the compile-time gate (no link needed).
    let check = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&check.stderr).into_owned();
    assert!(
        !check.status.success(),
        "an undeclared `send_output_buffer` id must FAIL `cobrust check`; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("DoraUnknownOutputId") || stderr.contains("unknown dora output id"),
        "check stderr must name the DoraUnknownOutputId reject; got:\n{stderr}"
    );
    assert!(
        stderr.contains("acton_typo"),
        "check stderr must name the offending id `acton_typo`; got:\n{stderr}"
    );
    assert!(
        stderr.contains("action"),
        "check stderr must name the declared output `action` (the §2.5-B FIX); got:\n{stderr}"
    );
}

/// NON-LITERAL — a computed/variable output id SKIPS the compile-time check
/// for `send_output_buffer` (cannot prove statically) — proving NO
/// false-positive (mirrors the `send_output` non-literal skip). The runtime
/// `-1` backstop covers the dynamic case.
#[test]
fn test_nonliteral_dora_send_output_buffer_id_type_checks() {
    let source = concat!(
        "import dora\n",
        "import coil\n",
        "\n",
        "@dora.node(inputs=[\"state\"], outputs=[\"action\"])\n",
        "fn policy(event: dora.Event) -> i64:\n",
        "    let obs: coil.Buffer = event.data_buffer()\n",
        "    let dyn_id: str = event.id()\n",
        "    let _ = event.send_output_buffer(dyn_id, obs)\n",
        "    return 0\n",
        "\n",
        "fn main() -> i64:\n",
        "    let node = dora.Node(\"policy_node\")\n",
        "    let _ = node.run()\n",
        "    return 0\n",
    );
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let check = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "a NON-literal `send_output_buffer` id must type-check (no false-positive); stderr=\n{}",
        String::from_utf8_lossy(&check.stderr)
    );
}
