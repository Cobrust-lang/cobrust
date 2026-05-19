//! `cobrust test` — compile + run every `.cb` file under `tests/`.
//!
//! Walks the cwd's `tests/` directory; for each `.cb` file, performs
//! `cobrust build --emit exe` then invokes the produced executable.
//! A test passes if the executable exits with code 0; otherwise it
//! contributes to the failure count.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::build::{self, EmitKind};
use crate::exit_codes;

/// Run `cobrust test`.
///
/// M12 amendment: if a `cobrust.toml` is reachable walking up from cwd,
/// switch to package mode (use the manifest's `[[test]]` array as the
/// source of truth). Otherwise fall back to the M11 directory-walk mode.
pub fn run(quiet: bool) -> u8 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cobrust_pkg::find_manifest(&cwd).is_some() {
        return crate::pkg_build::run_test(None, false, quiet);
    }

    let tests_dir = cwd.join("tests");
    if !tests_dir.is_dir() {
        eprintln!("cobrust test: no `tests/` directory in cwd");
        return exit_codes::USER_ERROR;
    }

    let cb_files = match collect_cb_files(&tests_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cobrust test: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    if cb_files.is_empty() {
        if !quiet {
            println!("cobrust test: no `.cb` files found in tests/");
        }
        return exit_codes::SUCCESS;
    }

    let mut passed: u32 = 0;
    let mut failed: u32 = 0;

    for f in &cb_files {
        let artifact = match build::build(f, None, EmitKind::Executable, false, None, true, None) {
            Ok(a) => a,
            Err(e) => {
                if !quiet {
                    println!("FAIL {} (build: {e})", f.display());
                }
                failed += 1;
                continue;
            }
        };
        let status = Command::new(artifact.path()).status();
        match status {
            Ok(s) if s.success() => {
                if !quiet {
                    println!("PASS {}", f.display());
                }
                passed += 1;
            }
            Ok(s) => {
                if !quiet {
                    println!("FAIL {} (exit {:?})", f.display(), s.code());
                }
                failed += 1;
            }
            Err(e) => {
                if !quiet {
                    println!("FAIL {} (cannot exec: {e})", f.display());
                }
                failed += 1;
            }
        }
    }

    if !quiet {
        println!(
            "cobrust test: {passed} passed, {failed} failed (out of {})",
            cb_files.len()
        );
    }
    if failed == 0 {
        exit_codes::SUCCESS
    } else {
        exit_codes::TEST_FAILURE
    }
}

fn collect_cb_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("readdir {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("cb") {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}
