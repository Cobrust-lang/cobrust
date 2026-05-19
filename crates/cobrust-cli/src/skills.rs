//! `cobrust skills` subcommand — binary-embedded agent skill cheatsheets.
//!
//! ADR-0061 §4: skill markdown files are embedded at compile time via
//! `rust-embed`, so every binary version serves version-matched docs.
//! No filesystem reads at runtime; no external network calls.
//!
//! Public surface:
//! - [`list_skills`] — returns sorted names (stem, no `.md`).
//! - [`get_skill`] — returns raw markdown bytes for a named skill.
//!
//! The CLI dispatches to [`cmd_skills`]:
//! - `cobrust skills list` → newline-separated names to stdout.
//! - `cobrust skills get <name>` → raw markdown to stdout.
//! - `cobrust skills get <name> --json` → `{"name":"…","version":"…","content":"…"}` to stdout.

// F34 anchor: skills-mod-v1 — module boundary; any breaking change needs ADR amendment.

use std::borrow::Cow;

use thiserror::Error;

/// Embedded skill assets.
///
/// The `#[folder]` path is relative to the crate root (`crates/cobrust-cli/`).
/// At build time `rust-embed` resolves the path to `docs/agent/skills/` and
/// bundles every `*.md` file found there.
#[derive(rust_embed::Embed)]
#[folder = "../../docs/agent/skills/"]
#[include = "*.md"]
struct SkillAssets;

/// Error type for the `skills` subcommand. F34 anchor: skills-err-v1.
#[derive(Debug, Error)]
pub enum SkillsError {
    /// Requested skill name was not found in the embedded catalog.
    #[error(
        "skill '{0}' not found\n  run 'cobrust skills list' to see available skills"
    )]
    NotFound(String),

    /// Embedded asset bytes are not valid UTF-8 (indicates a broken embed).
    #[error("skill '{0}' is corrupt (non-UTF-8 bytes in embedded asset)")]
    Corrupt(String),

    /// JSON serialisation failed (only possible for `--json` path).
    #[error("JSON serialisation failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Arguments for the `cobrust skills` subcommand.
#[derive(Debug, clap::Subcommand)]
pub enum SkillsArgs {
    /// List all available embedded skill names (one per line).
    List,
    /// Print the content of a named skill.
    Get {
        /// Skill name (e.g. `cobrust-language`). Run `cobrust skills list` to see names.
        name: String,
        /// Output as JSON object `{"name": "…", "version": "…", "content": "…"}`.
        #[arg(long)]
        json: bool,
    },
}

/// Return sorted skill names (file stem without `.md` extension).
///
/// F34 anchor: skills-list-v1.
pub fn list_skills() -> Vec<String> {
    let mut names: Vec<String> = SkillAssets::iter()
        .map(|f: std::borrow::Cow<'static, str>| {
            f.trim_end_matches(".md").to_owned()
        })
        .collect();
    names.sort();
    names
}

/// Return the raw markdown bytes for a named skill.
///
/// The `name` argument is the file stem (no `.md`).
/// Returns `None` if no such skill is embedded.
///
/// F34 anchor: skills-get-v1.
pub fn get_skill(name: &str) -> Option<Cow<'static, [u8]>> {
    let key = format!("{name}.md");
    SkillAssets::get(&key).map(|asset| asset.data)
}

/// Dispatch the `cobrust skills` subcommand.
///
/// Returns exit code: 0 = success, 1 = skill not found.
pub fn cmd_skills(args: &SkillsArgs) -> u8 {
    match args {
        SkillsArgs::List => {
            for name in list_skills() {
                println!("{name}");
            }
            0
        }
        SkillsArgs::Get { name, json } => {
            match run_get(name, *json) {
                Ok(()) => 0,
                Err(SkillsError::NotFound(msg)) => {
                    eprintln!("error: {msg}");
                    1
                }
                Err(SkillsError::Corrupt(msg)) => {
                    eprintln!("error: {msg}");
                    1
                }
                Err(SkillsError::JsonError(e)) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
    }
}

/// Inner implementation for `cobrust skills get`.
fn run_get(name: &str, json: bool) -> Result<(), SkillsError> {
    let bytes = get_skill(name).ok_or_else(|| SkillsError::NotFound(name.to_owned()))?;
    let content = std::str::from_utf8(bytes.as_ref())
        .map_err(|_| SkillsError::Corrupt(name.to_owned()))?;

    if json {
        let obj = serde_json::json!({
            "name": name,
            "version": env!("CARGO_PKG_VERSION"),
            "content": content,
        });
        println!("{}", serde_json::to_string(&obj)?);
    } else {
        print!("{content}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // F36: test name matches assertion
    #[test]
    fn list_skills_returns_four_expected_names() {
        let names = list_skills();
        assert!(
            names.contains(&"cobrust-language".to_owned()),
            "cobrust-language not in list: {names:?}"
        );
        assert!(
            names.contains(&"cobrust-stdlib".to_owned()),
            "cobrust-stdlib not in list: {names:?}"
        );
        assert!(
            names.contains(&"cobrust-error-codes".to_owned()),
            "cobrust-error-codes not in list: {names:?}"
        );
        assert!(
            names.contains(&"cobrust-debugger".to_owned()),
            "cobrust-debugger not in list: {names:?}"
        );
        assert_eq!(names.len(), 5, "expected 5 embedded skills (4 new + cobrust-first-try)");
    }

    // F36: test name matches assertion
    #[test]
    fn get_skill_cobrust_language_contains_py_compat() {
        let bytes = get_skill("cobrust-language").expect("cobrust-language must be embedded");
        let content = std::str::from_utf8(bytes.as_ref()).expect("valid UTF-8");
        assert!(
            content.contains("@py_compat"),
            "cobrust-language must mention @py_compat"
        );
    }

    // F36: test name matches assertion
    #[test]
    fn get_skill_nonexistent_returns_none() {
        assert!(get_skill("does-not-exist").is_none());
    }
}
