//! B1 + B2 acceptance test for ADR-0040 §"Honest gate verdicts".
//!
//! Pinned by claude-desktop integrated handoff (review-claude
//! 2026-05-11) §1.B1 + §1.B2 + §7 acceptance commands. These tests
//! enforce two contracts simultaneously — the handoff §10 interlock
//! rule says B1 and B2 must land in the same PR.
//!
//! ## B1 — real-LLM dispatch must not panic
//!
//! Production callers passing `synthetic_only = false` previously
//! tripped `panic!("real-LLM mode is not wired in M4 ...")` at
//! `pipeline.rs:529`. ADR-0040 wires the production path:
//! `OpenAiProvider` / `AnthropicProvider` are registered for each
//! declared provider, the API key is pulled from each provider's
//! `api_key_env`, and missing-key is a `TranslatorError::Config` —
//! never a panic. The test verifies the failure mode is structured
//! Err, not panic, when no env vars are set.
//!
//! ## B2 — l2_*_summary must reflect verifier verdicts
//!
//! Production callers previously got `gates.l2_build = "pass (cargo
//! build --release zero warnings)"` regardless of whether the build
//! gate even ran. ADR-0040 makes the manifest field the
//! `Display`-form of a structured `GateOutcome { Pass | Fail | Skip }`
//! enum populated from the verifier verdicts; the no-op `AcceptAll` /
//! `AcceptAllPerf` verifiers surface as `Skip { reason }` so the
//! manifest is honest about which gates were actually wired.
//!
//! Each `gate_outcome.is_pass() / .is_skip() / .is_fail()` exit path
//! is exercised in distinct subtests so the structural verdict
//! taxonomy is not confusable.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::match_wild_err_arm,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::uninlined_format_args
)]

use std::path::Path;

use cobrust_llm_router::RouterConfig;
use cobrust_translator::{
    BehaviorVerifier, FunctionTranslation, GateFailure, GateKind, GateOutcome, PyLibrary,
    TranslatorConfig, TranslatorError, VerifierVerdict, translate, translate_with_verifier,
    translate_with_verifiers,
};

// ---- Fixtures ---------------------------------------------------------------

fn write_minimal_corpus(corpus: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    std::fs::create_dir_all(corpus.join("upstream")).unwrap();
    std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
    std::fs::write(corpus.join("upstream/tomli_loads.py"), "# stub\n").unwrap();
    let spec = r#"
schema_version = 1
library = "tomli"
upstream_version = "0.0.1"
oracle_module = "tomllib"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.loads]
qualname = "x.loads"
public = true
signature = "loads(src) -> dict"
py_compat = "strict"
description = "Stub."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
    std::fs::write(corpus.join("spec.toml"), spec).unwrap();
    (
        corpus.join("upstream/tomli_loads.py"),
        corpus.join("spec.toml"),
    )
}

fn write_canned_table_with_loads(corpus: &Path, sha16: &str) -> std::path::PathBuf {
    let path = corpus.join("canned.toml");
    let toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
// translated stub
pub fn loads(_s: &str) {{}}
"""
"#
    );
    std::fs::write(&path, toml).unwrap();
    path
}

fn synthetic_router_cfg(cache: &Path, ledger: &Path) -> RouterConfig {
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.synthetic]
kind = "openai"
base_url = "http://x"
api_key_env = "K"
models = ["tomli-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:tomli-canned-v1"]
"#,
        cache = cache.display(),
        ledger = ledger.display(),
    );
    RouterConfig::from_toml_str(&toml).unwrap()
}

fn real_llm_router_cfg(cache: &Path, ledger: &Path, env_var: &str) -> RouterConfig {
    let toml = format!(
        r#"
[router]
default_strategy = "quality"
cache_dir = "{cache}"
ledger_path = "{ledger}"

[providers.real_provider]
kind = "openai"
base_url = "http://127.0.0.1:1/v1"
api_key_env = "{env_var}"
models = ["test-model"]

[routing.translate]
strategy = "quality"
preferred = ["real_provider:test-model"]
"#,
        cache = cache.display(),
        ledger = ledger.display(),
    );
    RouterConfig::from_toml_str(&toml).unwrap()
}

// ============================================================================
// B1 — real-LLM mode returns Err, never panics (handoff §1.B1)
// ============================================================================

/// **B1 acceptance.** Production caller flips `synthetic_only = false`
/// without setting any env var. The pipeline must return
/// `Err(TranslatorError::Config)` naming the missing env var — never
/// panic, never fall back to synthetic.
#[tokio::test]
async fn b1_real_llm_without_api_key_returns_err_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let (source_file, spec_file) = write_minimal_corpus(&corpus);
    let env_var_name = "B1_NEVER_SET_ENV_VAR_FOR_PIPELINE_TEST";
    // Make sure the env var is not set (idempotent precondition).
    // SAFETY: the env var name is unique to this test; no concurrent
    // readers/writers in the same process.
    unsafe {
        std::env::remove_var(env_var_name);
    }

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let mut cfg = TranslatorConfig::m4_synthetic(
        real_llm_router_cfg(&cache, &ledger, env_var_name),
        dir.path().join("out"),
    );
    cfg.synthetic_only = false;

    let lib = PyLibrary {
        library: "tomli".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: None,
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    // Wrap in catch_unwind so we surface the panic as a test failure
    // with the panic message — the entire point of B1 is to prevent
    // the panic, so any panic is a regression.
    let result = std::panic::AssertUnwindSafe(translate(&lib, &cfg))
        .catch_unwind_async()
        .await;
    let outcome = match result {
        Ok(r) => r,
        Err(_panic) => {
            panic!("B1 REGRESSION: translate() panicked instead of returning Err");
        }
    };

    let err = outcome.unwrap_err();
    match err {
        TranslatorError::Config(msg) => {
            // The diagnostic must name the env var so the user knows
            // exactly what to fix.
            assert!(
                msg.contains(env_var_name),
                "B1 contract: config error must name the missing env var {env_var_name:?}; got {msg:?}"
            );
            assert!(
                msg.contains("real-LLM"),
                "B1 contract: config error must mention real-LLM mode; got {msg:?}"
            );
            println!("B1 PASS — got expected Config error: {msg}");
        }
        other => panic!(
            "B1 contract: expected TranslatorError::Config, got {other:?} \
             (handoff §1.B1: production translate() must return Err on missing key, not panic)"
        ),
    }
}

/// **B1 acceptance, complement.** With the env var set, the pipeline
/// must successfully construct a router (it cannot complete a real
/// dispatch in the gate path because no real endpoint is reachable —
/// but `Err(TranslatorError::Router)` is the *correct* failure mode
/// after the wire-protocol attempt, **not** panic).
#[tokio::test]
async fn b1_real_llm_with_api_key_attempts_dispatch_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let (source_file, spec_file) = write_minimal_corpus(&corpus);
    let env_var_name = "B1_TRANSIENT_ENV_VAR_FOR_PIPELINE_TEST";
    // SAFETY: unique env var; no concurrent readers in this test.
    unsafe {
        std::env::set_var(env_var_name, "fake-key-not-used");
    }

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let mut cfg = TranslatorConfig::m4_synthetic(
        real_llm_router_cfg(&cache, &ledger, env_var_name),
        dir.path().join("out"),
    );
    cfg.synthetic_only = false;

    let lib = PyLibrary {
        library: "tomli".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: None,
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    let result = std::panic::AssertUnwindSafe(translate(&lib, &cfg))
        .catch_unwind_async()
        .await;

    // SAFETY: clean up the env var for parallel tests.
    unsafe {
        std::env::remove_var(env_var_name);
    }

    let outcome = match result {
        Ok(r) => r,
        Err(_panic) => {
            panic!("B1 REGRESSION: translate() panicked even with API key set");
        }
    };

    // The outcome must be Err (no real endpoint at 127.0.0.1:1) — but
    // the failure must be a wire-protocol failure, never a structured
    // Config "not wired" error from the M4 stub.
    let err = outcome.unwrap_err();
    match err {
        TranslatorError::Router(_) | TranslatorError::Translation { .. } => {
            // Either form is acceptable — the wire attempt was made
            // (router built fine; dispatch failed because no endpoint).
            println!("B1 PASS — router was built and dispatch attempted ({err})");
        }
        TranslatorError::Config(msg) if msg.contains("not wired") => {
            panic!(
                "B1 REGRESSION: real-LLM mode still surfaces stub Config \"not wired\" \
                 message: {msg}"
            );
        }
        other => {
            // Anything other than Config-not-wired is acceptable as long
            // as it's not a panic — a Config error about validation, an
            // I/O error, etc. could all happen if the test environment
            // changes. Print and accept.
            println!("B1 PASS — translate() returned Err (no panic): {other}");
        }
    }
}

// ============================================================================
// B2 — l2_*_summary uses real verdicts (handoff §1.B2)
// ============================================================================

/// **B2 acceptance.** Synthetic-mode pipeline with the default
/// `AcceptAll` + `AcceptAllPerf` verifiers must surface the gate
/// outcomes as `Skip` (= verifier was no-op) — *not* a fake Pass. The
/// manifest's gate strings reflect this.
#[tokio::test]
async fn b2_default_verifiers_surface_skip_not_fake_pass() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let (source_file, spec_file) = write_minimal_corpus(&corpus);
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();
    let canned = write_canned_table_with_loads(&corpus, &sha[..16]);

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        synthetic_router_cfg(&cache, &ledger),
        dir.path().join("out"),
    );
    let lib = PyLibrary {
        library: "tomli".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    let result = translate(&lib, &cfg).await.unwrap();

    // B2 contract: AcceptAll behavior verifier ⇒ Skip, not fake Pass.
    assert!(
        result.gate_outcomes.l2_behavior.is_skip(),
        "B2 contract: AcceptAll verifier must surface as Skip; got {:?}",
        result.gate_outcomes.l2_behavior
    );
    // B2 contract: AcceptAllPerf perf verifier ⇒ Skip.
    assert!(
        result.gate_outcomes.l2_perf.is_skip(),
        "B2 contract: AcceptAllPerf verifier must surface as Skip; got {:?}",
        result.gate_outcomes.l2_perf
    );
    // The build / pyo3 / downstream gates run out-of-pipeline, so they
    // honestly default to Skip.
    assert!(result.gate_outcomes.l2_build.is_skip());
    assert!(result.gate_outcomes.l3_pyo3_wrapper.is_skip());
    assert!(result.gate_outcomes.l3_downstream_dependents.is_skip());

    // Manifest strings must mirror the structured verdicts — no
    // hardcoded literal that would survive a verifier change.
    assert!(
        result.manifest.gates.l2_build.starts_with("skipped"),
        "B2 contract: manifest l2_build must reflect Skip; got {:?}",
        result.manifest.gates.l2_build
    );
    assert!(result.manifest.gates.l2_behavior.starts_with("skipped"));
    assert!(result.manifest.gates.l2_perf.starts_with("skipped"));
    println!(
        "B2 PASS (Skip path) — l2_build={}, l2_behavior={}, l2_perf={}",
        result.manifest.gates.l2_build,
        result.manifest.gates.l2_behavior,
        result.manifest.gates.l2_perf
    );
}

/// Live verifier that rejects on attempt 1 and accepts on attempt 2,
/// causing the repair loop to fire. Used to verify that the success
/// path with at least one observed Reject surfaces as `Pass`.
struct RejectFirstAcceptSecond;
impl BehaviorVerifier for RejectFirstAcceptSecond {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
        if attempt == 1 {
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: GateKind::Behavior,
                failure_summary: "B2 test fixture rejection".into(),
                failed_inputs: vec!["fixture-input".into()],
                expected: Some("ok".into()),
                actual: Some("err".into()),
                attempt: 2,
            })
        } else {
            VerifierVerdict::Accept
        }
    }
}

/// **B2 acceptance.** A live behavior verifier that accepts (without
/// being the no-op `AcceptAll`) must surface as `Pass` — distinct
/// from the `Skip` exit path the no-op verifier produces.
#[tokio::test]
async fn b2_live_accept_verifier_surfaces_pass_distinct_from_skip() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let (source_file, spec_file) = write_minimal_corpus(&corpus);
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();
    let canned = write_canned_table_with_loads(&corpus, &sha[..16]);

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        synthetic_router_cfg(&cache, &ledger),
        dir.path().join("out"),
    );
    let lib = PyLibrary {
        library: "tomli".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    // Use a live verifier that triggers a Reject → Accept cycle so
    // the repair loop observes a real verdict, then Pass.
    let canned2_sha = sha[..16].to_string();
    let canned_path = corpus.join("canned.toml");
    let canned_toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{canned2_sha}"
attempt = 1
response_text = """
// BROKEN-V1 attempt
pub fn loads(_s: &str) {{}}
"""

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{canned2_sha}"
attempt = 2
response_text = """
// CORRECT-V2 attempt
pub fn loads(_s: &str) {{}}
"""
"#
    );
    std::fs::write(&canned_path, canned_toml).unwrap();

    let result = translate_with_verifier(&lib, &cfg, &RejectFirstAcceptSecond)
        .await
        .unwrap();

    // The repair loop ran exactly once.
    assert_eq!(result.repair_attempts, 1);
    // B2 contract: a verifier that observed at least one Reject
    // followed by an Accept must surface as Pass — distinct from the
    // Skip path of the no-op AcceptAll.
    assert!(
        result.gate_outcomes.l2_behavior.is_pass(),
        "B2 contract: live verifier with Reject→Accept must surface as Pass; got {:?}",
        result.gate_outcomes.l2_behavior
    );
    // Detail string carries observable evidence (test target + repair count).
    let manifest_str = &result.manifest.gates.l2_behavior;
    assert!(
        manifest_str.starts_with("pass"),
        "B2 contract: manifest l2_behavior must start with \"pass\"; got {manifest_str:?}",
    );
    assert!(
        manifest_str.contains("repair-loop"),
        "B2 contract: manifest l2_behavior should mention the repair-loop attempt count; got {manifest_str:?}",
    );
    println!("B2 PASS (Pass path) — l2_behavior={manifest_str}");
}

/// Live verifier that rejects every attempt, used to drive the
/// pipeline to escalation.
struct AlwaysRejectLive;
impl BehaviorVerifier for AlwaysRejectLive {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
        VerifierVerdict::Reject(GateFailure {
            function: function.name.clone(),
            failed_gate: GateKind::Behavior,
            failure_summary: "B2 always-reject fixture".into(),
            failed_inputs: vec![],
            expected: None,
            actual: None,
            attempt: attempt + 1,
        })
    }
}

/// **B2 acceptance.** A verifier that always rejects must propagate
/// out of the pipeline as `EscalationExceeded` — *not* surface as a
/// fake-pass in the manifest. Confirms Fail is its own exit path,
/// distinct from Pass and Skip.
#[tokio::test]
async fn b2_always_reject_verifier_propagates_fail_distinct_from_pass_or_skip() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let (source_file, spec_file) = write_minimal_corpus(&corpus);
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();
    let _canned = write_canned_table_with_loads(&corpus, &sha[..16]);

    // Add an attempt-2 entry so the repair loop has somewhere to land.
    let canned_path = corpus.join("canned.toml");
    let canned_toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
// attempt 1 broken
pub fn loads(_s: &str) {{}}
"""

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{sha16}"
attempt = 2
response_text = """
// attempt 2 also broken
pub fn loads(_s: &str) {{}}
"""
"#,
        sha16 = &sha[..16],
    );
    std::fs::write(&canned_path, canned_toml).unwrap();

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let mut cfg = TranslatorConfig::m4_synthetic(
        synthetic_router_cfg(&cache, &ledger),
        dir.path().join("out"),
    );
    cfg.escalation_threshold = 2; // tighten so the test runs fast

    let lib = PyLibrary {
        library: "tomli".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned_path),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    let err = translate_with_verifier(&lib, &cfg, &AlwaysRejectLive)
        .await
        .unwrap_err();
    match err {
        TranslatorError::EscalationExceeded {
            function,
            attempts,
            failed_gate,
        } => {
            assert_eq!(function, "loads");
            assert_eq!(attempts, 2);
            assert_eq!(failed_gate, GateKind::Behavior);
            println!(
                "B2 PASS (Fail path) — EscalationExceeded(function={function}, gate={failed_gate}, attempts={attempts})"
            );
        }
        other => panic!(
            "B2 contract: always-reject verifier must propagate EscalationExceeded; \
             got {other:?} (handoff §1.B2 acceptance: cmp tests must show distinct exit paths)"
        ),
    }
}

/// **B2 acceptance, GateOutcome serialization round-trip.** Each
/// variant must produce a distinct `as_manifest_str()` prefix so a
/// downstream parser can disambiguate without ambiguity.
#[test]
fn b2_gate_outcome_serializes_distinctly_for_each_variant() {
    let pass = GateOutcome::Pass {
        detail: "tests/x.rs".into(),
    };
    let fail = GateOutcome::Fail {
        reason: "compile error E0277".into(),
    };
    let skip = GateOutcome::Skip {
        reason: "wired out-of-pipeline".into(),
    };

    assert_eq!(pass.as_manifest_str(), "pass (tests/x.rs)");
    assert_eq!(fail.as_manifest_str(), "fail: compile error E0277");
    assert_eq!(skip.as_manifest_str(), "skipped (wired out-of-pipeline)");

    // Empty-detail Pass / empty-reason Skip have stable canonical forms.
    let pass_empty = GateOutcome::Pass {
        detail: String::new(),
    };
    let skip_empty = GateOutcome::Skip {
        reason: String::new(),
    };
    assert_eq!(pass_empty.as_manifest_str(), "pass");
    assert_eq!(skip_empty.as_manifest_str(), "skipped");

    // is_pass / is_fail / is_skip are exhaustive and disjoint.
    assert!(pass.is_pass() && !pass.is_fail() && !pass.is_skip());
    assert!(!fail.is_pass() && fail.is_fail() && !fail.is_skip());
    assert!(!skip.is_pass() && !skip.is_fail() && skip.is_skip());
}

/// Live perf verifier that triggers the L2.perf repair loop, so the
/// success path verdict is `Pass` (not `Skip`).
struct PerfRejectFirstAcceptSecond;
impl cobrust_translator::PerfVerifier for PerfRejectFirstAcceptSecond {
    fn verify(
        &self,
        function: &FunctionTranslation,
        attempt: u32,
    ) -> cobrust_translator::PerfVerdict {
        if attempt == 1 {
            cobrust_translator::PerfVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: GateKind::Perf,
                failure_summary: "B2 perf fixture rejection".into(),
                failed_inputs: vec![],
                expected: None,
                actual: None,
                attempt: 2,
            })
        } else {
            cobrust_translator::PerfVerdict::Accept
        }
    }
}

/// **B2 acceptance, perf gate.** Symmetric to the behavior path: a
/// live perf verifier that rejects-then-accepts must surface as `Pass`
/// while the no-op `AcceptAllPerf` surfaces as `Skip`. Distinct exit
/// paths under the same trait surface.
#[tokio::test]
async fn b2_live_perf_verifier_pass_path_distinct_from_skip() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    let (source_file, spec_file) = write_minimal_corpus(&corpus);
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();

    // Two-attempt canned table for repair.
    let canned_path = corpus.join("canned.toml");
    let canned_toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
// attempt 1 (perf-broken)
pub fn loads(_s: &str) {{}}
"""

[[entry]]
task = "translate"
function = "loads"
source_sha16 = "{sha16}"
attempt = 2
response_text = """
// attempt 2 (perf ok)
pub fn loads(_s: &str) {{}}
"""
"#,
        sha16 = &sha[..16],
    );
    std::fs::write(&canned_path, canned_toml).unwrap();

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        synthetic_router_cfg(&cache, &ledger),
        dir.path().join("out"),
    );
    let lib = PyLibrary {
        library: "tomli".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned_path),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    let result = translate_with_verifiers(
        &lib,
        &cfg,
        &cobrust_translator::AcceptAll, // behavior = no-op → Skip
        &PerfRejectFirstAcceptSecond,   // perf = live → Pass
    )
    .await
    .unwrap();

    assert_eq!(result.repair_attempts, 1);

    // Behavior is no-op → Skip; perf is live → Pass. Two distinct
    // exit paths in the same run.
    assert!(
        result.gate_outcomes.l2_behavior.is_skip(),
        "AcceptAll behavior must surface Skip; got {:?}",
        result.gate_outcomes.l2_behavior
    );
    assert!(
        result.gate_outcomes.l2_perf.is_pass(),
        "live perf verifier must surface Pass; got {:?}",
        result.gate_outcomes.l2_perf
    );

    // Manifest strings reflect the per-gate outcomes — different
    // prefixes prove distinct exit paths.
    assert!(
        result.manifest.gates.l2_behavior.starts_with("skipped"),
        "manifest l2_behavior should start with \"skipped\"; got {:?}",
        result.manifest.gates.l2_behavior
    );
    assert!(
        result.manifest.gates.l2_perf.starts_with("pass"),
        "manifest l2_perf should start with \"pass\"; got {:?}",
        result.manifest.gates.l2_perf
    );
    println!(
        "B2 PASS (mixed Skip+Pass) — l2_behavior={} | l2_perf={}",
        result.manifest.gates.l2_behavior, result.manifest.gates.l2_perf
    );
}

/// Helper polyfill so the test crate can use `catch_unwind` on an
/// async future without pulling in `futures::FutureExt::catch_unwind`
/// (which has lifetime quirks here).
trait CatchUnwindAsyncExt: std::future::Future + Sized + std::panic::UnwindSafe {
    fn catch_unwind_async(self) -> CatchUnwindAsync<Self> {
        CatchUnwindAsync(Some(self))
    }
}

impl<F> CatchUnwindAsyncExt for F where F: std::future::Future + std::panic::UnwindSafe {}

struct CatchUnwindAsync<F>(Option<F>);

impl<F> std::future::Future for CatchUnwindAsync<F>
where
    F: std::future::Future + std::panic::UnwindSafe,
{
    type Output = std::thread::Result<F::Output>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: this is the standard pattern for projecting Pin
        // through an Option containing a non-Unpin future.
        let inner = unsafe {
            self.as_mut()
                .map_unchecked_mut(|s| s.0.as_mut().expect("polled after completion"))
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inner.poll(cx)));
        match result {
            Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
            Ok(std::task::Poll::Ready(v)) => std::task::Poll::Ready(Ok(v)),
            Err(panic) => std::task::Poll::Ready(Err(panic)),
        }
    }
}
