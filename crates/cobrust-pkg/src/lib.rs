//! Cobrust package format — M12.
//!
//! Public surface implements ADR-0026's binding contracts:
//!
//! - [`Manifest`] — parses + validates user-crate `cobrust.toml`.
//! - [`Lockfile`] — deterministic resolved-dep-graph artifact.
//! - [`Resolver`] — max-compatible semver resolution.
//! - [`Registry`] — content-addressed `~/.cobrust/registry/blake3/<hex>/` cache.
//! - [`Source`] — path / git / registry source backends.
//! - [`Tarball`] — deterministic-tarball helper for hashing + transport.
//!
//! Constitution `CLAUDE.md` §2.2 binds:
//!
//! > `__init__.py` / sys.path / packaging chaos → **single canonical
//! > package format, content-addressed, one tool**
//!
//! §2.4:
//!
//! > **Deterministic build IDs**: hash of source + toolchain + LLM
//! > router decisions, so any translation is reproducible bit-for-bit
//! > given the same inputs.
//!
//! Both promises are realized by this crate.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::single_match_else)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::if_not_else)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::unnecessary_literal_unwrap)]
#![allow(clippy::manual_strip)]

pub mod cpu_detect;
pub mod error;
pub mod lockfile;
pub mod manifest;
pub mod registry;
pub mod registry_client;
pub mod resolver;
pub mod sources;
pub mod tarball;
pub mod wheel_select;

pub use cpu_detect::{HostCpu, detect_host_cpu};
pub use error::{
    LockfileError, ManifestError, PkgError, RegistryError, ResolutionError, SourceError,
    TarballError,
};
pub use registry_client::{RegistryClient, RegistryClientError};
pub use wheel_select::{WheelMeta, select_wheel};
pub use lockfile::{
    LOCKFILE_VERSION, Lockfile, LockfileMetadata, LockfilePackage, load as load_lockfile,
    rewrite_dep_strings, save as save_lockfile,
};
pub use manifest::{
    BinTable, Dependency, DependencySpec, LibTable, Manifest, PackageTable, TestTable,
};
pub use registry::{Registry, RegistryEntry};
pub use resolver::{MaxCompatibleStrategy, Resolution, ResolutionStrategy, Resolver};
pub use sources::{Source, SourceFetchOutput};
pub use tarball::Tarball;

use std::path::Path;

/// Convenience: load a manifest from disk.
///
/// Walks no parents — the caller is responsible for finding the nearest
/// `cobrust.toml` if relevant. See [`find_manifest`] for that.
pub fn load_manifest(path: &Path) -> Result<Manifest, PkgError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| PkgError::Io(format!("read {}: {e}", path.display())))?;
    Manifest::parse_str(&contents)
}

/// Walk up from `start` looking for a `cobrust.toml` file with a
/// `[package]` table (i.e. a user-crate manifest, not a router config).
/// Returns the path of the first match, or `None` if we hit the
/// filesystem root without finding one.
pub fn find_manifest(start: &Path) -> Option<std::path::PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(p) = cur {
        let candidate = p.join("cobrust.toml");
        if candidate.is_file() {
            // Confirm it's a user-crate manifest, not a router config.
            if let Ok(s) = std::fs::read_to_string(&candidate) {
                if Manifest::looks_like_user_crate(&s) {
                    return Some(candidate);
                }
            }
        }
        cur = p.parent();
    }
    None
}

/// Resolve a manifest's deps end-to-end and emit a canonical, deterministic
/// lockfile. Idempotent given identical inputs.
///
/// `workspace_root` is the directory containing the manifest.
/// `registry` is an open registry handle.
pub fn resolve_and_lock(
    manifest: &Manifest,
    workspace_root: &Path,
    registry: &Registry,
) -> Result<Lockfile, PkgError> {
    let resolver = Resolver::new(MaxCompatibleStrategy);
    let resolution = resolver.resolve(manifest, workspace_root, registry)?;
    let mut lock = Lockfile::from_resolution(manifest, &resolution)?;
    rewrite_dep_strings(&mut lock);
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_smoke() {
        // Smoke-test the public re-exports compile.
        let _ = std::mem::size_of::<Manifest>();
        let _ = std::mem::size_of::<Lockfile>();
        let _ = std::mem::size_of::<Registry>();
        let _ = std::mem::size_of::<Source>();
    }
}
