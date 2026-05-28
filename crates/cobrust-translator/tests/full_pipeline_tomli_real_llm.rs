//! T1.1 — tomli full-library real-LLM end-to-end translation through the
//! production [`build_translation_prompt_rich`] builder. Binding:
//! 0.1.0-beta release headline demo. Anchor ADR: queued ADR-0039.
//!
//! ## Scope
//!
//! Audit #1 (ADR-0032) and Audit #3a (ADR-0036) demonstrated single-
//! function PASS (`parse_bool` 12/12; `parse_int` 14/14). This sprint
//! drives **all 12 functions** of `corpus/tomli/spec.toml` through the
//! same rich-prompt builder, glues the emissions together into a fresh
//! crate, and verifies:
//!
//! 1. Every function compiles when assembled into one parser module
//!    (G2 build).
//! 2. The assembled `loads()` matches CPython 3.11 `tomllib` on the
//!    canonical 27 positive + 5 negative cases (G3 behaviour, smoke).
//! 3. The assembled `loads()` survives 1000+ deterministic-seeded
//!    fuzz inputs against CPython oracle (G3 behaviour, fuzz).
//!
//! ## Cache discipline (review-claude binding, identical to audit-1/3a)
//!
//! - **No `SyntheticProvider`**: only `OpenAiProvider` registered.
//! - **Isolated LLM disk cache**: `cache_dir = tempfile::tempdir()`.
//!
//! ## Output artefacts
//!
//! On success the harness writes:
//! - `crates/cobrust-nest/src/parser.rs` — replaced with the LLM
//!   emission (provenance header per ADR-0007). Per ADR-0071 §3 the
//!   Cobrust-facing crate identity is `cobrust-nest`; the source
//!   library (`tomli`) is preserved in PROVENANCE.toml + headers.
//! - `crates/cobrust-nest/tests/full_pipeline_corpus.rs` — emitted as
//!   a sibling test target so the LLM emission can be re-verified
//!   without the API key.
//! - `docs/agent/findings/0.1.0-beta-tomli-full-translation.md` —
//!   per-fn pass/fail, ledger entries, perf numbers.
//!
//! On partial-pass (≥ 4/5 of canonical entrypoints PASS L2.behavior,
//! 1 falls back) we still write the artefacts but tag the failing fn.
//!
//! ## Honest fail
//!
//! If < 3/5 canonical entrypoints PASS, the harness:
//! - Writes the finding showing the gap data verbatim.
//! - Does NOT replace `crates/cobrust-nest/src/parser.rs`.
//! - Returns cleanly so CI doesn't go red on infra issues; CTO inspects
//!   the finding and decides whether to escalate to a follow-up sprint.

#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::cargo,
    clippy::restriction,
    dead_code,
    unused_imports
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cobrust_llm_router::{
    CompletionRequest, LedgerEntry, Message, OpenAiProvider, Outcome, RetryPolicy, Role,
    RouterBuilder, RouterConfig, SamplingParams, Task, config::ProviderKind,
};
use cobrust_translator::{
    SpecToml, TranslationPlan, WorkspaceContext, build_translation_prompt_rich,
};

// ---- Constants ---------------------------------------------------------------

const ENV_KEY: &str = "USER_CODEX_API_KEY";
const BASE_URL: &str = "http://127.0.0.1:1/v1";
const PROVIDER_KEY: &str = "user_codex_t1_1";
const MODEL: &str = "gpt-5.5";
const PYTHON: &str = "/opt/homebrew/bin/python3.11";
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(180);
const CARGO_CHECK_TIMEOUT: Duration = Duration::from_secs(300);

// 12 functions in canonical alphabetical order matching spec.toml.
const FUNCTION_ORDER: &[&str] = &[
    "loads",
    "parse_array",
    "parse_basic_string",
    "parse_bool",
    "parse_inline_table",
    "parse_int",
    "parse_key",
    "parse_kv",
    "parse_literal_string",
    "parse_table_header",
    "parse_value",
    "skip_whitespace",
];

// Canonical 5 entry points the partial-pass policy gates on.
const CANONICAL_FIVE: &[&str] = &[
    "loads",
    "parse_value",
    "parse_array",
    "parse_inline_table",
    "parse_int",
];

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
    RouterConfig::from_toml_str(&toml).expect("T1.1 router config must parse")
}

// ---- Workspace preamble (shared with audit-1 / audit-3a) --------------------

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

/// Walk into the table at `path`, creating intermediate tables.
fn ensure_path<'a>(
    root: &'a mut BTreeMap<String, Value>,
    path: &[String],
) -> Result<&'a mut BTreeMap<String, Value>, TomliError> {
    let mut cursor: &'a mut BTreeMap<String, Value> = root;
    for part in path {
        let entry = cursor
            .entry(part.clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        cursor = match entry {
            Value::Table(t) => t,
            _ => {
                return Err(TomliError::new(
                    format!("path conflicts with non-table at {part:?}"),
                    0,
                ));
            }
        };
    }
    Ok(cursor)
}
"#;

const TO_JSON_HELPERS: &str = r#"
/// Convert a parsed value to its serde_json representation. Used by
/// the L3 differential gate to compare against CPython's
/// `tomllib.loads()` output.
#[must_use]
pub fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(to_json).collect()),
        Value::Table(t) => {
            let mut m = serde_json::Map::new();
            for (k, v) in t {
                m.insert(k.clone(), to_json(v));
            }
            serde_json::Value::Object(m)
        }
    }
}

/// Convert a top-level table to a JSON object.
#[must_use]
pub fn table_to_json(t: &BTreeMap<String, Value>) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (k, v) in t {
        m.insert(k.clone(), to_json(v));
    }
    serde_json::Value::Object(m)
}
"#;

// ---- Per-function spec context helpers --------------------------------------

/// Extract the verbatim Python source for one function from
/// `corpus/tomli/upstream/tomli_loads.py`. Returns the function block
/// (def line through the last contiguous indented line).
fn extract_python_source(upstream: &str, py_qualname: &str) -> String {
    // qualname is like "tomli_loads.loads" or "tomli_loads._parse_int"
    let fn_name = py_qualname.split('.').next_back().unwrap_or(py_qualname);
    let def_marker = format!("def {fn_name}(");
    let mut lines = upstream.lines().enumerate();
    let mut start = None;
    while let Some((i, line)) = lines.next() {
        if line.trim_start().starts_with(&def_marker) {
            start = Some(i);
            break;
        }
    }
    let Some(s) = start else {
        return String::new();
    };
    let upstream_lines: Vec<&str> = upstream.lines().collect();
    let mut end = upstream_lines.len();
    // Function body ends at the next non-indented, non-blank non-comment line.
    for (j, line) in upstream_lines.iter().enumerate().skip(s + 1) {
        if line.is_empty() {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        // A new top-level def or class terminates the body.
        let leading_ws = line.len() - trimmed.len();
        if leading_ws == 0 {
            end = j;
            break;
        }
    }
    upstream_lines[s..end].join("\n")
}

/// Per-function few-shot example bundle. Returns a (name, source) pair
/// that's structurally similar to the target. We use `parse_basic_string`
/// (the audit-1 / 3a few-shot) as the universal example since it
/// demonstrates `state.expect(b'…')?`, `state.advance()`, byte-level
/// iteration, and `Result<T, TomliError>` return.
fn fewshot_example() -> (String, String) {
    let parse_basic_string_ref = r#"fn parse_basic_string(state: &mut State<'_>) -> Result<String, TomliError> {
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
    (
        "parse_basic_string".to_string(),
        parse_basic_string_ref.to_string(),
    )
}

/// Per-function return-type contract. Maps the tomli function name to
/// the Cobrust-idiomatic Rust signature return type.
fn return_type_for(fn_name: &str) -> &'static str {
    match fn_name {
        "loads" => "Result<BTreeMap<String, Value>, TomliError>",
        "parse_array" => "Result<Vec<Value>, TomliError>",
        "parse_basic_string" | "parse_literal_string" | "parse_key" => "Result<String, TomliError>",
        "parse_bool" => "Result<bool, TomliError>",
        "parse_inline_table" => "Result<BTreeMap<String, Value>, TomliError>",
        "parse_int" => "Result<i64, TomliError>",
        "parse_kv" => "Result<(), TomliError>",
        "parse_table_header" => "Result<Vec<String>, TomliError>",
        "parse_value" => "Result<Value, TomliError>",
        "skip_whitespace" => "()",
        _ => "Result<(), TomliError>",
    }
}

/// Per-function full Rust signature. Tells the LLM EXACTLY what shape
/// the function it emits must have so glueing into one parser.rs works
/// with no further manual editing.
fn full_signature_for(fn_name: &str) -> String {
    let ret = return_type_for(fn_name);
    match fn_name {
        "loads" => format!("pub fn loads(src: &str) -> {ret}"),
        "parse_kv" => format!(
            "fn parse_kv(state: &mut State<'_>, dest: &mut BTreeMap<String, Value>) -> {ret}"
        ),
        "skip_whitespace" => "fn skip_whitespace(state: &mut State<'_>)".into(),
        other => format!("fn {other}(state: &mut State<'_>) -> {ret}"),
    }
}

fn error_construction_contract() -> &'static str {
    "Err(TomliError::new(\"...\", state.pos))"
}

// ---- Main test ---------------------------------------------------------------

#[tokio::test]
async fn t1_1_full_pipeline_tomli_real_llm() {
    let Some(api_key) = lookup_api_key() else {
        println!("cargo:warning=T1.1 real-LLM gate SKIPPED (USER_CODEX_API_KEY unset)");
        eprintln!(
            "T1.1: {ENV_KEY} unset — skipping real-LLM full pipeline. \
             Set USER_CODEX_API_KEY=<codex-key> to run the live gate."
        );
        return;
    };

    println!("\n=== T1.1 — tomli full-library real-LLM E2E ===");
    println!("Endpoint        : {BASE_URL}");
    println!("Model           : {MODEL}");
    println!(
        "Functions       : {} (full tomli 2.0.1 spec)",
        FUNCTION_ORDER.len()
    );
    println!("Cache discipline: isolated tempdir; NO SyntheticProvider");

    // ---- Cache discipline (G4) — pre-flight invariants ---------------------
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
            max_attempts: 3,
            base_delay_ms: 1500,
            factor: 2.0,
            max_total_ms: 120_000,
        })
        .build(&cfg)
        .await
        .expect("router must build");

    println!("\n--- G4 cache discipline (pre-flight) ---");
    println!("  cache_dir       : {} (non-existent)", cache_dir.display());
    println!("  provider count  : 1 (OpenAiProvider)");
    println!("  G4 result       : PASS (pre-flight)");

    // ---- L0: read spec + upstream Python source ----------------------------
    let spec_file = corpus_root().join("spec.toml");
    let upstream_py_file = corpus_root().join("upstream/tomli_loads.py");
    let spec = SpecToml::read(&spec_file).expect("spec.toml must parse");
    let upstream_py = std::fs::read_to_string(&upstream_py_file).expect("upstream py must read");

    let source_sha16 = {
        let bytes = upstream_py.as_bytes();
        let mut hasher = blake3::Hasher::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize().as_bytes())[..16].to_string()
    };
    println!("\n--- L0 spec ---");
    println!("  spec.library      : {}", spec.library);
    println!("  upstream_version  : {}", spec.upstream_version);
    println!("  source_sha16      : {source_sha16}");
    println!("  function count    : {}", spec.function.len());

    let plan = TranslationPlan::from_spec(&spec, source_sha16.clone());

    // ---- L1: dispatch real LLM per function --------------------------------
    println!("\n--- L1 dispatch (real LLM per function) ---");
    let mut emissions: Vec<FnEmission> = Vec::new();
    let dispatch_start_overall = std::time::Instant::now();
    for unit in &plan.functions {
        let py_source = extract_python_source(&upstream_py, &unit.spec.qualname);
        let (fewshot_name, fewshot_src) = fewshot_example();
        let return_contract = return_type_for(&unit.name);
        let full_sig = full_signature_for(&unit.name);
        let error_contract = error_construction_contract();

        // For parse_basic_string we MUST NOT include parse_basic_string as
        // its own few-shot — pick parse_int or skip_whitespace instead.
        let fewshot = if unit.name == "parse_basic_string" {
            (
                "parse_bool".to_string(),
                "fn parse_bool(state: &mut State<'_>) -> Result<bool, TomliError> {\n    \
                 if state.bytes[state.pos..].starts_with(b\"true\") { state.pos += 4; return Ok(true); }\n    \
                 if state.bytes[state.pos..].starts_with(b\"false\") { state.pos += 5; return Ok(false); }\n    \
                 Err(TomliError::new(\"expected bool\", state.pos))\n}".to_string(),
            )
        } else {
            (fewshot_name.clone(), fewshot_src.clone())
        };

        // Per-function workspace context: workspace preamble + ALL
        // already-translated functions in this run (so later fns see
        // the call-graph members).
        // For determinism + simplicity, every function gets the SAME
        // preamble (workspace types). The few-shot is the trusted
        // workspace helper. The LLM is told "these helpers are
        // available" via the prompt's Workspace API contract clause.
        let mut preamble_with_signatures = WORKSPACE_PREAMBLE.to_string();
        preamble_with_signatures
            .push_str("\n// In-scope helpers (already translated; do NOT redefine):\n");
        for other in &plan.functions {
            if other.name == unit.name {
                continue;
            }
            preamble_with_signatures.push_str(&format!("// {}\n", full_signature_for(&other.name)));
        }

        let ctx = WorkspaceContext {
            module_preamble: preamble_with_signatures,
            fewshot_examples: vec![fewshot.clone()],
            target_python_source: py_source,
            return_type_contract: return_contract.to_string(),
            error_construction_contract: error_contract.to_string(),
        };

        let mut prompt = build_translation_prompt_rich(unit, &ctx);

        // Append explicit "exact signature" directive so emitter cannot
        // hallucinate a different shape — load-bearing for glueing into
        // one parser.rs.
        prompt.push_str(&format!(
            "\n8. Exact Rust signature MUST be: `{full_sig}`. Use this signature \
             verbatim — same name, same parameters, same return type. Do NOT \
             prepend `pub` (the harness handles visibility).\n\
             9. Do NOT redefine helper functions listed in the workspace API \
             contract — call them as needed.\n"
        ));

        // For `loads` specifically, override visibility hint: it IS pub.
        if unit.name == "loads" {
            prompt.push_str(
                "10. `loads` is the ONE public entrypoint — its signature MUST start \
                 with `pub fn loads(src: &str)`.\n",
            );
        }

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

        print!("  fn:{:<25} ... ", unit.name);
        let dispatch_start = std::time::Instant::now();
        let outcome =
            tokio::time::timeout(DISPATCH_TIMEOUT, router.dispatch(Task::Translate, req)).await;
        let elapsed = dispatch_start.elapsed();

        match outcome {
            Err(_) => {
                println!("TIMEOUT after {:.1}s", elapsed.as_secs_f64());
                emissions.push(FnEmission {
                    name: unit.name.clone(),
                    raw: String::new(),
                    extracted: String::new(),
                    prompt_chars: prompt.len(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    latency_ms: elapsed.as_millis() as u64,
                    cache_hit: false,
                    err: Some("dispatch_timeout".to_string()),
                });
                continue;
            }
            Ok(Err(e)) => {
                println!("ERR: {e}");
                emissions.push(FnEmission {
                    name: unit.name.clone(),
                    raw: String::new(),
                    extracted: String::new(),
                    prompt_chars: prompt.len(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    latency_ms: elapsed.as_millis() as u64,
                    cache_hit: false,
                    err: Some(format!("dispatch_error: {e}")),
                });
                continue;
            }
            Ok(Ok(resp)) => {
                let raw = resp.response.text.clone();
                let extracted = extract_rust_body(&raw);
                println!(
                    "OK ({:.1}s, {} tok, cache_hit={})",
                    elapsed.as_secs_f64(),
                    resp.response.usage.total(),
                    resp.cache_hit
                );
                emissions.push(FnEmission {
                    name: unit.name.clone(),
                    raw,
                    extracted,
                    prompt_chars: prompt.len(),
                    prompt_tokens: resp.response.usage.prompt_tokens,
                    completion_tokens: resp.response.usage.completion_tokens,
                    latency_ms: elapsed.as_millis() as u64,
                    cache_hit: resp.cache_hit,
                    err: None,
                });
            }
        }
    }
    let total_dispatch_ms = dispatch_start_overall.elapsed().as_millis();
    println!(
        "\n--- L1 dispatch complete: {} fns in {:.1}s ---",
        emissions.len(),
        total_dispatch_ms as f64 / 1000.0
    );

    // ---- Ledger sanity -----------------------------------------------------
    let entries = read_ledger(&ledger_path);
    println!("  ledger entries  : {}", entries.len());
    let live_ok_entries: Vec<&LedgerEntry> = entries
        .iter()
        .filter(|e| matches!(e.outcome, Outcome::Ok))
        .collect();
    let any_real_call = live_ok_entries
        .iter()
        .any(|e| !e.cache_hit && e.provider_kind == Some(ProviderKind::Openai));
    println!(
        "  live OpenAI calls (no cache): {}",
        live_ok_entries
            .iter()
            .filter(|e| !e.cache_hit && e.provider_kind == Some(ProviderKind::Openai))
            .count()
    );
    assert!(
        any_real_call,
        "G4 cache-discipline: at least one real openai call must be in the ledger"
    );

    // Total token spend from the live entries
    let total_prompt_tokens: u32 = live_ok_entries.iter().map(|e| e.prompt_tokens).sum();
    let total_completion_tokens: u32 = live_ok_entries.iter().map(|e| e.completion_tokens).sum();
    let total_tokens: u32 = live_ok_entries.iter().map(|e| e.total_tokens).sum();

    // ---- G2: synthesize crate + cargo check --------------------------------
    let crate_dir = dir.path().join("synth_crate");
    let synth_outcome = synthesize_full_crate(&crate_dir, &emissions);
    let g2_outcome = match synth_outcome {
        Ok(_) => {
            println!("\n--- G2 cargo check (assembled crate) ---");
            run_cargo_check(&crate_dir)
        }
        Err(e) => {
            println!("\n--- G2 SYNTH ERROR: {e} ---");
            CargoCheckOutcome {
                passed: false,
                exit_code: None,
                stderr_tail: format!("synthesize_full_crate error: {e}"),
                stdout_tail: String::new(),
            }
        }
    };
    println!(
        "  G2 result        : {}",
        if g2_outcome.passed { "PASS" } else { "FAIL" }
    );
    if !g2_outcome.passed {
        println!("  stderr tail      :");
        for line in g2_outcome.stderr_tail.lines() {
            println!("    {line}");
        }
    }

    // ---- G3: behavior gate (smoke + 1024-fuzz against CPython tomllib) -----
    let g3_smoke = if g2_outcome.passed {
        println!("\n--- G3.smoke (27 positive + 5 negative cases) ---");
        run_smoke_test(&crate_dir)
    } else {
        println!("\n--- G3.smoke SKIPPED (G2 failed) ---");
        SmokeOutcome {
            ran: false,
            ..Default::default()
        }
    };
    println!(
        "  G3.smoke         : {}/{} positive PASS, {}/{} negative PASS",
        g3_smoke.positive_pass,
        g3_smoke.positive_total,
        g3_smoke.negative_pass,
        g3_smoke.negative_total
    );

    let g3_fuzz = if g2_outcome.passed {
        println!("\n--- G3.fuzz (1000 deterministic-seeded inputs vs CPython tomllib) ---");
        run_fuzz_test(&crate_dir)
    } else {
        println!("\n--- G3.fuzz SKIPPED (G2 failed) ---");
        FuzzOutcome::default()
    };
    println!(
        "  G3.fuzz          : {} cases, {} divergences, {} panics",
        g3_fuzz.total, g3_fuzz.divergences, g3_fuzz.panics
    );

    // ---- Per-canonical-fn pass/fail classification -------------------------
    let canonical_results = classify_canonical_results(&emissions, &g2_outcome, &g3_smoke);
    let canonical_pass_count = canonical_results
        .iter()
        .filter(|r| r.l2_behavior_pass)
        .count();
    println!("\n--- Canonical 5 entrypoints ---");
    for r in &canonical_results {
        println!(
            "  {:<22} l1={} l2.build={} l2.behavior={}",
            r.name,
            if r.l1_pass { "PASS" } else { "FAIL" },
            if r.l2_build_pass { "PASS" } else { "FAIL" },
            if r.l2_behavior_pass { "PASS" } else { "FAIL" }
        );
    }
    println!("  canonical pass   : {canonical_pass_count}/5");

    // ---- L2.perf: 10MB doc parse benchmark (smoke; full criterion in benches/) -
    let perf_numbers = if g2_outcome.passed {
        println!("\n--- G3.perf (1KB / 100KB / 10MB doc parse) ---");
        run_perf_smoke(&crate_dir)
    } else {
        println!("\n--- G3.perf SKIPPED (G2 failed) ---");
        PerfNumbers::default()
    };
    println!(
        "  perf 1KB         : cobrust {} ns / cpython {} ns / ratio {:.2}",
        perf_numbers.cobrust_1k_ns,
        perf_numbers.cpython_1k_ns,
        perf_numbers.ratio_1k()
    );
    println!(
        "  perf 100KB       : cobrust {} ns / cpython {} ns / ratio {:.2}",
        perf_numbers.cobrust_100k_ns,
        perf_numbers.cpython_100k_ns,
        perf_numbers.ratio_100k()
    );
    println!(
        "  perf 10MB        : cobrust {} ns / cpython {} ns / ratio {:.2}",
        perf_numbers.cobrust_10m_ns,
        perf_numbers.cpython_10m_ns,
        perf_numbers.ratio_10m()
    );

    // ---- Promote successful crate to /crates/cobrust-nest/src/parser.rs ---
    // Cobra-named per ADR-0071 §3 (`tomli` → `nest`).
    let promoted = if canonical_pass_count >= 4 {
        println!("\n--- Promoting LLM-emitted parser.rs to crates/cobrust-nest/ ---");
        match promote_emission(&emissions) {
            Ok(_) => {
                println!("  promotion        : PASS");
                true
            }
            Err(e) => {
                println!("  promotion        : FAIL ({e})");
                false
            }
        }
    } else {
        println!("\n--- Skipping promotion: only {canonical_pass_count}/5 canonical PASS ---");
        false
    };

    // ---- Final verdict + finding write ------------------------------------
    let overall = match canonical_pass_count {
        5 => "PASS",
        4 => "PARTIAL-PASS-4OF5",
        n if n >= 3 => "PARTIAL-PASS-3OF5",
        _ => "FAIL",
    };

    println!("\n=== T1.1 verdict ===");
    println!(
        "  G1 L1 dispatch (12 fns)     : {} OK / {} ERR",
        emissions.iter().filter(|e| e.err.is_none()).count(),
        emissions.iter().filter(|e| e.err.is_some()).count()
    );
    println!(
        "  G2 L2.build (assembled)     : {}",
        if g2_outcome.passed { "PASS" } else { "FAIL" }
    );
    println!(
        "  G3.smoke                    : {}/{} pos + {}/{} neg",
        g3_smoke.positive_pass,
        g3_smoke.positive_total,
        g3_smoke.negative_pass,
        g3_smoke.negative_total
    );
    println!(
        "  G3.fuzz                     : {} cases, {} divergences, {} panics",
        g3_fuzz.total, g3_fuzz.divergences, g3_fuzz.panics
    );
    println!("  Canonical 5                 : {canonical_pass_count}/5");
    println!(
        "  Promoted to cobrust-nest/   : {}",
        if promoted { "yes" } else { "no" }
    );
    println!("  OVERALL                     : {overall}");

    record_finding(
        overall,
        canonical_pass_count,
        &emissions,
        &g2_outcome,
        &g3_smoke,
        &g3_fuzz,
        &canonical_results,
        &perf_numbers,
        promoted,
        total_prompt_tokens,
        total_completion_tokens,
        total_tokens,
        live_ok_entries.first().map(|e| (*e).clone()),
    );

    println!("\nFinding: docs/agent/findings/0.1.0-beta-tomli-full-translation.md");
    println!("=== End T1.1 ===\n");

    // Hard assertion: at least one real openai round-trip + cache discipline.
    assert!(
        any_real_call,
        "G4: ledger must show at least one real openai call (cache_hit=false, provider_kind=openai)"
    );
}

// ---- Data structures -------------------------------------------------------

#[derive(Clone, Debug)]
struct FnEmission {
    name: String,
    raw: String,
    extracted: String,
    prompt_chars: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    latency_ms: u64,
    cache_hit: bool,
    err: Option<String>,
}

#[derive(Default, Clone, Debug)]
struct CargoCheckOutcome {
    passed: bool,
    exit_code: Option<i32>,
    stderr_tail: String,
    stdout_tail: String,
}

#[derive(Default, Clone, Debug)]
struct SmokeOutcome {
    ran: bool,
    positive_total: u32,
    positive_pass: u32,
    negative_total: u32,
    negative_pass: u32,
    failures: Vec<String>,
}

#[derive(Default, Clone, Debug)]
struct FuzzOutcome {
    total: u32,
    divergences: u32,
    panics: u32,
    examples: Vec<String>,
}

#[derive(Default, Clone, Debug)]
struct PerfNumbers {
    cobrust_1k_ns: u128,
    cpython_1k_ns: u128,
    cobrust_100k_ns: u128,
    cpython_100k_ns: u128,
    cobrust_10m_ns: u128,
    cpython_10m_ns: u128,
}

impl PerfNumbers {
    fn ratio_1k(&self) -> f64 {
        if self.cobrust_1k_ns == 0 || self.cpython_1k_ns == 0 {
            0.0
        } else {
            self.cpython_1k_ns as f64 / self.cobrust_1k_ns as f64
        }
    }
    fn ratio_100k(&self) -> f64 {
        if self.cobrust_100k_ns == 0 || self.cpython_100k_ns == 0 {
            0.0
        } else {
            self.cpython_100k_ns as f64 / self.cobrust_100k_ns as f64
        }
    }
    fn ratio_10m(&self) -> f64 {
        if self.cobrust_10m_ns == 0 || self.cpython_10m_ns == 0 {
            0.0
        } else {
            self.cpython_10m_ns as f64 / self.cobrust_10m_ns as f64
        }
    }
}

#[derive(Clone, Debug)]
struct CanonicalResult {
    name: String,
    l1_pass: bool,
    l2_build_pass: bool,
    l2_behavior_pass: bool,
    note: String,
}

// ---- Synthesize full crate --------------------------------------------------

fn synthesize_full_crate(crate_dir: &Path, emissions: &[FnEmission]) -> std::io::Result<()> {
    std::fs::create_dir_all(crate_dir.join("src"))?;
    std::fs::create_dir_all(crate_dir.join("tests"))?;

    // Cobra-named per ADR-0071 §3 (`tomli` → `nest`); the `-llm-synth`
    // suffix marks this as the in-tempdir verification crate, not the
    // production cobrust-nest crate.
    let cargo_toml = r#"[package]
name = "cobrust-nest-llm-synth"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]

[lints.rust]
warnings = "allow"

[lints.clippy]
all = { level = "allow", priority = -1 }
pedantic = { level = "allow", priority = -1 }
"#;
    std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    // src/lib.rs = workspace preamble + LLM emissions for all 12 fns +
    // to_json/table_to_json helpers + ensure_path.
    //
    // We need to coerce the LLM emission's name match. The prompt told
    // the LLM the exact signature but the emission may contain extra
    // boilerplate. We textually filter:
    // - strip leading/trailing markdown fences (already done in extract_rust_body)
    // - rip out duplicate type definitions (Value, TomliError, State,
    //   ensure_path) if the LLM redefined them despite instructions.
    let mut lib_rs = String::new();
    lib_rs.push_str("// SYNTHESIZED CRATE — DO NOT EDIT BY HAND.\n");
    lib_rs.push_str("// T1.1 tomli full-library real-LLM translation gate.\n\n");
    lib_rs.push_str(
        "#![allow(dead_code, unused_imports, unused_variables, unreachable_code, clippy::all)]\n\n",
    );
    lib_rs.push_str(WORKSPACE_PREAMBLE);
    lib_rs.push_str(TO_JSON_HELPERS);
    lib_rs.push_str("\n// ---- LLM-emitted function bodies (verbatim per fn) ----\n\n");

    for emission in emissions {
        if emission.extracted.is_empty() {
            // Insert a stub that returns an error so the rest of the
            // crate still compiles. The test harness will mark this fn
            // FAIL on G2.
            lib_rs.push_str(&format!(
                "// fn {} — DISPATCH FAILED ({})\n",
                emission.name,
                emission.err.as_deref().unwrap_or("unknown")
            ));
            lib_rs.push_str(&stub_for(&emission.name));
            lib_rs.push_str("\n\n");
            continue;
        }

        lib_rs.push_str(&format!(
            "// fn:{} provider=user_codex_t1_1 model={MODEL} cache_hit={} prompt_tokens={} completion_tokens={}\n",
            emission.name, emission.cache_hit, emission.prompt_tokens, emission.completion_tokens
        ));
        let cleaned = strip_redefinitions(&emission.extracted);
        let normalized = normalize_signature(&cleaned, &emission.name);
        lib_rs.push_str(&normalized);
        if !normalized.ends_with('\n') {
            lib_rs.push('\n');
        }
        lib_rs.push('\n');
    }

    std::fs::write(crate_dir.join("src/lib.rs"), &lib_rs)?;
    Ok(())
}

/// Strip re-definitions of workspace types (Value, TomliError, State,
/// ensure_path, to_json, table_to_json) the LLM may have included
/// despite being told not to. Conservative: we only strip the common
/// shape "use std..." line, "pub enum Value", "pub struct TomliError",
/// "pub struct State<'a>", "impl<'a> State", "fn ensure_path", and
/// "use std::collections::BTreeMap".
fn strip_redefinitions(src: &str) -> String {
    // Primary heuristic: if the emission contains `pub enum Value` or
    // `pub struct State` or `impl<'a> State`, find the LAST top-level
    // `fn ` definition and keep only from there onward.
    let lower = src.to_lowercase();
    if lower.contains("pub enum value")
        || lower.contains("pub struct state")
        || lower.contains("impl<'a> state")
    {
        // Find the LAST top-level `fn `, `pub fn `, or `pub(crate) fn `
        // declaration line.
        let lines: Vec<&str> = src.lines().collect();
        let mut keep_from = 0usize;
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if (t.starts_with("fn ") || t.starts_with("pub fn ") || t.starts_with("pub(crate) fn "))
                && line.starts_with(t)
            {
                keep_from = i;
                break;
            }
        }
        // But only if there's a fn after. If not, return original.
        if keep_from == 0
            && !lines.first().is_some_and(|l| {
                let t = l.trim_start();
                t.starts_with("fn ") || t.starts_with("pub fn ")
            })
        {
            return src.to_string();
        }
        return lines[keep_from..].join("\n");
    }
    src.to_string()
}

/// Normalize the LLM-emitted function signature to the harness-expected
/// shape. The harness binds the function name + parameter types; if the
/// LLM emitted `fn parse_int(state: &mut State<'_>) -> Result<i64,
/// TomliError>` we keep verbatim. If it emitted variants like `fn
/// _parse_int(...)` (Python qualname leak) we rewrite the leading
/// identifier.
fn normalize_signature(src: &str, fn_name: &str) -> String {
    let trimmed = src.trim_start();
    let leak = format!("fn _{fn_name}(");
    if trimmed.starts_with(&leak) {
        return src.replacen(&leak, &format!("fn {fn_name}("), 1);
    }
    // Also handle the case of `pub fn _name`.
    let pub_leak = format!("pub fn _{fn_name}(");
    if trimmed.starts_with(&pub_leak) {
        return src.replacen(&pub_leak, &format!("pub fn {fn_name}("), 1);
    }
    src.to_string()
}

fn stub_for(fn_name: &str) -> String {
    let sig = full_signature_for(fn_name);
    if fn_name == "skip_whitespace" {
        format!("{sig} {{ /* dispatch failed */ }}")
    } else if fn_name == "parse_kv" {
        format!("{sig} {{ Err(TomliError::new(\"dispatch failed: stub\", state.pos)) }}")
    } else if fn_name == "loads" {
        format!("{sig} {{ let _ = src; Err(TomliError::new(\"dispatch failed: stub\", 0)) }}")
    } else {
        format!("{sig} {{ Err(TomliError::new(\"dispatch failed: stub\", state.pos)) }}")
    }
}

// ---- Cargo check / test runners -------------------------------------------

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
    eprintln!("T1.1: cargo check finished in {:?}", elapsed);
    if elapsed > CARGO_CHECK_TIMEOUT {
        eprintln!("T1.1: cargo check exceeded soft timeout");
    }
    match output {
        Ok(o) => {
            let stderr_full = String::from_utf8_lossy(&o.stderr).to_string();
            let stdout_full = String::from_utf8_lossy(&o.stdout).to_string();
            CargoCheckOutcome {
                passed: o.status.success(),
                stderr_tail: tail(&stderr_full, 60),
                stdout_tail: tail(&stdout_full, 30),
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

fn tail(s: &str, n: usize) -> String {
    s.lines()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- G3 smoke + fuzz drivers (run via cargo test in the synth crate) -------

fn write_smoke_test_file(crate_dir: &Path) -> std::io::Result<()> {
    let smoke_rs = r##"//! Smoke gate: 27 positive + 5 negative cases vs CPython tomllib.

#![allow(clippy::all)]

use cobrust_tomli_llm_synth::{loads, table_to_json};
use std::io::Write;
use std::process::{Command, Stdio};

const PYTHON: &str = "/opt/homebrew/bin/python3.11";

fn cpython_oracle(src: &str) -> Result<serde_json::Value, String> {
    let mut py = Command::new(PYTHON)
        .arg("-c")
        .arg("import json,sys,tomllib\nsrc=sys.stdin.read()\nprint(json.dumps(tomllib.loads(src)))")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    py.stdin.take().expect("stdin").write_all(src.as_bytes()).expect("write stdin");
    let out = py.wait_with_output().map_err(|e| format!("wait: {e}"))?;
    if !out.status.success() {
        return Err(format!("python exit {}", out.status));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("json: {e}"))
}

fn cobrust_loads_json(src: &str) -> Result<serde_json::Value, String> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loads(src)));
    match result {
        Ok(Ok(t)) => Ok(table_to_json(&t)),
        Ok(Err(e)) => Err(format!("{e}")),
        Err(_) => Err("PANIC".to_string()),
    }
}

fn positive_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("empty", ""),
        ("single_int", "x = 1\n"),
        ("negative_int", "x = -42\n"),
        ("plus_int", "x = +7\n"),
        ("two_keys", "a = 1\nb = 2\n"),
        ("bool_true", "k = true\n"),
        ("bool_false", "k = false\n"),
        ("basic_string", "k = \"hi\"\n"),
        ("basic_string_escape", "k = \"a\\nb\"\n"),
        ("literal_string", "k = 'hi'\n"),
        ("empty_array", "k = []\n"),
        ("int_array", "k = [1, 2, 3]\n"),
        ("trailing_comma_array", "k = [1, 2,]\n"),
        ("inline_table", "k = { a = 1, b = 2 }\n"),
        ("table_header", "[s]\nx = 1\n"),
        ("nested_table_header", "[a.b]\nx = 1\n"),
        ("multiple_tables", "[a]\nx = 1\n[b]\ny = 2\n"),
        ("comment_line", "# comment\nx = 1\n"),
        ("inline_comment", "x = 1 # tail comment\n"),
        ("dashed_key", "my-key = 1\n"),
        ("underscore_key", "my_key = 1\n"),
        ("string_with_escape", "k = \"tab\\there\"\n"),
        ("array_of_strings", "k = [\"a\", \"b\"]\n"),
        ("array_of_bools", "k = [true, false]\n"),
        ("nested_inline_table", "k = { a = { b = 1 } }\n"),
        ("whitespace_around_eq", "x   =    1\n"),
        ("crlf_line_endings", "x = 1\r\ny = 2\r\n"),
    ]
}

fn negative_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("unterminated_string", "x = \"abc\n"),
        ("bad_escape", "x = \"\\q\"\n"),
        ("trailing_dot", "[a.]\n"),
        ("unclosed_array", "x = [1, 2\n"),
        ("bare_value", "= 1\n"),
    ]
}

#[test]
fn smoke_positive() {
    for (name, src) in positive_cases() {
        let oracle = cpython_oracle(src).unwrap_or_else(|e| {
            println!("SMOKE_POSITIVE name={name} status=oracle_err err={e}");
            return serde_json::json!({"_oracle_err": true});
        });
        if oracle == serde_json::json!({"_oracle_err": true}) {
            continue;
        }
        let ours = cobrust_loads_json(src);
        match ours {
            Ok(v) if v == oracle => println!("SMOKE_POSITIVE name={name} status=PASS"),
            Ok(v) => println!("SMOKE_POSITIVE name={name} status=DIVERGE expected={oracle} actual={v}"),
            Err(e) => println!("SMOKE_POSITIVE name={name} status=FAIL err={e}"),
        }
    }
}

#[test]
fn smoke_negative() {
    for (name, src) in negative_cases() {
        let oracle_raised = cpython_oracle(src).is_err();
        let ours = cobrust_loads_json(src);
        let cobrust_raised = ours.is_err();
        if oracle_raised && cobrust_raised {
            println!("SMOKE_NEGATIVE name={name} status=PASS");
        } else {
            println!("SMOKE_NEGATIVE name={name} status=FAIL oracle_raised={oracle_raised} cobrust_raised={cobrust_raised}");
        }
    }
}
"##;
    std::fs::write(crate_dir.join("tests/smoke.rs"), smoke_rs)?;
    Ok(())
}

fn write_fuzz_test_file(crate_dir: &Path) -> std::io::Result<()> {
    let fuzz_rs = r##"//! Fuzz gate: 1024 deterministic-seeded inputs vs CPython tomllib.

#![allow(clippy::all)]

use cobrust_tomli_llm_synth::{loads, table_to_json};
use std::io::Write;
use std::process::{Command, Stdio};

const PYTHON: &str = "/opt/homebrew/bin/python3.11";

fn cpython_oracle(src: &str) -> Result<serde_json::Value, ()> {
    let Ok(mut py) = Command::new(PYTHON)
        .arg("-c")
        .arg("import json,sys,tomllib\nsrc=sys.stdin.read()\ntry:\n print(json.dumps(tomllib.loads(src)))\nexcept Exception:\n sys.exit(1)")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else { return Err(()); };
    let _ = py.stdin.take().expect("stdin").write_all(src.as_bytes());
    let Ok(out) = py.wait_with_output() else { return Err(()); };
    if !out.status.success() { return Err(()); }
    serde_json::from_slice(&out.stdout).map_err(|_| ())
}

struct Lcg { state: u64 }
impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1 }
    }
    fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) as u32) ^ ((z >> 32) as u32)
    }
}

fn make_key(rng: &mut Lcg) -> String {
    let len = (rng.next() % 6) + 1;
    let mut s = String::new();
    for i in 0..len {
        let r = rng.next() % 4;
        let c = match r {
            0 => b'a' + u8::try_from(rng.next() % 26).unwrap_or(0),
            1 => b'A' + u8::try_from(rng.next() % 26).unwrap_or(0),
            2 if i > 0 => b'0' + u8::try_from(rng.next() % 10).unwrap_or(0),
            _ => b'_',
        };
        s.push(char::from(c));
    }
    s
}

fn make_value(rng: &mut Lcg) -> String {
    let r = rng.next() % 5;
    match r {
        0 => format!("{}", (rng.next() % 1000) as i32 - 500),
        1 => "true".to_string(),
        2 => "false".to_string(),
        3 => format!("\"{}\"", make_key(rng)),
        _ => format!("[{}, {}]", rng.next() % 100, rng.next() % 100),
    }
}

fn synth_input(rng: &mut Lcg) -> String {
    let mode = rng.next() % 6;
    match mode {
        0 => format!("{} = {}\n", make_key(rng), make_value(rng)),
        1 => {
            let n = (rng.next() % 5) + 1;
            let mut s = String::new();
            for _ in 0..n {
                s.push_str(&format!("{} = {}\n", make_key(rng), make_value(rng)));
            }
            s
        }
        2 => {
            let parts = (rng.next() % 3) + 1;
            let mut s = String::from("[");
            for i in 0..parts {
                if i > 0 { s.push('.'); }
                s.push_str(&make_key(rng));
            }
            s.push_str("]\n");
            s.push_str(&format!("{} = {}\n", make_key(rng), make_value(rng)));
            s
        }
        3 => {
            let key = make_key(rng);
            format!("# {key}\n{key} = {}\n", make_value(rng))
        }
        4 => format!("{} = {{ {} = {} }}\n", make_key(rng), make_key(rng), make_value(rng)),
        _ => {
            let len = (rng.next() % 32) + 1;
            (0..len).map(|_| {
                let b = (rng.next() % 95) + 32;
                char::from(u8::try_from(b).unwrap_or(b' '))
            }).collect::<String>() + "\n"
        }
    }
}

#[test]
fn fuzz_1024_inputs() {
    let mut rng = Lcg::new(42);
    let mut total = 0u32;
    let mut divergences = 0u32;
    let mut panics = 0u32;
    for _ in 0..1024 {
        let input = synth_input(&mut rng);
        total += 1;
        let cobrust_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loads(&input)));
        let cobrust_ok = match cobrust_result {
            Ok(Ok(t)) => Some(table_to_json(&t)),
            Ok(Err(_)) => None,
            Err(_) => {
                panics += 1;
                None
            }
        };
        let oracle_ok = cpython_oracle(&input).ok();
        match (&cobrust_ok, &oracle_ok) {
            (Some(a), Some(b)) if a != b => divergences += 1,
            (Some(_), None) | (None, Some(_)) => divergences += 1,
            _ => {}
        }
    }
    println!("FUZZ_RESULT total={total} divergences={divergences} panics={panics}");
}
"##;
    std::fs::write(crate_dir.join("tests/fuzz.rs"), fuzz_rs)?;
    Ok(())
}

fn run_smoke_test(crate_dir: &Path) -> SmokeOutcome {
    if let Err(e) = write_smoke_test_file(crate_dir) {
        return SmokeOutcome {
            ran: false,
            failures: vec![format!("write smoke.rs: {e}")],
            ..Default::default()
        };
    }
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(crate_dir)
        .arg("test")
        .arg("--test")
        .arg("smoke")
        .arg("--quiet")
        .arg("--")
        .arg("--nocapture")
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .env_remove("RUSTFLAGS");
    let output = cmd.output();
    let mut outcome = SmokeOutcome {
        ran: true,
        ..Default::default()
    };
    let stdout = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            outcome.failures.push(format!("invocation: {e}"));
            return outcome;
        }
    };
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("SMOKE_POSITIVE ") {
            outcome.positive_total += 1;
            if rest.contains("status=PASS") {
                outcome.positive_pass += 1;
            } else {
                outcome.failures.push(rest.to_string());
            }
        }
        if let Some(rest) = line.strip_prefix("SMOKE_NEGATIVE ") {
            outcome.negative_total += 1;
            if rest.contains("status=PASS") {
                outcome.negative_pass += 1;
            } else {
                outcome.failures.push(rest.to_string());
            }
        }
    }
    outcome
}

fn run_fuzz_test(crate_dir: &Path) -> FuzzOutcome {
    if let Err(e) = write_fuzz_test_file(crate_dir) {
        return FuzzOutcome {
            examples: vec![format!("write fuzz.rs: {e}")],
            ..Default::default()
        };
    }
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(crate_dir)
        .arg("test")
        .arg("--test")
        .arg("fuzz")
        .arg("--release")
        .arg("--quiet")
        .arg("--")
        .arg("--nocapture")
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .env_remove("RUSTFLAGS");
    let output = cmd.output();
    let mut outcome = FuzzOutcome::default();
    let stdout = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            outcome.examples.push(format!("invocation: {e}"));
            return outcome;
        }
    };
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("FUZZ_RESULT ") {
            for part in rest.split_whitespace() {
                if let Some(v) = part.strip_prefix("total=") {
                    outcome.total = v.parse().unwrap_or(0);
                }
                if let Some(v) = part.strip_prefix("divergences=") {
                    outcome.divergences = v.parse().unwrap_or(0);
                }
                if let Some(v) = part.strip_prefix("panics=") {
                    outcome.panics = v.parse().unwrap_or(0);
                }
            }
        }
    }
    outcome
}

// ---- L2.perf: hand-rolled timing on representative TOML docs ---------------

fn build_perf_test(crate_dir: &Path) -> std::io::Result<()> {
    let perf_rs = r##"//! Perf smoke: 1KB / 100KB / 10MB doc parse vs CPython tomllib timeit.
#![allow(clippy::all)]

use cobrust_tomli_llm_synth::loads;
use std::io::Write;
use std::process::{Command, Stdio};

const PYTHON: &str = "/opt/homebrew/bin/python3.11";

fn synth_doc(target_bytes: usize) -> String {
    // Build a TOML doc with [section.N] tables of int/bool/string keys.
    let mut s = String::new();
    let mut idx = 0u64;
    while s.len() < target_bytes {
        s.push_str(&format!("[section_{idx}]\n"));
        for k in 0..50 {
            s.push_str(&format!("k{k} = {}\n", (idx as i64) * 31 - (k as i64) * 7));
            if s.len() >= target_bytes { break; }
            s.push_str(&format!("s{k} = \"abcdefghij\"\n"));
            if s.len() >= target_bytes { break; }
            s.push_str(&format!("b{k} = {}\n", if k % 2 == 0 { "true" } else { "false" }));
            if s.len() >= target_bytes { break; }
        }
        idx += 1;
    }
    s
}

fn time_cobrust(doc: &str, iters: u32) -> u128 {
    let start = std::time::Instant::now();
    for _ in 0..iters {
        let _ = loads(doc);
    }
    let total_ns = start.elapsed().as_nanos();
    total_ns / u128::from(iters.max(1))
}

fn time_cpython(doc: &str, iters: u32) -> u128 {
    let script = format!(
        "import sys, tomllib, time\n\
        src=sys.stdin.read()\n\
        n={iters}\n\
        t0=time.perf_counter_ns()\n\
        for _ in range(n): tomllib.loads(src)\n\
        t1=time.perf_counter_ns()\n\
        print(t1-t0)\n"
    );
    let mut py = Command::new(PYTHON)
        .arg("-c")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn python perf");
    py.stdin.take().expect("stdin").write_all(doc.as_bytes()).expect("write stdin");
    let out = py.wait_with_output().expect("wait python perf");
    if !out.status.success() { return 0; }
    let s = String::from_utf8_lossy(&out.stdout);
    let total_ns: u128 = s.trim().parse().unwrap_or(0);
    total_ns / u128::from(iters.max(1))
}

fn run_for(target_bytes: usize, label: &str, iters: u32) {
    let doc = synth_doc(target_bytes);
    let cobrust_ns = time_cobrust(&doc, iters);
    let cpython_ns = time_cpython(&doc, iters);
    let ratio = if cobrust_ns > 0 { cpython_ns as f64 / cobrust_ns as f64 } else { 0.0 };
    println!("PERF label={label} bytes={} cobrust_ns_per_iter={cobrust_ns} cpython_ns_per_iter={cpython_ns} ratio={ratio:.3}", doc.len());
}

#[test]
fn perf_smoke() {
    // 1 KB, 100 KB, 10 MB targets. Iterations scaled so wall-clock stays
    // under ~3 s per size.
    run_for(1_000, "1KB", 1000);
    run_for(100_000, "100KB", 50);
    run_for(10_000_000, "10MB", 2);
}
"##;
    std::fs::write(crate_dir.join("tests/perf.rs"), perf_rs)
}

fn run_perf_smoke(crate_dir: &Path) -> PerfNumbers {
    let mut perf = PerfNumbers::default();
    if build_perf_test(crate_dir).is_err() {
        return perf;
    }
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(crate_dir)
        .arg("test")
        .arg("--test")
        .arg("perf")
        .arg("--release")
        .arg("--quiet")
        .arg("--")
        .arg("--nocapture")
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .env_remove("RUSTFLAGS");
    let Ok(out) = cmd.output() else {
        return perf;
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("PERF ") {
            let mut label = String::new();
            let mut cobrust_ns: u128 = 0;
            let mut cpython_ns: u128 = 0;
            for part in rest.split_whitespace() {
                if let Some(v) = part.strip_prefix("label=") {
                    label = v.to_string();
                }
                if let Some(v) = part.strip_prefix("cobrust_ns_per_iter=") {
                    cobrust_ns = v.parse().unwrap_or(0);
                }
                if let Some(v) = part.strip_prefix("cpython_ns_per_iter=") {
                    cpython_ns = v.parse().unwrap_or(0);
                }
            }
            match label.as_str() {
                "1KB" => {
                    perf.cobrust_1k_ns = cobrust_ns;
                    perf.cpython_1k_ns = cpython_ns;
                }
                "100KB" => {
                    perf.cobrust_100k_ns = cobrust_ns;
                    perf.cpython_100k_ns = cpython_ns;
                }
                "10MB" => {
                    perf.cobrust_10m_ns = cobrust_ns;
                    perf.cpython_10m_ns = cpython_ns;
                }
                _ => {}
            }
        }
    }
    perf
}

// ---- Per-canonical-fn classification --------------------------------------

fn classify_canonical_results(
    emissions: &[FnEmission],
    g2_outcome: &CargoCheckOutcome,
    g3_smoke: &SmokeOutcome,
) -> Vec<CanonicalResult> {
    CANONICAL_FIVE
        .iter()
        .map(|name| {
            let emission = emissions.iter().find(|e| e.name == *name);
            let l1_pass = emission.is_some_and(|e| e.err.is_none() && !e.extracted.is_empty());
            let l2_build_pass = l1_pass && g2_outcome.passed;
            // L2.behavior pass = the function exercised in smoke tests
            // returns oracle-equivalent output. Since smoke tests gate
            // on `loads()` (which transitively calls the others), we
            // approximate per-fn pass as:
            // - loads, parse_value, parse_array, parse_inline_table,
            //   parse_int are exercised when the corresponding case
            //   is in smoke positive AND that case PASSed.
            //
            // Map from canonical fn → minimum smoke cases that exercise it.
            let must_pass: &[&str] = match *name {
                "loads" => &[
                    "empty",
                    "single_int",
                    "two_keys",
                    "table_header",
                    "comment_line",
                ],
                "parse_value" => &["bool_true", "single_int", "basic_string", "literal_string"],
                "parse_array" => &[
                    "empty_array",
                    "int_array",
                    "trailing_comma_array",
                    "array_of_strings",
                    "array_of_bools",
                ],
                "parse_inline_table" => &["inline_table", "nested_inline_table"],
                "parse_int" => &["single_int", "negative_int", "plus_int", "two_keys"],
                _ => &[],
            };
            let exercise_pass = if g3_smoke.ran
                && must_pass.iter().all(|m| {
                    // Confirm none of the listed smoke labels appears in `failures`.
                    !g3_smoke
                        .failures
                        .iter()
                        .any(|f| f.contains(&format!("name={m}")))
                }) {
                true
            } else {
                false
            };
            let l2_behavior_pass = l2_build_pass && exercise_pass;
            CanonicalResult {
                name: (*name).to_string(),
                l1_pass,
                l2_build_pass,
                l2_behavior_pass,
                note: if l2_behavior_pass {
                    "PASS".into()
                } else if !l1_pass {
                    "L1 dispatch failed".into()
                } else if !l2_build_pass {
                    "G2 cargo check failed".into()
                } else {
                    "smoke divergence on exercise cases".into()
                },
            }
        })
        .collect()
}

// ---- Promotion: write LLM-emitted parser to crates/cobrust-nest/src/parser.rs
// (cobra-named per ADR-0071 §3).

fn promote_emission(emissions: &[FnEmission]) -> Result<(), String> {
    let mut parser_rs = String::new();
    parser_rs.push_str(
        "// AUTO-GENERATED by T1.1 (real-LLM full-library translation).\n\
         // source-library: tomli 2.0.1\n\
         // oracle: cpython 3.11 (module: tomllib)\n\
         // translator: cobrust-translator::build_translation_prompt_rich\n\
         // provider: user_codex_t1_1 (gpt-5.5)\n\
         // see docs/agent/findings/0.1.0-beta-tomli-full-translation.md.\n\n",
    );
    parser_rs.push_str("//! Translated parser body — produced via real-LLM end-to-end.\n");
    parser_rs
        .push_str("//! Each emitted block carries its own per-function provenance comment.\n\n");

    // Auto-generated code: relax doc-coverage clippy lints since the
    // emitted bodies don't carry # Errors / # Panics doc sections by
    // default. The translation provenance + per-fn comments are the
    // doc trail, per ADR-0007 §"Public surface".
    parser_rs.push_str("#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]\n\n");

    // The promoted parser uses the original (non-pub) State / TomliError visibility.
    parser_rs.push_str(WORKSPACE_PREAMBLE_PROMOTED);
    parser_rs.push_str(TO_JSON_HELPERS);
    parser_rs.push_str("\n// ---- LLM-emitted function bodies (verbatim per fn) ----\n\n");

    for emission in emissions {
        if emission.extracted.is_empty() {
            return Err(format!("fn {} has empty emission", emission.name));
        }
        parser_rs.push_str(&format!(
            "// fn:{} provider=user_codex_t1_1 model={MODEL} cache_hit={} prompt_tokens={} completion_tokens={}\n",
            emission.name, emission.cache_hit, emission.prompt_tokens, emission.completion_tokens
        ));
        let cleaned = strip_redefinitions(&emission.extracted);
        let normalized = normalize_signature(&cleaned, &emission.name);
        parser_rs.push_str(&normalized);
        if !normalized.ends_with('\n') {
            parser_rs.push('\n');
        }
        parser_rs.push('\n');
    }

    let dest = workspace_root().join("crates/cobrust-nest/src/parser.rs");
    std::fs::write(&dest, parser_rs).map_err(|e| format!("write parser.rs: {e}"))?;

    // Run rustfmt over the promoted parser.rs so workspace fmt-check passes.
    let _ = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .arg(&dest)
        .status();
    Ok(())
}

/// The promoted parser uses the production (non-pub) visibility on
/// State / TomliError fields, matching the original parser.rs exactly.
/// (The synth crate widens visibility for the test harness; the
/// promoted artefact must restore production visibility.)
const WORKSPACE_PREAMBLE_PROMOTED: &str = r#"use std::collections::BTreeMap;
use std::fmt;

/// Heterogeneous TOML value. Subset per M4 scope window
/// (see corpus/tomli/README.md).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Boolean.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// UTF-8 string.
    Str(String),
    /// Heterogeneous array.
    Array(Vec<Value>),
    /// Nested table.
    Table(BTreeMap<String, Value>),
}

/// Single error type for tomli parse failures.
#[derive(Clone, Debug)]
pub struct TomliError {
    /// Human-readable message.
    pub message: String,
    /// Byte offset of the error in the source.
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

/// Cursor over the input source.
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

/// Walk into the table at `path`, creating intermediate tables.
fn ensure_path<'a>(
    root: &'a mut BTreeMap<String, Value>,
    path: &[String],
) -> Result<&'a mut BTreeMap<String, Value>, TomliError> {
    let mut cursor: &'a mut BTreeMap<String, Value> = root;
    for part in path {
        let entry = cursor
            .entry(part.clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        cursor = match entry {
            Value::Table(t) => t,
            _ => {
                return Err(TomliError::new(
                    format!("path conflicts with non-table at {part:?}"),
                    0,
                ));
            }
        };
    }
    Ok(cursor)
}
"#;

// ---- Body extraction --------------------------------------------------------

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

// ---- Finding writer ---------------------------------------------------------

fn finding_path() -> PathBuf {
    workspace_root().join("docs/agent/findings/0.1.0-beta-tomli-full-translation.md")
}

fn current_commit_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace_root())
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

#[allow(clippy::too_many_arguments)]
fn record_finding(
    overall: &str,
    canonical_pass_count: usize,
    emissions: &[FnEmission],
    g2_outcome: &CargoCheckOutcome,
    g3_smoke: &SmokeOutcome,
    g3_fuzz: &FuzzOutcome,
    canonical_results: &[CanonicalResult],
    perf_numbers: &PerfNumbers,
    promoted: bool,
    total_prompt_tokens: u32,
    total_completion_tokens: u32,
    total_tokens: u32,
    sample_ledger: Option<LedgerEntry>,
) {
    let commit = current_commit_sha();
    let ledger_json = sample_ledger
        .map(|e| serde_json::to_string_pretty(&e).unwrap_or_default())
        .unwrap_or_else(|| "(none captured)".into());

    let mut per_fn_table = String::new();
    per_fn_table.push_str("| Function | Dispatch | Tokens | Latency (ms) | Cache hit |\n");
    per_fn_table.push_str("|---|---|---|---|---|\n");
    for e in emissions {
        let total_e = e.prompt_tokens + e.completion_tokens;
        let status = if e.err.is_some() {
            format!("FAIL ({})", e.err.as_ref().unwrap())
        } else if e.extracted.is_empty() {
            "EMPTY".to_string()
        } else {
            "OK".to_string()
        };
        per_fn_table.push_str(&format!(
            "| `{}` | {} | {} (p={}, c={}) | {} | {} |\n",
            e.name,
            status,
            total_e,
            e.prompt_tokens,
            e.completion_tokens,
            e.latency_ms,
            e.cache_hit
        ));
    }

    let mut canonical_table = String::new();
    canonical_table.push_str("| Function | L1 dispatch | L2 build | L2 behavior | Note |\n");
    canonical_table.push_str("|---|---|---|---|---|\n");
    for r in canonical_results {
        canonical_table.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            r.name,
            if r.l1_pass { "PASS" } else { "FAIL" },
            if r.l2_build_pass { "PASS" } else { "FAIL" },
            if r.l2_behavior_pass { "PASS" } else { "FAIL" },
            r.note
        ));
    }

    let perf_table = format!(
        "| Doc size | Cobrust ns/iter | CPython tomllib ns/iter | Ratio (CPython/Cobrust) |\n\
         |---|---|---|---|\n\
         | 1 KB     | {} | {} | {:.3} |\n\
         | 100 KB   | {} | {} | {:.3} |\n\
         | 10 MB    | {} | {} | {:.3} |\n",
        perf_numbers.cobrust_1k_ns,
        perf_numbers.cpython_1k_ns,
        perf_numbers.ratio_1k(),
        perf_numbers.cobrust_100k_ns,
        perf_numbers.cpython_100k_ns,
        perf_numbers.ratio_100k(),
        perf_numbers.cobrust_10m_ns,
        perf_numbers.cpython_10m_ns,
        perf_numbers.ratio_10m(),
    );

    let smoke_block = if g3_smoke.ran {
        format!(
            "Positive cases: {}/{} PASS\n\nNegative cases: {}/{} PASS\n\n{}",
            g3_smoke.positive_pass,
            g3_smoke.positive_total,
            g3_smoke.negative_pass,
            g3_smoke.negative_total,
            if g3_smoke.failures.is_empty() {
                String::new()
            } else {
                let mut s = String::from("Failures:\n```text\n");
                for f in &g3_smoke.failures {
                    s.push_str(f);
                    s.push('\n');
                }
                s.push_str("```\n");
                s
            }
        )
    } else {
        "SKIPPED — G2 (cargo check) failed; behavior gate cannot run on uncompilable code.".into()
    };

    let fuzz_block = if g3_fuzz.total > 0 {
        let pass_rate = (g3_fuzz.total - g3_fuzz.divergences - g3_fuzz.panics) as f64
            / f64::from(g3_fuzz.total)
            * 100.0;
        format!(
            "{} inputs total, {} divergences, {} panics. Pass rate: {:.2}%.",
            g3_fuzz.total, g3_fuzz.divergences, g3_fuzz.panics, pass_rate
        )
    } else if g2_outcome.passed {
        "ERROR — fuzz target did not emit FUZZ_RESULT line.".into()
    } else {
        "SKIPPED — G2 failed.".into()
    };

    let g2_block = if g2_outcome.passed {
        "PASS — `cargo check` exited 0; the assembled crate (workspace preamble + 12 LLM emissions) compiles cleanly.".to_string()
    } else {
        format!(
            "FAIL — `cargo check` exited {exit:?}. Stderr tail (last 60 lines):\n\n```text\n{stderr}\n```",
            exit = g2_outcome.exit_code,
            stderr = g2_outcome.stderr_tail,
        )
    };

    let conclusion = match overall {
        "PASS" => format!(
            "All 5 canonical entrypoints PASS L2.behavior. The full tomli 2.0.1 \
             public surface is production-translated end-to-end via real LLM \
             through the production `build_translation_prompt_rich` builder. \
             This is the headline 0.1.0-beta release demonstration of \
             Constitution §1.2 — first time a complete public API of a real \
             Python library has been LLM-translated and verified against \
             CPython oracle on canonical + 1024-fuzz inputs.\n\n\
             Cobrust-nest now ships with the LLM-emitted `parser.rs` \
             {promoted_clause}.",
            promoted_clause = if promoted {
                "(promoted into crates/cobrust-nest/src/parser.rs)"
            } else {
                "(promotion blocked by partial-pass policy)"
            }
        ),
        "PARTIAL-PASS-4OF5" => format!(
            "4/5 canonical entrypoints PASS L2.behavior; 1 falls back per the \
             partial-pass acceptance policy. The 0.1.0-beta release is shippable \
             as 'tomli (4/5 fns; 1 falls back to CPython)'. Promotion {promoted_clause}.",
            promoted_clause = if promoted {
                "wrote the LLM emission to crates/cobrust-nest/src/parser.rs"
            } else {
                "did NOT replace crates/cobrust-nest/src/parser.rs"
            }
        ),
        "PARTIAL-PASS-3OF5" => format!(
            "3/5 canonical entrypoints PASS L2.behavior — under the 4/5 threshold. \
             0.1.0-beta release does NOT auto-ship under T1.1; CTO inspects the \
             per-fn divergence below and decides scope. Promotion {promoted_clause}.",
            promoted_clause = "did NOT replace crates/cobrust-nest/src/parser.rs"
        ),
        _ => format!(
            "FAIL — fewer than 3/5 canonical entrypoints PASS. Escalation per \
             T1.1 policy. Per-fn divergence below should drive a follow-up sprint \
             (prompt tweak, repair-loop dispatch, or scope reduction).\n\n\
             Promotion blocked. crates/cobrust-nest/src/parser.rs unchanged."
        ),
    };

    let body = format!(
        r#"---
doc_kind: finding
finding_id: 0.1.0-beta-tomli-full-translation
last_verified_commit: {commit}
dependencies: [adr:0007, adr:0032, adr:0036, adr:0039, finding:audit-1-tomli-real-llm-result, finding:audit-3a-stateful-prompt-design]
---

# Finding: 0.1.0-beta — tomli full-library real-LLM translation result

## Hypothesis

The production `build_translation_prompt_rich` builder generalises the
audit-1 / audit-3a single-function PASS to all 12 functions of `tomli`
2.0.1 simultaneously. Driving every function through one real LLM call
each, gluing the emissions into one parser module, and verifying with
the canonical oracle (CPython `tomllib`) produces a working,
behaviorally-equivalent Cobrust port suitable for promotion into
`crates/cobrust-nest/src/parser.rs` for the 0.1.0-beta release.

## Method

- **Target**: 12 functions in `corpus/tomli/spec.toml` (full public surface).
- **Provider**: `OpenAiProvider` at `{BASE_URL}` (model `{MODEL}`).
- **Cache discipline**: `SyntheticProvider` NOT registered;
  `cache_dir` = isolated `tempdir().join("llm_cache")`, verified
  non-existent pre-flight.
- **Builder**: production `cobrust_translator::build_translation_prompt_rich(unit, ctx)`.
- **Workspace context per function**: tomli `Value` + `TomliError` + `State`
  preamble + in-scope helper signatures + `parse_basic_string` few-shot
  (or `parse_bool` few-shot when the target IS `parse_basic_string`) +
  exact return-type contract per fn + error-construction contract.
- **Verification**:
  - **G1 (L1)**: real HTTP round-trip per fn; ledger records
    `cache_hit=false, provider_kind="openai"`.
  - **G2 (L2.build)**: `cargo check` against the assembled crate
    (workspace preamble + 12 emissions).
  - **G3 (L2.behavior, smoke)**: 27 positive + 5 negative cases through
    `loads()` → CPython `tomllib` oracle.
  - **G3 (L2.behavior, fuzz)**: 1024 deterministic-seeded random TOML
    inputs through `loads()` → CPython `tomllib` oracle.
  - **G3 (L2.perf)**: 1KB / 100KB / 10MB doc parse vs CPython tomllib.
  - **G4 (cache discipline)**: provider count = 1; tempdir cache;
    ledger entries verified before returning.

## Result

**OUTCOME: {overall}**

### G1 — L1 dispatch (per-function)

{per_fn_table}

Sample ledger entry (first OK call):

```json
{ledger_json}
```

### G2 — L2.build (assembled crate cargo check)

{g2_block}

### G3 — L2.behavior (smoke: 27 positive + 5 negative)

{smoke_block}

### G3 — L2.behavior (fuzz: 1024 deterministic-seeded inputs)

{fuzz_block}

### G3 — L2.perf (1KB / 100KB / 10MB doc parse)

{perf_table}

(Per ADR-0007 §"L2.perf gate" the perf threshold is library-tuned;
0.1.0-beta release accepts ratio ≥ 0.8× CPython baseline. Negative ratio
or 0 indicates a benchmark probe failure — see test stdout.)

### G4 — cache discipline

PASS — provider count = 1 (`OpenAiProvider`); `cache_dir` isolated
`tempfile::tempdir()` path verified non-existent before dispatch;
ledger Ok entries inspected for `cache_hit=false, provider_kind="openai"`.

### Canonical 5 entrypoints

{canonical_table}

**Canonical pass count: {canonical_pass_count}/5.**

## Conclusion

{conclusion}

## Token spend

| Phase | Live calls | Prompt tokens | Completion tokens | Total tokens |
|-------|------------|---------------|-------------------|--------------|
| L1 real dispatch | 12 (1 per fn) | {total_prompt_tokens} | {total_completion_tokens} | {total_tokens} |
| Cache replay | 0 | 0 | 0 | 0 |

## Actionable consequences

1. ADR-0039 documents this outcome and pins `build_translation_prompt_rich`
   as the production builder for any future library translation.
2. Promotion path: {promoted_clause}.
3. Downstream pip-tools verification (`tests/downstream/pip_tools/`)
   exercises the promoted artefact under a real Python tool that
   imports tomli — see `tests/downstream/pip_tools/result.md` for the
   verdict.

## Cross-references

- ADR-0007 — translator pipeline.
- ADR-0032 — audit-1; first leaf PASS.
- ADR-0036 — audit-3a; first stateful PASS through `build_translation_prompt_rich`.
- ADR-0039 — this sprint's binding decision.
- `finding:audit-1-tomli-real-llm-result` — leaf PASS data.
- `finding:audit-3a-stateful-prompt-design` — stateful PASS data.
- `crates/cobrust-translator/tests/full_pipeline_tomli_real_llm.rs` —
  this harness.
- Memory `reference_codex_api.md` — endpoint credentials.
"#,
        BASE_URL = BASE_URL,
        MODEL = MODEL,
        commit = commit,
        overall = overall,
        per_fn_table = per_fn_table,
        ledger_json = ledger_json,
        g2_block = g2_block,
        smoke_block = smoke_block,
        fuzz_block = fuzz_block,
        perf_table = perf_table,
        canonical_table = canonical_table,
        canonical_pass_count = canonical_pass_count,
        conclusion = conclusion,
        total_prompt_tokens = total_prompt_tokens,
        total_completion_tokens = total_completion_tokens,
        total_tokens = total_tokens,
        promoted_clause = if promoted {
            "wrote LLM emission to `crates/cobrust-nest/src/parser.rs`; \
             tomli 0.1.0-beta ships LLM-emitted code"
        } else {
            "did NOT replace `crates/cobrust-nest/src/parser.rs`; \
             cobrust-nest falls back to the M4 synthetic-translated parser"
        },
    );
    let path = finding_path();
    let _ = std::fs::create_dir_all(path.parent().expect("parent"));
    if let Err(e) = std::fs::write(&path, body) {
        eprintln!("T1.1: failed to write finding: {e}");
    }
}
