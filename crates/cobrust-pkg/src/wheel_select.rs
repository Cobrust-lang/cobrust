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

use serde::{Deserialize, Serialize};

use crate::cpu_detect::HostCpu;

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
    /// Size of the wheel archive in bytes.
    pub size_bytes: u64,
    /// HTTP(S) URL to download the wheel from.
    pub download_url: String,
}

/// Pick the best wheel for `host` from `available`.
///
/// Algorithm (ADR-0065 §3.3.2 steps 4–6):
/// 1. Filter `available` to entries whose `triple` matches the host triple.
/// 2. Among the matching entries, prefer the highest-tier match for the host.
/// 3. If no specific tier matches, fall back to the architecture's baseline
///    (`v1` for x86_64, `neon` for aarch64 Linux, `m1` for aarch64 Apple).
/// 4. Return `None` if the filtered set is empty (no wheels built for this
///    triple at all).
#[must_use]
pub fn select_wheel<'a>(host: &HostCpu, available: &'a [WheelMeta]) -> Option<&'a WheelMeta> {
    let triple = host_triple(host)?;
    let matches: Vec<&WheelMeta> = available.iter().filter(|w| w.triple == triple).collect();
    if matches.is_empty() {
        return None;
    }

    // Build the priority list of cpu_level strings to try, highest to lowest.
    let priority = cpu_level_priority(host);

    // Pick the first wheel that matches a tier in priority order.
    for tier in &priority {
        if let Some(w) = matches.iter().find(|w| &w.cpu_level == tier) {
            return Some(*w);
        }
    }

    // Last-resort: if priority list exhausted and we still haven't found a
    // baseline (e.g. registry is missing the baseline tier), return whatever
    // wheel we have for this triple — caller will emit a warning.
    matches.first().copied()
}

/// Map [`HostCpu`] to the canonical target-triple string used in wheel names.
fn host_triple(host: &HostCpu) -> Option<&'static str> {
    match host {
        HostCpu::X86_64 { .. } => Some(canonical_x86_64_triple()),
        HostCpu::Aarch64 {
            apple_m1: true, ..
        }
        | HostCpu::Aarch64 {
            apple_m2: true, ..
        } => Some("aarch64-apple-darwin"),
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
/// highest tier first.  The final element is always the architecture's
/// baseline so callers always reach a fallback.
fn cpu_level_priority(host: &HostCpu) -> Vec<&'static str> {
    match host {
        HostCpu::X86_64 { v4: true, .. } => vec!["v4", "v3", "v1"],
        HostCpu::X86_64 { v3: true, .. } => vec!["v3", "v1"],
        HostCpu::X86_64 { .. } => vec!["v1"],
        HostCpu::Aarch64 { sve: true, .. } => vec!["sve", "neon"],
        HostCpu::Aarch64 {
            apple_m2: true, ..
        } => vec!["m2", "m1"],
        HostCpu::Aarch64 {
            apple_m1: true, ..
        } => vec!["m1"],
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
            size_bytes: 1024,
            download_url: format!("https://example/{triple}-{cpu_level}.tar.gz"),
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
        let chosen = select_wheel(&host, &wheels).expect("must match");
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
        let chosen = select_wheel(&host, &wheels).expect("must match");
        assert_eq!(chosen.cpu_level, "v3");
    }

    #[test]
    fn no_matching_triple_returns_none() {
        let host = HostCpu::X86_64 {
            v3: false,
            v4: false,
        };
        let wheels = [make("aarch64-apple-darwin", "m1")];
        assert!(select_wheel(&host, &wheels).is_none());
    }

    #[test]
    fn apple_m1_host_falls_back_when_only_m2_unavailable() {
        let host = HostCpu::Aarch64 {
            sve: false,
            apple_m1: true,
            apple_m2: false,
        };
        let wheels = [make("aarch64-apple-darwin", "m1")];
        let chosen = select_wheel(&host, &wheels).expect("must match");
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
        let chosen = select_wheel(&host, &wheels).expect("must match");
        assert_eq!(chosen.cpu_level, "m2");
    }
}
