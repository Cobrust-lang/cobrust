//! M5 dateutil pipeline integration test.
//!
//! Drives `cobrust_translator::translate_with_verifier` against
//! `corpus/dateutil/`, exercising:
//!  - L0 spec loading + canned-table loading.
//!  - L1 dispatch through the synthetic provider.
//!  - L2.behavior verifier hook → repair loop on `parse_iso` (the
//!    canned table ships a deliberately broken attempt-1 + correct
//!    attempt-2 entry — see `corpus/dateutil/canned_llm_responses.toml`
//!    + ADR-0008 §5).
//!  - L3 downstream-dependents driver against the vendored
//!    croniter + freezegun subsets (per ADR-0009 §3).
//!  - Manifest emission with `gates.dependents` populated.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use cobrust_translator::{
    BehaviorVerifier, FunctionTranslation, GateFailure, PyLibrary, TranslatorConfig,
    TranslatorError, VerifierVerdict, dateutil_m5_dependents, run_dependent,
    translate_with_verifier,
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

fn dateutil_corpus_root() -> PathBuf {
    workspace_root().join("corpus/dateutil")
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
models = ["dateutil-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:dateutil-canned-v1"]
"#
    );
    cobrust_llm_router::RouterConfig::from_toml_str(&toml).unwrap()
}

fn dateutil_pylibrary(corpus: &std::path::Path) -> PyLibrary {
    PyLibrary {
        library: "dateutil".into(),
        version: "2.9.0.post0".into(),
        source_file: corpus.join("upstream/dateutil_core.py"),
        spec_file: corpus.join("spec.toml"),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(corpus.join("canned_llm_responses.toml")),
        seeds: vec![42, 1337, 0xDEAD_BEEF],
        fuzz_inputs_per_fn: 1024,
    }
}

/// Verifier that rejects any `parse_iso` emission whose body still
/// contains the deliberately-broken attempt-1 marker. All other
/// functions accept on every attempt.
struct ParseIsoBrokenAttemptVerifier;

impl BehaviorVerifier for ParseIsoBrokenAttemptVerifier {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
        if function.name == "parse_iso" && function.emitted_text.contains("BROKEN-V1") {
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_behavior".into(),
                failure_summary:
                    "parse_iso swapped year/month, returns wrong tuple on every L2.behavior input"
                        .into(),
                failed_inputs: vec!["2026-04-30".into(), "2026-04-30T12:34:56".into()],
                expected: Some("(2026, 4, 30, 0, 0, 0, 0, 0, 10)".into()),
                actual: Some("(4, 2026, 30, 0, 0, 0, 0, 0, 10)".into()),
                attempt: attempt + 1,
            })
        } else {
            VerifierVerdict::Accept
        }
    }
}

#[tokio::test]
async fn dateutil_pipeline_repair_loop_recovers_on_attempt_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = dateutil_pylibrary(&dateutil_corpus_root());
    let result = translate_with_verifier(&lib, &cfg, &ParseIsoBrokenAttemptVerifier)
        .await
        .expect("translate succeeds via repair loop");
    // Repair loop ran exactly once for parse_iso.
    assert_eq!(
        result.repair_attempts, 1,
        "expected exactly 1 repair attempt; got {}",
        result.repair_attempts
    );
    // Final emission is the corrected attempt-2 body — verify the
    // BROKEN marker is gone.
    let parse_iso_fn = result
        .functions
        .iter()
        .find(|f| f.name == "parse_iso")
        .expect("parse_iso emitted");
    assert!(
        !parse_iso_fn.emitted_text.contains("BROKEN-V1"),
        "parse_iso still has the broken attempt-1 marker"
    );
    // Diagnostic blob persisted under out/dateutil/diagnostics.
    let diag = dir
        .path()
        .join("out/dateutil/diagnostics/parse_iso__2.toml");
    assert!(diag.exists(), "diagnostic blob not persisted at {diag:?}");
    // Manifest validates and records the dependents split per ADR-0009/0010.
    // M6 widened from 2/5 to 4/5 covered + 1/5 skipped (pendulum tz).
    result.manifest.validate().unwrap();
    assert_eq!(
        result.manifest.gates.dependents.covered,
        vec![
            "croniter".to_string(),
            "freezegun".to_string(),
            "pandas".to_string(),
            "sqlalchemy".to_string(),
        ]
    );
    assert_eq!(result.manifest.gates.dependents.skipped, vec!["pendulum"]);
    assert_eq!(result.manifest.gates.dependents.deferred.len(), 0);
    // Per ADR-0040 §"Honest gate verdicts": l3_downstream_dependents is
    // a Skip verdict (no in-pipeline driver). The structured covered/
    // skipped split above is the authoritative source of truth.
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
async fn dateutil_pipeline_emits_eight_functions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = dateutil_pylibrary(&dateutil_corpus_root());
    let result = translate_with_verifier(&lib, &cfg, &ParseIsoBrokenAttemptVerifier)
        .await
        .expect("translate succeeds");
    assert_eq!(result.functions.len(), 8);
    let mut names: Vec<&str> = result.functions.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "days_in_month",
            "expect_char",
            "is_digit",
            "is_leap_year",
            "normalize_datetime",
            "parse_iso",
            "relativedelta_add",
            "take_digits",
        ]
    );
}

#[tokio::test]
async fn dateutil_pipeline_writes_python_wrapper_and_provenance() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = dateutil_pylibrary(&dateutil_corpus_root());
    let result = translate_with_verifier(&lib, &cfg, &ParseIsoBrokenAttemptVerifier)
        .await
        .expect("translate succeeds");
    assert!(result.crate_dir.join("Cargo.toml").exists());
    assert!(result.crate_dir.join("src/lib.rs").exists());
    assert!(result.crate_dir.join("src/parser.rs").exists());
    assert!(result.crate_dir.join("PROVENANCE.toml").exists());
    // Cobra-named per ADR-0071 §3 (`dateutil` → `molt`).
    assert!(result.crate_dir.join("python/molt_init.py").exists());
}

/// Drives the L3 downstream subset against the *real* upstream
/// `dateutil` library (the test runs the croniter + freezegun
/// subsets via Python subprocess; if Python or upstream dateutil
/// is missing, the test logs a skip without failing).
#[test]
fn dateutil_l3_downstream_dependents_subprocess_runs() {
    if !std::path::Path::new(PYTHON_PATH).exists() {
        eprintln!("L3 dateutil dependents: skipping — python3.11 not on PATH");
        return;
    }
    // Confirm dateutil is importable; otherwise skip cleanly.
    let probe = std::process::Command::new(PYTHON_PATH)
        .arg("-c")
        .arg("import dateutil")
        .status();
    let dateutil_present = probe.map(|s| s.success()).unwrap_or(false);
    if !dateutil_present {
        eprintln!("L3 dateutil dependents: skipping — python3.11 lacks dateutil");
        return;
    }

    let corpus = dateutil_corpus_root();
    let specs = dateutil_m5_dependents(&corpus);
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
    // M5 done-means: at least one dependent's testsuite must run >= 1
    // test. We don't require all-green because upstream dateutil's
    // exact tuple shape differs from our cobrust DateTuple; the test
    // file falls back to upstream behaviour, so the count > 0 case
    // is a real signal that the L3 driver works end-to-end.
    assert!(
        total_run >= 1,
        "no dependents emitted any test result (total_run = {total_run}, total_passed = {total_passed})"
    );
}

#[tokio::test]
async fn dateutil_pipeline_ledger_records_repair_attempts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = dateutil_pylibrary(&dateutil_corpus_root());
    let _ = translate_with_verifier(&lib, &cfg, &ParseIsoBrokenAttemptVerifier)
        .await
        .expect("translate succeeds");
    // The ledger must have at least 9 entries (8 functions + 1 repair).
    let body = std::fs::read_to_string(&ledger).expect("ledger exists");
    let n = body.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        n >= 9,
        "ledger should record initial 8 + 1 repair (got {n})"
    );
}

#[tokio::test]
async fn dateutil_pipeline_escalates_when_attempt_2_also_broken() {
    // Ship a verifier that rejects every attempt; repair loop must hit
    // the threshold and emit failure_report.md.
    struct AlwaysReject;
    impl BehaviorVerifier for AlwaysReject {
        fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
            if function.name == "parse_iso" {
                VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: "synthetic: every attempt rejected".into(),
                    failed_inputs: vec!["x".into()],
                    expected: None,
                    actual: None,
                    attempt: attempt + 1,
                })
            } else {
                VerifierVerdict::Accept
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
    cfg.escalation_threshold = 2; // tight for the test
    let lib = dateutil_pylibrary(&dateutil_corpus_root());
    let err = translate_with_verifier(&lib, &cfg, &AlwaysReject)
        .await
        .expect_err("translate must fail when escalation hit");
    match err {
        TranslatorError::EscalationExceeded {
            function,
            attempts,
            failed_gate,
        } => {
            assert_eq!(function, "parse_iso");
            assert_eq!(attempts, 2);
            assert_eq!(failed_gate, "l2_behavior");
        }
        other => panic!("expected EscalationExceeded, got {other:?}"),
    }
    // failure_report.md exists.
    // Cobra-named per ADR-0071 §3 (`dateutil` → `molt`).
    let report = dir.path().join("out/cobrust-molt/failure_report.md");
    assert!(report.exists(), "failure_report.md not at {report:?}");
}
