//! [`Artifact`] + [`ArtifactKind`] — what we emit (per ADR-0023).

use std::path::{Path, PathBuf};

/// What the codegen subsystem is asked to produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum ArtifactKind {
    /// Relocatable object file (`.o`).
    #[default]
    Object,
    /// Linked executable.
    Executable,
    /// Dynamic library (`.so` / `.dylib`).
    DynamicLibrary,
}

impl ArtifactKind {
    /// Platform-specific filename extension (without leading dot).
    #[must_use]
    pub fn extension(self, triple: &target_lexicon::Triple) -> &'static str {
        use target_lexicon::OperatingSystem;
        match self {
            ArtifactKind::Object => "o",
            ArtifactKind::Executable => "",
            ArtifactKind::DynamicLibrary => match triple.operating_system {
                OperatingSystem::Darwin(_) | OperatingSystem::IOS(_) => "dylib",
                OperatingSystem::Windows => "dll",
                _ => "so",
            },
        }
    }
}

/// What the codegen subsystem produced.
#[derive(Clone, Debug)]
pub enum Artifact {
    /// Relocatable object file at the given path.
    Object(PathBuf),
    /// Linked executable at the given path.
    Executable(PathBuf),
    /// Dynamic library at the given path.
    DynamicLibrary(PathBuf),
}

impl Artifact {
    /// The filesystem path of the emitted artifact.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Artifact::Object(p) | Artifact::Executable(p) | Artifact::DynamicLibrary(p) => p,
        }
    }

    /// True if the artifact is an executable (runnable).
    #[must_use]
    pub fn is_executable(&self) -> bool {
        matches!(self, Artifact::Executable(_))
    }
}
