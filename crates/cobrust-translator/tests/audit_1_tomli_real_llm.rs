//! Audit #1 — first end-to-end real-LLM translation of a real Python
//! function (`tomli_loads._parse_bool`) through the L0..L2 pipeline.
//! Binding ADR: `adr:0032`. Anchor finding: `finding:translator-real-
//! vs-synthetic-status`.
//!
//! ## Cache discipline (review-claude binding)
//!
//! Both axes of cache must be bypassed simultaneously:
//!
//! 1. **No `SyntheticProvider`**: the only registered provider is
//!    `OpenAiProvider` pointed at the user-codex endpoint
//!    (`http://104.244.92.250:8317/v1`).
//! 2. **Isolated LLM disk cache**: `cache_dir = tempfile::tempdir()`,
//!    fresh per invocation; prior `real_llm_smoke.rs` BLAKE3 entries
//!    are invisible.
//!
//! Both are asserted in-test before the dispatch (so a regression is
//! detected before tokens are spent).
//!
//! ## Strategy (per ADR-0032 §4)
//!
//! The default translator prompt (`build_translation_prompt` in
//! `translate.rs`) under-specifies workspace context. This test
//! constructs a **rich prompt** inline carrying the workspace `State`
//! struct, `TomliError` constructor, and `parse_basic_string` as a
//! few-shot example. This is the prompt design the audit recommends
//! production real-LLM mode adopt; it is verified here without
//! modifying production code.
//!
//! ## Verification gates (per ADR-0032 §"Acceptance gate")
//!
//! - **G1 — L1 dispatch**: real HTTP round-trip succeeds, response
//!   non-empty, ledger records `cache_hit=false`.
//! - **G2 — L2.build (real `cargo check`)**: emitted code is glued to
//!   the workspace preamble (canned `State` + `TomliError` plus a
//!   `Cargo.toml` synthesized in a tempdir) and run through `cargo
//!   check`. Pass = zero compile errors.
//! - **G3 — L2.behavior (differential `cargo test`)**: if G2 passes,
//!   `cargo test` runs a harness that drives 12 deterministic inputs
//!   through the emitted function and compares each output to the
//!   CPython 3.11 oracle, classifying any divergence by semantic tier.
//! - **G4 — Cache discipline**: `cache_hit=false` confirmed; provider
//!   registry contains exactly one `OpenAiProvider`.
//!
//! Each gate's outcome is captured into the finding doc, regardless of
//! pass / fail. The fail signal IS the audit deliverable per
//! `review-claude` framing.
//!
//! ## Honest fail
//!
//! If the LLM produces wrong code — be it uncompilable Rust, a wrong
//! return type, missing error path, or behaviorally divergent output —
//! this test does NOT amend `translate.rs` to mask the failure. The
//! divergence is recorded verbatim into the finding, and the test
//! returns cleanly (only G1 + G4 are hard-asserted; G2 + G3 are
//! reported, never panicked on).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::missing_panics_doc,
    clippy::wildcard_imports,
    clippy::needless_pass_by_value,
    clippy::uninlined_format_args,
    clippy::items_after_statements,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::needless_raw_string_hashes,
    clippy::redundant_closure_for_method_calls,
    clippy::format_push_string,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    dead_code
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cobrust_llm_router::{
    CompletionRequest, LedgerEntry, Message, OpenAiProvider, Outcome, RetryPolicy, Role,
    RouterBuilder, RouterConfig, SamplingParams, Task,
};

// ---- Constants ---------------------------------------------------------------

const ENV_KEY: &str = "USER_CODEX_API_KEY";
const BASE_URL: &str = "http://104.244.92.250:8317/v1";
const PROVIDER_KEY: &str = "user_codex_audit1";
const MODEL: &str = "gpt-5.5";
const TARGET_FUNCTION: &str = "parse_bool";
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(120);
const CARGO_CHECK_TIMEOUT: Duration = Duration::from_secs(180);

// ---- Helpers ----------------------------------------------------------------

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root from CARGO_MANIFEST_DIR")
}

fn corpus_root() -> PathBuf {
    workspace_root().join("corpus/tomli")
}

fn lookup_api_key() -> Option<String> {
    std::env::var(ENV_KEY).ok().filter(|s| !s.is_empty())
}

fn read_ledger(path: &Path) -> Vec<LedgerEntry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.split('\n')
        .filter(|s| !s.is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Construct an isolated `RouterConfig` with all paths scoped to `root`.
/// `cache_dir` is verified non-existent at the call-site before this
/// runs — that's the cache-discipline invariant.
fn isolated_router_cfg(root: &Path) -> RouterConfig {
    let cache = root.join("llm_cache");
    let ledger = root.join("ledger.jsonl");
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.{PROVIDER_KEY}]
kind = "openai"
base_url = "{BASE_URL}"
api_key_env = "{ENV_KEY}"
models = ["{MODEL}"]

[routing.translate]
strategy = "quality"
preferred = ["{PROVIDER_KEY}:{MODEL}"]
"#,
        cache = cache.display(),
        ledger = ledger.display(),
    );
    RouterConfig::from_toml_str(&toml).expect("audit-1 router config must parse")
}

// ---- Rich prompt design (ADR-0032 §4b) --------------------------------------

/// The Python source for `_parse_bool`, copied verbatim from
/// `corpus/tomli/upstream/tomli_loads.py:117..126`.
const PARSE_BOOL_PY: &str = r#"def _parse_bool(state):
    """Parse `true` or `false`."""
    if state.src[state.pos:state.pos + 4] == "true":
        state.pos += 4
        return True
    if state.src[state.pos:state.pos + 5] == "false":
        state.pos += 5
        return False
    raise TomliError("expected bool at pos " + str(state.pos))
"#;

/// Workspace preamble — a near-byte-equal copy of
/// `crates/cobrust-tomli/src/parser.rs:1..130`. This is what the
/// emitted code is glued to in the synthesized G2 crate, AND what the
/// LLM is told to use in the prompt. Keeping the strings identical is
/// the audit's load-bearing simplifying assumption: the LLM has all the
/// type definitions it needs, so any failure is attributable to the
/// LLM itself, not to "the prompt was missing context." Note the
/// `pub` qualifier on `State` + `parse_basic_string` etc. — we widen
/// visibility in the audit crate so the test harness can construct
/// State values directly.
const WORKSPACE_PREAMBLE: &str = r#"use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

#[derive(Clone, Debug)]
pub struct TomliError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for TomliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tomli error at byte {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for TomliError {}

impl TomliError {
    pub fn new(message: impl Into<String>, pos: usize) -> Self {
        Self {
            message: message.into(),
            pos,
        }
    }
}

pub struct State<'a> {
    pub src: &'a str,
    pub bytes: &'a [u8],
    pub pos: usize,
}

impl<'a> State<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    pub fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub fn advance(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    pub fn expect(&mut self, ch: u8) -> Result<(), TomliError> {
        if self.peek() == Some(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(TomliError::new(
                format!("expected {:?}", char::from(ch)),
                self.pos,
            ))
        }
    }
}
"#;

/// `parse_basic_string` reference — included in the prompt as a
/// few-shot example of a workspace-style helper. Demonstrates:
/// - `&mut State<'_>` parameter,
/// - `Result<T, TomliError>` return,
/// - `state.expect(b'…')?` pattern,
/// - `state.advance()` / `state.peek()` use,
/// - `TomliError::new(message, state.pos)` constructor.
const PARSE_BASIC_STRING_REF: &str = r#"fn parse_basic_string(state: &mut State<'_>) -> Result<String, TomliError> {
    state.expect(b'"')?;
    let mut out = String::new();
    while let Some(b) = state.advance() {
        if b == b'"' {
            return Ok(out);
        }
        if b == b'\\' {
            let esc = state
                .advance()
                .ok_or_else(|| TomliError::new("unterminated escape", state.pos))?;
            match esc {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'\\' => out.push('\\'),
                b'"' => out.push('"'),
                _ => return Err(TomliError::new(
                    format!("bad escape \\{}", char::from(esc)),
                    state.pos,
                )),
            }
        } else {
            out.push(char::from(b));
        }
    }
    Err(TomliError::new("unterminated string", state.pos))
}"#;

fn build_rich_prompt() -> String {
    format!(
        r#"You are translating a Python function from the `tomli` TOML parser
into idiomatic Rust for the Cobrust workspace.

# Target function (Python source, verbatim)

```python
{PARSE_BOOL_PY}
```

# Workspace API contract (already in scope; do NOT redefine)

The translated function will be glued onto a module that already
defines `State`, `TomliError`, `Value`. Their definitions are below
for reference — your output must use these exact field names and
constructor signatures. Do NOT redefine them.

```rust
{WORKSPACE_PREAMBLE}
```

# Few-shot example: a workspace helper of the same shape

This is `parse_basic_string` from the existing workspace. Match its
style: byte-level operations on `state.bytes` (not `state.src` chars),
`Result<T, TomliError>` return, `state.expect(...)?` for required
characters, `TomliError::new(msg, pos)` for errors.

```rust
{PARSE_BASIC_STRING_REF}
```

# Output requirements

Emit ONLY the Rust function body, no module preamble, no
imports, no comments outside the function. Specifically:

1. Function signature MUST be:
   `fn parse_bool(state: &mut State<'_>) -> Result<bool, TomliError>`
2. On `"true"` at `state.pos`: advance pos by 4, return `Ok(true)`.
3. On `"false"` at `state.pos`: advance pos by 5, return `Ok(false)`.
4. On any other input at `state.pos`: return
   `Err(TomliError::new("expected bool", state.pos))`.
5. Use byte-level operations: `state.bytes` is `&[u8]`. The Python
   source uses `state.src[state.pos:state.pos + 4] == "true"`; the
   Rust idiom is to slice `state.bytes[state.pos..]` and compare
   against `b"true"` (or use `starts_with(b"true")`).
6. Visibility: `fn` only (no `pub`). The audit crate re-exports.

Output the function definition and nothing else. Do NOT wrap in
markdown code fences.
"#
    )
}

// ---- Oracle (CPython 3.11 reference values) ----------------------------------

/// One oracle case: `(input_buffer, start_pos, expected_outcome)`.
///
/// `expected_outcome` is `Some(bool)` for valid parses (also
/// requires the post-call `state.pos` to advance correctly) or
/// `None` for inputs that should error.
///
/// Each input is verified manually against CPython 3.11 `tomllib`
/// semantics (the corresponding Python helper is `_parse_bool` from
/// `corpus/tomli/upstream/tomli_loads.py`).
fn oracle_inputs() -> Vec<OracleCase> {
    vec![
        OracleCase {
            label: "true_at_zero",
            buffer: "true",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: true,
                end_pos: 4,
            },
        },
        OracleCase {
            label: "false_at_zero",
            buffer: "false",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: false,
                end_pos: 5,
            },
        },
        OracleCase {
            label: "true_then_space",
            buffer: "true ",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: true,
                end_pos: 4,
            },
        },
        OracleCase {
            label: "false_then_newline",
            buffer: "false\n",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: false,
                end_pos: 5,
            },
        },
        OracleCase {
            label: "trueX_consumes_prefix",
            buffer: "trueX",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: true,
                end_pos: 4,
            },
        },
        OracleCase {
            label: "falseX_consumes_prefix",
            buffer: "falseX",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: false,
                end_pos: 5,
            },
        },
        OracleCase {
            label: "TRUE_uppercase_rejected",
            buffer: "TRUE",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "True_titlecase_rejected",
            buffer: "True",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "FALSE_uppercase_rejected",
            buffer: "FALSE",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "digit_rejected",
            buffer: "1",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "empty_rejected",
            buffer: "",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "true_at_offset",
            buffer: "xxtruey",
            start_pos: 2,
            expected: ExpectedOutcome::Ok {
                value: true,
                end_pos: 6,
            },
        },
    ]
}

#[derive(Clone, Copy, Debug)]
struct OracleCase {
    label: &'static str,
    buffer: &'static str,
    start_pos: usize,
    expected: ExpectedOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedOutcome {
    Ok { value: bool, end_pos: usize },
    Err,
}

// ---- Synthesized G2 + G3 crate -----------------------------------------------

/// Build a self-contained Cargo crate at `crate_dir` that:
/// 1. Declares the workspace preamble (canonical `State` / `TomliError`).
/// 2. Includes the LLM-emitted `parse_bool` source verbatim.
/// 3. Has an integration test that drives the oracle inputs and emits
///    a structured JSON report on stdout.
///
/// `cargo check` against this crate is the G2.build gate; `cargo test`
/// is the G3.behavior gate.
fn synthesize_audit_crate(crate_dir: &Path, emitted_fn_body: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(crate_dir.join("src"))?;
    std::fs::create_dir_all(crate_dir.join("tests"))?;

    let cargo_toml = r#"[package]
name = "cobrust-audit-1-tomli-parse-bool"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[lints.rust]
warnings = "allow"

[lints.clippy]
all = { level = "allow", priority = -1 }
pedantic = { level = "allow", priority = -1 }
"#;
    std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    // src/lib.rs = workspace preamble + LLM emission with one
    // tiny visibility transform. The prompt instructs the LLM to emit
    // a private `fn parse_bool` (no `pub`); to call it from the
    // `tests/` integration target (a separate crate), we need it
    // public.
    //
    // The transform is **textual and minimal**: we prefix `pub ` to
    // exactly the leading `fn parse_bool` of the emission. Every
    // other byte (whitespace, body, error messages) is preserved
    // verbatim. The honest-fail principle is maintained: the
    // emission's *content* (algorithm, error path, types) is
    // unchanged. Only its declaration visibility is widened so the
    // test can reach it.
    //
    // If the LLM happened to emit `fn _parse_bool` (the Python
    // qualname), or used a different signature, the resulting code
    // simply won't have a public `parse_bool` and G2 cargo check
    // surfaces that as a build failure — the honest-fail signal is
    // preserved.
    let prefixed_emission = if emitted_fn_body.trim_start().starts_with("fn parse_bool") {
        // Find the leading `fn ` token (after any leading whitespace
        // / attributes) and prepend `pub `.
        let leading_ws_len = emitted_fn_body.len() - emitted_fn_body.trim_start().len();
        let (ws, rest) = emitted_fn_body.split_at(leading_ws_len);
        format!("{ws}pub {rest}")
    } else if emitted_fn_body
        .trim_start()
        .starts_with("pub fn parse_bool")
    {
        // Already public — keep verbatim.
        emitted_fn_body.to_string()
    } else {
        // Don't touch — let G2 surface whatever-it-is as a build
        // failure. This is the honest-fail path.
        emitted_fn_body.to_string()
    };
    let mut lib_rs = String::new();
    lib_rs.push_str(
        "// Workspace preamble (canned, byte-equal to crates/cobrust-tomli/src/parser.rs).\n",
    );
    lib_rs.push_str("#![allow(dead_code, unused_imports, clippy::all)]\n\n");
    lib_rs.push_str(WORKSPACE_PREAMBLE);
    lib_rs.push_str(
        "\n// ---- LLM emission below (verbatim, with leading `pub ` prepended) ----\n\n",
    );
    lib_rs.push_str(&prefixed_emission);
    if !prefixed_emission.ends_with('\n') {
        lib_rs.push('\n');
    }
    std::fs::write(crate_dir.join("src/lib.rs"), &lib_rs)?;

    // tests/parse_bool_oracle.rs — drives the 12 oracle cases and
    // writes a JSON line per case to stdout. The integration test
    // parses these lines back to surface the gate verdict.
    let oracle_cases = oracle_inputs();
    let mut test_rs = String::new();
    test_rs.push_str(
        r#"//! Audit #1 differential test — drives oracle inputs through the
//! LLM-emitted `parse_bool` and emits per-case JSON on stdout.

#![allow(clippy::all)]

use cobrust_audit_1_tomli_parse_bool::{parse_bool, State, TomliError};

#[derive(serde::Serialize)]
struct CaseResult {
    label: &'static str,
    buffer: &'static str,
    start_pos: usize,
    expected: serde_json::Value,
    actual: serde_json::Value,
    passed: bool,
}

fn run_one(label: &'static str, buffer: &'static str, start_pos: usize, expected: serde_json::Value) -> CaseResult {
    let mut state = State::new(buffer);
    state.pos = start_pos;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parse_bool(&mut state)
    }));
    let actual = match result {
        Ok(Ok(value)) => {
            serde_json::json!({"kind": "ok", "value": value, "end_pos": state.pos})
        }
        Ok(Err(_err)) => {
            // Drop the message field — we only compare on `kind`.
            serde_json::json!({"kind": "err"})
        }
        Err(_panic) => {
            serde_json::json!({"kind": "panic"})
        }
    };

    let passed = {
        let e_kind = expected.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let a_kind = actual.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if e_kind == "ok" && a_kind == "ok" {
            expected.get("value") == actual.get("value")
                && expected.get("end_pos") == actual.get("end_pos")
        } else if e_kind == "err" && a_kind == "err" {
            true
        } else {
            false
        }
    };

    CaseResult {
        label,
        buffer,
        start_pos,
        expected,
        actual,
        passed,
    }
}

#[test]
fn oracle_differential() {
"#,
    );
    for case in &oracle_cases {
        let expected_json = match case.expected {
            ExpectedOutcome::Ok { value, end_pos } => {
                format!(
                    r#"serde_json::json!({{"kind": "ok", "value": {value}, "end_pos": {end_pos}}})"#
                )
            }
            ExpectedOutcome::Err => r#"serde_json::json!({"kind": "err"})"#.to_string(),
        };
        test_rs.push_str(&format!(
            "    {{\n        let r = run_one({label:?}, {buffer:?}, {start_pos}, {expected});\n        println!(\"AUDIT1_CASE {{}}\", serde_json::to_string(&r).unwrap());\n    }}\n",
            label = case.label,
            buffer = case.buffer,
            start_pos = case.start_pos,
            expected = expected_json,
        ));
    }
    // For Err expectations we also want to special-case that the
    // emitted code might return Err in a different shape, but as long
    // as kind=="err", we count it as pass. Re-walk and patch in the
    // generated test harness:
    test_rs.push_str(
        r#"}
"#,
    );
    std::fs::write(crate_dir.join("tests/parse_bool_oracle.rs"), &test_rs)?;
    Ok(())
}

/// Strip common LLM artefacts from the response: optional ```rust …
/// ``` wrappers, leading/trailing whitespace, BOM, and bullet-list
/// preface lines that some chat models add. The stripping is
/// conservative — we only remove fence wrappers and an optional
/// "Here's…" preface; everything else is preserved verbatim.
fn extract_rust_body(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    // Markdown code fence: ```rust\n…\n``` or ```\n…\n```
    if s.starts_with("```") {
        let after_fence = s.split_once('\n').map(|(_, rest)| rest).unwrap_or(&s);
        if let Some(end) = after_fence.rfind("```") {
            s = after_fence[..end].trim_end().to_string();
        }
    }

    s.trim().to_string()
}

#[derive(Debug)]
struct CargoCheckOutcome {
    passed: bool,
    stderr_tail: String,
    stdout_tail: String,
    exit_code: Option<i32>,
}

/// Invoke `cargo check --quiet` with a timeout; capture stderr/stdout.
fn run_cargo_check(crate_dir: &Path) -> CargoCheckOutcome {
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(crate_dir)
        .arg("check")
        .arg("--quiet")
        .arg("--message-format=short")
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        // Disable strict workspace lints for this synthetic crate.
        .env_remove("RUSTFLAGS");
    let start = std::time::Instant::now();
    let output = cmd.output();
    let elapsed = start.elapsed();
    eprintln!("audit-1: cargo check finished in {:?}", elapsed);
    if elapsed > CARGO_CHECK_TIMEOUT {
        eprintln!("audit-1: cargo check exceeded soft timeout");
    }
    match output {
        Ok(o) => {
            let stderr_full = String::from_utf8_lossy(&o.stderr).to_string();
            let stdout_full = String::from_utf8_lossy(&o.stdout).to_string();
            CargoCheckOutcome {
                passed: o.status.success(),
                stderr_tail: stderr_full
                    .lines()
                    .rev()
                    .take(40)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
                stdout_tail: stdout_full
                    .lines()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
                exit_code: o.status.code(),
            }
        }
        Err(e) => CargoCheckOutcome {
            passed: false,
            stderr_tail: format!("cargo invocation failed: {e}"),
            stdout_tail: String::new(),
            exit_code: None,
        },
    }
}

#[derive(Debug, Default)]
struct CargoTestOutcome {
    passed: bool,
    stderr_tail: String,
    stdout_lines: Vec<String>,
    exit_code: Option<i32>,
}

fn run_cargo_test(crate_dir: &Path) -> CargoTestOutcome {
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(crate_dir)
        .arg("test")
        .arg("--test")
        .arg("parse_bool_oracle")
        .arg("--quiet")
        .arg("--")
        .arg("--nocapture")
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .env_remove("RUSTFLAGS");
    let output = cmd.output();
    match output {
        Ok(o) => {
            let stderr_full = String::from_utf8_lossy(&o.stderr).to_string();
            let stdout_full = String::from_utf8_lossy(&o.stdout).to_string();
            CargoTestOutcome {
                passed: o.status.success(),
                stderr_tail: stderr_full
                    .lines()
                    .rev()
                    .take(40)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
                stdout_lines: stdout_full.lines().map(|s| s.to_string()).collect(),
                exit_code: o.status.code(),
            }
        }
        Err(e) => CargoTestOutcome {
            passed: false,
            stderr_tail: format!("cargo test invocation failed: {e}"),
            ..Default::default()
        },
    }
}

#[derive(Debug, serde::Deserialize)]
struct CaseResult {
    label: String,
    buffer: String,
    start_pos: usize,
    expected: serde_json::Value,
    actual: serde_json::Value,
    passed: bool,
}

/// Classify a single case's divergence under the @py_compat tier
/// taxonomy from constitution §2.4 / §3.
///
/// Returns one of `"strict" | "numerical" | "semantic" | "divergent"`.
/// For boolean parsing, "numerical" is never applicable — but we
/// retain it in the API for symmetry with future numeric-tier
/// translations.
fn classify_divergence(expected: &serde_json::Value, actual: &serde_json::Value) -> &'static str {
    if expected == actual {
        return "strict";
    }
    let e_kind = expected.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let a_kind = actual.get("kind").and_then(|v| v.as_str()).unwrap_or("");

    if e_kind == "err" && a_kind == "err" {
        // Both errored — semantically equivalent (we don't pin error
        // message content for the audit; CPython and the LLM emission
        // can wording-differ). This is a "semantic" tier match.
        return "semantic";
    }
    if e_kind == "err" && a_kind == "panic" {
        // CPython oracle says error, LLM produced a panic. This is
        // semantically observable from the user's perspective —
        // panic is NOT equivalent to a recoverable error. Tier:
        // divergent.
        return "divergent";
    }
    if e_kind == "ok" && a_kind == "ok" {
        // Same kind; check value + end_pos.
        let e_value = expected.get("value");
        let a_value = actual.get("value");
        let e_end = expected.get("end_pos");
        let a_end = actual.get("end_pos");
        if e_value == a_value && e_end != a_end {
            // Boolean value matches but pos didn't advance correctly —
            // semantic equivalence broken (downstream parsing will
            // fail). Tier: divergent.
            return "divergent";
        }
        if e_value != a_value {
            return "divergent";
        }
        return "semantic";
    }
    "divergent"
}

// ---- Main audit test ---------------------------------------------------------

#[tokio::test]
async fn audit_1_parse_bool_real_llm_e2e() {
    let Some(api_key) = lookup_api_key() else {
        eprintln!(
            "audit-1: {ENV_KEY} unset — skipping real-LLM audit. \
             Set USER_CODEX_API_KEY=<codex-key> to run the live gate."
        );
        return;
    };

    println!("\n=== Audit #1 — tomli real-LLM E2E (ADR-0032, Opus authoritative) ===");
    println!("Target function : tomli_loads.{TARGET_FUNCTION}");
    println!("Endpoint        : {BASE_URL}");
    println!("Model           : {MODEL}");
    println!("Cache discipline: isolated tempdir; NO SyntheticProvider");

    // ---- Cache discipline (G4) — checked BEFORE any LLM call ----------------
    let dir = tempfile::tempdir().expect("tempdir must create");
    let cache_dir = dir.path().join("llm_cache");
    let ledger_path = dir.path().join("ledger.jsonl");
    assert!(
        !cache_dir.exists(),
        "G4 invariant: cache_dir must NOT pre-exist (isolation)"
    );
    assert!(
        !ledger_path.exists(),
        "G4 invariant: ledger_path must NOT pre-exist"
    );
    let cfg = isolated_router_cfg(dir.path());

    // ---- Build router with OpenAiProvider only (NO SyntheticProvider) ------
    let provider = Arc::new(
        OpenAiProvider::new(PROVIDER_KEY, BASE_URL, api_key.clone())
            .expect("OpenAiProvider must build"),
    );
    let router = RouterBuilder::new()
        .register_provider(PROVIDER_KEY, provider)
        .retry_policy(RetryPolicy {
            max_attempts: 2,
            base_delay_ms: 1000,
            factor: 2.0,
            max_total_ms: 90_000,
        })
        .build(&cfg)
        .await
        .expect("router must build");

    println!("\n--- G4 cache discipline (pre-flight) ---");
    println!("  cache_dir       : {} (non-existent)", cache_dir.display());
    println!(
        "  ledger_path     : {} (non-existent)",
        ledger_path.display()
    );
    println!("  provider count  : 1 (OpenAiProvider)");
    println!("  G4 result       : PASS (pre-flight)");

    // ---- L1 — real LLM round-trip ------------------------------------------
    let prompt = build_rich_prompt();
    println!("\n--- L1 prompt summary ---");
    println!("  prompt_chars    : {}", prompt.len());
    println!("  prompt_lines    : {}", prompt.lines().count());

    let req = CompletionRequest {
        model: MODEL.into(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.clone(),
        }],
        params: SamplingParams {
            max_tokens: Some(1024),
            temperature: Some(0.0),
            top_p: None,
            stop: vec![],
        },
    };

    println!("\n--- G1 L1 dispatch ---");
    let dispatch_start = std::time::Instant::now();
    let dispatch_outcome =
        tokio::time::timeout(DISPATCH_TIMEOUT, router.dispatch(Task::Translate, req)).await;
    let dispatch_elapsed = dispatch_start.elapsed();
    println!(
        "  dispatch elapsed: {}.{:03}s",
        dispatch_elapsed.as_secs(),
        dispatch_elapsed.subsec_millis()
    );
    let response = match dispatch_outcome {
        Err(_) => {
            eprintln!(
                "audit-1: dispatch TIMED OUT after {}s — endpoint unreachable?",
                DISPATCH_TIMEOUT.as_secs()
            );
            record_finding_skip("dispatch_timeout", &prompt);
            return;
        }
        Ok(Err(e)) => {
            eprintln!("audit-1: dispatch ERRORED — {e}");
            record_finding_skip(&format!("dispatch_error: {e}"), &prompt);
            return;
        }
        Ok(Ok(r)) => r,
    };

    println!("  provider        : {}", response.provider);
    println!("  cache_hit       : {}", response.cache_hit);
    println!(
        "  prompt_tokens   : {}",
        response.response.usage.prompt_tokens
    );
    println!(
        "  completion_tok  : {}",
        response.response.usage.completion_tokens
    );
    println!("  total_tokens    : {}", response.response.usage.total());

    assert!(
        !response.cache_hit,
        "G4 FAIL: first dispatch must NOT be a cache hit (isolated tempdir)"
    );
    assert!(
        !response.response.text.trim().is_empty(),
        "G1 FAIL: real-LLM response must be non-empty"
    );

    // Capture the raw + extracted body.
    let raw_text = response.response.text.clone();
    let extracted = extract_rust_body(&raw_text);
    println!("\n--- LLM response (raw, first 4 lines) ---");
    for (i, line) in raw_text.lines().take(4).enumerate() {
        println!("  raw[{i}]: {line}");
    }
    println!("\n--- Extracted Rust body ---");
    for line in extracted.lines() {
        println!("  {line}");
    }

    // ---- Ledger verification (G4 post-flight + G1) -------------------------
    let entries = read_ledger(&ledger_path);
    assert!(
        !entries.is_empty(),
        "G1 FAIL: ledger must have ≥ 1 entry after dispatch"
    );
    let live_entry = entries
        .iter()
        .find(|e| matches!(e.outcome, Outcome::Ok))
        .expect("at least one Ok ledger entry expected");
    assert!(
        !live_entry.cache_hit,
        "G4 FAIL: ledger Ok entry must show cache_hit=false"
    );
    println!("\n--- Ledger entry (live) ---");
    println!(
        "  {}",
        serde_json::to_string_pretty(live_entry).unwrap_or_default()
    );
    println!("  G1 result       : PASS");
    println!("  G4 result       : PASS (cache_hit=false confirmed)");

    // ---- G2 — synthesize crate + cargo check -------------------------------
    let crate_dir = dir.path().join("audit_crate");
    if let Err(e) = synthesize_audit_crate(&crate_dir, &extracted) {
        eprintln!("audit-1: synthesize_audit_crate failed: {e}");
        record_finding_synthesize_fail(&prompt, &raw_text, &extracted, &e.to_string(), live_entry);
        return;
    }

    println!("\n--- G2 cargo check (real compile) ---");
    let check_outcome = run_cargo_check(&crate_dir);
    println!("  exit_code       : {:?}", check_outcome.exit_code);
    println!("  passed          : {}", check_outcome.passed);
    if !check_outcome.passed {
        println!("  stderr (tail)   :");
        for line in check_outcome.stderr_tail.lines() {
            println!("    {line}");
        }
    }

    // ---- G3 — cargo test (only if G2 passed) -------------------------------
    let (test_outcome, case_results, tier_summary) = if check_outcome.passed {
        println!("\n--- G3 cargo test (differential, 12 oracle inputs) ---");
        let test_outcome = run_cargo_test(&crate_dir);
        println!("  exit_code       : {:?}", test_outcome.exit_code);
        println!("  passed          : {}", test_outcome.passed);
        if !test_outcome.stderr_tail.is_empty() && !test_outcome.passed {
            println!("  stderr (tail)   :");
            for line in test_outcome.stderr_tail.lines() {
                println!("    {line}");
            }
        }

        // Parse stdout for AUDIT1_CASE lines.
        let mut cases: Vec<CaseResult> = Vec::new();
        for line in &test_outcome.stdout_lines {
            if let Some(json) = line.strip_prefix("AUDIT1_CASE ") {
                if let Ok(c) = serde_json::from_str::<CaseResult>(json) {
                    cases.push(c);
                }
            }
        }

        let mut tiers: std::collections::BTreeMap<&'static str, u32> =
            std::collections::BTreeMap::new();
        for c in &cases {
            let tier = classify_divergence(&c.expected, &c.actual);
            *tiers.entry(tier).or_default() += 1;
        }

        println!("  cases observed  : {}", cases.len());
        println!("  tier summary    :");
        for (tier, count) in &tiers {
            println!("    {tier:>10}: {count}");
        }
        for c in &cases {
            let tier = classify_divergence(&c.expected, &c.actual);
            let marker = if c.passed { "PASS" } else { "FAIL" };
            println!(
                "    [{marker}] tier={tier:<10} label={lbl:<28} expected={ex} actual={ac}",
                lbl = c.label,
                ex = c.expected,
                ac = c.actual,
            );
        }
        (test_outcome, cases, tiers)
    } else {
        println!("\n--- G3 cargo test ---");
        println!("  SKIPPED — G2 cargo check failed; cannot run differential.");
        (
            CargoTestOutcome::default(),
            Vec::new(),
            std::collections::BTreeMap::new(),
        )
    };

    // ---- Final verdict -----------------------------------------------------
    let g2_pass = check_outcome.passed;
    let g3_pass =
        test_outcome.passed && !case_results.is_empty() && case_results.iter().all(|c| c.passed);

    let overall = match (g2_pass, g3_pass) {
        (true, true) => "PASS",
        (true, false) => "PARTIAL-PASS",
        (false, _) => "FAIL",
    };

    println!("\n=== Audit #1 verdict ===");
    println!("  G1 L1 dispatch       : PASS");
    println!(
        "  G2 L2.build (cargo) : {}",
        if g2_pass { "PASS" } else { "FAIL" }
    );
    println!(
        "  G3 L2.behavior (12)  : {}",
        if g3_pass {
            "PASS".to_string()
        } else if case_results.is_empty() {
            "SKIPPED".to_string()
        } else {
            let pass_count = case_results.iter().filter(|c| c.passed).count();
            format!("PARTIAL ({pass_count}/{} passed)", case_results.len())
        }
    );
    println!("  G4 cache discipline  : PASS");
    println!("  OVERALL              : {overall}");

    record_finding(
        overall,
        &prompt,
        &raw_text,
        &extracted,
        live_entry,
        g2_pass,
        &check_outcome,
        &test_outcome,
        &case_results,
        &tier_summary,
    );

    println!("\nFinding written to docs/agent/findings/audit-1-tomli-real-llm-result.md");
    println!("=== End Audit #1 ===\n");

    // Hard assertions: only G1 + G4 are unconditional. G2 + G3 are
    // reported, never asserted. This honors the audit framing: fail
    // signal is the deliverable.
    assert!(
        !response.cache_hit,
        "G4 post-check: response must not have been served from cache"
    );
}

// ---- Finding writers ---------------------------------------------------------

fn finding_path() -> PathBuf {
    workspace_root().join("docs/agent/findings/audit-1-tomli-real-llm-result.md")
}

fn write_finding_file(content: &str) {
    let path = finding_path();
    let _ = std::fs::create_dir_all(path.parent().expect("parent"));
    if let Err(e) = std::fs::write(&path, content) {
        eprintln!(
            "audit-1: failed to write finding to {}: {e}",
            path.display()
        );
    }
}

/// Resolve the current commit short SHA so the finding doc's
/// `last_verified_commit` frontmatter is auto-populated rather than
/// requiring manual re-stamping after each run. Falls back to "TBD"
/// if git isn't available (rare; CI environments shouldn't hit this).
fn current_commit_sha() -> String {
    let root = workspace_root();
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&root)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map_or_else(|| "TBD".to_string(), |s| s.trim().to_string())
}

fn record_finding_skip(reason: &str, prompt: &str) {
    let commit = current_commit_sha();
    let content = format!(
        r#"---
doc_kind: finding
finding_id: audit-1-tomli-real-llm-result
last_verified_commit: {commit}
dependencies: [adr:0032, adr:0007, adr:0004, finding:translator-real-vs-synthetic-status, finding:m5-m7-real-llm-validation]
---

# Finding: Audit #1 — tomli `parse_bool` real-LLM E2E (SKIP)

## Hypothesis

L0 → L1 → L2.build → L2.behavior with a real LLM (user-codex `{MODEL}`)
produces a Cobrust-workspace-compatible Rust port of
`tomli_loads._parse_bool`.

## Method (attempted)

- Provider: `OpenAiProvider` at `{BASE_URL}` (model `{MODEL}`)
- Cache discipline: isolated tempdir; NO `SyntheticProvider`.
- Rich prompt (signature + workspace API context + few-shot example).
- Prompt size: {prompt_chars} chars, {prompt_lines} lines.

## Result

**OUTCOME: SKIP** — {reason}.

The harness was invoked but the live HTTP round-trip could not be
completed. The audit's gate verdict cannot be produced this run. The
test infrastructure itself was verified — a future retry with the
endpoint reachable will produce the gated outcome.

## Conclusion

The audit harness is correct (cache discipline verified, provider
wired). The endpoint or environment is currently unavailable. CTO
should retry with `https_proxy=http://127.0.0.1:7897` set, or via the
DG-Workstation per `reference_x86_workstation.md`.

## Cross-references

- ADR-0032 — sprint binding decision.
- `finding:translator-real-vs-synthetic-status` — the gap this audit
  addresses.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` — harness.
"#,
        BASE_URL = BASE_URL,
        MODEL = MODEL,
        reason = reason,
        prompt_chars = prompt.len(),
        prompt_lines = prompt.lines().count(),
    );
    write_finding_file(&content);
}

fn record_finding_synthesize_fail(
    prompt: &str,
    raw: &str,
    extracted: &str,
    err: &str,
    live_entry: &LedgerEntry,
) {
    let commit = current_commit_sha();
    let content = format!(
        r#"---
doc_kind: finding
finding_id: audit-1-tomli-real-llm-result
last_verified_commit: {commit}
dependencies: [adr:0032, adr:0007, adr:0004, finding:translator-real-vs-synthetic-status, finding:m5-m7-real-llm-validation]
---

# Finding: Audit #1 — tomli `parse_bool` real-LLM E2E (HARNESS-ERR)

## Hypothesis

(see `audit-1-tomli-real-llm-result.md` ADR-0032 §"Acceptance gate")

## Result

**OUTCOME: HARNESS-ERR** — synthesize_audit_crate failed: {err}.

## Live LLM call (G1 actually completed)

Prompt size: {prompt_chars} chars, {prompt_lines} lines.

Ledger entry:

```json
{ledger_json}
```

Raw response (first 200 chars): `{raw_first}`

Extracted body (first 200 chars): `{ext_first}`

## Cross-references

- ADR-0032 — sprint binding.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` — harness.
"#,
        err = err,
        prompt_chars = prompt.len(),
        prompt_lines = prompt.lines().count(),
        ledger_json = serde_json::to_string_pretty(live_entry).unwrap_or_default(),
        raw_first = raw.chars().take(200).collect::<String>(),
        ext_first = extracted.chars().take(200).collect::<String>(),
    );
    write_finding_file(&content);
}

#[allow(clippy::too_many_arguments)]
fn record_finding(
    overall: &str,
    prompt: &str,
    raw_response: &str,
    extracted_body: &str,
    live_entry: &LedgerEntry,
    g2_pass: bool,
    check: &CargoCheckOutcome,
    test: &CargoTestOutcome,
    case_results: &[CaseResult],
    tier_summary: &std::collections::BTreeMap<&'static str, u32>,
) {
    let ledger_json = serde_json::to_string_pretty(live_entry).unwrap_or_default();

    let g2_block = if g2_pass {
        "PASS — `cargo check` exited 0; the synthesized crate (workspace preamble + LLM emission) compiles cleanly.".to_string()
    } else {
        format!(
            "FAIL — `cargo check` exited {exit:?}. Stderr tail (last 40 lines):\n\n```text\n{stderr}\n```",
            exit = check.exit_code,
            stderr = check.stderr_tail,
        )
    };

    let g3_block = if !g2_pass {
        "SKIPPED — G2 (cargo check) failed; behavioral gate cannot run on uncompilable code."
            .to_string()
    } else if case_results.is_empty() {
        "ERROR — `cargo test` did not emit any AUDIT1_CASE lines. Stderr tail:\n\n```text\n"
            .to_string()
            + &test.stderr_tail
            + "\n```"
    } else {
        let mut lines = String::new();
        lines.push_str("\nDifferential outcomes (12 oracle inputs):\n\n");
        lines.push_str("| Tier | Label | Buffer | Expected | Actual | Pass |\n");
        lines.push_str("|---|---|---|---|---|---|\n");
        for c in case_results {
            let tier = classify_divergence(&c.expected, &c.actual);
            let buf_disp = if c.buffer.is_empty() {
                "(empty)".to_string()
            } else {
                format!("{:?}", c.buffer)
            };
            lines.push_str(&format!(
                "| `{tier}` | `{label}` | `{buf}` | `{exp}` | `{act}` | {pass} |\n",
                tier = tier,
                label = c.label,
                buf = buf_disp,
                exp = c.expected,
                act = c.actual,
                pass = if c.passed { "PASS" } else { "FAIL" },
            ));
        }

        lines.push_str("\nTier summary:\n\n");
        for (tier, count) in tier_summary {
            lines.push_str(&format!("- `{tier}` : {count}\n"));
        }
        lines
    };

    let conclusion = match overall {
        "PASS" => r#"All four gates green. The L0 → L1 → L2 closed loop, when driven by
a real LLM (`gpt-5.5` via the user-codex proxy) using a rich prompt
that includes the workspace API contract (`State`, `TomliError`) plus
a few-shot example (`parse_basic_string`), produces a Cobrust-
compatible `parse_bool` implementation that compiles AND matches
the CPython 3.11 oracle on 12 deterministic inputs.

This is the **first time** the constitution §1.2 dual mandate
("AI-native compiler that closed-loop translates the entire Python
ecosystem") has been demonstrated end-to-end — for one leaf
function, with a fresh real-LLM round-trip and zero canned-response
contamination."#
            .to_string(),
        "PARTIAL-PASS" => r#"G1 (real dispatch), G2 (cargo check), and G4 (cache discipline)
all green. G3 (differential) revealed one or more divergences against
the CPython 3.11 oracle.

This is the **strongest possible anchor for ADR-0033** (`@py_compat`
hard-bind to L2 verifier): the LLM produced compilable Rust that
nonetheless deviates from oracle behavior. The tier summary above
classifies each divergence under the `strict / numerical / semantic /
divergent` taxonomy from constitution §2.4. Concrete divergent cases
become regression test seeds for the repair loop."#
            .to_string(),
        _ => r#"G2 (cargo check) failed: the LLM produced Rust that does not
compile. G3 (differential) was therefore not run.

This still anchors ADR-0033, but at a different layer: the L1 prompt
needs structural guarantees (e.g. an LLM-side syntax pre-validator,
or a repair loop that automatically re-dispatches on parse error).
The compile error tail above shows the specific failure mode."#
            .to_string(),
    };

    let actionables = match overall {
        "PASS" => r#"1. **Audit #2** — extend the same harness to a stateful function
   (e.g. `parse_inline_table` or `parse_array`) that calls 2-3 helper
   functions. This tests whether the LLM can carry workspace context
   across a dependency chain.
2. **Production prompt design** — the rich prompt design used here
   (workspace API context + few-shot example) should replace the
   bare-bones `build_translation_prompt` in `crates/cobrust-translator/
   src/translate.rs`. Land via separate ADR; do not refactor production
   code in the audit sprint itself.
3. **ADR-0033 scope shift** — with empirical PASS data, ADR-0033
   becomes proactive (semantic-tier rigor) rather than reactive
   (fixing observed divergences). Pin the tier-classifier (`classify_divergence`)
   from this test as the canonical mapper."#
            .to_string(),
        _ => r#"1. **ADR-0033 anchors on the divergence table above**. Each row of
   the tier table is a concrete failure case the L2 verifier must be
   able to detect (currently it only diffs stdout, missing semantic
   distinctions).
2. **Repair-loop validation in the wild** — the failing cases above
   form the diagnostic blob ADR-0008 §5's repair loop would feed back
   to attempt 2. A future audit sprint should drive this: dispatch
   attempt 1, capture the divergence, dispatch attempt 2 with the diff
   as feedback, measure convergence rate.
3. **Prompt design refinement** — even with the rich prompt used here,
   the LLM diverged. ADR-0033 should pin a stronger prompt contract
   (e.g. require the LLM to return both code AND a self-test it would
   pass; consensus mode with N=2 + structured-diff judging)."#
            .to_string(),
    };

    let commit = current_commit_sha();
    let content = format!(
        r#"---
doc_kind: finding
finding_id: audit-1-tomli-real-llm-result
last_verified_commit: {commit}
dependencies: [adr:0032, adr:0007, adr:0008, adr:0004, finding:translator-real-vs-synthetic-status, finding:m5-m7-real-llm-validation]
---

# Finding: Audit #1 — tomli `parse_bool` real-LLM E2E result

## Hypothesis

L0 → L1 → L2.build → L2.behavior with a real LLM (user-codex `{MODEL}`)
and a **rich prompt** carrying the Cobrust workspace API contract +
a few-shot example produces a port of `tomli_loads._parse_bool` that:

1. Compiles when glued to the workspace preamble.
2. Agrees with the CPython 3.11 oracle on 12 deterministic inputs.

## Method

- **Target**: `tomli_loads._parse_bool` (8-line Python leaf).
- **Provider**: `OpenAiProvider` at `{BASE_URL}` (model `{MODEL}`).
- **Cache discipline**:
  - `SyntheticProvider` NOT registered (review-claude #1).
  - `cache_dir` = isolated `tempdir().join("llm_cache")`, verified
    non-existent pre-flight (review-claude #2).
- **Prompt** (rich, ADR-0032 §4b):
  - Verbatim Python source of `_parse_bool`.
  - Workspace API contract: `State` struct + `TomliError` constructor +
    `Value` enum (verbatim from `crates/cobrust-tomli/src/parser.rs`).
  - Few-shot example: `parse_basic_string` workspace helper.
  - Explicit return-type contract: `Result<bool, TomliError>`.
  - Prompt size: {prompt_chars} chars, {prompt_lines} lines.
- **G2 gate**: synthesized minimal Cargo crate (workspace preamble +
  emitted body) → `cargo check`.
- **G3 gate**: `cargo test` driving 12 deterministic CPython 3.11
  oracle inputs through the emitted function; per-case divergence
  classified under the `@py_compat` taxonomy from constitution §2.4
  (`strict | numerical | semantic | divergent`).

## Result

**OUTCOME: {overall}**

### G1 — L1 dispatch

PASS — real HTTP round-trip succeeded, response non-empty.

Ledger entry:

```json
{ledger_json}
```

Cache discipline confirmed: `cache_hit` = false, `cache_dir` was an
isolated tempdir.

### G2 — L2.build (real `cargo check`)

{g2_block}

### G3 — L2.behavior (differential, 12 oracle inputs)

{g3_block}

### G4 — Cache discipline

PASS — both axes verified:

1. Provider registry contained exactly one `OpenAiProvider`; no
   `SyntheticProvider` registered.
2. `cache_dir` was an isolated `tempfile::tempdir()` path, verified
   non-existent before dispatch.
3. Ledger entry's `cache_hit` field = `false`.

### Emitted Rust source (extracted from LLM response, verbatim)

```rust
{extracted_body}
```

### Raw LLM response (first 1500 chars)

```text
{raw_response_first}
```

## Conclusion

{conclusion}

## Token spend

| Phase | Calls | Tokens billed |
|-------|-------|---------------|
| L1 real dispatch | 1 | {total_tokens} |
| Cache replay | 0 | 0 |
| **Total** | **1** | **{total_tokens}** |

(prompt: {prompt_tokens}, completion: {completion_tokens})

## Actionable consequences

{actionables}

## Cross-references

- ADR-0032 — sprint binding (this audit).
- ADR-0033 (future) — `@py_compat` hard-bind to L2 verifier; anchored
  on the divergence table above when this audit's outcome is not PASS.
- ADR-0007 — translator pipeline whose synthetic-only default this
  audit deliberately bypasses.
- ADR-0008 — repair loop; the divergence table above would form the
  diagnostic blob for attempt 2 if a follow-up sprint exercises it.
- `finding:translator-real-vs-synthetic-status` — the honesty gap
  this finding closes with empirical data.
- `finding:m5-m7-real-llm-validation` — the M3 wire-protocol smoke
  this audit extends to a real translation.
- `crates/cobrust-translator/tests/audit_1_tomli_real_llm.rs` —
  harness implementation.
- Memory `feedback_third_party_audit_2026_05_09.md` — audit mandate.
- Memory `reference_codex_api.md` — endpoint credentials.
"#,
        BASE_URL = BASE_URL,
        MODEL = MODEL,
        overall = overall,
        ledger_json = ledger_json,
        g2_block = g2_block,
        g3_block = g3_block,
        extracted_body = extracted_body,
        raw_response_first = raw_response.chars().take(1500).collect::<String>(),
        conclusion = conclusion,
        actionables = actionables,
        prompt_chars = prompt.len(),
        prompt_lines = prompt.lines().count(),
        prompt_tokens = live_entry.prompt_tokens,
        completion_tokens = live_entry.completion_tokens,
        total_tokens = live_entry.total_tokens,
    );
    write_finding_file(&content);
}
