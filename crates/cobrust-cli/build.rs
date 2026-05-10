//! Cobrust CLI build script.
//!
//! Responsibilities (T1.3 install-path):
//!
//! 1. Build `cobrust-stdlib` as a static archive (`libcobrust_stdlib.a`)
//!    so that `cargo install cobrust-cli` produces a self-contained binary
//!    without requiring a separate `cargo build -p cobrust-stdlib` step.
//! 2. Emit `cargo:rustc-env=COBRUST_STDLIB_ARCHIVE_PATH=<path>` so the
//!    runtime `locate_stdlib_archive` function can use the compiled-in
//!    path regardless of the install layout.
//! 3. Compile `runtime/cobrust_main.c` into an object and emit
//!    `cargo:rustc-env=COBRUST_RUNTIME_OBJ_PATH=<path>` for the same
//!    reason.
//!
//! NOTE: `cargo install` runs the build script in the build environment
//! and the resulting paths are baked into the binary at compile-time via
//! `option_env!("COBRUST_STDLIB_ARCHIVE_PATH")`. This means the binary
//! always finds the right files on the install machine.

#![allow(clippy::manual_assert)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set by cargo"));
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));

    // -----------------------------------------------------------------------
    // 1. Build cobrust-stdlib static archive into OUT_DIR.
    // -----------------------------------------------------------------------
    let stdlib_archive = build_stdlib_archive(&out_dir, &manifest_dir);

    // -----------------------------------------------------------------------
    // 2. Compile the C runtime shim into OUT_DIR.
    // -----------------------------------------------------------------------
    let runtime_obj = compile_runtime_shim(&out_dir, &manifest_dir);

    // -----------------------------------------------------------------------
    // 3. Emit rustc-env vars for the binary to find at runtime.
    // -----------------------------------------------------------------------
    println!(
        "cargo:rustc-env=COBRUST_STDLIB_ARCHIVE_PATH={}",
        stdlib_archive.display()
    );
    println!(
        "cargo:rustc-env=COBRUST_RUNTIME_OBJ_PATH={}",
        runtime_obj.display()
    );

    // Re-run if the stdlib source or runtime C shim changes.
    println!("cargo:rerun-if-changed=../cobrust-stdlib/src");
    println!("cargo:rerun-if-changed=runtime/cobrust_main.c");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
}

/// Build `cobrust-stdlib` as a static archive using `cargo build` and
/// return the path to `libcobrust_stdlib.a` inside `out_dir`.
fn build_stdlib_archive(out_dir: &Path, manifest_dir: &Path) -> PathBuf {
    // Workspace root is two directories above the CLI crate manifest.
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root must be two levels above crates/cobrust-cli");

    let target_dir = out_dir.join("cobrust-stdlib-build");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(workspace_root)
        .arg("build")
        .arg("-p")
        .arg("cobrust-stdlib")
        .arg("--target-dir")
        .arg(&target_dir);
    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd
        .status()
        .expect("cargo build -p cobrust-stdlib failed to start");
    assert!(
        status.success(),
        "cargo build -p cobrust-stdlib failed with status: {status:?}"
    );

    let archive_path = target_dir.join(&profile).join("libcobrust_stdlib.a");
    assert!(
        archive_path.exists(),
        "libcobrust_stdlib.a not found at {} after build",
        archive_path.display()
    );
    archive_path
}

/// Compile `runtime/cobrust_main.c` into `cobrust_main.o` inside `out_dir`.
fn compile_runtime_shim(out_dir: &Path, manifest_dir: &Path) -> PathBuf {
    let runtime_src = manifest_dir.join("runtime/cobrust_main.c");
    assert!(
        runtime_src.exists(),
        "runtime/cobrust_main.c not found at {}",
        runtime_src.display()
    );

    let runtime_obj = out_dir.join("cobrust_main.o");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = Command::new(&cc)
        .arg("-c")
        .arg(&runtime_src)
        .arg("-O0")
        .arg("-o")
        .arg(&runtime_obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke {cc}: {e}"));
    assert!(
        status.success(),
        "runtime shim compilation failed: {status:?}"
    );
    runtime_obj
}
