//! Error taxonomy for the M12 pkg crate (ADR-0026).

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PkgError {
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("lockfile error: {0}")]
    Lockfile(#[from] LockfileError),

    #[error("resolution error: {0}")]
    Resolution(#[from] ResolutionError),

    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),

    #[error("source error: {0}")]
    Source(#[from] SourceError),

    /// (M9) Tarball validation failure — adversarial entry detected.
    #[error("tarball error: {0}")]
    Tarball(#[from] TarballError),

    #[error("io error: {0}")]
    Io(String),
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("toml parse error: {0}")]
    TomlParse(String),

    #[error("missing required field `{0}`")]
    MissingField(String),

    #[error("invalid package name `{0}`: must match [a-zA-Z][a-zA-Z0-9_-]* and be ≤ 64 chars")]
    InvalidName(String),

    #[error("invalid version `{name} = {version}`: not valid semver")]
    InvalidVersion { name: String, version: String },

    #[error("invalid dependency spec for `{name}`: {reason}")]
    InvalidDependency { name: String, reason: String },

    #[error("manifest must declare at least one of [bin] or [lib]")]
    NoTarget,

    #[error("conflicting target paths: [bin].path == [lib].path == {path}")]
    ConflictingPaths { path: String },

    #[error("this looks like an LLM-router config (has [router]); not a user-crate manifest")]
    IsRouterConfig,

    #[error("unknown root key(s): {0:?} — forward-compat warning hardened to error")]
    UnknownRootKeys(Vec<String>),
}

#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("toml parse error: {0}")]
    TomlParse(String),

    #[error("toml serialize error: {0}")]
    TomlSerialize(String),

    #[error("incompatible lockfile_version: have {have}, want {want}")]
    IncompatibleVersion { have: u32, want: u32 },

    #[error(
        "manifest hash mismatch: lockfile records {recorded} but manifest hashes to {computed}"
    )]
    ManifestHashMismatch { recorded: String, computed: String },
}

#[derive(Debug, Error)]
pub enum ResolutionError {
    #[error("conflict resolving package `{package}`: requirements {reqs:?} have no common version")]
    Conflict { package: String, reqs: Vec<String> },

    #[error("dependency cycle detected: {path:?}")]
    Cycle { path: Vec<String> },

    #[error("missing package `{name}`: not found in registry or local sources")]
    MissingPackage { name: String },

    #[error("package `{name}` declared at multiple sources; expected exactly one")]
    AmbiguousSource { name: String },
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("registry root not writable at {0}")]
    NotWritable(PathBuf),

    #[error("offline: registry source for `{name}` not reachable at M12 (registry stub)")]
    Offline { name: String },

    #[error("hash mismatch: expected {expected}, computed {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("entry not found in registry: {hash}")]
    EntryNotFound { hash: String },
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("path source `{0}` does not exist")]
    PathMissing(PathBuf),

    #[error("path source `{0}` is not a directory")]
    PathNotDirectory(PathBuf),

    #[error("git source error: {0}")]
    Git(String),

    #[error("registry source error: {0}")]
    Registry(String),

    #[error("invalid source spec: {0}")]
    Invalid(String),

    /// (M8) A git ref or URL that starts with `-` would be interpreted as a
    /// command-line flag by the `git` binary. Reject it unconditionally.
    #[error(
        "adversarial git ref/url `{0}`: values starting with `-` are forbidden \
         (they would be parsed as git CLI flags)"
    )]
    AdversarialRef(String),
}

/// (M9) Errors arising from tarball security validation.
#[derive(Debug, Error)]
pub enum TarballError {
    /// An entry whose path contains `..` or is absolute would escape the
    /// extraction directory ("zip-slip" / path traversal attack).
    #[error(
        "tarball entry `{0}` contains a path-traversal component (`..` or \
         absolute path) — extraction aborted"
    )]
    PathEscape(String),

    /// A symlink or hard-link entry was found. Cobrust tarballs never
    /// contain symlinks; any such entry in an external tarball is suspect.
    #[error(
        "tarball entry `{0}` is a symlink or hard-link — extraction aborted \
         (symlinks can redirect writes outside the extraction directory)"
    )]
    SymlinkEntry(String),

    /// An I/O error while iterating tarball entries during pre-extraction
    /// validation.
    #[error("failed to read tarball entries during validation: {0}")]
    ReadEntries(String),

    /// An entry path could not be decoded (non-UTF-8 in path components).
    #[error("tarball entry has an undecodable path: {0}")]
    BadEntryPath(String),
}
