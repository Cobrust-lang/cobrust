//! M14 REPL golden-session corpus replay (per ADR-0029 §"Public surface").
//!
//! Replays the 50 curated session scripts from `examples/repl-session.txt`
//! against the live `cobrust repl` binary. Each session block is:
//!
//! - `=== <name>` — session header
//! - `> <line>` — REPL input
//! - `= <substr>` — required substring in stdout
//! - `e <substr>` — required substring in stderr
//!
//! Per ADR-0019 §"M14 — REPL" binding done-means: 50 curated session
//! scripts produce expected outputs.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

#[derive(Debug, Default)]
struct Session {
    name: String,
    inputs: Vec<String>,
    stdout_expects: Vec<String>,
    stderr_expects: Vec<String>,
}

fn parse_corpus(text: &str) -> Vec<Session> {
    let mut sessions = Vec::new();
    let mut current: Option<Session> = None;
    for raw in text.lines() {
        // Comments outside of session bodies (and blank lines): only
        // treat lines that don't begin with `>`/`=`/`e`/`===` as comments.
        if raw.starts_with('#') {
            continue;
        }
        if let Some(rest) = raw.strip_prefix("=== ") {
            if let Some(s) = current.take() {
                sessions.push(s);
            }
            current = Some(Session {
                name: rest.trim().to_string(),
                ..Default::default()
            });
        } else if let Some(s) = current.as_mut() {
            if let Some(input) = raw.strip_prefix("> ") {
                s.inputs.push(input.to_string());
            } else if raw == ">" {
                s.inputs.push(String::new());
            } else if let Some(out) = raw.strip_prefix("= ") {
                s.stdout_expects.push(out.to_string());
            } else if raw == "= " || raw == "=" {
                // empty stdout-expect — used to mean "don't care" for
                // blank-input no-op cases.
                s.stdout_expects.push(String::new());
            } else if let Some(err) = raw.strip_prefix("e ") {
                s.stderr_expects.push(err.to_string());
            }
            // any other line is treated as a comment and skipped
        }
    }
    if let Some(s) = current {
        sessions.push(s);
    }
    sessions
}

fn drive_repl(inputs: &[String]) -> (String, String, i32) {
    let bin = cobrust_binary();
    // Each session sends its inputs then `:quit` to exit cleanly.
    let mut script = String::new();
    for line in inputs {
        script.push_str(line);
        script.push('\n');
    }
    script.push_str(":quit\n");

    let mut child = Command::new(&bin)
        .arg("repl")
        .env("HOME", "/tmp/cobrust-repl-corpus-home")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cobrust repl");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn corpus_replays_all_50_sessions_successfully() {
    let corpus_path = workspace_root().join("examples/repl-session.txt");
    let text = std::fs::read_to_string(&corpus_path)
        .unwrap_or_else(|e| panic!("missing {}: {e}", corpus_path.display()));
    let sessions = parse_corpus(&text);
    assert!(
        sessions.len() >= 50,
        "expected ≥ 50 sessions in corpus, got {}",
        sessions.len()
    );

    let mut failures = Vec::new();
    for (i, s) in sessions.iter().enumerate() {
        let (stdout, stderr, code) = drive_repl(&s.inputs);
        if code != 0 {
            failures.push(format!(
                "[{:02}] {}: exited with code {} stderr={:?}",
                i + 1,
                s.name,
                code,
                stderr
            ));
            continue;
        }
        for expected in &s.stdout_expects {
            if expected.is_empty() {
                continue; // no-op marker
            }
            if !stdout.contains(expected) {
                failures.push(format!(
                    "[{:02}] {}: stdout missing {expected:?}\n--- stdout ---\n{stdout}\n---",
                    i + 1,
                    s.name,
                ));
            }
        }
        for expected in &s.stderr_expects {
            if !stderr.contains(expected) {
                failures.push(format!(
                    "[{:02}] {}: stderr missing {expected:?}\n--- stderr ---\n{stderr}\n---",
                    i + 1,
                    s.name,
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} session(s) failed:\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

#[test]
fn corpus_file_present_at_workspace_root() {
    let corpus_path = workspace_root().join("examples/repl-session.txt");
    assert!(
        corpus_path.exists(),
        "examples/repl-session.txt missing at {}",
        corpus_path.display()
    );
}

#[test]
fn corpus_has_at_least_50_sessions() {
    let corpus_path = workspace_root().join("examples/repl-session.txt");
    let text = std::fs::read_to_string(&corpus_path).expect("read corpus");
    let count = text.lines().filter(|l| l.starts_with("=== ")).count();
    assert!(
        count >= 50,
        "expected ≥ 50 sessions (=== headers), got {count}"
    );
}
