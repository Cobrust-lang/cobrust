//! M7.1 numpy ufunc pipeline integration test.
//!
//! Drives `cobrust_translator::translate_with_verifiers` against
//! `corpus/numpy/M7.1/`, exercising:
//!
//!   - L0 spec loading (12 entries: 10 public ufuncs + 2 helpers).
//!   - L1 dispatch through the synthetic provider (canned table at
//!     `corpus/numpy/M7.1/canned_llm_responses.toml`).
//!   - L2.behavior verifier hook (no-op `AcceptAll`).
//!   - L2.perf verifier hook — M7.1 includes a deliberate-fail case
//!     (a `PerfVerifier` that always rejects) demonstrating the gate
//!     is wired and triggers `EscalationExceeded` (mirrors M6's
//!     `msgpack_pipeline_escalates_when_perf_always_fails`).
//!   - Manifest emission and validation.
//!
//! This test verifies the **translator pipeline path itself**; the
//! production cobrust-coil crate at `crates/cobrust-coil/src/`
//! is the gate-stable byte snapshot (M5/M6 precedent).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::if_not_else)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use cobrust_translator::{
    AcceptAll, AcceptAllPerf, FunctionTranslation, GateFailure, PerfVerdict, PerfVerifier,
    PyLibrary, TranslatorConfig, TranslatorError, translate_with_verifiers,
};
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn numpy_corpus_root() -> PathBuf {
    workspace_root().join("corpus/numpy/M7.1")
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
models = ["numpy-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:numpy-canned-v1"]
"#
    );
    cobrust_llm_router::RouterConfig::from_toml_str(&toml).unwrap()
}

fn numpy_pylibrary(corpus: &std::path::Path) -> PyLibrary {
    PyLibrary {
        library: "numpy".into(),
        version: "2.0.2".into(),
        source_file: corpus.join("upstream/ufunc_core.py"),
        spec_file: corpus.join("spec.toml"),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(corpus.join("canned_llm_responses.toml")),
        seeds: vec![42, 1337, 0xDEAD_BEEF],
        fuzz_inputs_per_fn: 1024,
    }
}

#[tokio::test]
async fn ufunc_pipeline_emits_twelve_functions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = numpy_pylibrary(&numpy_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &AcceptAllPerf)
        .await
        .expect("synthetic-LLM translate succeeds for the M7.1 corpus");
    assert_eq!(
        result.functions.len(),
        12,
        "M7.1 corpus has 12 functions per spec.toml"
    );

    let mut names: Vec<&str> = result.functions.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "add",
            "broadcast_shape",
            "cos",
            "divide",
            "exp",
            "log",
            "multiply",
            "power",
            "result_type",
            "sin",
            "sqrt",
            "subtract",
        ]
    );
}

#[tokio::test]
async fn ufunc_pipeline_every_function_carries_provenance() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = numpy_pylibrary(&numpy_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &AcceptAllPerf)
        .await
        .expect("translate succeeds");

    // Every function carries provenance fields.
    for fn_t in &result.functions {
        assert!(
            !fn_t.emitted_text.trim().is_empty(),
            "function {} emitted empty body",
            fn_t.name
        );
        assert_eq!(fn_t.source_sha16, "d9407aa94ead71f4");
        assert!(
            fn_t.router_decision_id.starts_with("blake3:"),
            "router_decision_id must be blake3:<hex>; got {}",
            fn_t.router_decision_id
        );
    }
}

#[tokio::test]
async fn ufunc_pipeline_writes_parser_rs_with_all_functions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = numpy_pylibrary(&numpy_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &AcceptAllPerf)
        .await
        .expect("translate succeeds");

    let parser =
        std::fs::read_to_string(result.crate_dir.join("src/parser.rs")).expect("parser.rs exists");
    for name in [
        "add", "cos", "divide", "exp", "log", "multiply", "power", "sin", "sqrt", "subtract",
    ] {
        assert!(
            parser.contains(&format!("pub fn {name}(")),
            "parser.rs missing `pub fn {name}(...)`"
        );
        assert!(
            parser.contains(&format!("// fn:{name}")),
            "parser.rs missing per-function provenance comment for {name}"
        );
    }
}

#[tokio::test]
async fn ufunc_pipeline_manifest_validates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    let lib = numpy_pylibrary(&numpy_corpus_root());
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &AcceptAllPerf)
        .await
        .expect("translate succeeds");
    result.manifest.validate().expect("manifest validates");
    assert_eq!(result.manifest.source.library, "numpy");
    assert_eq!(result.manifest.source.version, "2.0.2");
    assert_eq!(result.manifest.gates.l1_files_emitted, 12);
}

// ---- L2.perf gate is enforced — escalation test (per ADR-0014 §5) ----

/// A perf verifier that rejects every attempt for one specific
/// function (`add`), demonstrating the gate is wired and capable of
/// triggering the M5+ repair loop. Mirrors M6's
/// `msgpack_pipeline_escalates_when_perf_always_fails` where only
/// `pack_uint` is selectively rejected.
struct PerfAlwaysRejectAdd;

impl PerfVerifier for PerfAlwaysRejectAdd {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> PerfVerdict {
        if function.name == "add" {
            PerfVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_perf".into(),
                failure_summary: format!(
                    "synthetic always-fail perf verifier (M7.1 enforced gate per ADR-0014 §5); attempt {attempt}"
                ),
                failed_inputs: vec!["a/b".into()],
                expected: None,
                actual: None,
                attempt: attempt + 1,
            })
        } else {
            PerfVerdict::Accept
        }
    }
}

#[tokio::test]
async fn ufunc_pipeline_escalates_when_perf_always_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let mut cfg = TranslatorConfig::m4_synthetic(
        canned_router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
        dir.path().join("out"),
    );
    cfg.escalation_threshold = 2;
    let lib = numpy_pylibrary(&numpy_corpus_root());
    let err = translate_with_verifiers(&lib, &cfg, &AcceptAll, &PerfAlwaysRejectAdd)
        .await
        .expect_err("perf verifier always rejects → escalation");
    match err {
        TranslatorError::EscalationExceeded {
            function,
            attempts,
            failed_gate,
        } => {
            assert_eq!(function, "add");
            assert_eq!(attempts, 2);
            assert_eq!(failed_gate, "l2_perf");
        }
        other => panic!("expected EscalationExceeded, got {other:?}"),
    }
    let report = dir.path().join("out/cobrust-numpy/failure_report.md");
    assert!(report.exists(), "failure_report.md not at {report:?}");
}
