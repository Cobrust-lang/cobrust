//! `cobrust run` — `cobrust build --emit exe`, then invoke the produced
//! executable, propagating its exit code per ADR-0024 §"Exit-code scheme".

use std::path::Path;
use std::process::Command;

use crate::build::{self, BuildError, EmitKind};
use crate::exit_codes;

/// Run `cobrust run <file.cb>`.
pub fn run(file: &Path, release: bool, target: Option<&str>, quiet: bool) -> u8 {
    let artifact = match build::build(file, None, EmitKind::Executable, release, target, quiet) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("cobrust run: {e}");
            return e.exit_code();
        }
    };

    let exe = artifact.path();
    if !artifact.is_executable() {
        eprintln!("cobrust run: build produced non-executable artifact");
        return BuildError::Internal("non-executable build artifact".into()).exit_code();
    }

    let status = Command::new(exe).status();
    match status {
        Ok(s) if s.success() => exit_codes::SUCCESS,
        Ok(s) => {
            // Map the program's exit code into our scheme: any non-zero
            // child status is reported as RUNTIME_PANIC at M10. M11+ may
            // refine the mapping.
            if let Some(code) = s.code() {
                if (1..=99).contains(&code) {
                    return code as u8;
                }
            }
            exit_codes::RUNTIME_PANIC
        }
        Err(e) => {
            eprintln!("cobrust run: cannot exec {}: {e}", exe.display());
            exit_codes::INTERNAL_PANIC
        }
    }
}
