//! 3-CPU-class install round-trip smoke tests (ADR-0065 §7.4 W4).
//!
//! Uses a mock registry (in-memory `wheels.json`) and mock CPU detection to
//! exercise the three critical scenarios:
//!
//! 1. `install_v3_on_v3_host_succeeds` — a Haswell (x86-64-v3) host gets the
//!    v3 wheel, not v1.
//! 2. `install_v3_on_v1_host_falls_back_v1` — a baseline (x86-64-v1) host
//!    cannot run v3; falls back to v1.
//! 3. `install_sve_requires_allow_experimental` — a Graviton-3 (aarch64+SVE)
//!    host wants SVE but the `--allow-experimental` flag is missing ->
//!    `SelectError::ExperimentalNotAllowed`.
//!
//! All tests are purely in-process (no disk writes, no actual HTTP).

use cobrust_pkg::cpu_detect::HostCpu;
use cobrust_pkg::wheel_select::{COBRUST_ABI_VERSION, SelectError, WheelMeta, select_wheel};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Platform-specific triple for x86_64 (mirrors `canonical_x86_64_triple`).
#[cfg(target_os = "linux")]
const X86_64_TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(target_os = "macos")]
const X86_64_TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const X86_64_TRIPLE: &str = "x86_64-unknown-linux-gnu";

/// Build a WheelMeta with sensible defaults for the given triple + cpu_level.
fn wheel(triple: &str, cpu_level: &str) -> WheelMeta {
    WheelMeta {
        filename: format!("cobrust-hello-0.4.0-{triple}-{cpu_level}.tar.gz"),
        triple: triple.to_owned(),
        cpu_level: cpu_level.to_owned(),
        sha256: "a".repeat(64),
        cobrust_abi: "0.1".to_owned(),
        cobrust_abi_version: COBRUST_ABI_VERSION,
        experimental: false,
        size_bytes: 4_000_000,
        download_url: format!("https://example.com/{triple}-{cpu_level}.tar.gz"),
    }
}

fn wheel_experimental(triple: &str, cpu_level: &str) -> WheelMeta {
    WheelMeta {
        experimental: true,
        ..wheel(triple, cpu_level)
    }
}

// ── test 1: v3 host gets v3 wheel ────────────────────────────────────────────

/// A Haswell (v3) x86-64 host should receive the v3 wheel, not v1.
///
/// Registry has v1, v3, v4 available. Host reports v3 support only (no AVX-512).
#[test]
fn install_v3_on_v3_host_succeeds() {
    let host = HostCpu::X86_64 {
        v3: true,
        v4: false,
    };

    // Registry: three variants available; use the platform-canonical triple
    // so the selector finds a match regardless of whether the test runs on
    // Linux or macOS.
    let registry: Vec<WheelMeta> = vec![
        wheel(X86_64_TRIPLE, "v1"),
        wheel(X86_64_TRIPLE, "v3"),
        wheel(X86_64_TRIPLE, "v4"),
    ];

    let chosen = select_wheel(&host, &registry, false).expect("v3 host must select a wheel");

    assert_eq!(
        chosen.cpu_level, "v3",
        "v3 host must select v3 wheel (not fall back to v1 or escalate to v4)"
    );
    assert!(!chosen.experimental, "v3 wheel must not be experimental");
    assert_eq!(chosen.cobrust_abi_version, COBRUST_ABI_VERSION);
}

// ── test 2: baseline v1 host falls back from v3 ──────────────────────────────

/// A baseline (v1-only) x86-64 host cannot run a v3 wheel; it must fall back
/// to the v1 wheel.
///
/// Registry: only v3 and v1 available. Host reports no AVX2 / AVX-512.
#[test]
fn install_v3_on_v1_host_falls_back_v1() {
    let host = HostCpu::X86_64 {
        v3: false,
        v4: false,
    };

    // Registry: v1 and v3 variants; no v4.
    let registry: Vec<WheelMeta> = vec![wheel(X86_64_TRIPLE, "v1"), wheel(X86_64_TRIPLE, "v3")];

    let chosen = select_wheel(&host, &registry, false).expect("v1 host must select a wheel");

    assert_eq!(
        chosen.cpu_level, "v1",
        "v1-only host must fall back to v1 wheel, not incorrectly select v3"
    );
    assert!(!chosen.experimental);
    assert_eq!(chosen.cobrust_abi_version, COBRUST_ABI_VERSION);
}

// ── test 3: SVE requires --allow-experimental ────────────────────────────────

/// A Graviton-3 (aarch64 + SVE) host wants the SVE wheel, but SVE is
/// experimental. Without `--allow-experimental`, `select_wheel` must return
/// `SelectError::ExperimentalNotAllowed`.
///
/// With the flag set, the SVE wheel IS selected.
#[test]
fn install_sve_requires_allow_experimental() {
    let host = HostCpu::Aarch64 {
        sve: true,
        apple_m1: false,
        apple_m2: false,
    };

    // Registry: only SVE available for this triple (Graviton-3 niche).
    let registry: Vec<WheelMeta> = vec![wheel_experimental("aarch64-unknown-linux-gnu", "sve")];

    // Without flag: must error.
    let err_result = select_wheel(&host, &registry, false);
    assert_eq!(
        err_result,
        Err(SelectError::ExperimentalNotAllowed),
        "SVE wheel without --allow-experimental must return ExperimentalNotAllowed"
    );

    // With flag: must succeed and return the SVE wheel.
    let ok_result = select_wheel(&host, &registry, true).expect("SVE wheel with flag must succeed");
    assert_eq!(ok_result.cpu_level, "sve");
    assert!(
        ok_result.experimental,
        "SVE wheel must be marked experimental"
    );
}

// ── bonus: neon fallback when both neon+sve available, no flag ───────────────

/// When both neon (stable) and SVE (experimental) are available and
/// `--allow-experimental` is not set, the stable neon wheel wins.
#[test]
fn install_graviton3_prefers_neon_over_experimental_sve_without_flag() {
    let host = HostCpu::Aarch64 {
        sve: true,
        apple_m1: false,
        apple_m2: false,
    };

    let registry: Vec<WheelMeta> = vec![
        wheel("aarch64-unknown-linux-gnu", "neon"),
        wheel_experimental("aarch64-unknown-linux-gnu", "sve"),
    ];

    let chosen =
        select_wheel(&host, &registry, false).expect("neon fallback must succeed without flag");

    assert_eq!(
        chosen.cpu_level, "neon",
        "stable neon wheel must win over experimental SVE when flag not set"
    );
    assert!(!chosen.experimental);
}
