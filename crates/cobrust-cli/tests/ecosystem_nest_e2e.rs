//! ADR-0072 second-module proof — end-to-end `.cb` source → compile →
//! link → run for the `nest` ecosystem-import wiring.
//!
//! Twin of `ecosystem_den_e2e.rs`: confirms the SAME chain that the
//! `den` first proof exercises generalizes to a SECOND module. The
//! `nest` surface is the simplest cheap generalization — pure value-
//! in-value-out (`str → str`), no handles, no callbacks — proving the
//! chain isn't den-specific.
//!
//! ```text
//! `import nest` + `nest.loads_str(toml)`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_nest_loads_str)
//!   → cobrust-codegen extern + existing Str drop schedule
//!   → cobrust-nest C-ABI shim (libnest.a)
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

/// ADR-0072 second-module proof — the smallest TOML → JSON program
/// proving the chain generalizes to `nest`. Parses `title = "hello"`
/// and prints its canonical-JSON rendering.
#[test]
fn test_e2e_nest_loads_str_simple_key_value() {
    let stdout = build_and_run_source(concat!(
        "import nest\n",
        "\n",
        "fn main() -> i64:\n",
        "    let toml_input: str = \"title = \\\"hello\\\"\\n\"\n",
        "    let canonical_json: str = nest.loads_str(toml_input)\n",
        "    print(canonical_json)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "{\"title\":\"hello\"}\n");
}

/// Nested-table variant — exercises a multi-key TOML and the JSON
/// canonicalization of a sub-object.
#[test]
fn test_e2e_nest_loads_str_nested_table() {
    let stdout = build_and_run_source(concat!(
        "import nest\n",
        "\n",
        "fn main() -> i64:\n",
        "    let toml_input: str = \"[server]\\nport = 8080\\n\"\n",
        "    let canonical_json: str = nest.loads_str(toml_input)\n",
        "    print(canonical_json)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "{\"server\":{\"port\":8080}}\n");
}
