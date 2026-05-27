//! M7.0 numpy pipeline integration test.
//!
//! Drives `cobrust_translator::translate_with_verifiers` against
//! `corpus/numpy/M7.0/`, exercising:
//!
//!   - L0 spec loading (8 entries: 4 public constructors + 4 helpers).
//!   - L1 dispatch through the synthetic provider (the canned table at
//!     `corpus/numpy/M7.0/canned_llm_responses.toml` ships one entry
//!     per function under `task = translate, attempt = 1`).
//!   - L2.behavior verifier hook (no-op `AcceptAll` for M7.0 — the
//!     bytes-identical contract for cobrust-coil is enforced by
//!     `tests/numpy_differential.rs` against upstream numpy 2.0.2,
//!     not by re-running the synthetic emission through behavior
//!     gates here).
//!   - L2.perf verifier hook (no-op `AcceptAllPerf` — perf is
//!     informational at M7.0 per ADR-0013 §"M7.0 manifest fields").
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

use cobrust_translator::{
    AcceptAll, AcceptAllPerf, PyLibrary, TranslatorConfig, translate_with_verifiers,
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
    workspace_root().join("corpus/numpy/M7.0")
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
        source_file: corpus.join("upstream/array_core.py"),
        spec_file: corpus.join("spec.toml"),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(corpus.join("canned_llm_responses.toml")),
        seeds: vec![42, 1337, 0xDEAD_BEEF],
        fuzz_inputs_per_fn: 1024,
    }
}

#[tokio::test]
async fn numpy_pipeline_emits_eight_functions() {
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
        .expect("synthetic-LLM translate succeeds for the M7.0 corpus");
    assert_eq!(
        result.functions.len(),
        8,
        "M7.0 corpus has 8 functions per spec.toml"
    );

    let mut names: Vec<&str> = result.functions.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "arange",
            "array",
            "array_repr",
            "cast_to_dtype",
            "ones",
            "parse_dtype",
            "shape_size",
            "zeros",
        ]
    );
}

#[tokio::test]
async fn numpy_pipeline_every_function_carries_provenance() {
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
        assert_eq!(fn_t.source_sha16, "a445a74f03a0570c");
        assert!(
            fn_t.router_decision_id.starts_with("blake3:"),
            "router_decision_id must be blake3:<hex>; got {}",
            fn_t.router_decision_id
        );
    }
}

#[tokio::test]
async fn numpy_pipeline_writes_parser_rs_with_all_functions() {
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

    // src/parser.rs must contain every function name as a `pub fn`.
    let parser =
        std::fs::read_to_string(result.crate_dir.join("src/parser.rs")).expect("parser.rs exists");
    for name in [
        "arange",
        "array",
        "array_repr",
        "cast_to_dtype",
        "ones",
        "parse_dtype",
        "shape_size",
        "zeros",
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
async fn numpy_pipeline_manifest_validates() {
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
    assert_eq!(result.manifest.gates.l1_files_emitted, 8);
}
