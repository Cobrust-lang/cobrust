//! ADR-0072 first proof — end-to-end `.cb` source → compile → link →
//! run for the `den` ecosystem-import wiring.
//!
//! Confirms the FULL vertical slice that no single layer can exercise
//! alone:
//!
//! ```text
//! `import den` + `den.connect(...)` / `conn.execute(...)` / `.fetchall()`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_den_* Constant::Str)
//!   → cobrust-codegen externs + nominal-handle drop schedule
//!   → cobrust-den C-ABI shims (libden.a)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → stdout
//! ```
//!
//! The milestone program (ADR-0072 §4), wrapped in the `fn main()` the
//! AOT entrypoint requires (bare module-level code is a separate,
//! pre-existing toolchain limitation — `_cobrust_user_main` is emitted
//! from `fn main`). It opens `:memory:`, CREATE/INSERT/SELECTs, and
//! prints the fetched row set `[(42,)]`.

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

/// ADR-0072 §4 milestone program — the smallest slice proving every
/// layer. Must print the fetched row `[(42,)]` and exit 0.
#[test]
fn test_e2e_den_connect_execute_fetchall_prints_row() {
    let stdout = build_and_run_source(concat!(
        "import den\n",
        "\n",
        "fn main() -> i64:\n",
        "    let conn = den.connect(\":memory:\")\n",
        "    let cur = conn.execute(\"CREATE TABLE t(x INTEGER)\")\n",
        "    let _ = conn.execute(\"INSERT INTO t VALUES (42)\")\n",
        "    let rows = conn.execute(\"SELECT x FROM t\").fetchall()\n",
        "    print(rows)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "[(42,)]\n");
}

/// Multi-row / multi-statement variant — exercises the handle drop
/// schedule across several `execute` cursors and renders multiple rows.
#[test]
fn test_e2e_den_multi_row_fetchall() {
    let stdout = build_and_run_source(concat!(
        "import den\n",
        "\n",
        "fn main() -> i64:\n",
        "    let conn = den.connect(\":memory:\")\n",
        "    let _c = conn.execute(\"CREATE TABLE nums(n INTEGER)\")\n",
        "    let _a = conn.execute(\"INSERT INTO nums VALUES (1)\")\n",
        "    let _b = conn.execute(\"INSERT INTO nums VALUES (2)\")\n",
        "    let rows = conn.execute(\"SELECT n FROM nums ORDER BY n\").fetchall()\n",
        "    print(rows)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "[(1,), (2,)]\n");
}
