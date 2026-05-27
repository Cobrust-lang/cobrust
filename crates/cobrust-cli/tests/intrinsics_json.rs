//! v0.7.0 Stream Z.5 — end-to-end `.cb` source → compile → run tests for
//! the flat-fn `std.json` surface (`json_loads` / `json_dumps` /
//! `json_dumps_indent`).
//!
//! Confirms the FULL wiring that the Z.5 stdlib sprint could not exercise
//! on its own (it was fenced off `cobrust-codegen` during the F56 sprint):
//! source → frontend prelude stub → cli intrinsic-rewrite →
//! `__cobrust_json_*` extern (declared in `llvm_backend.rs`
//! `declare_runtime_helpers`) → `cobrust-stdlib::json` C-ABI shim → stdout.
//!
//! `@py_compat = semantic`: object keys re-emit in alphabetical order
//! (serde_json `BTreeMap`), not CPython insertion order; scalar/container
//! formatting + separators (`", "` / `": "`) match CPython 3.11 `json`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::process::Command;

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
    assert!(run.status.success(), "run failed: {:?}", run.status);
    String::from_utf8_lossy(&run.stdout).into_owned()
}

#[test]
fn test_e2e_json_loads_canonicalizes_and_sorts_keys() {
    let stdout = build_and_run_source(concat!(
        "fn main() -> i64:\n",
        "    let out: str = json_loads(\"{\\\"b\\\": 2, \\\"a\\\": 1}\")\n",
        "    print(out)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "{\"a\": 1, \"b\": 2}\n");
}

#[test]
fn test_e2e_json_dumps_compact_list_uses_cpython_separators() {
    let stdout = build_and_run_source(concat!(
        "fn main() -> i64:\n",
        "    let out: str = json_dumps(\"[1,2,3]\")\n",
        "    print(out)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "[1, 2, 3]\n");
}

#[test]
fn test_e2e_json_dumps_indent_pretty_prints() {
    let stdout = build_and_run_source(concat!(
        "fn main() -> i64:\n",
        "    let out: str = json_dumps_indent(\"{\\\"a\\\":1}\", 2)\n",
        "    print(out)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "{\n  \"a\": 1\n}\n");
}

#[test]
fn test_e2e_json_loads_malformed_returns_empty_sentinel() {
    let stdout = build_and_run_source(concat!(
        "fn main() -> i64:\n",
        "    let out: str = json_loads(\"{not valid\")\n",
        "    print(out)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "\n");
}
