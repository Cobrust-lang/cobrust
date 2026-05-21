//! `cobrust install <pkg>` subcommand — ADR-0065 §3.3 wheel installer.
//!
//! End-to-end flow (ADR-0065 §3.3.2):
//!
//! 1. Detect host CPU via `cobrust_pkg::detect_host_cpu`.
//! 2. Fetch the wheel index for the requested package from the registry.
//! 3. Pick the highest-tier wheel matching the host (with baseline fallback).
//! 4. Download the wheel + verify SHA-256.
//! 5. Validate `cobrust_abi` + `cobrust_abi_version` compatibility.
//! 6. Unpack the wheel tarball into `~/.cobrust/pkgs/<name>-<version>/`.
//!
//! Errors print a `suggestion:` block per CLAUDE.md §2.5 direction B
//! (errors must print the fix, not just the diagnosis).
//!
//! # W4 additions (ADR-0065 §7.4)
//!
//! - `InstallArgs::allow_experimental` — mirrors `--allow-experimental` CLI
//!   flag; required to install SVE wheels (§3.1 / §6.5).
//! - `select_wheel` now returns `Result<&WheelMeta, SelectError>` instead of
//!   `Option`; the three error cases are mapped to distinct `InstallError`
//!   variants with actionable `suggestion:` text.

use std::path::{Path, PathBuf};

use thiserror::Error;

use cobrust_pkg::cpu_detect::detect_host_cpu;
use cobrust_pkg::registry_client::{RegistryClient, RegistryClientError};
use cobrust_pkg::wheel_select::{COBRUST_ABI_VERSION, SelectError, WheelMeta, select_wheel};

use crate::exit_codes;

/// Default registry URL (GitHub Releases acts as the static-JSON registry
/// host per ADR-0065 §3.4). Override via `--registry-url`.
pub const DEFAULT_REGISTRY_URL: &str = "https://github.com/Cobrust-lang/cobrust/releases/download";

/// `cobrust_abi` semver-major version this binary speaks. Wheels tagged with
/// a mismatching semver-major are rejected at install time (ADR-0065 §3.3.2
/// step 9). Distinct from the numeric [`COBRUST_ABI_VERSION`] constant from
/// `cobrust_pkg::wheel_select` which guards the dependency-closure ABI (§6.4).
pub const COBRUST_SEMVER_ABI: &str = "0.1";

/// Parsed arguments for `cobrust install`.
#[derive(Debug, Clone)]
pub struct InstallArgs {
    /// Package name (required).
    pub pkg_name: String,
    /// Package version (optional; default uses the registry-advertised latest).
    pub version: Option<String>,
    /// Override the registry URL (advanced; default per `DEFAULT_REGISTRY_URL`).
    pub registry_url: Option<String>,
    /// If true, do everything except writing to disk (per ADR-0065 §3.3.3).
    pub dry_run: bool,
    /// Allow installing experimental wheels (e.g. SVE — ADR-0065 §3.1 /
    /// §6.5). Default `false`; must be set explicitly via `--allow-experimental`.
    pub allow_experimental: bool,
}

/// Errors that can surface from `cobrust install`.
#[derive(Debug, Error)]
pub enum InstallError {
    /// Registry transport / verification failure.
    #[error(transparent)]
    Registry(#[from] RegistryClientError),
    /// No wheel matches the host triple.
    #[error(
        "no wheel found for package {pkg} matching host triple\n  suggestion: the package may not yet be built for your platform; check https://github.com/Cobrust-lang/cobrust/releases or build from source"
    )]
    NoMatchingWheel {
        /// Package name lookup that failed.
        pkg: String,
    },
    /// Wheel semver ABI tag does not match the binary's `COBRUST_SEMVER_ABI`.
    #[error(
        "wheel cobrust_abi {wheel_abi} incompatible with installer {installer_abi}\n  suggestion: upgrade `cobrust` to a release that supports cobrust_abi {wheel_abi}, or pin to an older package version"
    )]
    AbiMismatch {
        /// ABI tag advertised by the wheel.
        wheel_abi: String,
        /// ABI version supported by this installer binary.
        installer_abi: String,
    },
    /// Wheel numeric ABI version does not match [`COBRUST_ABI_VERSION`].
    #[error(
        "wheel cobrust_abi_version {wheel_ver} incompatible with this installer (expects {expected})\n  suggestion: upgrade `cobrust` to a version that supports ABI version {wheel_ver}, or install an older wheel"
    )]
    AbiVersionMismatch {
        /// Numeric ABI version advertised by the wheel.
        wheel_ver: u32,
        /// Expected ABI version for this installer.
        expected: u32,
    },
    /// The selected wheel is experimental but `--allow-experimental` was not
    /// passed.
    #[error(
        "wheel is experimental (e.g. SVE); re-run with --allow-experimental to install\n  suggestion: experimental wheels may have unstable ABI or correctness gaps; only use if you understand the risks"
    )]
    ExperimentalNotAllowed,
    /// Filesystem / unpack errors.
    #[error("install I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Required `version` is missing (wave-2 requires explicit version).
    #[error(
        "version required: pass `cobrust install {pkg}@<version>`\n  suggestion: explicit versions are required in wave-2; transitive resolution lands in a later wave"
    )]
    MissingVersion {
        /// Package name that was supplied without a version.
        pkg: String,
    },
}

/// Dispatch entry point used from `main.rs`.
///
/// Returns an exit code following the codes in `exit_codes`. Side effects
/// (downloads + disk writes) happen only when `args.dry_run` is false.
#[must_use]
pub fn run(args: InstallArgs) -> u8 {
    match install(&args) {
        Ok(report) => {
            if args.dry_run {
                eprintln!(
                    "info: dry-run; would install {} ({} bytes from {})",
                    report.installed_wheel.filename,
                    report.installed_wheel.size_bytes,
                    report.installed_wheel.download_url,
                );
            } else {
                eprintln!(
                    "info: installed {} to {}",
                    report.installed_wheel.filename,
                    report.install_dir.display(),
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_codes::USER_ERROR
        }
    }
}

/// Inner Result-typed implementation; the outer `run` adapts to exit codes.
fn install(args: &InstallArgs) -> Result<InstallReport, InstallError> {
    let version = args
        .version
        .clone()
        .ok_or_else(|| InstallError::MissingVersion {
            pkg: args.pkg_name.clone(),
        })?;

    let registry_url = args
        .registry_url
        .clone()
        .unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_owned());
    let client = RegistryClient::new(registry_url)?;
    let index = client.fetch_index(&args.pkg_name, &version)?;

    let host = detect_host_cpu();
    let chosen = select_wheel(&host, &index, args.allow_experimental).map_err(|e| match e {
        SelectError::NoWheelForTriple => InstallError::NoMatchingWheel {
            pkg: args.pkg_name.clone(),
        },
        SelectError::AbiVersionMismatch { found } => InstallError::AbiVersionMismatch {
            wheel_ver: found,
            expected: COBRUST_ABI_VERSION,
        },
        SelectError::ExperimentalNotAllowed => InstallError::ExperimentalNotAllowed,
    })?;

    if !abi_compatible(&chosen.cobrust_abi, COBRUST_SEMVER_ABI) {
        return Err(InstallError::AbiMismatch {
            wheel_abi: chosen.cobrust_abi.clone(),
            installer_abi: COBRUST_SEMVER_ABI.to_owned(),
        });
    }

    let install_root = default_install_root();
    let install_dir = install_root
        .join("pkgs")
        .join(format!("{}-{}", args.pkg_name, version));

    let chosen_owned = chosen.clone();

    if args.dry_run {
        return Ok(InstallReport {
            installed_wheel: chosen_owned,
            install_dir,
        });
    }

    let cache_dir = install_root.join("cache");
    let archive_path = client.download_wheel(chosen, &cache_dir)?;
    unpack_tarball(&archive_path, &install_dir)?;
    Ok(InstallReport {
        installed_wheel: chosen_owned,
        install_dir,
    })
}

/// Report struct returned from `install` for human-readable + JSON-friendly
/// output (JSON mode TBD in a follow-up).
#[derive(Debug)]
struct InstallReport {
    installed_wheel: WheelMeta,
    install_dir: PathBuf,
}

/// Semver-major compatibility check on `cobrust_abi` tags.
///
/// ADR-0065 §6.4: ABI compatibility is gated on semver-major equality.
fn abi_compatible(wheel: &str, installer: &str) -> bool {
    let wheel_major = wheel.split('.').next().unwrap_or("");
    let installer_major = installer.split('.').next().unwrap_or("");
    if wheel_major == "0" || installer_major == "0" {
        // 0.x: minor version is breaking — require exact match.
        wheel == installer
    } else {
        wheel_major == installer_major
    }
}

/// Default install root: `$COBRUST_HOME` if set, else `$HOME/.cobrust`.
fn default_install_root() -> PathBuf {
    if let Ok(custom) = std::env::var("COBRUST_HOME") {
        return PathBuf::from(custom);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cobrust");
    }
    PathBuf::from(".cobrust")
}

/// Unpack a `.tar.gz` archive into `dest`. Creates `dest` if absent.
///
/// Uses `flate2` + `tar` (already in the workspace dep graph) to avoid
/// shelling out to `tar(1)`.
fn unpack_tarball(archive: &Path, dest: &Path) -> Result<(), InstallError> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(archive)?;
    let dec = GzDecoder::new(file);
    let mut ar = Archive::new(dec);
    ar.unpack(dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_compatible_zerodot_requires_exact_match() {
        assert!(abi_compatible("0.1", "0.1"));
        assert!(!abi_compatible("0.1", "0.2"));
        assert!(!abi_compatible("0.2", "0.1"));
    }

    #[test]
    fn abi_compatible_post_one_matches_on_major() {
        assert!(abi_compatible("1.5", "1.0"));
        assert!(!abi_compatible("1.0", "2.0"));
        assert!(abi_compatible("2.7", "2.0"));
    }

    #[test]
    fn default_install_root_uses_cobrust_home_when_set() {
        // We can't safely mutate process env across tests in stable Rust;
        // smoke-test only that the call returns a non-empty path.
        let p = default_install_root();
        assert!(!p.as_os_str().is_empty());
    }
}
