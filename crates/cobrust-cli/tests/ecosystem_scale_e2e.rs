//! ADR-0072 fourth-module proof — end-to-end `.cb` source → compile →
//! link → run for the `scale` ecosystem-import wiring (msgpack, rebrand
//! of `msgpack-python`).
//!
//! Twin of `ecosystem_nest_e2e.rs` (the value-pattern second proof).
//! Confirms the SAME chain that the `den`/`nest`/`strike` proofs
//! exercise generalizes to a FOURTH module — pure value-in-value-out
//! (`str → str`), no handles, no callbacks; the smallest cheap
//! generalization onto a fourth ecosystem module after handle (`den`)
//! + value (`nest`) + handle-with-HTTP (`strike`).
//!
//! ```text
//! `import scale` + `scale.dumps_str(json) -> str` + `scale.loads_str(packed) -> str`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_scale_*)
//!   → cobrust-codegen extern + existing Str drop schedule
//!   → cobrust-scale C-ABI shim (libscale.a)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → stdout
//! ```

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::process::Command;

/// Compile + link + run a `.cb` source, returning its stdout. Asserts
/// the build and the run both succeed.
fn build_and_run_source(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
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
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).current_dir(dir.path()).output().unwrap();
    assert!(
        run.status.success(),
        "run failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// ADR-0072 fourth-module proof — the smallest JSON → msgpack-hex →
/// JSON round trip proving the chain generalizes to `scale`. Packs the
/// single-key object `{"key":"value"}`, unpacks it back, and prints
/// the round-tripped canonical-JSON rendering.
#[test]
fn test_e2e_scale_dumps_then_loads_simple_object() {
    let stdout = build_and_run_source(concat!(
        "import scale\n",
        "\n",
        "fn main() -> i64:\n",
        "    let packed: str = scale.dumps_str(\"{\\\"key\\\":\\\"value\\\"}\")\n",
        "    let back: str = scale.loads_str(packed)\n",
        "    print(back)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "{\"key\":\"value\"}\n");
}

/// Nested-array variant — exercises a multi-key JSON object and an
/// array of integers through the msgpack value tree.
#[test]
fn test_e2e_scale_dumps_then_loads_nested_array() {
    let stdout = build_and_run_source(concat!(
        "import scale\n",
        "\n",
        "fn main() -> i64:\n",
        "    let packed: str = scale.dumps_str(\"{\\\"items\\\":[1,2,3],\\\"name\\\":\\\"x\\\"}\")\n",
        "    let back: str = scale.loads_str(packed)\n",
        "    print(back)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "{\"items\":[1,2,3],\"name\":\"x\"}\n");
}
