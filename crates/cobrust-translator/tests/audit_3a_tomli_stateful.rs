//! Audit #3a — stateful tomli function E2E through the production
//! `build_translation_prompt_rich` builder. Binding ADR: `adr:0036`.
//! Anchor finding: `finding:audit-1-tomli-real-llm-result`
//! (sonnet PARTIAL-FAIL with bare prompt) and the §1.2
//! mechanism-demonstrated → production-validated upgrade signal.
//!
//! ## Scope vs Audit #1
//!
//! Audit #1 (ADR-0032) ran `parse_bool` (a *leaf* function: 8 lines,
//! no state mutation beyond `state.pos +=`, no helper calls). PASS
//! on 12/12 strict tier — but the rich prompt was hand-built inline
//! in the test. Audit #3a's job per `adr:0036`:
//!
//! 1. Lift the audit-1 design into the production
//!    `build_translation_prompt_rich(unit, ctx)` builder.
//! 2. Verify it on a *stateful* tomli function — `parse_int` —
//!    that audit-1 sonnet's bare prompt would have failed PARTIAL on
//!    the same way it failed `parse_bool` (return type, error path,
//!    field names).
//!
//! `parse_int` qualifies on every constraint per `adr:0036 §"Step C"`:
//! - mutates `state.pos` in two distinct places (sign + digits loop);
//! - non-trivial error path (`Err(TomliError::new("expected digit"))`);
//! - enumerable oracle inputs (≥ 14: positive/negative/zero/signs/
//!   empty/letters/at-offset);
//! - different shape from audit-1's leaf (loop-driven, not branching).
//!
//! ## Cache discipline (review-claude binding, identical to audit-1)
//!
//! 1. **No `SyntheticProvider`**: only `OpenAiProvider` registered.
//! 2. **Isolated LLM disk cache**: `cache_dir = tempfile::tempdir()`.
//!
//! Both asserted in-test before dispatch.
//!
//! ## Verification gates
//!
//! - **G1 — L1 dispatch**: real HTTP round-trip non-empty;
//!   `cache_hit=false`.
//! - **G2 — L2.build**: synthesized crate (workspace preamble + LLM
//!   emission) `cargo check`s with zero errors.
//! - **G3 — L2.behavior**: 14 deterministic inputs through the
//!   emitted function; per-case classification under the
//!   `strict | numerical | semantic | divergent` taxonomy.
//! - **G4 — Cache discipline**: provider count = 1; tempdir verified
//!   non-existent pre-flight; `cache_hit=false` post.
//!
//! ## Honest fail
//!
//! Per `adr:0036` Acceptance Gate, the fail signal IS the deliverable.
//! Only G1 + G4 are hard-asserted; G2 + G3 are reported, never
//! panicked on. ADR-0037 (#3b) anchored on whatever divergence emerges.

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
use cobrust_translator::{
    FunctionSpec, SpecToml, TranslationPlan, WorkspaceContext, build_translation_prompt_rich,
};

// ---- Constants ---------------------------------------------------------------

const ENV_KEY: &str = "USER_CODEX_API_KEY";
const BASE_URL: &str = "http://104.244.92.250:8317/v1";
const PROVIDER_KEY: &str = "user_codex_audit3a";
const MODEL: &str = "gpt-5.5";
const TARGET_FUNCTION: &str = "parse_int";
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(180);
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
    RouterConfig::from_toml_str(&toml).expect("audit-3a router config must parse")
}

// ---- Workspace context (audit-3a tomli stateful) ----------------------------

/// Workspace preamble — verbatim from `crates/cobrust-tomli/src/parser.rs`
/// with `pub` widened on `State` and helpers so the synthesized G2 crate
/// can construct values from its `tests/` integration target.
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

/// Few-shot example: `parse_bool` (the audit-1 leaf), already translated
/// in the workspace. Stylistic anchor — the LLM should match this
/// shape (Result<T, TomliError>, byte-level slicing on state.bytes,
/// state.pos += N for advance, TomliError::new for the err path).
const PARSE_BOOL_REF: &str = r#"fn parse_bool(state: &mut State<'_>) -> Result<bool, TomliError> {
    if state.bytes[state.pos..].starts_with(b"true") {
        state.pos += 4;
        return Ok(true);
    }
    if state.bytes[state.pos..].starts_with(b"false") {
        state.pos += 5;
        return Ok(false);
    }
    Err(TomliError::new("expected bool", state.pos))
}"#;

/// Verbatim Python source for `_parse_int`, copied from
/// `corpus/tomli/upstream/tomli_loads.py:104..114`. This is the L0
/// "target_python_source" fed into the rich prompt.
const PARSE_INT_PY: &str = r#"def _parse_int(state):
    """Parse a decimal integer. Optional leading '-' or '+'."""
    start = state.pos
    if state.peek() == "-" or state.peek() == "+":
        state.pos += 1
    digits_start = state.pos
    while not state.eof() and state.peek() >= "0" and state.peek() <= "9":
        state.pos += 1
    if state.pos == digits_start:
        raise TomliError("expected digit at pos " + str(start))
    return int(state.src[start:state.pos])
"#;

/// Construct the audit-3a `WorkspaceContext` for `parse_int`. This is
/// what production callers would build for any tomli function.
fn parse_int_workspace_context() -> WorkspaceContext {
    WorkspaceContext {
        module_preamble: WORKSPACE_PREAMBLE.to_string(),
        fewshot_examples: vec![("parse_bool".to_string(), PARSE_BOOL_REF.to_string())],
        target_python_source: PARSE_INT_PY.to_string(),
        return_type_contract: "Result<i64, TomliError>".to_string(),
        error_construction_contract: "Err(TomliError::new(\"expected digit\", start))".to_string(),
    }
}

/// Build the synthetic `FunctionUnit` for `parse_int` using the same
/// L0 spec entry as the production pipeline (pulled from
/// `corpus/tomli/spec.toml`).
fn parse_int_function_unit() -> cobrust_translator::translate::FunctionUnit {
    let spec = FunctionSpec {
        qualname: "tomli_loads._parse_int".to_string(),
        public: false,
        signature: "_parse_int(state: _State) -> int".to_string(),
        py_compat: "strict".to_string(),
        description: "Parse decimal int with optional + or - sign.".to_string(),
        exemplars: Vec::new(),
        errors_on: Vec::new(),
        task: "translate".to_string(),
    };
    let mut spec_table = std::collections::BTreeMap::new();
    spec_table.insert("parse_int".to_string(), spec);
    let spec_toml = SpecToml {
        schema_version: 1,
        library: "tomli".to_string(),
        upstream_version: "2.0.1".to_string(),
        oracle_module: "tomllib".to_string(),
        oracle_runtime: "cpython".to_string(),
        oracle_runtime_version: "3.11".to_string(),
        function: spec_table,
        verification: cobrust_translator::spec::VerificationBudget {
            seeds: vec![42],
            fuzz_inputs_per_fn: 1,
            tolerance: "exact".to_string(),
        },
    };
    let plan = TranslationPlan::from_spec(&spec_toml, "audit3a01".into());
    plan.functions
        .into_iter()
        .next()
        .expect("plan has parse_int unit")
}

// ---- Oracle (CPython 3.11 reference values) ---------------------------------

#[derive(Clone, Copy, Debug)]
struct OracleCase {
    label: &'static str,
    buffer: &'static str,
    start_pos: usize,
    expected: ExpectedOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedOutcome {
    Ok { value: i64, end_pos: usize },
    Err,
}

/// 14 deterministic CPython-3.11 oracle inputs covering: pos/neg/zero
/// values, sign-only failure, multi-digit, post-prefix consumption,
/// at-offset start, empty input, and letter-leading rejection. Each
/// expected value verified manually via Python `_parse_int` semantics.
fn oracle_inputs() -> Vec<OracleCase> {
    vec![
        OracleCase {
            label: "plain_zero",
            buffer: "0",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 0,
                end_pos: 1,
            },
        },
        OracleCase {
            label: "plain_one",
            buffer: "1",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 1,
                end_pos: 1,
            },
        },
        OracleCase {
            label: "multi_digit",
            buffer: "12345",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 12345,
                end_pos: 5,
            },
        },
        OracleCase {
            label: "negative",
            buffer: "-42",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: -42,
                end_pos: 3,
            },
        },
        OracleCase {
            label: "positive_sign",
            buffer: "+7",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 7,
                end_pos: 2,
            },
        },
        OracleCase {
            label: "zero_neg",
            buffer: "-0",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 0,
                end_pos: 2,
            },
        },
        OracleCase {
            label: "digit_then_letter",
            buffer: "99x",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 99,
                end_pos: 2,
            },
        },
        OracleCase {
            label: "digit_then_space",
            buffer: "8 ",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 8,
                end_pos: 1,
            },
        },
        OracleCase {
            label: "only_minus",
            buffer: "-",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "only_plus",
            buffer: "+",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "empty",
            buffer: "",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "letter_first",
            buffer: "abc",
            start_pos: 0,
            expected: ExpectedOutcome::Err,
        },
        OracleCase {
            label: "big_int",
            buffer: "1234567890",
            start_pos: 0,
            expected: ExpectedOutcome::Ok {
                value: 1_234_567_890,
                end_pos: 10,
            },
        },
        OracleCase {
            label: "at_offset",
            buffer: "xx-15y",
            start_pos: 2,
            expected: ExpectedOutcome::Ok {
                value: -15,
                end_pos: 5,
            },
        },
    ]
}

// ---- Synthesized G2 + G3 crate ----------------------------------------------

fn synthesize_audit_crate(crate_dir: &Path, emitted_fn_body: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(crate_dir.join("src"))?;
    std::fs::create_dir_all(crate_dir.join("tests"))?;

    let cargo_toml = r#"[package]
name = "cobrust-audit-3a-tomli-parse-int"
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

    // Same minimal `pub` widen as audit-1: prefix the leading
    // `fn parse_int` with `pub ` so the integration test target can
    // reach the symbol. Honest-fail invariant preserved: if the LLM
    // emitted a different signature shape, no `pub fn parse_int` will
    // exist and G2 cargo check surfaces the failure.
    let prefixed_emission = if emitted_fn_body.trim_start().starts_with("fn parse_int") {
        // Find the leading `fn ` token (after any leading whitespace
        // / attributes) and prepend `pub `.
        let leading_ws_len = emitted_fn_body.len() - emitted_fn_body.trim_start().len();
        let (ws, rest) = emitted_fn_body.split_at(leading_ws_len);
        format!("{ws}pub {rest}")
    } else if emitted_fn_body.trim_start().starts_with("pub fn parse_int") {
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

    let oracle_cases = oracle_inputs();
    let mut test_rs = String::new();
    test_rs.push_str(
        r#"//! Audit #3a differential test — drives oracle inputs through the
//! LLM-emitted `parse_int` and emits per-case JSON on stdout.

#![allow(clippy::all)]

use cobrust_audit_3a_tomli_parse_int::{parse_int, State, TomliError};

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
        parse_int(&mut state)
    }));
    let actual = match result {
        Ok(Ok(value)) => {
            serde_json::json!({"kind": "ok", "value": value, "end_pos": state.pos})
        }
        Ok(Err(_err)) => serde_json::json!({"kind": "err"}),
        Err(_panic) => serde_json::json!({"kind": "panic"}),
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
            "    {{\n        let r = run_one({label:?}, {buffer:?}, {start_pos}, {expected});\n        println!(\"AUDIT3A_CASE {{}}\", serde_json::to_string(&r).unwrap());\n    }}\n",
            label = case.label,
            buffer = case.buffer,
            start_pos = case.start_pos,
            expected = expected_json,
        ));
    }
    test_rs.push_str(
        r#"}
"#,
    );
    std::fs::write(crate_dir.join("tests/parse_int_oracle.rs"), &test_rs)?;
    Ok(())
}

fn extract_rust_body(raw: &str) -> String {
    let mut s = raw.trim().to_string();
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

fn run_cargo_check(crate_dir: &Path) -> CargoCheckOutcome {
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(crate_dir)
        .arg("check")
        .arg("--quiet")
        .arg("--message-format=short")
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .env_remove("RUSTFLAGS");
    let start = std::time::Instant::now();
    let output = cmd.output();
    let elapsed = start.elapsed();
    eprintln!("audit-3a: cargo check finished in {:?}", elapsed);
    if elapsed > CARGO_CHECK_TIMEOUT {
        eprintln!("audit-3a: cargo check exceeded soft timeout");
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
        .arg("parse_int_oracle")
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

/// Tier classifier — verbatim from `audit_1_tomli_real_llm.rs:806`.
/// Per `adr:0036 §"Decision"` Step C, this is the canonical classifier
/// shared across audit-1 + audit-3a + future audits until ADR-0037
/// promotes it to a public crate API.
fn classify_divergence(expected: &serde_json::Value, actual: &serde_json::Value) -> &'static str {
    if expected == actual {
        return "strict";
    }
    let e_kind = expected.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let a_kind = actual.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if e_kind == "err" && a_kind == "err" {
        return "semantic";
    }
    if e_kind == "err" && a_kind == "panic" {
        return "divergent";
    }
    if e_kind == "ok" && a_kind == "ok" {
        let e_value = expected.get("value");
        let a_value = actual.get("value");
        let e_end = expected.get("end_pos");
        let a_end = actual.get("end_pos");
        if e_value == a_value && e_end != a_end {
            return "divergent";
        }
        if e_value != a_value {
            return "divergent";
        }
        return "semantic";
    }
    "divergent"
}

// ---- Main audit-3a test -----------------------------------------------------

#[tokio::test]
async fn audit_3a_parse_int_real_llm_e2e() {
    let Some(api_key) = lookup_api_key() else {
        eprintln!(
            "audit-3a: {ENV_KEY} unset — skipping real-LLM audit. \
             Set USER_CODEX_API_KEY=<codex-key> to run the live gate."
        );
        return;
    };

    println!("\n=== Audit #3a — tomli stateful real-LLM E2E (ADR-0036) ===");
    println!("Target function : tomli_loads.{TARGET_FUNCTION}");
    println!("Endpoint        : {BASE_URL}");
    println!("Model           : {MODEL}");
    println!("Builder         : production build_translation_prompt_rich (ADR-0036 §Decision)");
    println!("Cache discipline: isolated tempdir; NO SyntheticProvider");

    // ---- G4 (pre-flight) ----------------------------------------------------
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

    // ---- L1 — production rich-prompt build + dispatch -----------------------
    let unit = parse_int_function_unit();
    let ctx = parse_int_workspace_context();
    let prompt = build_translation_prompt_rich(&unit, &ctx);
    println!("\n--- L1 prompt summary (production builder) ---");
    println!("  prompt_chars    : {}", prompt.len());
    println!("  prompt_lines    : {}", prompt.lines().count());

    let req = CompletionRequest {
        model: MODEL.into(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.clone(),
        }],
        params: SamplingParams {
            max_tokens: Some(2048),
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
                "audit-3a: dispatch TIMED OUT after {}s — endpoint unreachable?",
                DISPATCH_TIMEOUT.as_secs()
            );
            record_finding_skip("dispatch_timeout", &prompt);
            return;
        }
        Ok(Err(e)) => {
            eprintln!("audit-3a: dispatch ERRORED — {e}");
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

    let raw_text = response.response.text.clone();
    let extracted = extract_rust_body(&raw_text);
    println!("\n--- LLM response (raw, first 6 lines) ---");
    for (i, line) in raw_text.lines().take(6).enumerate() {
        println!("  raw[{i}]: {line}");
    }
    println!("\n--- Extracted Rust body ---");
    for line in extracted.lines() {
        println!("  {line}");
    }

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

    // ---- G2 ------------------------------------------------------------------
    let crate_dir = dir.path().join("audit_3a_crate");
    if let Err(e) = synthesize_audit_crate(&crate_dir, &extracted) {
        eprintln!("audit-3a: synthesize_audit_crate failed: {e}");
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

    // ---- G3 ------------------------------------------------------------------
    let (test_outcome, case_results, tier_summary) = if check_outcome.passed {
        println!("\n--- G3 cargo test (differential, 14 oracle inputs) ---");
        let test_outcome = run_cargo_test(&crate_dir);
        println!("  exit_code       : {:?}", test_outcome.exit_code);
        println!("  passed          : {}", test_outcome.passed);
        if !test_outcome.stderr_tail.is_empty() && !test_outcome.passed {
            println!("  stderr (tail)   :");
            for line in test_outcome.stderr_tail.lines() {
                println!("    {line}");
            }
        }

        let mut cases: Vec<CaseResult> = Vec::new();
        for line in &test_outcome.stdout_lines {
            if let Some(json) = line.strip_prefix("AUDIT3A_CASE ") {
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
                "    [{marker}] tier={tier:<10} label={lbl:<24} expected={ex} actual={ac}",
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

    let g2_pass = check_outcome.passed;
    let g3_pass =
        test_outcome.passed && !case_results.is_empty() && case_results.iter().all(|c| c.passed);

    let overall = match (g2_pass, g3_pass) {
        (true, true) => "PASS",
        (true, false) => "PARTIAL-PASS",
        (false, _) => "FAIL",
    };

    println!("\n=== Audit #3a verdict ===");
    println!("  G1 L1 dispatch       : PASS");
    println!(
        "  G2 L2.build (cargo) : {}",
        if g2_pass { "PASS" } else { "FAIL" }
    );
    println!(
        "  G3 L2.behavior (14)  : {}",
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

    println!("\nFinding written to docs/agent/findings/audit-3a-stateful-prompt-design.md");
    println!("=== End Audit #3a ===\n");

    // Hard assertions: only G1 + G4 (per ADR-0036 §"Acceptance gate").
    assert!(
        !response.cache_hit,
        "G4 post-check: response must not have been served from cache"
    );
}

// ---- Finding writers --------------------------------------------------------

fn finding_path() -> PathBuf {
    workspace_root().join("docs/agent/findings/audit-3a-stateful-prompt-design.md")
}

fn write_finding_file(content: &str) {
    let path = finding_path();
    let _ = std::fs::create_dir_all(path.parent().expect("parent"));
    if let Err(e) = std::fs::write(&path, content) {
        eprintln!(
            "audit-3a: failed to write finding to {}: {e}",
            path.display()
        );
    }
}

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
finding_id: audit-3a-stateful-prompt-design
last_verified_commit: {commit}
dependencies: [adr:0036, adr:0032, adr:0007, finding:audit-1-tomli-real-llm-result, finding:translator-real-vs-synthetic-status]
---

# Finding: Audit #3a — tomli `parse_int` (stateful) real-LLM E2E (SKIP)

## Hypothesis

`build_translation_prompt_rich(unit, ctx)` (production builder per
ADR-0036) produces a Cobrust-workspace-compatible Rust port of
`tomli_loads._parse_int`. This sprint extends audit-1's empirical PASS
on the *leaf* `parse_bool` to a *stateful* helper that mutates
`state.pos` across two distinct phases and has a non-trivial error
path.

## Method (attempted)

- Provider: `OpenAiProvider` at `{BASE_URL}` (model `{MODEL}`)
- Cache discipline: isolated tempdir; NO `SyntheticProvider`.
- Builder: `cobrust_translator::build_translation_prompt_rich` (production).
- Workspace context: tomli `Value` + `TomliError` + `State` preamble +
  `parse_bool` few-shot + return-type `Result<i64, TomliError>` +
  error contract `Err(TomliError::new("expected digit", start))`.
- Prompt size: {prompt_chars} chars, {prompt_lines} lines.

## Result

**OUTCOME: SKIP** — {reason}.

The harness was invoked but the live HTTP round-trip could not be
completed. The audit's gate verdict cannot be produced this run. The
test infrastructure itself was verified — a future retry with the
endpoint reachable will produce the gated outcome.

## Conclusion

Harness correct (cache discipline verified, provider wired, production
builder used). Endpoint unavailable. CTO retries with
`https_proxy=http://127.0.0.1:7897` set, or via <self-hosted-runner> per
`reference_x86_workstation.md`.

## Cross-references

- ADR-0036 — sprint binding decision.
- `crates/cobrust-translator/tests/audit_3a_tomli_stateful.rs` — harness.
- `finding:audit-1-tomli-real-llm-result` — leaf PASS this extends.
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
finding_id: audit-3a-stateful-prompt-design
last_verified_commit: {commit}
dependencies: [adr:0036, adr:0032, adr:0007, finding:audit-1-tomli-real-llm-result, finding:translator-real-vs-synthetic-status]
---

# Finding: Audit #3a — tomli `parse_int` (stateful) real-LLM E2E (HARNESS-ERR)

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

- ADR-0036 — sprint binding.
- `crates/cobrust-translator/tests/audit_3a_tomli_stateful.rs` — harness.
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
        "ERROR — `cargo test` did not emit any AUDIT3A_CASE lines. Stderr tail:\n\n```text\n"
            .to_string()
            + &test.stderr_tail
            + "\n```"
    } else {
        let mut lines = String::new();
        lines.push_str("\nDifferential outcomes (14 oracle inputs):\n\n");
        lines.push_str("| Tier | Label | Buffer | start_pos | Expected | Actual | Pass |\n");
        lines.push_str("|---|---|---|---|---|---|---|\n");
        for c in case_results {
            let tier = classify_divergence(&c.expected, &c.actual);
            let buf_disp = if c.buffer.is_empty() {
                "(empty)".to_string()
            } else {
                format!("{:?}", c.buffer)
            };
            lines.push_str(&format!(
                "| `{tier}` | `{label}` | `{buf}` | {sp} | `{exp}` | `{act}` | {pass} |\n",
                tier = tier,
                label = c.label,
                buf = buf_disp,
                sp = c.start_pos,
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
        "PASS" => r#"All four gates green on the **stateful** function `parse_int`,
through the **production** `build_translation_prompt_rich` builder.

This is the §1.2 production-validated upgrade signal: the audit-1
PASS on the leaf `parse_bool` (12/12 strict) generalises to a function
that mutates `state.pos` across two distinct phases (sign + digits
loop) and carries a non-trivial error path. The bare M4
`build_translation_prompt` would have produced the audit-1 sonnet
PARTIAL-FAIL pattern (wrong return type, `panic!` instead of
`TomliError::new`, hallucinated field names); the rich variant via
`WorkspaceContext` injection lifts every gap.

The audit-1 sonnet branch (`feature/audit-1-tomli-real-llm`,
PARTIAL-FAIL) is empirically retired — the bare prompt was the bug,
not the model."#
            .to_string(),
        "PARTIAL-PASS" => r#"G1 (real dispatch), G2 (cargo check), and G4 (cache discipline)
all green. G3 (differential) revealed one or more divergences.

This is the strongest possible anchor for ADR-0037 (`@py_compat`
hard-bind to L2 verifier) at the **stateful** axis: the rich prompt
closes the structural gaps audit-1 sonnet PARTIAL-FAIL identified
(return type, error path, field names) but a deeper semantic
divergence remains on stateful execution. The tier summary above
classifies each divergence under `strict / numerical / semantic /
divergent`. Concrete divergent rows become regression seeds for the
repair loop and the verifier hard-bind."#
            .to_string(),
        _ => r#"G2 (cargo check) failed: the production rich-prompt builder
produced Rust that does not compile. G3 was therefore not run.

This is a stronger signal than a behavior divergence: the structural
guarantees the rich prompt was supposed to encode (workspace types,
return type contract, error construction) did not all land. The
compile error tail above shows the specific failure mode. ADR-0037's
scope likely needs to extend beyond verifier hard-bind to include an
LLM-side syntax pre-validator or a repair loop that re-dispatches
on parse error."#
            .to_string(),
    };

    let production_signal = if overall == "PASS" {
        "**yes** — §1.2 mechanism-demonstrated → production-validated upgrade signal achieved."
    } else {
        "**no** — divergence surfaced; ADR-0037 anchored on the divergence table above."
    };

    let actionables = match overall {
        "PASS" => r#"1. **Audit #3b (ADR-0037)** — `@py_compat` hard-bind shifts from
   reactive (fix observed divergences) to proactive (semantic-tier
   rigor). Pin the tier classifier `classify_divergence` from this
   harness as the canonical mapper.
2. **Production rollout** — extend `WorkspaceContext` to the dateutil
   / msgpack / requests / click translators. Each library author
   builds one bundle (preamble + 1 few-shot + return/error contracts);
   afterwards every function in that library benefits.
3. **Retire audit-1 sonnet branch** — `feature/audit-1-tomli-real-llm`
   PARTIAL-FAIL data is now superseded; the bare prompt was the bug,
   the production rich variant fixes it."#
            .to_string(),
        _ => r#"1. **ADR-0037 anchors on the divergence table above**. Each row is
   a concrete failure case the L2 verifier must detect (currently it
   only diffs stdout, missing semantic distinctions for stateful
   functions).
2. **Repair-loop validation in the wild** — the failing cases form
   the diagnostic blob ADR-0008 §5's repair loop would feed back to
   attempt 2. A future audit sprint should drive convergence rate
   measurement.
3. **Prompt design refinement** — even with the production rich
   prompt, divergence remains. ADR-0037 should pin a stronger
   contract (e.g. require both code AND a self-test the LLM would
   pass; consensus mode N=2 with structured-diff judge)."#
            .to_string(),
    };

    let commit = current_commit_sha();
    let content = format!(
        r#"---
doc_kind: finding
finding_id: audit-3a-stateful-prompt-design
last_verified_commit: {commit}
dependencies: [adr:0036, adr:0032, adr:0007, adr:0008, finding:audit-1-tomli-real-llm-result, finding:translator-real-vs-synthetic-status]
---

# Finding: Audit #3a — tomli `parse_int` (stateful) real-LLM E2E result

## Hypothesis

`build_translation_prompt_rich(unit, ctx)` — the production builder
introduced by ADR-0036 — produces a Cobrust-workspace-compatible Rust
port of the **stateful** `tomli_loads._parse_int` function that:

1. Compiles when glued to the workspace preamble.
2. Agrees with the CPython 3.11 oracle on 14 deterministic inputs.

This generalises the audit-1 PASS data (leaf `parse_bool`) to a
function with `state.pos` mutation across two distinct phases (sign +
digits loop) and a non-trivial error path. The audit-1 sonnet branch
(bare prompt, `feature/audit-1-tomli-real-llm`) PARTIAL-FAILed on
`parse_bool` for three reasons (wrong return type, `panic!` instead of
`TomliError::new`, hallucinated field names). This sprint's job: show
those gaps are closed structurally for a stateful function too.

## Method

- **Target**: `tomli_loads._parse_int` (11-line Python helper; mutates
  `state.pos` in two phases; non-trivial error path).
- **Provider**: `OpenAiProvider` at `{BASE_URL}` (model `{MODEL}`).
- **Cache discipline**:
  - `SyntheticProvider` NOT registered.
  - `cache_dir` = isolated `tempdir().join("llm_cache")`, verified
    non-existent pre-flight.
- **Builder**: production
  `cobrust_translator::build_translation_prompt_rich(unit, ctx)` per
  ADR-0036 §"Decision".
- **Workspace context**: tomli `Value` + `TomliError` + `State`
  preamble (verbatim from `crates/cobrust-tomli/src/parser.rs`),
  `parse_bool` few-shot example (audit-1's PASS-validated leaf),
  return-type contract `Result<i64, TomliError>`, error contract
  `Err(TomliError::new("expected digit", start))`.
- **Prompt size**: {prompt_chars} chars, {prompt_lines} lines.
- **G2 gate**: synthesized minimal Cargo crate (workspace preamble +
  emitted body) → `cargo check`.
- **G3 gate**: `cargo test` driving 14 deterministic CPython 3.11
  oracle inputs through the emitted function; per-case divergence
  classified `strict | numerical | semantic | divergent`.

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

### G3 — L2.behavior (differential, 14 oracle inputs)

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

## Production-validated signal (§1.2)

{production_signal}

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

- ADR-0036 — sprint binding (this audit).
- ADR-0037 (future) — `@py_compat` hard-bind; anchored on this
  audit's divergence taxonomy if the outcome is not PASS.
- ADR-0032 — audit-1 leaf PASS this audit extends to stateful.
- ADR-0007 — translator pipeline whose synthetic-only default this
  audit deliberately bypasses.
- ADR-0008 — repair loop; the divergence table above forms the
  diagnostic blob for attempt 2 if a follow-up sprint exercises it.
- `finding:audit-1-tomli-real-llm-result` — the leaf PASS this
  builds on.
- `finding:translator-real-vs-synthetic-status` — the gap this
  finding closes for the stateful axis.
- `crates/cobrust-translator/tests/audit_3a_tomli_stateful.rs` —
  harness implementation.
- `crates/cobrust-translator/src/translate.rs::build_translation_prompt_rich`
  — production builder ADR-0036 introduces.
- Memory `feedback_third_party_audit_2026_05_09.md` — handoff §A.3.
- Memory `reference_codex_api.md` — endpoint credentials.
"#,
        BASE_URL = BASE_URL,
        MODEL = MODEL,
        overall = overall,
        ledger_json = ledger_json,
        g2_block = g2_block,
        g3_block = g3_block,
        production_signal = production_signal,
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
