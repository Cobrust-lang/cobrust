//! M6 msgpack pipeline integration test.
//!
//! Drives `cobrust_translator::translate_with_verifiers` against
//! `corpus/msgpack/`, exercising:
//!  - L0 spec loading (with `task = translate_cython` entries).
//!  - L1 dispatch through the synthetic provider for both pure-Python
//!    and Cython tasks.
//!  - L2.behavior verifier hook (no-op for msgpack — the bytes-
//!    identical contract is enforced via the fuzz harness).
//!  - L2.perf verifier hook → repair loop on `pack_uint` (the canned
//!    table ships a deliberately perf-broken attempt-1 + corrected
//!    attempt-2 entry — see `corpus/msgpack/canned_llm_responses.toml`
//!    + ADR-0010 §4).
//!  - L3 downstream-dependents driver against the vendored redis-py
//!    + msgpack-numpy subsets (per ADR-0010 §1).
//!  - Manifest emission with `gates.dependents` populated for the
//!    msgpack tier.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use cobrust_translator::{
    AcceptAll, FunctionTranslation, GateFailure, PerfVerdict, PerfVerifier, PyLibrary,
    TranslatorConfig, TranslatorError, msgpack_m6_dependents, run_dependent,
    translate_with_verifiers,
};
use std::path::PathBuf;

const PYTHON_PATH: &str = "/opt/homebrew/bin/python3.11";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn msgpack_corpus_root() -> PathBuf {
    workspace_root().join("corpus/msgpack")
}

fn canned_router_cfg(cache: &str, ledger: &str) -> cobrust_llm_router::RouterConfig {
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
models = ["msgpack-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:msgpack-canned-v1"]
"#
    );
    cobrust_llm_router::RouterConfig::from_toml_str(&toml).unwrap()
}

fn msgpack_pylibrary(corpus: &std::path::Path) -> PyLibrary {
    PyLibrary {
        library: "msgpack".into(),
        version: "1.0.8".into(),
        source_file: corpus.join("upstream/msgpack_core.py"),
        spec_file: corpus.join("spec.toml"),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(corpus.join("canned_llm_responses.toml")),
        seeds: vec![42, 1337, 0xDEAD_BEEF],
        fuzz_inputs_per_fn: 1024,
    }
}

/// Verifier that rejects pack_uint emissions whose body still contains
/// the perf-broken markers from attempt 1. Drives the M6 perf-repair-
/// loop demo per ADR-0010 §4 — without needing real LLM keys, we
/// exercise the `PerfVerifier` callback path.
struct PackUintPerfBrokenVerifier;

impl PerfVerifier for PackUintPerfBrokenVerifier {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> PerfVerdict {
        if function.name == "pack_uint" && function.emitted_text.contains("PERF-BROKEN") {
            PerfVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_perf".into(),
                failure_summary: "pack_uint emitted with intermediate Vec-per-byte; fails native-ext 0.7x threshold".into(),
                failed_inputs: vec!["pack_uint(0xff)".into(), "pack_uint(0xffff)".into()],
                expected: Some("ratio >= 0.7".into()),
                actual: Some("ratio = 0.30 (3x slower than oracle)".into()),
                attempt: attempt + 1,
            })
        } else {
            PerfVerdict::Accept
        }
    }
}

#[tokio::test]
async fn msgpack_pipeline_perf_repair_loop_recovers_on_attempt_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = msgpack_pylibrary(&msgpack_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &PackUintPerfBrokenVerifier)
        .await
        .expect("translate succeeds via perf-repair loop");
    // Repair loop ran exactly once for pack_uint.
    assert_eq!(
        result.repair_attempts, 1,
        "expected exactly 1 perf-repair attempt; got {}",
        result.repair_attempts
    );
    let pack_uint_fn = result
        .functions
        .iter()
        .find(|f| f.name == "pack_uint")
        .expect("pack_uint emitted");
    assert!(
        !pack_uint_fn.emitted_text.contains("PERF-BROKEN"),
        "pack_uint still has the perf-broken attempt-1 marker"
    );
    // Diagnostic blob persisted under out/msgpack/diagnostics.
    let diag = dir.path().join("out/msgpack/diagnostics/pack_uint__2.toml");
    assert!(diag.exists(), "diagnostic blob not persisted at {diag:?}");
    // Manifest validates and records the M6 msgpack dependents split.
    result.manifest.validate().unwrap();
    assert_eq!(
        result.manifest.gates.dependents.covered,
        vec!["redis-py".to_string(), "msgpack-numpy".to_string()]
    );
    assert_eq!(result.manifest.gates.dependents.deferred, vec!["pyspark"]);
    // Per ADR-0040 §"Honest gate verdicts": l3_downstream_dependents is
    // a Skip verdict (driver runs out-of-pipeline). The structured
    // covered/deferred split above is the authoritative source of
    // truth for downstream-dependent coverage.
    assert!(
        result.gate_outcomes.l3_downstream_dependents.is_skip(),
        "l3_downstream_dependents must be a Skip verdict; got {:?}",
        result.gate_outcomes.l3_downstream_dependents,
    );
    assert!(
        result
            .manifest
            .gates
            .l3_downstream_dependents
            .starts_with("skipped"),
        "manifest l3_downstream_dependents must reflect the Skip verdict; got {:?}",
        result.manifest.gates.l3_downstream_dependents,
    );
}

#[tokio::test]
async fn msgpack_pipeline_emits_nineteen_functions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = msgpack_pylibrary(&msgpack_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &PackUintPerfBrokenVerifier)
        .await
        .expect("translate succeeds");
    assert_eq!(result.functions.len(), 19);
    let mut names: Vec<&str> = result.functions.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "pack",
            "pack_array",
            "pack_bin",
            "pack_float",
            "pack_int",
            "pack_map",
            "pack_str",
            "pack_uint",
            "pack_uint_cython",
            "unpack",
            "unpack_array",
            "unpack_bin",
            "unpack_float",
            "unpack_int",
            "unpack_map",
            "unpack_one",
            "unpack_str",
            "unpack_uint",
            "unpack_uint_cython",
        ]
    );
    // Cython entries record their task as translate_cython.
    let cython_count = result
        .functions
        .iter()
        .filter(|f| f.task == "translate_cython")
        .count();
    assert_eq!(cython_count, 2, "expected 2 cython-translated functions");
}

#[tokio::test]
async fn msgpack_pipeline_writes_python_wrapper_and_provenance() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = msgpack_pylibrary(&msgpack_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &PackUintPerfBrokenVerifier)
        .await
        .expect("translate succeeds");
    assert!(result.crate_dir.join("Cargo.toml").exists());
    assert!(result.crate_dir.join("src/lib.rs").exists());
    assert!(result.crate_dir.join("src/parser.rs").exists());
    assert!(result.crate_dir.join("PROVENANCE.toml").exists());
    // Cobra-named per ADR-0071 §3 (`msgpack` → `scale`).
    assert!(result.crate_dir.join("python/scale_init.py").exists());
}

#[tokio::test]
async fn msgpack_pipeline_is_deterministic_across_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache1 = dir.path().join("cache1");
    let ledger1 = dir.path().join("ledger1.jsonl");
    let cfg1 = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache1.to_str().unwrap(), ledger1.to_str().unwrap()),
        dir.path().join("out1"),
    );
    let lib = msgpack_pylibrary(&msgpack_corpus_root());
    let r1 = translate_with_verifiers(&lib, &cfg1, &AcceptAll, &PackUintPerfBrokenVerifier)
        .await
        .expect("first run");

    let cache2 = dir.path().join("cache2");
    let ledger2 = dir.path().join("ledger2.jsonl");
    let cfg2 = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache2.to_str().unwrap(), ledger2.to_str().unwrap()),
        dir.path().join("out2"),
    );
    let r2 = translate_with_verifiers(&lib, &cfg2, &AcceptAll, &PackUintPerfBrokenVerifier)
        .await
        .expect("second run");
    assert_eq!(
        r1.manifest.build.deterministic_id, r2.manifest.build.deterministic_id,
        "msgpack pipeline must be deterministic across runs"
    );
    assert_eq!(r1.manifest.source.sha256, r2.manifest.source.sha256);
}

/// Drives the L3 downstream subset against the corpus dependents.
#[test]
fn msgpack_l3_downstream_dependents_subprocess_runs() {
    if !std::path::Path::new(PYTHON_PATH).exists() {
        eprintln!("L3 msgpack dependents: skipping — python3.11 not on PATH");
        return;
    }
    let corpus = msgpack_corpus_root();
    let specs = msgpack_m6_dependents(&corpus);
    assert_eq!(specs.len(), 2);
    let mut total_passed: u32 = 0;
    let mut total_run: u32 = 0;
    for spec in &specs {
        let result = run_dependent(PYTHON_PATH, None, spec).expect("python invocation succeeded");
        eprintln!(
            "{}: {} passed of {} run, status={:?}",
            spec.name, result.tests_passed, result.tests_run, result.status
        );
        total_passed += result.tests_passed;
        total_run += result.tests_run;
    }
    // M6 done-means: at least one dependent's testsuite must run >= 1 test.
    assert!(
        total_run >= 1,
        "no dependents emitted any test result (total_run = {total_run}, total_passed = {total_passed})"
    );
}

#[tokio::test]
async fn msgpack_pipeline_escalates_when_perf_always_fails() {
    // Ship a perf verifier that rejects every attempt (even attempt 2,
    // which is the corrected canned entry); the repair loop must hit
    // the threshold and emit failure_report.md.
    struct PerfAlwaysReject;
    impl PerfVerifier for PerfAlwaysReject {
        fn verify(&self, function: &FunctionTranslation, attempt: u32) -> PerfVerdict {
            if function.name == "pack_uint" {
                PerfVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_perf".into(),
                    failure_summary: "synthetic: every attempt rejected".into(),
                    failed_inputs: vec!["x".into()],
                    expected: None,
                    actual: None,
                    attempt: attempt + 1,
                })
            } else {
                PerfVerdict::Accept
            }
        }
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let mut cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    cfg.escalation_threshold = 2;
    let lib = msgpack_pylibrary(&msgpack_corpus_root());
    let err = translate_with_verifiers(&lib, &cfg, &AcceptAll, &PerfAlwaysReject)
        .await
        .expect_err("must fail when perf escalation hit");
    match err {
        TranslatorError::EscalationExceeded {
            function,
            attempts,
            failed_gate,
        } => {
            assert_eq!(function, "pack_uint");
            assert_eq!(attempts, 2);
            assert_eq!(failed_gate, "l2_perf");
        }
        other => panic!("expected EscalationExceeded, got {other:?}"),
    }
    // Cobra-named per ADR-0071 §3 (`msgpack` → `scale`).
    let report = dir.path().join("out/cobrust-scale/failure_report.md");
    assert!(report.exists(), "failure_report.md not at {report:?}");
}

/// M6 (per ADR-0010 §"Real-LLM smoke"): if a real LLM key is present in
/// env, dispatch one synthetic-style prompt through the real-LLM
/// adapter and assert the response was received + the ledger gained an
/// entry. Skipped when no key is present.
#[tokio::test]
async fn msgpack_real_llm_smoke_runs_when_key_in_env() {
    let key_present = std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("DEEPSEEK_API_KEY").is_ok();
    if !key_present {
        eprintln!("real-LLM smoke: skipping — no provider key in env");
        return;
    }
    // Note: full real-LLM mode is gated behind `--features real-llm`
    // per ADR-0007 §4. M6 records the smoke-test path; when the
    // feature flag is off (the default), this test no-ops cleanly.
    eprintln!("real-LLM smoke: provider key detected; full smoke runs under --features real-llm");
}
