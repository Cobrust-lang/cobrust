//! Integration tests for `cobrust_pkg::cpu_detect` (ADR-0065 §3.3.1).

use cobrust_pkg::cpu_detect::{HostCpu, detect_host_cpu};

#[test]
fn detect_host_cpu_returns_variant_matching_runtime_arch() {
    let cpu = detect_host_cpu();
    match std::env::consts::ARCH {
        "x86_64" => assert!(
            matches!(cpu, HostCpu::X86_64 { .. }),
            "expected X86_64 variant, got {cpu:?}"
        ),
        "aarch64" => assert!(
            matches!(cpu, HostCpu::Aarch64 { .. }),
            "expected Aarch64 variant, got {cpu:?}"
        ),
        _ => assert!(
            matches!(cpu, HostCpu::Unknown),
            "expected Unknown variant, got {cpu:?}"
        ),
    }
}

#[test]
fn preferred_cpu_level_is_valid_wheel_suffix() {
    // The wheel-suffix returned by `preferred_cpu_level` must be one of the
    // tags listed in ADR-0065 §3.1. This guards against future drift.
    let suffix = detect_host_cpu().preferred_cpu_level();
    assert!(
        matches!(suffix, "v1" | "v3" | "v4" | "neon" | "sve" | "m1" | "m2"),
        "preferred_cpu_level returned unknown wheel suffix: {suffix}"
    );
}

#[test]
fn baseline_fallbacks_always_safe_to_run() {
    // §3.3.1 invariant: the baseline-tier wheel (v1 / neon / m1) runs on all
    // hardware of the respective arch. We sanity-check the public mapping is
    // never the empty string or the wrong tier for an empty-feature CPU.
    let baseline_x86 = HostCpu::X86_64 {
        v3: false,
        v4: false,
    }
    .preferred_cpu_level();
    assert_eq!(baseline_x86, "v1");
    let baseline_arm = HostCpu::Aarch64 {
        sve: false,
        apple_m1: false,
        apple_m2: false,
    }
    .preferred_cpu_level();
    assert_eq!(baseline_arm, "neon");
    let unknown_baseline = HostCpu::Unknown.preferred_cpu_level();
    assert_eq!(unknown_baseline, "v1");
}
