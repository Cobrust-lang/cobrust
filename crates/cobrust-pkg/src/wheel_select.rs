//! Wheel selection — pick the best matching wheel for a host CPU per
//! ADR-0065 §3.3.2 (selection algorithm).
//!
//! Inputs:
//!   - [`HostCpu`] — the detected host (from `cpu_detect`).
//!   - A slice of [`WheelMeta`] entries (from `registry_client::fetch_index`).
//!
//! Output:
//!   - `Option<&WheelMeta>` — the highest-tier wheel compatible with the host
//!     triple, falling back to the baseline (v1 / neon / m1) if no higher tier
//!     matches. `None` only when the registry has zero entries for the host
//!     triple (a hard "package not built for your platform" error).
//!
//! # ABI version check (ADR-0065 §6.4)
//!
//! `WheelMeta::cobrust_abi_version` must equal [`COBRUST_ABI_VERSION`] before a
//! wheel is considered a valid candidate. Wheels with a mismatching ABI version
//! are filtered out by [`select_wheel`] before the tier-priority pass.

use serde::{Deserialize, Serialize};

use crate::cpu_detect::HostCpu;

/// ABI version this build of `cobrust-pkg` speaks.
///
/// Increment this constant (and bump the `cobrust_abi_version` field in any
/// freshly generated wheel index) whenever the wheel binary interface changes
/// in a backward-incompatible way. Wheels whose `cobrust_abi_version` differs
/// from this constant are rejected by [`select_wheel`] before the tier-priority
/// pass (ADR-0065 §6.4).
pub const COBRUST_ABI_VERSION: u32 = 1;

/// Metadata describing one wheel artifact in the registry index.
///
/// Mirrors the ADR-0065 §3.4 wheel index entry shape. Field names are the
/// JSON wire names (snake_case) per the §3.4 JSON sample.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WheelMeta {
    /// Wheel archive filename (e.g. `cobrust-numpy-0.1.0-x86_64-...-v3.tar.gz`).
    pub filename: String,
    /// Target triple this wheel targets (e.g. `x86_64-unknown-linux-gnu`).
    pub triple: String,
    /// CPU level suffix (e.g. `v3`, `neon`, `m1`).
    pub cpu_level: String,
    /// SHA-256 of the wheel payload, lowercase hex.
    pub sha256: String,
    /// `cobrust_abi` semver-major tag (e.g. `"0.1"`).
    pub cobrust_abi: String,
    /// Numeric ABI version for dependency-closure compatibility check
    /// (ADR-0065 §6.4). Default for v0.4.0 wheels is `1`.
    /// Wheels whose value differs from [`COBRUST_ABI_VERSION`] are
    /// rejected before the tier-priority pass.
    #[serde(default = "default_cobrust_abi_version")]
    pub cobrust_abi_version: u32,
    /// If `true` this wheel is experimental and must not be installed without
    /// `--allow-experimental` (ADR-0065 §3.1 / §6.5; SVE wheels are
    /// experimental until SVE ABI is declared stable).
    #[serde(default)]
    pub experimental: bool,
    /// Size of the wheel archive in bytes.
    pub size_bytes: u64,
    /// HTTP(S) URL to download the wheel from.
    pub download_url: String,
}

/// Serde default for `cobrust_abi_version`: old indexes without this field
/// are treated as version 1 (the initial stable ABI for v0.4.0 wheels).
fn default_cobrust_abi_version() -> u32 {
    1
}

/// Error returned when [`select_wheel`] cannot find a compatible wheel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectError {
    /// No wheel exists for the host triple at all.
    NoWheelForTriple,
    /// Wheel exists for the host triple but all candidates have an ABI version
    /// incompatible with [`COBRUST_ABI_VERSION`].
    AbiVersionMismatch {
        /// The mismatching ABI version found in the index.
        found: u32,
    },
    /// The best available wheel is experimental but `--allow-experimental` was
    /// not passed.
    ExperimentalNotAllowed,
}

/// Pick the best wheel for `host` from `available`.
///
/// `allow_experimental` mirrors the `--allow-experimental` CLI flag.
///
/// Algorithm (ADR-0065 §3.3.2 steps 4–6 + W4 ABI + SVE rules):
/// 1. Filter `available` to entries whose `triple` matches the host triple.
/// 2. Filter to entries whose `cobrust_abi_version == COBRUST_ABI_VERSION`.
/// 3. Among the remaining entries, prefer non-experimental wheels of the
///    highest-tier match for the host; fall back to experimental only when
///    `allow_experimental` is `true` and no stable wheel matches.
/// 4. Return `Err` if the filtered set is empty.
///
/// # Errors
/// Returns [`SelectError`] when no compatible wheel can be chosen.
pub fn select_wheel<'a>(
    host: &HostCpu,
    available: &'a [WheelMeta],
    allow_experimental: bool,
) -> Result<&'a WheelMeta, SelectError> {
    let triple = host_triple(host).ok_or(SelectError::NoWheelForTriple)?;

    // Step 1: filter by triple.
    let by_triple: Vec<&WheelMeta> = available.iter().filter(|w| w.triple == triple).collect();
    if by_triple.is_empty() {
        return Err(SelectError::NoWheelForTriple);
    }

    // Step 2: filter by ABI version.
    let abi_ok: Vec<&WheelMeta> = by_triple
        .iter()
        .copied()
        .filter(|w| w.cobrust_abi_version == COBRUST_ABI_VERSION)
        .collect();
    if abi_ok.is_empty() {
        // Report the first mismatching ABI version found.
        let found = by_triple[0].cobrust_abi_version;
        return Err(SelectError::AbiVersionMismatch { found });
    }

    // Step 3a: stable candidates (non-experimental).
    let stable: Vec<&WheelMeta> = abi_ok.iter().copied().filter(|w| !w.experimental).collect();

    let priority = cpu_level_priority(host);

    // Try stable candidates first.
    for tier in &priority {
        if let Some(w) = stable.iter().find(|w| &w.cpu_level == tier) {
            return Ok(*w);
        }
    }

    // No stable match found — if experimental is allowed, try experimental.
    if allow_experimental {
        for tier in &priority {
            if let Some(w) = abi_ok.iter().find(|w| &w.cpu_level == tier) {
                return Ok(*w);
            }
        }
        // Last-resort: return any ABI-compatible wheel.
        return abi_ok.first().copied().ok_or(SelectError::NoWheelForTriple);
    }

    // Experimental would be the only match but flag not set.
    if abi_ok.iter().any(|w| w.experimental) {
        return Err(SelectError::ExperimentalNotAllowed);
    }

    // Fallback: last-resort stable.
    stable.first().copied().ok_or(SelectError::NoWheelForTriple)
}

/// Map [`HostCpu`] to the canonical target-triple string used in wheel names.
fn host_triple(host: &HostCpu) -> Option<&'static str> {
    match host {
        HostCpu::X86_64 { .. } => Some(canonical_x86_64_triple()),
        HostCpu::Aarch64 { apple_m1: true, .. } | HostCpu::Aarch64 { apple_m2: true, .. } => {
            Some("aarch64-apple-darwin")
        }
        HostCpu::Aarch64 { .. } => Some("aarch64-unknown-linux-gnu"),
        HostCpu::Unknown => None,
    }
}

#[cfg(target_os = "linux")]
const fn canonical_x86_64_triple() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

#[cfg(target_os = "macos")]
const fn canonical_x86_64_triple() -> &'static str {
    "x86_64-apple-darwin"
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const fn canonical_x86_64_triple() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

/// Return the priority list of cpu_level strings to consult for `host`,
/// highest tier first. The final element is always the architecture's
/// baseline so callers always reach a fallback.
fn cpu_level_priority(host: &HostCpu) -> Vec<&'static str> {
    match host {
        HostCpu::X86_64 { v4: true, .. } => vec!["v4", "v3", "v1"],
        HostCpu::X86_64 { v3: true, .. } => vec!["v3", "v1"],
        HostCpu::X86_64 { .. } => vec!["v1"],
        HostCpu::Aarch64 { sve: true, .. } => vec!["sve", "neon"],
        HostCpu::Aarch64 { apple_m2: true, .. } => vec!["m2", "m1"],
        HostCpu::Aarch64 { apple_m1: true, .. } => vec!["m1"],
        HostCpu::Aarch64 { .. } => vec!["neon"],
        HostCpu::Unknown => vec!["v1"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(triple: &str, cpu_level: &str) -> WheelMeta {
        WheelMeta {
            filename: format!("cobrust-pkg-0.1.0-{triple}-{cpu_level}.tar.gz"),
            triple: triple.to_owned(),
            cpu_level: cpu_level.to_owned(),
            sha256: "0".repeat(64),
            cobrust_abi: "0.1".to_owned(),
            cobrust_abi_version: COBRUST_ABI_VERSION,
            experimental: false,
            size_bytes: 1024,
            download_url: format!("https://example/{triple}-{cpu_level}.tar.gz"),
        }
    }

    fn make_experimental(triple: &str, cpu_level: &str) -> WheelMeta {
        WheelMeta {
            experimental: true,
            ..make(triple, cpu_level)
        }
    }

    fn make_wrong_abi(triple: &str, cpu_level: &str) -> WheelMeta {
        WheelMeta {
            cobrust_abi_version: 99,
            ..make(triple, cpu_level)
        }
    }

    #[test]
    fn x86_64_v4_host_prefers_v4_then_v3_then_v1() {
        let host = HostCpu::X86_64 { v3: true, v4: true };
        let wheels = [
            make(canonical_x86_64_triple(), "v1"),
            make(canonical_x86_64_triple(), "v3"),
            make(canonical_x86_64_triple(), "v4"),
        ];
        let chosen = select_wheel(&host, &wheels, false).expect("must match");
        assert_eq!(chosen.cpu_level, "v4");
    }

    #[test]
    fn x86_64_v3_host_with_no_v4_available_falls_back_to_v3() {
        let host = HostCpu::X86_64 {
            v3: true,
            v4: false,
        };
        let wheels = [
            make(canonical_x86_64_triple(), "v1"),
            make(canonical_x86_64_triple(), "v3"),
        ];
        let chosen = select_wheel(&host, &wheels, false).expect("must match");
        assert_eq!(chosen.cpu_level, "v3");
    }

    #[test]
    fn no_matching_triple_returns_err() {
        let host = HostCpu::X86_64 {
            v3: false,
            v4: false,
        };
        let wheels = [make("aarch64-apple-darwin", "m1")];
        assert_eq!(
            select_wheel(&host, &wheels, false),
            Err(SelectError::NoWheelForTriple)
        );
    }

    #[test]
    fn apple_m1_host_falls_back_when_only_m2_unavailable() {
        let host = HostCpu::Aarch64 {
            sve: false,
            apple_m1: true,
            apple_m2: false,
        };
        let wheels = [make("aarch64-apple-darwin", "m1")];
        let chosen = select_wheel(&host, &wheels, false).expect("must match");
        assert_eq!(chosen.cpu_level, "m1");
    }

    #[test]
    fn apple_m2_host_prefers_m2_over_m1() {
        let host = HostCpu::Aarch64 {
            sve: false,
            apple_m1: true,
            apple_m2: true,
        };
        let wheels = [
            make("aarch64-apple-darwin", "m1"),
            make("aarch64-apple-darwin", "m2"),
        ];
        let chosen = select_wheel(&host, &wheels, false).expect("must match");
        assert_eq!(chosen.cpu_level, "m2");
    }

    // ── ABI version gate ─────────────────────────────────────────────────────

    #[test]
    fn abi_version_mismatch_rejects_wheel() {
        let host = HostCpu::X86_64 {
            v3: false,
            v4: false,
        };
        let wheels = [make_wrong_abi(canonical_x86_64_triple(), "v1")];
        assert!(matches!(
            select_wheel(&host, &wheels, false),
            Err(SelectError::AbiVersionMismatch { found: 99 })
        ));
    }

    // ── Experimental / SVE gate ──────────────────────────────────────────────

    #[test]
    fn experimental_wheel_rejected_without_flag() {
        let host = HostCpu::Aarch64 {
            sve: true,
            apple_m1: false,
            apple_m2: false,
        };
        // Only SVE available, and it is experimental.
        let wheels = [make_experimental("aarch64-unknown-linux-gnu", "sve")];
        assert_eq!(
            select_wheel(&host, &wheels, false),
            Err(SelectError::ExperimentalNotAllowed)
        );
    }

    #[test]
    fn experimental_wheel_accepted_with_flag() {
        let host = HostCpu::Aarch64 {
            sve: true,
            apple_m1: false,
            apple_m2: false,
        };
        let wheels = [make_experimental("aarch64-unknown-linux-gnu", "sve")];
        let chosen = select_wheel(&host, &wheels, true).expect("must match with flag");
        assert_eq!(chosen.cpu_level, "sve");
    }

    #[test]
    fn experimental_skipped_when_stable_available() {
        let host = HostCpu::Aarch64 {
            sve: true,
            apple_m1: false,
            apple_m2: false,
        };
        // Both neon (stable) and sve (experimental) available.
        let wheels = [
            make("aarch64-unknown-linux-gnu", "neon"),
            make_experimental("aarch64-unknown-linux-gnu", "sve"),
        ];
        // Without flag: picks neon (stable) over sve (experimental)
        let chosen = select_wheel(&host, &wheels, false).expect("neon fallback");
        assert_eq!(chosen.cpu_level, "neon");
    }
}
