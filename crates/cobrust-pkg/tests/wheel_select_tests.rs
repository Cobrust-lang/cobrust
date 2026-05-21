//! Integration tests for `cobrust_pkg::wheel_select` (ADR-0065 §3.3.2).

use cobrust_pkg::cpu_detect::HostCpu;
use cobrust_pkg::wheel_select::{COBRUST_ABI_VERSION, WheelMeta, select_wheel};

fn make(triple: &str, cpu_level: &str) -> WheelMeta {
    WheelMeta {
        filename: format!("cobrust-pkg-0.1.0-{triple}-{cpu_level}.tar.gz"),
        triple: triple.to_owned(),
        cpu_level: cpu_level.to_owned(),
        sha256: "0".repeat(64),
        cobrust_abi: "0.1".to_owned(),
        cobrust_abi_version: COBRUST_ABI_VERSION,
        experimental: false,
        size_bytes: 4096,
        download_url: format!("https://example/{triple}-{cpu_level}.tar.gz"),
    }
}

#[test]
fn exact_match_v3_picked_when_host_advertises_v3() {
    let host = HostCpu::X86_64 {
        v3: true,
        v4: false,
    };
    let wheels = vec![
        make("x86_64-unknown-linux-gnu", "v1"),
        make("x86_64-unknown-linux-gnu", "v3"),
    ];
    #[cfg(target_os = "linux")]
    {
        let chosen = select_wheel(&host, &wheels, false).expect("expected a wheel");
        assert_eq!(chosen.cpu_level, "v3");
    }
    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux hosts the canonical X86_64 triple is different; the
        // selector returns an error because no wheel matches the host triple.
        // This is the desired contract.
        let _ = (host, wheels);
    }
}

#[test]
fn fallback_to_baseline_when_higher_tier_missing() {
    let host = HostCpu::X86_64 {
        v3: true,
        v4: false,
    };
    let triple = if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let wheels = vec![make(triple, "v1")];
    let chosen = select_wheel(&host, &wheels, false).expect("baseline fallback should match");
    assert_eq!(chosen.cpu_level, "v1");
}

#[test]
fn no_wheels_for_host_triple_returns_err() {
    let host = HostCpu::X86_64 {
        v3: true,
        v4: false,
    };
    let wheels = vec![make("aarch64-apple-darwin", "m1")];
    assert!(select_wheel(&host, &wheels, false).is_err());
}

#[test]
fn multi_tier_preference_picks_highest_available() {
    let host = HostCpu::X86_64 { v3: true, v4: true };
    let triple = if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let wheels = vec![make(triple, "v1"), make(triple, "v3"), make(triple, "v4")];
    let chosen = select_wheel(&host, &wheels, false).expect("expected a wheel");
    assert_eq!(chosen.cpu_level, "v4");
}

#[test]
fn apple_silicon_m1_specific_match() {
    let host = HostCpu::Aarch64 {
        sve: false,
        apple_m1: true,
        apple_m2: false,
    };
    let wheels = vec![make("aarch64-apple-darwin", "m1")];
    let chosen = select_wheel(&host, &wheels, false).expect("expected a wheel");
    assert_eq!(chosen.cpu_level, "m1");
    assert_eq!(chosen.triple, "aarch64-apple-darwin");
}
