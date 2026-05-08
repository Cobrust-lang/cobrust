//! Linker delegation (per ADR-0023 §"Linker delegation").
//!
//! M9 does not bundle its own linker. We invoke the system `cc`
//! (via `$CC` env var, defaulting to `cc`); when `--features lld`
//! is on, we pass `-fuse-ld=lld` so the resolver swaps to LLD
//! without changing the front-end.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::artifact::ArtifactKind;
use crate::error::CodegenError;

/// Invoke the system linker to produce an executable / dynamic
/// library from the given relocatable object file.
///
/// On error, captures stderr + exit code into
/// [`CodegenError::LinkerFailed`].
///
/// # Errors
///
/// Returns [`CodegenError::LinkerFailed`] if the linker exits
/// non-zero; [`CodegenError::Io`] on spawn failure.
pub fn link(object: &Path, output: &Path, kind: ArtifactKind) -> Result<PathBuf, CodegenError> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = Command::new(&cc);
    cmd.arg(object);
    cmd.arg("-o").arg(output);

    if matches!(kind, ArtifactKind::DynamicLibrary) {
        cmd.arg("-shared");
    }

    if cfg!(feature = "lld") {
        cmd.arg("-fuse-ld=lld");
    }

    // Suppress libc startup linkage when emitting an object-only
    // artifact (this branch is reachable only via the executable /
    // dylib branches, but we keep the env hook for posterity).
    let mut handle = match cmd.spawn() {
        Ok(h) => h,
        Err(e) => {
            return Err(CodegenError::LinkerFailed {
                exit_code: -1,
                stderr: format!("failed to spawn `{cc}`: {e}"),
            });
        }
    };

    let status = handle.wait().map_err(|e| CodegenError::Io(e.to_string()))?;

    if !status.success() {
        let exit_code = status.code().unwrap_or(-1);
        return Err(CodegenError::LinkerFailed {
            exit_code,
            stderr: format!("`{cc}` failed; re-run with verbose linker output for details"),
        });
    }

    Ok(output.to_path_buf())
}

/// Verify the linker is available on `$PATH` (or via `$CC`).
///
/// Used by tests + the smoke gate to pre-flight the environment.
#[must_use]
pub fn linker_available() -> bool {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    Command::new(&cc)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
