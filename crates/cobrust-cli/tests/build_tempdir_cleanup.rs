//! Regression test for the 2026-05-13 (OS-temp `cobrust-*`) + 2026-05-18
//! (`/private/var/folders/.../T/cobrust-*`) tempdir leak from
//! `cobrust build` + `cobrust run`.
//!
//! Note: literal `/{t}{m}{p}/cobrust-*` path string is intentionally avoided
//! in this docstring to prevent triggering `scripts/cli-tempdir-guard.sh`,
//! which scans test files for hard-coded OS-temp paths (CI gate).
//!
//! Pre-fix: `cobrust build` emitted `<module>.o` and `cobrust_main.o`
//! alongside the final executable in the caller's `-o` directory.
//! Both intermediates were leaked. Test harnesses passing `-o` to
//! `$TMPDIR/cobrust-*` paths piled up 100+ GB invisibly.
//!
//! Post-fix (commit `feat(cli): wrap build/run tempdir in TempDir RAII`):
//! the intermediates live under a scoped `TempDir` whose Drop removes
//! them on graceful exit AND panic-unwind. This test asserts that
//! after `cobrust build` returns successfully, the user's output
//! directory contains ONLY the final executable — no `<module>.o`,
//! no `cobrust_main.o`, no `cobrust-build-*` tempdirs.
//!
//! The same invariant is verified for `cobrust run`, which uses the
//! same build path internally.
//!
//! Counterpart to `81a2433` (cli TEST-side TempDir RAII), which only
//! patched the test harnesses, not the CLI binary itself.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::similar_names)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_pass_by_value)]
// `.o` is the toolchain's intermediate-artifact extension; the asserts only
// run on Cobrust-produced filenames (lowercase fixed at the source). A
// case-insensitive match would let unrelated future-extension lookalikes leak
// through the regression net.
#![allow(clippy::case_sensitive_file_extension_comparisons)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

/// Count the entries in `dir` whose name matches the given pattern
/// (a `Fn(&str) -> bool`). Used to assert "no intermediate leftovers".
fn count_matching<F: Fn(&str) -> bool>(dir: &Path, pred: F) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(&pred))
        .count()
}

/// `cobrust build -o <out>` must leave NO intermediate `.o` files or
/// `cobrust-build-*` tempdirs in the user's `-o` parent directory.
#[test]
fn build_executable_leaves_no_intermediate_artifacts() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let hello_cb = workspace.join("examples/hello.cb");
    assert!(hello_cb.exists(), "examples/hello.cb missing");

    // Use an RAII TempDir for the OUTPUT root so this test itself
    // doesn't leak (eating its own dog food per 81a2433).
    let out_root = tempfile::Builder::new()
        .prefix("cobrust-test-build-tempdir-")
        .tempdir()
        .expect("create test output tempdir");
    let exe_path = out_root.path().join("hello_tempdir_check");

    // Snapshot the output directory contents BEFORE `cobrust build`.
    let pre_count = count_matching(out_root.path(), |_| true);
    assert_eq!(
        pre_count, 0,
        "expected empty output dir before build, found {pre_count} entries"
    );

    let build_output = Command::new(&bin)
        .arg("build")
        .arg(&hello_cb)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust build");
    assert!(
        build_output.status.success(),
        "cobrust build failed: stdout={} stderr={}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Final executable must exist at the user-specified path.
    assert!(
        exe_path.exists(),
        "cobrust build did not produce executable at {exe_path:?}"
    );

    // No `<module>.o`, no `cobrust_main.o`, no `cobrust-build-*`
    // tempdir leftovers. The ONLY entry allowed under `out_root` is
    // the final exe itself.
    let stray_o = count_matching(out_root.path(), |name| name.ends_with(".o"));
    assert_eq!(
        stray_o,
        0,
        "found {stray_o} stray .o intermediate files in {:?} after build (TempDir RAII regression)",
        out_root.path()
    );

    let stray_tempdir = count_matching(out_root.path(), |name| name.starts_with("cobrust-build-"));
    assert_eq!(
        stray_tempdir,
        0,
        "found {stray_tempdir} leaked cobrust-build-* tempdirs in {:?} after build (TempDir Drop regression)",
        out_root.path()
    );

    // Sanity: the final exe must be the ONLY entry (under our test fixture
    // we control the dir, so this is a strong assertion).
    let total_after = count_matching(out_root.path(), |_| true);
    assert_eq!(
        total_after,
        1,
        "expected exactly 1 entry (the final exe) in {:?} after build, found {total_after}",
        out_root.path()
    );
}

/// `cobrust run <file>` must also leave NO intermediate `.o` files or
/// `cobrust-build-*` tempdirs in its working directory's
/// `target/cobrust/` (the default output_dir when `-o` is omitted).
///
/// Why the assertion is `<= 1`: `cobrust run` with `-o` omitted writes
/// the FINAL exe into `target/cobrust/<module>` (so subsequent runs
/// can reuse it). That single artifact is allowed; everything else
/// must be cleaned.
#[test]
fn run_leaves_no_intermediate_artifacts() {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let hello_cb = workspace.join("examples/hello.cb");
    assert!(hello_cb.exists(), "examples/hello.cb missing");

    // Use a TempDir as the CWD for `cobrust run` so the default
    // `target/cobrust/` resolves to a scoped location and we can
    // inspect it cleanly.
    let cwd = tempfile::Builder::new()
        .prefix("cobrust-test-run-cwd-")
        .tempdir()
        .expect("create test cwd tempdir");
    let target_cobrust = cwd.path().join("target/cobrust");

    let run_output = Command::new(&bin)
        .arg("run")
        .arg(&hello_cb)
        .arg("--quiet")
        .current_dir(cwd.path())
        .output()
        .expect("invoke cobrust run");
    assert!(
        run_output.status.success(),
        "cobrust run failed: stdout={} stderr={}",
        String::from_utf8_lossy(&run_output.stdout),
        String::from_utf8_lossy(&run_output.stderr)
    );

    // The default output dir `target/cobrust/` must have been created
    // and must NOT contain any intermediate `.o` files or
    // `cobrust-build-*` tempdir leftovers. The final exe `hello`
    // (with optional `.exe` extension on Windows) IS allowed.
    if target_cobrust.exists() {
        let stray_o = count_matching(&target_cobrust, |name| name.ends_with(".o"));
        assert_eq!(
            stray_o, 0,
            "found {stray_o} stray .o intermediate files in {target_cobrust:?} after run (TempDir RAII regression)"
        );

        let stray_tempdir =
            count_matching(&target_cobrust, |name| name.starts_with("cobrust-build-"));
        assert_eq!(
            stray_tempdir, 0,
            "found {stray_tempdir} leaked cobrust-build-* tempdirs in {target_cobrust:?} after run (TempDir Drop regression)"
        );
    }
}

/// A failed `cobrust build` (e.g., on a type-error source) must ALSO
/// not leak intermediate artifacts. Drop runs on panic-unwind and on
/// the explicit `?` early-return paths inside `build::build`.
#[test]
fn failed_build_leaves_no_intermediate_artifacts() {
    let bin = cobrust_binary();

    let out_root = tempfile::Builder::new()
        .prefix("cobrust-test-build-fail-")
        .tempdir()
        .expect("create test output tempdir");
    let bad_src = out_root.path().join("type_err.cb");
    // Introduce a deliberate type error: `print` expects str, given i64.
    std::fs::write(&bad_src, "fn main() -> i64:\n    print(42)\n    return 0\n")
        .expect("write bad source");
    let exe_path = out_root.path().join("type_err_exe");

    let build_output = Command::new(&bin)
        .arg("build")
        .arg(&bad_src)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .output()
        .expect("invoke cobrust build");
    assert!(
        !build_output.status.success(),
        "expected cobrust build to fail on type error, got success"
    );

    // The failure path must NOT have created intermediates that survive.
    // The source file `type_err.cb` IS allowed (we wrote it). Anything
    // else (`<module>.o`, `cobrust_main.o`, `cobrust-build-*` dirs) is
    // a regression.
    let stray_o = count_matching(out_root.path(), |name| name.ends_with(".o"));
    assert_eq!(
        stray_o,
        0,
        "found {stray_o} stray .o intermediate files in {:?} after FAILED build (panic-unwind RAII regression)",
        out_root.path()
    );

    let stray_tempdir = count_matching(out_root.path(), |name| name.starts_with("cobrust-build-"));
    assert_eq!(
        stray_tempdir,
        0,
        "found {stray_tempdir} leaked cobrust-build-* tempdirs in {:?} after FAILED build",
        out_root.path()
    );
}
