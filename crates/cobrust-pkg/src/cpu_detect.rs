//! Host CPU detection for ADR-0065 Tier 3 wheel selection.
//!
//! Pure-read detection: parses `/proc/cpuinfo` on Linux and `sysctl` on macOS.
//! No `cpuid` intrinsics required.  Best-effort: containers, VMs, and rosetta
//! may mask features — fallback paths always return the baseline tier
//! (`v1` / `neon` / `m1`) which is guaranteed to run on all hardware of the
//! given architecture.
//!
//! ADR-0065 §3.3.1 specifies the detection table this module implements.

#[cfg(any(target_os = "linux", test))]
use std::path::Path;

/// Detected host CPU.  Variant carries enough state to pick a wheel from the
/// ADR-0065 §3.1 matrix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostCpu {
    /// x86_64 host (Linux or macOS Intel).
    X86_64 {
        /// True if the host advertises AVX2 (`v3` tier prerequisite).
        v3: bool,
        /// True if the host advertises AVX-512F (`v4` tier prerequisite).
        v4: bool,
    },
    /// aarch64 host (Linux ARM or Apple Silicon).
    Aarch64 {
        /// True if SVE is present (Graviton3 / Ampere Altra).
        sve: bool,
        /// True if running on Apple Silicon M1.
        apple_m1: bool,
        /// True if running on Apple Silicon M2 / M3+ (M2 Pro+ or newer).
        apple_m2: bool,
    },
    /// Unknown architecture — caller should pick the universally-safe baseline.
    Unknown,
}

impl HostCpu {
    /// Map the detected CPU to the wheel suffix tier per ADR-0065 §3.1.
    ///
    /// The returned `&'static str` is the literal suffix used in the wheel
    /// filename (e.g. `"v3"`, `"neon"`, `"m2"`).  Callers join this with the
    /// triple to form the full asset suffix.
    #[must_use]
    pub fn preferred_cpu_level(&self) -> &'static str {
        match self {
            Self::X86_64 { v4: true, .. } => "v4",
            Self::X86_64 { v3: true, .. } => "v3",
            Self::X86_64 { .. } => "v1",
            Self::Aarch64 { sve: true, .. } => "sve",
            Self::Aarch64 { apple_m2: true, .. } => "m2",
            Self::Aarch64 { apple_m1: true, .. } => "m1",
            Self::Aarch64 { .. } => "neon",
            Self::Unknown => "v1",
        }
    }
}

/// Detect the host CPU using OS-specific introspection.
///
/// Returns [`HostCpu::Unknown`] for unsupported architectures; callers should
/// then fall back to the baseline wheel.
#[must_use]
pub fn detect_host_cpu() -> HostCpu {
    match std::env::consts::ARCH {
        "x86_64" => detect_x86_64(),
        "aarch64" => detect_aarch64(),
        _ => HostCpu::Unknown,
    }
}

#[cfg(target_arch = "x86_64")]
fn detect_x86_64() -> HostCpu {
    // is_x86_feature_detected! is the standard library cpuid wrapper.
    let v3 = is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma");
    let v4 = is_x86_feature_detected!("avx512f");
    HostCpu::X86_64 { v3, v4 }
}

#[cfg(not(target_arch = "x86_64"))]
fn detect_x86_64() -> HostCpu {
    // We're not running on x86_64, so we can only consult /proc/cpuinfo /
    // sysctl as a probe — but that situation does not arise during normal
    // installs (the wheel selector queries the host that's actually running
    // this code).  Default to the baseline.
    HostCpu::X86_64 {
        v3: false,
        v4: false,
    }
}

#[cfg(target_arch = "aarch64")]
fn detect_aarch64() -> HostCpu {
    let sve = std::arch::is_aarch64_feature_detected!("sve");
    let (apple_m1, apple_m2) = detect_apple_silicon();
    HostCpu::Aarch64 {
        sve,
        apple_m1,
        apple_m2,
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn detect_aarch64() -> HostCpu {
    HostCpu::Aarch64 {
        sve: false,
        apple_m1: false,
        apple_m2: false,
    }
}

/// Apple-silicon brand-string detection.  Returns `(is_m1_family, is_m2_or_later)`.
///
/// `is_m1_family` is true on all Apple Silicon (`m1` is the baseline wheel
/// that runs on every Mac with an Apple-designed SoC).  `is_m2_or_later` is
/// true if the brand string mentions `M2`, `M3`, or a successor — used to
/// upgrade the wheel from `m1` to `m2`.
#[cfg(target_os = "macos")]
fn detect_apple_silicon() -> (bool, bool) {
    // sysctl machdep.cpu.brand_string is the canonical Apple-supplied source
    // for CPU model name on macOS.  Examples:
    //   "Apple M1"        → (true, false)
    //   "Apple M2 Pro"    → (true, true)
    //   "Apple M3 Max"    → (true, true)
    let brand = sysctl_string("machdep.cpu.brand_string").unwrap_or_default();
    let is_apple_silicon = brand.contains("Apple M");
    let is_m2_or_later = brand.contains("Apple M2")
        || brand.contains("Apple M3")
        || brand.contains("Apple M4")
        || brand.contains("Apple M5");
    (is_apple_silicon, is_m2_or_later)
}

#[cfg(not(target_os = "macos"))]
fn detect_apple_silicon() -> (bool, bool) {
    (false, false)
}

/// Parse `/proc/cpuinfo` for an x86_64 feature flag.  Public for tests.
///
/// Reads the file at `path` and looks for `flags : ...` lines containing the
/// requested feature token.  Returns `false` on any I/O error.
#[cfg(any(target_os = "linux", test))]
pub fn proc_cpuinfo_has_flag(path: &Path, flag: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("flags") {
            // Format: `flags		: fpu vme de pse tsc msr ...`
            if let Some(idx) = rest.find(':') {
                let flags_str = &rest[idx + 1..];
                for tok in flags_str.split_whitespace() {
                    if tok == flag {
                        return true;
                    }
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("Features") {
            // ARM kernels use `Features` (capitalized) rather than `flags`.
            if let Some(idx) = rest.find(':') {
                let flags_str = &rest[idx + 1..];
                for tok in flags_str.split_whitespace() {
                    if tok == flag {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Spawn `sysctl -n <name>` and capture stdout as a trimmed string.
/// Returns `None` if the binary is absent or returns non-zero.
#[cfg(target_os = "macos")]
fn sysctl_string(name: &str) -> Option<String> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    Some(s.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_host_cpu_returns_variant_matching_arch() {
        let cpu = detect_host_cpu();
        match std::env::consts::ARCH {
            "x86_64" => assert!(matches!(cpu, HostCpu::X86_64 { .. })),
            "aarch64" => assert!(matches!(cpu, HostCpu::Aarch64 { .. })),
            _ => assert!(matches!(cpu, HostCpu::Unknown)),
        }
    }

    #[test]
    fn preferred_cpu_level_x86_64_picks_highest_tier() {
        assert_eq!(
            HostCpu::X86_64 { v3: true, v4: true }.preferred_cpu_level(),
            "v4"
        );
        assert_eq!(
            HostCpu::X86_64 {
                v3: true,
                v4: false
            }
            .preferred_cpu_level(),
            "v3"
        );
        assert_eq!(
            HostCpu::X86_64 {
                v3: false,
                v4: false
            }
            .preferred_cpu_level(),
            "v1"
        );
    }

    #[test]
    fn preferred_cpu_level_aarch64_priority_order() {
        // SVE wins over everything (assumes Linux Graviton).
        assert_eq!(
            HostCpu::Aarch64 {
                sve: true,
                apple_m1: false,
                apple_m2: false,
            }
            .preferred_cpu_level(),
            "sve"
        );
        // Apple M2 wins over M1 baseline.
        assert_eq!(
            HostCpu::Aarch64 {
                sve: false,
                apple_m1: true,
                apple_m2: true,
            }
            .preferred_cpu_level(),
            "m2"
        );
        assert_eq!(
            HostCpu::Aarch64 {
                sve: false,
                apple_m1: true,
                apple_m2: false,
            }
            .preferred_cpu_level(),
            "m1"
        );
        // Generic ARMv8 Linux without SVE.
        assert_eq!(
            HostCpu::Aarch64 {
                sve: false,
                apple_m1: false,
                apple_m2: false,
            }
            .preferred_cpu_level(),
            "neon"
        );
    }

    #[test]
    fn unknown_arch_falls_back_to_baseline() {
        assert_eq!(HostCpu::Unknown.preferred_cpu_level(), "v1");
    }

    #[test]
    fn proc_cpuinfo_parser_handles_x86_64_format() {
        let dir = tempfile::tempdir().expect("test setup");
        let path = dir.path().join("cpuinfo");
        std::fs::write(
            &path,
            "processor\t: 0\nflags\t\t: fpu vme avx avx2 fma sse4_2\n",
        )
        .expect("test setup");
        assert!(proc_cpuinfo_has_flag(&path, "avx2"));
        assert!(proc_cpuinfo_has_flag(&path, "fma"));
        assert!(!proc_cpuinfo_has_flag(&path, "avx512f"));
    }

    #[test]
    fn proc_cpuinfo_parser_handles_arm_features_line() {
        let dir = tempfile::tempdir().expect("test setup");
        let path = dir.path().join("cpuinfo");
        std::fs::write(&path, "processor\t: 0\nFeatures\t: fp asimd evtstrm sve\n").expect("test setup");
        assert!(proc_cpuinfo_has_flag(&path, "sve"));
        assert!(proc_cpuinfo_has_flag(&path, "asimd"));
        assert!(!proc_cpuinfo_has_flag(&path, "missing"));
    }

    #[test]
    fn proc_cpuinfo_parser_returns_false_on_missing_file() {
        assert!(!proc_cpuinfo_has_flag(Path::new("/no/such/file"), "avx2"));
    }
}
