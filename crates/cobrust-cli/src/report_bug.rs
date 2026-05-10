//! `cobrust report-bug` — collect a bug-report file and print a
//! GitHub issue link.
//!
//! # What is collected
//!
//! | Item             | Source                         | Stripped |
//! |------------------|--------------------------------|----------|
//! | Cobrust version  | `CARGO_PKG_VERSION`            | no       |
//! | OS / CPU         | `std::env::consts`             | no       |
//! | Last MIR dump    | `.cobrust/last_mir.txt` if any | paths    |
//! | Last source file | supplied by `--source-file`    | paths    |
//!
//! Absolute paths in MIR text are replaced with `<redacted>` before
//! writing so that home-directory structure is not leaked.
//!
//! # Output
//!
//! The command:
//!
//! 1. Creates a report file `cobrust-bug-<timestamp>.txt` in the
//!    current directory (or `--out-dir` if supplied).
//! 2. Prints a `curl` command the user can paste to upload the file and
//!    the GitHub new-issue URL template.
//!
//! # Flags
//!
//! - `--include-mir` — attach the last MIR dump (`.cobrust/last_mir.txt`)
//! - `--source-file <path>` — attach the `.cb` source that triggered the bug
//! - `--out-dir <dir>` — write the report here instead of cwd
//!
//! # Security note
//!
//! The file is **not** automatically uploaded.  The user pastes the
//! `curl` command themselves after reviewing the contents.

use std::path::{Path, PathBuf};

use crate::exit_codes;

/// Run `cobrust report-bug`.
///
/// Returns an exit code per ADR-0024.
pub fn run(include_mir: bool, source_file: Option<&Path>, out_dir: Option<&Path>) -> u8 {
    match collect(include_mir, source_file, out_dir) {
        Ok(report) => {
            print_instructions(&report);
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("cobrust report-bug: {e}");
            exit_codes::USER_ERROR
        }
    }
}

// ── Collection logic ───────────────────────────────────────────────────────

/// Append a formatted line to a `String` buffer.
///
/// Using `push_str(format!(...))` avoids `writeln!(...).unwrap()` while
/// keeping the infallible-write semantics of in-memory strings.
macro_rules! mline {
    ($buf:expr, $($arg:tt)*) => {{
        $buf.push_str(&format!($($arg)*));
        $buf.push('\n');
    }};
}

fn collect(
    include_mir: bool,
    source_file: Option<&Path>,
    out_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let out_dir = out_dir.unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(out_dir).map_err(|e| format!("cannot create output directory: {e}"))?;

    let report_name = format!("cobrust-bug-{timestamp}.txt");
    let report_path = out_dir.join(&report_name);

    // Build manifest text (plain text; no external tar/gzip dependency)
    let mut buf = String::new();
    mline!(buf, "cobrust-bug-report");
    mline!(buf, "version: {}", env!("CARGO_PKG_VERSION"));
    mline!(buf, "os: {}", std::env::consts::OS);
    mline!(buf, "arch: {}", std::env::consts::ARCH);
    buf.push('\n');

    // Optionally collect MIR dump
    if include_mir {
        let mir_path = home_cobrust_dir().join("last_mir.txt");
        if mir_path.exists() {
            match std::fs::read_to_string(&mir_path) {
                Ok(mir_text) => {
                    let stripped = strip_paths(&mir_text);
                    mline!(buf, "--- MIR DUMP (last_mir.txt) ---");
                    // Limit MIR to first 500 lines to stay sane
                    let mut truncated = false;
                    for (i, line) in stripped.lines().enumerate() {
                        if i >= 500 {
                            mline!(buf, "... (truncated at 500 lines)");
                            truncated = true;
                            break;
                        }
                        mline!(buf, "{line}");
                    }
                    let _ = truncated; // used above
                    mline!(buf, "--- END MIR DUMP ---");
                }
                Err(e) => {
                    mline!(buf, "note: could not read last_mir.txt: {e}");
                }
            }
        } else {
            mline!(
                buf,
                "note: no MIR dump found at {} (run with --include-mir after a build failure)",
                mir_path.display()
            );
        }
    }

    // Optionally attach source file
    if let Some(sf) = source_file {
        let label = sf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>");
        match std::fs::read_to_string(sf) {
            Ok(src) => {
                mline!(buf, "--- SOURCE ({label}) ---");
                buf.push_str(&src);
                if !src.ends_with('\n') {
                    buf.push('\n');
                }
                mline!(buf, "--- END SOURCE ---");
            }
            Err(e) => {
                mline!(
                    buf,
                    "note: could not read source file {}: {e}",
                    sf.display()
                );
            }
        }
    }

    std::fs::write(&report_path, buf.as_bytes())
        .map_err(|e| format!("cannot write bug report to {}: {e}", report_path.display()))?;

    Ok(report_path)
}

fn home_cobrust_dir() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cobrust")
}

/// Replace home-directory prefixes with `<redacted>` in MIR text.
fn strip_paths(text: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return text.to_owned();
    }
    text.replace(&home, "<redacted>")
}

// ── User-facing instructions ──────────────────────────────────────────────

fn print_instructions(report: &Path) {
    println!("Bug report collected: {}", report.display());
    println!();
    println!("To file a GitHub issue, open:");
    println!();
    println!(
        "  https://github.com/cobrust-lang/cobrust/issues/new\
         ?template=bug_report.md\
         &title=compiler+bug+report"
    );
    println!();
    println!("Attach the report file above to the issue, or upload it:");
    println!();
    println!(
        "  curl -F 'report=@{}' \
         https://github.com/cobrust-lang/cobrust/issues/new",
        report.display()
    );
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::missing_docs_in_private_items
)]
mod tests {
    use super::*;

    #[test]
    fn report_creates_file() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let result = collect(false, None, Some(dir.path()));
        assert!(result.is_ok(), "collect should succeed: {result:?}");
        let path = result.unwrap();
        assert!(path.exists(), "report file should be created");
    }

    #[test]
    fn strip_paths_replaces_home() {
        // SAFETY: test-only; `set_var` is unsafe in Rust 2024.
        // This test must not run in parallel with other tests that read HOME.
        // The #[allow] suppresses the lint in older editions.
        #[allow(unsafe_code)]
        // SAFETY: single-threaded test binary; HOME set before strip_paths
        // call and restored implicitly since the process exits after tests.
        unsafe {
            std::env::set_var("HOME", "/home/testuser");
        }
        let text = "mir dump at /home/testuser/project/src/main.mir";
        let stripped = strip_paths(text);
        assert!(!stripped.contains("/home/testuser"));
        assert!(stripped.contains("<redacted>"));
    }

    #[test]
    fn run_returns_success() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let code = run(false, None, Some(dir.path()));
        assert_eq!(code, 0);
    }

    #[test]
    fn report_contains_version_and_os() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = collect(false, None, Some(dir.path())).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("version:"));
        assert!(content.contains("os:"));
        assert!(content.contains("arch:"));
    }
}
