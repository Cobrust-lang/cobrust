//! ADR-0075 Phase 1 Sprint A — riscv64gc-unknown-linux-gnu cross-compile E2E.
//!
//! Gated cleanly on toolchain availability per F59-style discipline:
//! when ANY of the required cross-toolchain pieces are missing the test
//! prints a single-line skip note and returns success. So this file is
//! safe to commit + run on a clean dev box (macOS without RV cross-cc
//! installed); CI jobs with the toolchain installed exercise it for real.
//!
//! Required toolchain (see `docs/agent/setup/cross-toolchain.md`):
//! - `rustup target add riscv64gc-unknown-linux-gnu`
//! - `riscv64-linux-gnu-gcc` on PATH (Debian apt `gcc-riscv64-linux-gnu`)
//!   OR a working `clang` plus a sysroot the user wires via $CC.
//! - `qemu-riscv64` on PATH (Debian apt `qemu-user-static`,
//!   Homebrew `qemu`).
//!
//! What the test exercises:
//! 1. Writes a "hello cobrust riscv64" `.cb` source.
//! 2. Runs the cobrust CLI with
//!    `cobrust build --target=riscv64gc-unknown-linux-gnu prog.cb -o prog`.
//! 3. Runs the produced ELF under `qemu-riscv64`.
//! 4. Asserts stdout contains "hello cobrust riscv64".

use std::path::PathBuf;
use std::process::Command;

const TARGET_TRIPLE: &str = "riscv64gc-unknown-linux-gnu";

/// Returns `true` when the named binary responds to `--version` (i.e.
/// is on PATH and executable).
fn binary_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Returns `true` when `rustup target list --installed` reports the
/// target as installed.
fn rust_target_installed(triple: &str) -> bool {
    let Ok(output) = Command::new("rustup")
        .arg("target")
        .arg("list")
        .arg("--installed")
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.lines().any(|l| l.trim() == triple)
}

/// Locate the freshly-built `cobrust` CLI binary inside the workspace
/// `target/<profile>/` directory. Mirrors the lookup pattern from
/// `cli_smoke.rs` etc. so the test works whether `cargo test` was
/// invoked in debug or release mode.
fn locate_cobrust_binary() -> Option<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = std::path::Path::new(manifest_dir).parent()?.parent()?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace.join("target"), PathBuf::from);
    for profile in ["debug", "release"] {
        let candidate = target_dir.join(profile).join("cobrust");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[test]
fn cross_compile_riscv64_hello() {
    // ---- skip-gate ------------------------------------------------------
    if !rust_target_installed(TARGET_TRIPLE) {
        eprintln!(
            "cross_compile_riscv64_e2e: skipping cleanly: \
             rustup target `{TARGET_TRIPLE}` not installed. \
             Run `rustup target add {TARGET_TRIPLE}` to enable."
        );
        return;
    }
    // The C shim + final link need a cross-cc. Either the gnu prefix,
    // or `clang`, OR a user-set `$CC` / `$COBRUST_CC_<TRIPLE>` env.
    let has_cross_cc = binary_available("riscv64-linux-gnu-gcc")
        || binary_available("clang")
        || std::env::var("CC").map(|v| !v.is_empty()).unwrap_or(false)
        || std::env::var("COBRUST_CC_RISCV64GC_UNKNOWN_LINUX_GNU")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
    if !has_cross_cc {
        eprintln!(
            "cross_compile_riscv64_e2e: skipping cleanly: \
             no cross C compiler found (tried `riscv64-linux-gnu-gcc`, \
             `clang`, $CC, $COBRUST_CC_RISCV64GC_UNKNOWN_LINUX_GNU). \
             See docs/agent/setup/cross-toolchain.md."
        );
        return;
    }
    if !binary_available("qemu-riscv64") {
        eprintln!(
            "cross_compile_riscv64_e2e: skipping cleanly: \
             `qemu-riscv64` not on PATH. \
             Install via `apt-get install qemu-user-static` (Linux) \
             or `brew install qemu` (macOS)."
        );
        return;
    }

    let Some(cobrust) = locate_cobrust_binary() else {
        eprintln!(
            "cross_compile_riscv64_e2e: skipping cleanly: \
             `cobrust` binary not located under target/{{debug,release}}/. \
             Test invocations should run after `cargo build -p cobrust-cli`."
        );
        return;
    };

    // ---- live path ------------------------------------------------------
    let tmp = tempfile::tempdir().expect("tempdir for cross E2E");
    let src_path = tmp.path().join("hello_rv.cb");
    let out_path = tmp.path().join("hello_rv");
    // F67: source MUST declare `fn main` — codegen only emits the
    // `_cobrust_user_main` symbol for bare-name `main`. Module-level
    // `print(...)` lowers to `_cobrust_init_<n>` (see
    // `cobrust-codegen/src/llvm_backend.rs:3221-3229`) which the C
    // runtime shim never calls, leaving `_cobrust_user_main` undefined
    // at link time. Same wrapping discipline as
    // `ecosystem_den_e2e.rs:18-19` and `examples/hello.cb`.
    std::fs::write(
        &src_path,
        "fn main() -> i64:\n    print(\"hello cobrust riscv64\")\n    return 0\n",
    )
    .expect("write hello_rv.cb");

    let mut build_cmd = Command::new(&cobrust);
    build_cmd
        .arg("build")
        .arg("--target")
        .arg(TARGET_TRIPLE)
        .arg("--quiet")
        .arg("-o")
        .arg(&out_path)
        .arg(&src_path);
    // Force LLVM-prefix env through to the subprocess (the workspace
    // build.rs also passes through CC).
    if let Ok(v) = std::env::var("LLVM_SYS_181_PREFIX") {
        build_cmd.env("LLVM_SYS_181_PREFIX", v);
    }
    let build_out = build_cmd.output().expect("spawn cobrust build");
    assert!(
        build_out.status.success(),
        "cobrust build --target={TARGET_TRIPLE} failed (status {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        build_out.status,
        String::from_utf8_lossy(&build_out.stdout),
        String::from_utf8_lossy(&build_out.stderr),
    );
    assert!(
        out_path.exists(),
        "build claimed success but `{}` doesn't exist",
        out_path.display()
    );

    // ---- run under qemu -------------------------------------------------
    // `-L /usr/riscv64-linux-gnu` lets qemu find the dynamic linker. On
    // setups without the sysroot at /usr the variable is harmless (qemu
    // ignores missing -L dirs). Override via $COBRUST_QEMU_RV_SYSROOT
    // when CI installs the sysroot elsewhere.
    let sysroot = std::env::var("COBRUST_QEMU_RV_SYSROOT")
        .unwrap_or_else(|_| "/usr/riscv64-linux-gnu".to_string());
    let run_out = Command::new("qemu-riscv64")
        .arg("-L")
        .arg(&sysroot)
        .arg(&out_path)
        .output()
        .expect("spawn qemu-riscv64");
    assert!(
        run_out.status.success(),
        "qemu-riscv64 run failed (status {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        run_out.status,
        String::from_utf8_lossy(&run_out.stdout),
        String::from_utf8_lossy(&run_out.stderr),
    );
    let stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        stdout.contains("hello cobrust riscv64"),
        "expected stdout to contain `hello cobrust riscv64`; got: {stdout:?}"
    );
}
