//! Integration test for the M4 pipeline against the `tomli` corpus.
//!
//! Runs the full L0 → L1 pipeline in synthetic-LLM mode, generates the
//! `cobrust-tomli` crate, and verifies:
//!
//! - Manifest validates and is well-formed.
//! - `deterministic_id` is stable across two independent runs.
//! - Every spec function has an emitted block in `parser.rs`.
//! - The generated crate writes to `<workspace>/crates/cobrust-tomli/`
//!   when the env var `COBRUST_REGENERATE_TOMLI=1` is set; otherwise
//!   it writes to a temp dir and only verifies invariants.

use std::path::{Path, PathBuf};

use cobrust_llm_router::RouterConfig;
use cobrust_translator::{PyLibrary, TranslatorConfig, translate};

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn corpus_root() -> PathBuf {
    workspace_root().join("corpus/tomli")
}

fn router_config(out_root: &Path) -> RouterConfig {
    let cache = out_root.join(".cobrust/llm_cache");
    let ledger = out_root.join(".cobrust/ledger.jsonl");
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
    RouterConfig::from_toml_str(&toml).expect("config parses")
}

fn build_library() -> PyLibrary {
    let corpus = corpus_root();
    PyLibrary {
        library: "tomli".into(),
        version: "2.0.1".into(),
        source_file: corpus.join("upstream/tomli_loads.py"),
        spec_file: corpus.join("spec.toml"),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(corpus.join("canned_llm_responses.toml")),
        seeds: vec![42, 1337, 0xDEAD_BEEF],
        fuzz_inputs_per_fn: 1024,
    }
}

fn run_pipeline(out_dir: PathBuf) -> cobrust_translator::TranslatedCrate {
    let cfg = TranslatorConfig::m4_synthetic(router_config(&out_dir), out_dir);
    let lib = build_library();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(translate(&lib, &cfg))
        .expect("pipeline must succeed in synthetic mode")
}

#[test]
fn pipeline_emits_all_twelve_tomli_functions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let result = run_pipeline(dir.path().to_path_buf());
    assert_eq!(
        result.functions.len(),
        12,
        "M4 spec covers exactly 12 functions; if this changes update spec.toml"
    );
    for f in &result.functions {
        assert!(
            !f.emitted_text.trim().is_empty(),
            "function {} got empty emission",
            f.name
        );
    }
}

#[test]
fn pipeline_manifest_validates_and_is_well_formed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let result = run_pipeline(dir.path().to_path_buf());
    result.manifest.validate().expect("manifest must validate");
    assert_eq!(result.manifest.source.library, "tomli");
    assert_eq!(result.manifest.source.version, "2.0.1");
    assert_eq!(result.manifest.source.sha256.len(), 64);
    assert!(
        result
            .manifest
            .source
            .sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    );
    assert_eq!(result.manifest.gates.l1_files_emitted, 12);
    assert!(result.manifest.gates.l0_spec_emitted);
    assert_eq!(result.manifest.router.strategy, "synthetic");
    assert!(
        result
            .manifest
            .router
            .models_used
            .iter()
            .any(|m| m.starts_with("synthetic:"))
    );
}

#[test]
fn pipeline_deterministic_id_is_stable_across_runs() {
    let d1 = tempfile::tempdir().expect("tempdir 1");
    let d2 = tempfile::tempdir().expect("tempdir 2");
    let r1 = run_pipeline(d1.path().to_path_buf());
    let r2 = run_pipeline(d2.path().to_path_buf());
    assert_eq!(
        r1.manifest.build.deterministic_id, r2.manifest.build.deterministic_id,
        "deterministic_id must be byte-identical across runs"
    );
    assert_eq!(r1.manifest.source.sha256, r2.manifest.source.sha256);
}

#[test]
fn pipeline_writes_buildable_crate_layout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let result = run_pipeline(dir.path().to_path_buf());
    assert!(result.crate_dir.join("Cargo.toml").exists());
    assert!(result.crate_dir.join("src/lib.rs").exists());
    assert!(result.crate_dir.join("src/parser.rs").exists());
    assert!(result.crate_dir.join("PROVENANCE.toml").exists());
    assert!(result.crate_dir.join("python/tomli_init.py").exists());
    assert!(result.crate_dir.join("python/setup.py").exists());
    let cargo = std::fs::read_to_string(result.crate_dir.join("Cargo.toml")).expect("read");
    assert!(cargo.contains("name = \"cobrust-tomli\""));
    assert!(cargo.contains("DO NOT EDIT BY HAND"));
    let parser = std::fs::read_to_string(result.crate_dir.join("src/parser.rs")).expect("read");
    assert!(parser.contains("// fn:loads"));
    assert!(parser.contains("// fn:parse_value"));
    assert!(parser.contains("// fn:skip_whitespace"));
}

#[test]
fn pipeline_regenerates_cobrust_tomli_when_env_set() {
    if std::env::var("COBRUST_REGENERATE_TOMLI").as_deref() != Ok("1") {
        return;
    }
    let out = workspace_root().join("crates");
    let cfg = TranslatorConfig::m4_synthetic(router_config(&out), out.clone());
    let lib = build_library();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = rt.block_on(translate(&lib, &cfg)).expect("pipeline");
    eprintln!("regenerated {}", result.crate_dir.display());
}
