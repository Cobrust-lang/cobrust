//! `cobrust add <name> [--path PATH | --git URL --rev REV | --version REQ]`
//! (M12, ADR-0026 §"Public CLI surface impact").
//!
//! Appends a row to the nearest `cobrust.toml`'s `[dependencies]` (or
//! `[dev-dependencies]` with `--dev`). The append is "best effort
//! comment-preserving" — we operate on the raw TOML text rather than
//! re-serializing through `Manifest::canonical_toml` (which would lose
//! the user's preferred formatting).

use std::path::Path;

use cobrust_pkg::find_manifest;

use crate::exit_codes;

/// Run `cobrust add`.
pub fn run(
    name: &str,
    path: Option<&Path>,
    git: Option<&str>,
    rev: Option<&str>,
    version: Option<&str>,
    dev: bool,
) -> u8 {
    if !is_valid_name(name) {
        eprintln!("cobrust add: invalid dep name `{name}`");
        return exit_codes::USER_ERROR;
    }

    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cobrust add: cannot read cwd: {e}");
            return exit_codes::INTERNAL_PANIC;
        }
    };
    let manifest_path = match find_manifest(&cwd) {
        Some(p) => p,
        None => {
            eprintln!(
                "cobrust add: no `cobrust.toml` walking up from {}",
                cwd.display()
            );
            return exit_codes::USER_ERROR;
        }
    };

    // Build the dep-row TOML.
    let dep_row = match (path, git, rev, version) {
        (Some(p), None, None, None) => format!("{name} = {{ path = \"{}\" }}", escape_path(p)),
        (None, Some(g), Some(r), None) => format!("{name} = {{ git = \"{g}\", rev = \"{r}\" }}"),
        (None, None, None, Some(v)) => format!("{name} = \"{v}\""),
        (None, Some(_), None, _) => {
            eprintln!("cobrust add: --git requires --rev");
            return exit_codes::USER_ERROR;
        }
        _ => {
            eprintln!("cobrust add: pick exactly one of --path / --git/--rev / --version");
            return exit_codes::USER_ERROR;
        }
    };

    let table = if dev {
        "dev-dependencies"
    } else {
        "dependencies"
    };

    let original = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cobrust add: cannot read {}: {e}", manifest_path.display());
            return exit_codes::INTERNAL_PANIC;
        }
    };
    let new_text = match append_dep_row(&original, table, &dep_row) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cobrust add: {e}");
            return exit_codes::INTERNAL_PANIC;
        }
    };
    if let Err(e) = std::fs::write(&manifest_path, new_text) {
        eprintln!("cobrust add: cannot write {}: {e}", manifest_path.display());
        return exit_codes::INTERNAL_PANIC;
    }

    println!(
        "cobrust: added `{name}` under [{table}] in {}",
        manifest_path.display()
    );
    exit_codes::SUCCESS
}

fn escape_path(p: &Path) -> String {
    p.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Insert `dep_row` under the `[<table>]` heading. If the heading isn't
/// present, append a fresh section at the end.
fn append_dep_row(original: &str, table: &str, dep_row: &str) -> Result<String, String> {
    // Search for the [<table>] heading.
    let heading = format!("[{table}]");
    let mut out = String::with_capacity(original.len() + dep_row.len() + 16);
    let mut found = false;
    let mut inserted = false;
    let mut current_section: Option<String> = None;

    for (i, line) in original.lines().enumerate() {
        // Detect a section heading line.
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // If we were inside the target section but never inserted, do it
            // now (right before the next section).
            if current_section.as_deref() == Some(table) && !inserted {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(dep_row);
                out.push('\n');
                inserted = true;
            }
            current_section = Some(
                trimmed
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string(),
            );
            if trimmed == heading {
                found = true;
            }
        }
        out.push_str(line);
        out.push('\n');
        let _ = i; // suppress unused
    }
    // EOF case: if we ended up inside the target section without inserting,
    // append the row.
    if found && !inserted {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(dep_row);
        out.push('\n');
        inserted = true;
    }
    // Brand-new section.
    if !found {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&heading);
        out.push('\n');
        out.push_str(dep_row);
        out.push('\n');
        inserted = true;
    }
    if !inserted {
        return Err(format!("could not insert dep `{dep_row}` into [{table}]"));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const BASE: &str = "[package]\nname = \"x\"\nversion = \"0.1.0\"\ncobrust-version = \"0.0.1\"\n\n[bin]\nname = \"x\"\npath = \"src/main.cb\"\n";

    #[test]
    fn add_to_existing_dependencies() {
        let original = format!("{BASE}\n[dependencies]\nfoo = \"1.0\"\n");
        let new = append_dep_row(&original, "dependencies", "bar = \"2.0\"").unwrap();
        assert!(new.contains("foo = \"1.0\""));
        assert!(new.contains("bar = \"2.0\""));
    }

    #[test]
    fn add_creates_section_when_absent() {
        let new = append_dep_row(BASE, "dependencies", "foo = \"1.0\"").unwrap();
        assert!(new.contains("[dependencies]"));
        assert!(new.contains("foo = \"1.0\""));
    }

    #[test]
    fn add_dev_section() {
        let new = append_dep_row(BASE, "dev-dependencies", "helper = \"0.1\"").unwrap();
        assert!(new.contains("[dev-dependencies]"));
        assert!(new.contains("helper = \"0.1\""));
    }
}
