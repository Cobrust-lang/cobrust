//! ADR-0075 Phase 2 Sprint D — wasm32-wasip1 cross-compile E2E.
//!
//! Mirrors [`cross_compile_riscv64_e2e`] for the wasm32-wasip1 target:
//! gated cleanly on toolchain availability per F59-style discipline.
//! When ANY of the required cross-toolchain pieces are missing the test
//! prints a single-line skip note and returns success. Safe to commit +
//! run on a clean dev box (macOS without `wasmtime` installed); CI jobs
//! with the toolchain installed exercise it for real.
//!
//! Required toolchain (see `docs/agent/setup/cross-toolchain.md`):
//! - `rustup target add wasm32-wasip1`
//! - A wasi-libc sysroot (ADR-0075 Phase 2 Sprint E): apt's `clang-18`
//!   does NOT bundle one, so a real wasi-sdk is required. Install
//!   wasi-sdk and set `$WASI_SDK_PATH` (sysroot auto-derived at
//!   `<SDK>/share/wasi-sysroot`) OR set `$COBRUST_WASI_SYSROOT` directly.
//!   Pin the cross-cc to the SDK's bundled clang via
//!   `$COBRUST_CC_WASM32_WASIP1=<SDK>/bin/clang` (version-matched to the
//!   sysroot); plain `clang` / `clang-18` also work IF a sysroot is set.
//! - `wasmtime` on PATH (Linux: `cargo install wasmtime-cli --locked`;
//!   macOS: `brew install wasmtime`).
//!
//! What the test exercises:
//! 1. Writes a "hello cobrust wasm32" `.cb` source (F67-style: source
//!    wraps `print(...)` in `fn main() -> i64:` so the codegen emits the
//!    `_cobrust_user_main` symbol the C runtime shim links against).
//! 2. Runs the cobrust CLI with
//!    `cobrust build --target=wasm32-wasip1 prog.cb -o prog.wasm`.
//! 3. Runs the produced `.wasm` module under `wasmtime`.
//! 4. Asserts stdout contains "hello cobrust wasm32".

use std::path::PathBuf;
use std::process::Command;

const TARGET_TRIPLE: &str = "wasm32-wasip1";

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

/// ADR-0075 Phase 2 Sprint E — returns `true` when a wasi-libc sysroot
/// is discoverable via the same env vars `build.rs::resolve_wasi_sysroot`
/// reads: `$COBRUST_WASI_SYSROOT` (direct) or `$WASI_SDK_PATH` (sysroot
/// auto-derived at `<SDK>/share/wasi-sysroot`). Apt's `clang-18` does NOT
/// bundle one, so without this the live build fails at the cross-cc step
/// with `bits/libc-header-start.h file not found`. Gating on it keeps the
/// macOS dev host (no wasi-sdk) skipping cleanly.
fn wasi_sysroot_available() -> bool {
    if let Ok(p) = std::env::var("COBRUST_WASI_SYSROOT")
        && !p.is_empty()
        && std::path::Path::new(&p).is_dir()
    {
        return true;
    }
    if let Ok(sdk) = std::env::var("WASI_SDK_PATH")
        && !sdk.is_empty()
        && std::path::Path::new(&sdk)
            .join("share")
            .join("wasi-sysroot")
            .is_dir()
    {
        return true;
    }
    false
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
/// `cross_compile_riscv64_e2e.rs` so the test works whether `cargo test`
/// was invoked in debug or release mode.
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
fn cross_compile_wasm32_hello() {
    // ---- skip-gate ------------------------------------------------------
    if !rust_target_installed(TARGET_TRIPLE) {
        eprintln!(
            "cross_compile_wasm32_e2e: skipping cleanly: \
             rustup target `{TARGET_TRIPLE}` not installed. \
             Run `rustup target add {TARGET_TRIPLE}` to enable."
        );
        return;
    }
    // The C shim + final link need clang (LLVM-18 driver). Either
    // `clang-18`, plain `clang`, OR a user-set `$CC` / `$COBRUST_CC_<TRIPLE>` env.
    let has_cross_cc = binary_available("clang-18")
        || binary_available("clang")
        || std::env::var("CC").map(|v| !v.is_empty()).unwrap_or(false)
        || std::env::var("COBRUST_CC_WASM32_WASIP1")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
    if !has_cross_cc {
        eprintln!(
            "cross_compile_wasm32_e2e: skipping cleanly: \
             no wasm C compiler found (tried `clang-18`, `clang`, $CC, \
             $COBRUST_CC_WASM32_WASIP1). \
             Install LLVM 18+ (see docs/agent/setup/cross-toolchain.md)."
        );
        return;
    }
    // ADR-0075 Phase 2 Sprint E — a wasi-libc sysroot is mandatory; apt's
    // `clang-18` doesn't ship one. Without `$WASI_SDK_PATH` /
    // `$COBRUST_WASI_SYSROOT` the cross-cc would fail with
    // `bits/libc-header-start.h file not found`. Skip cleanly here (matches
    // the macOS dev host, which has no wasi-sdk) instead of hitting a hard
    // error mid-build.
    if !wasi_sysroot_available() {
        eprintln!(
            "cross_compile_wasm32_e2e: skipping cleanly: \
             no wasi-libc sysroot found (set $WASI_SDK_PATH or \
             $COBRUST_WASI_SYSROOT). Install wasi-sdk \
             (see docs/agent/setup/cross-toolchain.md §6)."
        );
        return;
    }
    if !binary_available("wasmtime") {
        eprintln!(
            "cross_compile_wasm32_e2e: skipping cleanly: \
             `wasmtime` not on PATH. \
             Install via `cargo install wasmtime-cli --locked` (Linux/CI) \
             or `brew install wasmtime` (macOS)."
        );
        return;
    }

    let Some(cobrust) = locate_cobrust_binary() else {
        eprintln!(
            "cross_compile_wasm32_e2e: skipping cleanly: \
             `cobrust` binary not located under target/{{debug,release}}/. \
             Test invocations should run after `cargo build -p cobrust-cli`."
        );
        return;
    };

    // ---- live path ------------------------------------------------------
    let tmp = tempfile::tempdir().expect("tempdir for cross E2E");
    let src_path = tmp.path().join("hello_wasm.cb");
    let out_path = tmp.path().join("hello_wasm.wasm");
    // F67: source MUST declare `fn main` — codegen only emits the
    // `_cobrust_user_main` symbol for bare-name `main`. Module-level
    // `print(...)` lowers to `_cobrust_init_<n>` (see
    // `cobrust-codegen/src/llvm_backend.rs:3221-3229`) which the C
    // runtime shim never calls, leaving `_cobrust_user_main` undefined
    // at link time. Same wrapping discipline as
    // `cross_compile_riscv64_e2e.rs:130-138` and `examples/hello.cb`.
    std::fs::write(
        &src_path,
        "fn main() -> i64:\n    print(\"hello cobrust wasm32\")\n    return 0\n",
    )
    .expect("write hello_wasm.cb");

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

    // ---- run under wasmtime ---------------------------------------------
    // wasmtime needs no sysroot flag; the .wasm module is self-contained
    // and WASI imports are bound from the host. `wasmtime run <bin.wasm>`
    // suffices.
    let run_out = Command::new("wasmtime")
        .arg("run")
        .arg(&out_path)
        .output()
        .expect("spawn wasmtime");
    assert!(
        run_out.status.success(),
        "wasmtime run failed (status {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        run_out.status,
        String::from_utf8_lossy(&run_out.stdout),
        String::from_utf8_lossy(&run_out.stderr),
    );
    let stdout = String::from_utf8_lossy(&run_out.stdout);
    assert!(
        stdout.contains("hello cobrust wasm32"),
        "expected stdout to contain `hello cobrust wasm32`; got: {stdout:?}"
    );
}
