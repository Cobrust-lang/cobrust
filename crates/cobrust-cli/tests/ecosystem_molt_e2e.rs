//! ADR-0072 fifth-module proof — end-to-end `.cb` source → compile →
//! link → run for the `molt` ecosystem-import wiring (datetime, rebrand
//! of `python-dateutil`).
//!
//! Twin of `ecosystem_den_e2e.rs` (handle pattern) and
//! `ecosystem_strike_e2e.rs` (handle + free-function entrypoint). Proves
//! the chain handles a THIRD handle-pattern module without touching any
//! chain logic — only manifest, codegen extern, recognizer alternation,
//! and the new shim crate were added. Unlike `strike`, `molt` is purely
//! local-clock so no test server is needed.
//!
//! ```text
//! `import molt` + `molt.now()` + `dt.isoformat()` + `.unix_timestamp()`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_molt_*)
//!   → cobrust-codegen externs + DateTime handle drop schedule
//!   → cobrust-molt C-ABI shims (libmolt.a)
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

/// ADR-0072 fifth-module proof — the smallest local-time program proving
/// the chain generalizes to `molt`. Captures the current UTC time and
/// prints its RFC3339 isoformat + unix epoch seconds. The exact values
/// are checked structurally (shape + bracket) since the wall clock
/// advances between test runs.
#[test]
fn test_e2e_molt_now_isoformat_and_unix_timestamp() {
    let stdout = build_and_run_source(concat!(
        "import molt\n",
        "\n",
        "fn main() -> i64:\n",
        "    let now = molt.now()\n",
        "    let iso: str = now.isoformat()\n",
        "    let stamp: i64 = now.unix_timestamp()\n",
        "    print(iso)\n",
        "    print(stamp)\n",
        "    return 0\n",
    ));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected 2 lines (iso + stamp), got: {stdout}"
    );
    let iso = lines[0];
    let stamp_text = lines[1];

    // ISO line — RFC3339 shape: `YYYY-MM-DDTHH:MM:SS(.fff)?(Z|+HH:MM)`.
    assert!(iso.len() >= 20, "iso line too short: {iso}");
    assert!(iso.contains('T'), "iso must contain 'T': {iso}");
    assert!(
        iso.ends_with('Z') || iso.contains('+') || iso.matches('-').count() >= 3,
        "iso must carry tz suffix: {iso}"
    );

    // Unix-timestamp line — parses to a sane bracket (2024–2100).
    let stamp: i64 = stamp_text
        .parse()
        .expect("unix_timestamp must parse to i64");
    assert!(
        stamp > 1_700_000_000,
        "unix_timestamp seems too small: {stamp}"
    );
    assert!(
        stamp < 4_102_444_800,
        "unix_timestamp seems too large: {stamp}"
    );
}

/// Twin run — proves a second invocation lands consistently (clock
/// monotonic on the wall-clock side; second `stamp` is `>=` the first).
/// Exercises the drop schedule across two scope-local handles.
#[test]
fn test_e2e_molt_two_invocations_monotone() {
    let stdout = build_and_run_source(concat!(
        "import molt\n",
        "\n",
        "fn main() -> i64:\n",
        "    let first = molt.now()\n",
        "    let s1: i64 = first.unix_timestamp()\n",
        "    let second = molt.now()\n",
        "    let s2: i64 = second.unix_timestamp()\n",
        "    print(s1)\n",
        "    print(s2)\n",
        "    return 0\n",
    ));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "expected 2 lines, got: {stdout}");
    let s1: i64 = lines[0].parse().expect("first stamp must parse");
    let s2: i64 = lines[1].parse().expect("second stamp must parse");
    assert!(
        s2 >= s1,
        "second invocation must not go back in time: s1={s1}, s2={s2}"
    );
}
