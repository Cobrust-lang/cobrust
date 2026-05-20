//! Integration tests for `cobrust skills` subcommand (ADR-0061 §6).
//!
//! Three acceptance gates required for ADR-0061 to be marked `accepted`:
//!
//! 1. `test_skills_list_nonempty` — `cobrust skills list` exits 0 + stdout
//!    contains at least the expected 4 skill names.
//! 2. `test_skills_get_language_returns_content` — `cobrust skills get cobrust-language`
//!    exits 0 + stdout length > 100 bytes + contains `@py_compat`.
//! 3. `test_skills_get_json_valid` — `cobrust skills get cobrust-language --json`
//!    exits 0 + stdout is valid JSON + has keys `name`, `version`, `content`.
//!
//! F34 anchor: test-skills-integration-v1
//! F36: test fn names match their assertion scope.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn cobrust_bin() -> PathBuf {
    // Use CARGO_BIN_EXE_cobrust env var injected by cargo test when building
    // the integration test for the cobrust binary.
    // Fall back to workspace target directory for robustness.
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_cobrust") {
        return PathBuf::from(p);
    }
    workspace_root()
        .join("target")
        .join("debug")
        .join("cobrust")
}

// F36: name matches assertion — list exits 0 + contains all four expected names
#[test]
fn test_skills_list_nonempty() {
    let output = Command::new(cobrust_bin())
        .args(["skills", "list"])
        .output()
        .expect("failed to spawn cobrust skills list");

    assert!(
        output.status.success(),
        "cobrust skills list must exit 0; got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be UTF-8");
    assert!(
        !stdout.trim().is_empty(),
        "cobrust skills list stdout must be non-empty"
    );

    let expected_names = [
        "cobrust-language",
        "cobrust-stdlib",
        "cobrust-error-codes",
        "cobrust-debugger",
    ];
    for name in expected_names {
        assert!(
            stdout.lines().any(|l| l.trim() == name),
            "cobrust skills list must include '{name}'\nactual stdout:\n{stdout}"
        );
    }
}

// F36: name matches assertion — get exits 0 + non-trivial content + contains @py_compat
#[test]
fn test_skills_get_language_returns_content() {
    let output = Command::new(cobrust_bin())
        .args(["skills", "get", "cobrust-language"])
        .output()
        .expect("failed to spawn cobrust skills get cobrust-language");

    assert!(
        output.status.success(),
        "cobrust skills get cobrust-language must exit 0; got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be UTF-8");
    assert!(
        stdout.len() > 100,
        "cobrust skills get cobrust-language stdout must be > 100 bytes; got {} bytes",
        stdout.len()
    );

    assert!(
        stdout.contains("@py_compat"),
        "cobrust-language skill must contain '@py_compat'; actual stdout snippet:\n{}",
        &stdout[..std::cmp::min(500, stdout.len())]
    );
}

// F36: name matches assertion — --json exits 0 + valid JSON + keys name/version/content
#[test]
fn test_skills_get_json_valid() {
    let output = Command::new(cobrust_bin())
        .args(["skills", "get", "cobrust-language", "--json"])
        .output()
        .expect("failed to spawn cobrust skills get cobrust-language --json");

    assert!(
        output.status.success(),
        "cobrust skills get cobrust-language --json must exit 0; got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be UTF-8");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON");

    assert!(
        parsed.is_object(),
        "cobrust skills get --json must produce a JSON object"
    );

    let obj = parsed.as_object().unwrap();
    assert!(obj.contains_key("name"), "JSON object must have key 'name'");
    assert!(
        obj.contains_key("version"),
        "JSON object must have key 'version'"
    );
    assert!(
        obj.contains_key("content"),
        "JSON object must have key 'content'"
    );

    assert_eq!(
        obj["name"].as_str().unwrap(),
        "cobrust-language",
        "JSON 'name' field must equal 'cobrust-language'"
    );

    let content = obj["content"].as_str().unwrap();
    assert!(
        content.len() > 100,
        "JSON 'content' must be > 100 bytes; got {}",
        content.len()
    );
}
