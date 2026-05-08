//! [`TargetSpec`] + supporting selectors (per ADR-0023 §"Public surface").

use std::path::PathBuf;

use target_lexicon::Triple;

use crate::artifact::ArtifactKind;

/// Target specification — drives backend selection, optimization
/// level, output kind, and module identity.
///
/// The `triple` is parsed via `target-lexicon`; supported triples
/// at M9 are enumerated in ADR-0023 §"Target triple matrix".
#[derive(Clone, Debug)]
pub struct TargetSpec {
    /// Target triple (e.g. `x86_64-unknown-linux-gnu`).
    pub triple: Triple,
    /// Optimization level. Cranelift maps this to its own opt
    /// settings; LLVM maps to `-O0` / `-O2` / `-Oz`.
    pub opt_level: OptLevel,
    /// Backend selection. [`Backend::Cranelift`] is the default
    /// for `cargo build`; [`Backend::Llvm`] requires `--features llvm`.
    pub backend: Backend,
    /// Artifact kind (`Object` / `Executable` / `DynamicLibrary`).
    pub artifact: ArtifactKind,
    /// Output directory. The emitted file's name is derived from
    /// `module_name` + the platform extension.
    pub output_dir: PathBuf,
    /// Module name — used for the artifact filename and as the
    /// linker symbol prefix.
    pub module_name: String,
}

impl TargetSpec {
    /// A "host development build" target — uses the host triple,
    /// no opt, Cranelift, executable artifact, output to a temp dir.
    ///
    /// Useful for tests + CLI smoke checks.
    #[must_use]
    pub fn host_dev(output_dir: PathBuf, module_name: impl Into<String>) -> Self {
        Self {
            triple: Triple::host(),
            opt_level: OptLevel::None,
            backend: Backend::default_for_dev(),
            artifact: ArtifactKind::Executable,
            output_dir,
            module_name: module_name.into(),
        }
    }

    /// A "host release build" target — host triple, full opt,
    /// LLVM if `--features llvm` else Cranelift, executable.
    #[must_use]
    pub fn host_release(output_dir: PathBuf, module_name: impl Into<String>) -> Self {
        Self {
            triple: Triple::host(),
            opt_level: OptLevel::Speed,
            backend: Backend::default_for_release(),
            artifact: ArtifactKind::Executable,
            output_dir,
            module_name: module_name.into(),
        }
    }

    /// A "host object" target — emit a relocatable `.o` only.
    #[must_use]
    pub fn host_object(output_dir: PathBuf, module_name: impl Into<String>) -> Self {
        Self {
            triple: Triple::host(),
            opt_level: OptLevel::None,
            backend: Backend::default_for_dev(),
            artifact: ArtifactKind::Object,
            output_dir,
            module_name: module_name.into(),
        }
    }
}

/// Optimization level passed to the backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum OptLevel {
    /// `-O0` — no opt; fastest compile, slowest run.
    #[default]
    None,
    /// Speed-focused opt (Cranelift `speed`, LLVM `-O2`).
    Speed,
    /// Speed + size opt (Cranelift `speed_and_size`, LLVM `-Oz`).
    SpeedAndSize,
}

/// Backend selector — Cranelift is the default; LLVM is opt-in
/// via `--features llvm`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// Pure-Rust Cranelift backend (default for `cargo build`).
    Cranelift,
    /// LLVM via inkwell (requires `--features llvm`).
    Llvm,
}

impl Backend {
    /// Recommended default for development / debug builds:
    /// always Cranelift (fast compile).
    #[must_use]
    pub fn default_for_dev() -> Self {
        Backend::Cranelift
    }

    /// Recommended default for release builds: LLVM if available,
    /// else Cranelift.
    #[must_use]
    pub fn default_for_release() -> Self {
        if cfg!(feature = "llvm") {
            Backend::Llvm
        } else {
            Backend::Cranelift
        }
    }
}

impl Default for Backend {
    fn default() -> Self {
        Backend::Cranelift
    }
}
