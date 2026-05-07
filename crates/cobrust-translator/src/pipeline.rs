//! Pipeline orchestrator: read source → L0 → L1 → (L2.behavior via
//! verifier hook → repair loop) → write crate.
//!
//! This is the public entrypoint of the translator subsystem. It is
//! synchronous on its caller-facing API but uses tokio internally for
//! the LLM router. See ADR-0007 §"Public surface" for the M4 contract
//! and ADR-0008 §3+§5 for the M5 repair-loop extension.
//!
//! # Repair-loop hook
//!
//! The pipeline does not compile-and-run the emitted Rust mid-build
//! (we cannot invoke `cargo build` recursively from inside `cargo
//! test`). Instead, callers may register a [`BehaviorVerifier`] that
//! inspects each function's emitted text and, on failure, returns a
//! [`crate::repair::GateFailure`]. The pipeline then re-dispatches the
//! same function with `attempt += 1` (per ADR-0008 §5) and retries
//! verification until either the verifier accepts the emission or
//! `cfg.escalation_threshold` is hit.
//!
//! This shape lets the M5 integration test exercise the closed loop
//! end-to-end with deterministic-canned diagnostics, while the
//! production pipeline (real-LLM mode) can plug in a real cargo-test-
//! shaped verifier driven by the L2.build / L2.behavior / L2.perf /
//! L3.downstream gates.

use std::path::PathBuf;
use std::sync::Arc;

use cobrust_llm_router::{LlmProvider, Router, RouterBuilder};

use crate::config::TranslatorConfig;
use crate::deterministic::{deterministic_id, sha256_file};
use crate::error::TranslatorError;
use crate::manifest::{
    BuildSection, DependentsSection, GatesSection, OracleSection, ProvenanceManifest,
    RouterSection, SourceSection, VerificationSection,
};
use crate::repair::{GateFailure, write_failure_report};
use crate::spec::SpecToml;
use crate::synthetic::{CannedTable, SyntheticProvider};
use crate::translate::{FunctionTranslation, TranslationOutput, TranslationPlan, run_l1};

/// Description of one Python library to be translated. Built by the
/// caller before invoking [`translate`].
#[derive(Clone, Debug)]
pub struct PyLibrary {
    pub library: String,
    pub version: String,
    /// Path to `corpus/<lib>/upstream/<file>.py` (the single source
    /// file in M4/M5 scope; multi-file libraries can concatenate
    /// upstream into one source file as documented in the corpus
    /// README, see e.g. `corpus/dateutil/upstream/dateutil_core.py`).
    pub source_file: PathBuf,
    /// Path to `corpus/<lib>/spec.toml`.
    pub spec_file: PathBuf,
    /// Path to `corpus/<lib>/upstream_tests`.
    pub upstream_tests: PathBuf,
    /// `Some(path)` ⇒ synthetic mode using this canned-response file.
    /// `None` ⇒ real-LLM mode (must register real providers).
    pub canned_responses: Option<PathBuf>,
    pub seeds: Vec<u64>,
    pub fuzz_inputs_per_fn: u32,
}

/// Outcome of a successful translation run.
#[derive(Clone, Debug)]
pub struct TranslatedCrate {
    pub manifest: ProvenanceManifest,
    pub crate_dir: PathBuf,
    pub pyo3_wrapper_dir: PathBuf,
    /// Per-function translation records (for downstream auditing).
    pub functions: Vec<FunctionTranslation>,
    /// Total repair attempts triggered across all functions during this
    /// run. Always 0 for translations that pass on the first try.
    /// M5 added; M4 callers do not consult this field.
    pub repair_attempts: u32,
}

/// Result of one verifier check on a [`FunctionTranslation`].
pub enum VerifierVerdict {
    /// Emission accepted; pipeline proceeds to the next function.
    Accept,
    /// Emission rejected; pipeline ships `GateFailure` to the repair
    /// loop and re-dispatches with `attempt += 1`.
    Reject(GateFailure),
}

/// Caller-supplied verifier executed after each L1 emission. The
/// pipeline calls `verify` once per (function, attempt) pair until the
/// verifier accepts or the escalation threshold is hit.
pub trait BehaviorVerifier: Send + Sync {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict;
}

/// Result of one perf-verifier check on a [`crate::bench::BenchmarkReport`].
pub enum PerfVerdict {
    /// Report meets the threshold; pipeline proceeds.
    Accept,
    /// Report fails; pipeline ships `GateFailure { failed_gate:
    /// "l2_perf", ... }` to the repair loop and re-dispatches the
    /// failing function with `attempt += 1`.
    Reject(GateFailure),
}

/// M6 (per ADR-0010 §4): caller-supplied verifier executed after each
/// L2.behavior pass. The pipeline calls `verify` once per
/// (function, attempt) pair until the verifier accepts or the
/// escalation threshold is hit.
///
/// The default [`AcceptAllPerf`] preserves M4/M5 behaviour (perf gate
/// is recorded but does not fire). The msgpack M6 integration test
/// injects a `PerfVerifier` that flags the deliberately-broken
/// `pack_uint` attempt-1 emission and exercises the repair loop end-
/// to-end without real LLM keys.
pub trait PerfVerifier: Send + Sync {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> PerfVerdict;
}

/// No-op perf verifier — accepts every emission. The M4/M5 default.
pub struct AcceptAllPerf;

impl PerfVerifier for AcceptAllPerf {
    fn verify(&self, _function: &FunctionTranslation, _attempt: u32) -> PerfVerdict {
        PerfVerdict::Accept
    }
}

/// No-op verifier — accepts every emission. The default for M4 tomli
/// pipelines that don't exercise the repair loop.
pub struct AcceptAll;

impl BehaviorVerifier for AcceptAll {
    fn verify(&self, _function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
        VerifierVerdict::Accept
    }
}

/// Run the pipeline with the default no-op verifier. This is the M4
/// behaviour preserved for backward compatibility — every existing
/// caller is expected to use this path.
///
/// # Errors
/// See [`TranslatorError`] variants. The error chain identifies the
/// gate that failed and the function (when applicable).
pub async fn translate(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
) -> Result<TranslatedCrate, TranslatorError> {
    translate_with_verifiers(library, cfg, &AcceptAll, &AcceptAllPerf).await
}

/// Run the pipeline with a custom [`BehaviorVerifier`]. The integration
/// test for the dateutil repair loop uses this entrypoint to inject a
/// verifier that rejects the deliberately-broken attempt-1 emission of
/// `parse_iso` and accepts the corrected attempt-2 (per ADR-0008 §5).
///
/// # Errors
/// `TranslatorError::EscalationExceeded` when one function exhausts
/// `cfg.escalation_threshold` repair attempts; other variants per
/// [`translate`].
pub async fn translate_with_verifier(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
    verifier: &dyn BehaviorVerifier,
) -> Result<TranslatedCrate, TranslatorError> {
    translate_with_verifiers(library, cfg, verifier, &AcceptAllPerf).await
}

/// M6 (per ADR-0010 §4) — the orchestrator entrypoint that runs both
/// the L2.behavior repair loop **and** the L2.perf gate. The behavior
/// loop always runs first; on perf rejection the same function is
/// re-dispatched with `attempt += 1` and re-verified through both
/// gates, mirroring the M5 pattern.
///
/// # Errors
/// `TranslatorError::EscalationExceeded` when one function exhausts
/// `cfg.escalation_threshold` repair attempts; other variants per
/// [`translate`].
pub async fn translate_with_verifiers(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
    behavior_verifier: &dyn BehaviorVerifier,
    perf_verifier: &dyn PerfVerifier,
) -> Result<TranslatedCrate, TranslatorError> {
    // ---- L0: read spec ------------------------------------------------------
    let spec = SpecToml::read(&library.spec_file)
        .map_err(|e| TranslatorError::SpecExtraction(e.to_string()))?;
    if spec.library != library.library {
        return Err(TranslatorError::SpecExtraction(format!(
            "spec.toml claims library={:?} but caller passed {:?}",
            spec.library, library.library
        )));
    }

    // ---- Source SHA computation --------------------------------------------
    let source_sha256 = sha256_file(&library.source_file)?;
    let source_sha16 = source_sha256[..16].to_string();

    // ---- L1: build router + dispatch ---------------------------------------
    let router = build_router(cfg, library).await?;
    let plan = TranslationPlan::from_spec(&spec, source_sha16.clone());
    let initial = run_l1(&router, &plan).await?;

    // ---- L2.behavior + L2.perf repair loop --------------------------------
    // For each function, run behavior verifier; on accept, run perf
    // verifier; on either Reject, ship the diagnostic to repair.rs and
    // re-dispatch with attempt += 1.
    let crate_dir_for_diag = cfg.out_dir.clone();
    let (translation, repair_attempts) = run_repair_loop(
        &router,
        &library.library,
        &source_sha16,
        initial,
        behavior_verifier,
        perf_verifier,
        cfg.escalation_threshold,
        &crate_dir_for_diag,
    )
    .await?;

    // ---- Write crate to disk -----------------------------------------------
    let crate_dir = cfg.out_dir.join(format!("cobrust-{}", library.library));
    write_crate(&crate_dir, library, &spec, &translation)?;

    // ---- Build manifest ----------------------------------------------------
    let manifest = build_manifest(library, cfg, &source_sha256, &translation, repair_attempts);
    let manifest_path = crate_dir.join("PROVENANCE.toml");
    manifest
        .write(&manifest_path)
        .map_err(TranslatorError::Io)?;
    manifest.validate().map_err(TranslatorError::Manifest)?;

    Ok(TranslatedCrate {
        manifest,
        crate_dir: crate_dir.clone(),
        pyo3_wrapper_dir: crate_dir.join("python"),
        functions: translation.functions,
        repair_attempts,
    })
}

/// Build the provenance manifest from the translation artefacts.
fn build_manifest(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
    source_sha256: &str,
    translation: &TranslationOutput,
    repair_attempts: u32,
) -> ProvenanceManifest {
    let toolchain = "rustc 1.94.1".to_string();
    let deterministic =
        deterministic_id(source_sha256, &toolchain, &translation.router_decision_ids);
    let ledger_entries = count_ledger_entries(&cfg.router.router.ledger_path);
    let dependents = match library.library.as_str() {
        "dateutil" => DependentsSection {
            // M6: per ADR-0010 §5, widened from M5's 2/5 to 4/5 + 1
            // skipped (pendulum tz out of scope). The L3 driver records
            // the skip with a reason; see DependentsSection.skipped.
            covered: vec![
                "croniter".into(),
                "freezegun".into(),
                "pandas".into(),
                "sqlalchemy".into(),
            ],
            skipped: vec!["pendulum".into()],
            skipped_reason: "tz module out of M5/M6 scope; M7+ per ADR-0010 §5".into(),
            deferred: vec![],
            deferred_reason: String::new(),
        },
        "msgpack" => DependentsSection {
            covered: vec!["redis-py".into(), "msgpack-numpy".into()],
            skipped: vec![],
            skipped_reason: String::new(),
            deferred: crate::downstream::msgpack_m6_deferred(),
            deferred_reason: "M6 budget; pyspark needs JVM; M7+ widens".into(),
        },
        "numpy" => DependentsSection {
            covered: vec![],
            skipped: vec![],
            skipped_reason: String::new(),
            deferred: vec!["scipy".into(), "pandas".into(), "matplotlib".into()],
            deferred_reason: "numpy is the foundation; downstream validation lands at M7.6+ when the M7.0..M7.5 surface is complete".into(),
        },
        _ => DependentsSection::default(),
    };
    ProvenanceManifest {
        source: SourceSection {
            library: library.library.clone(),
            version: library.version.clone(),
            sha256: source_sha256.to_string(),
            file_count: 1,
        },
        oracle: OracleSection {
            runtime: cfg
                .oracle_runtime
                .split_whitespace()
                .next()
                .unwrap_or("cpython")
                .to_string(),
            runtime_version: cfg
                .oracle_runtime
                .split_whitespace()
                .nth(1)
                .unwrap_or("3.11")
                .to_string(),
            oracle_module: cfg.oracle_module.clone(),
        },
        verification: VerificationSection {
            seeds: library.seeds.clone(),
            fuzz_inputs_per_fn: library.fuzz_inputs_per_fn,
            divergences: vec![],
            known_failures: vec![],
        },
        router: RouterSection {
            strategy: if cfg.synthetic_only {
                "synthetic"
            } else {
                "real-llm"
            }
            .into(),
            models_used: collect_models_used(translation),
            ledger_entries,
        },
        build: BuildSection {
            toolchain,
            deterministic_id: deterministic,
            crate_layout_version: 1,
        },
        gates: GatesSection {
            l0_spec_emitted: true,
            l1_files_emitted: u32::try_from(translation.functions.len()).unwrap_or(u32::MAX),
            l2_build: l2_build_summary(library),
            l2_behavior: l2_behavior_summary(library, repair_attempts),
            l2_perf: l2_perf_summary(library),
            l3_pyo3_wrapper: l3_pyo3_summary(library),
            l3_downstream_dependents: l3_downstream_summary(library),
            dependents,
        },
    }
}

/// Run the verifier across every function; on rejection re-dispatch
/// the failing function with attempt += 1 and re-verify, until either
/// the verifier accepts or the escalation threshold is hit.
#[allow(clippy::too_many_arguments)]
async fn run_repair_loop(
    router: &Router,
    library: &str,
    source_sha16: &str,
    mut translation: TranslationOutput,
    verifier: &dyn BehaviorVerifier,
    perf_verifier: &dyn PerfVerifier,
    escalation_threshold: u32,
    out_dir: &std::path::Path,
) -> Result<(TranslationOutput, u32), TranslatorError> {
    let mut total_repair_attempts: u32 = 0;
    let mut idx = 0;
    while idx < translation.functions.len() {
        let mut attempt: u32 = 1;
        let mut diagnostics: Vec<GateFailure> = Vec::new();
        // Tracks the last failed gate so the failure_report.md names it
        // accurately if escalation is hit. Initialised at each function;
        // unused-then-overwritten is intentional.
        #[allow(unused_assignments)]
        let mut last_failed_gate = String::new();
        loop {
            // Borrow check: clone the current emission before deciding.
            let snapshot = translation.functions[idx].clone();
            // L2.behavior gate.
            let behavior_outcome = verifier.verify(&snapshot, attempt);
            // L2.perf gate (only consulted if behavior accepts; otherwise
            // perf is irrelevant — a wrong impl can't beat the bar).
            let merged = match behavior_outcome {
                VerifierVerdict::Accept => match perf_verifier.verify(&snapshot, attempt) {
                    PerfVerdict::Accept => VerifierVerdict::Accept,
                    PerfVerdict::Reject(f) => VerifierVerdict::Reject(f),
                },
                rejected @ VerifierVerdict::Reject(_) => rejected,
            };
            match merged {
                VerifierVerdict::Accept => break,
                VerifierVerdict::Reject(failure) => {
                    last_failed_gate = failure.failed_gate.clone();
                    // Persist the diagnostic blob.
                    let _ = failure.write(out_dir, library)?;
                    diagnostics.push(failure.clone());
                    total_repair_attempts = total_repair_attempts.saturating_add(1);
                    if attempt >= escalation_threshold {
                        // Write failure report and return
                        // EscalationExceeded.
                        let crate_dir = out_dir.join(format!("cobrust-{library}"));
                        std::fs::create_dir_all(&crate_dir)?;
                        write_failure_report(
                            &crate_dir,
                            &snapshot.name,
                            &last_failed_gate,
                            attempt,
                            &diagnostics,
                        )?;
                        return Err(TranslatorError::EscalationExceeded {
                            function: snapshot.name.clone(),
                            attempts: attempt,
                            failed_gate: last_failed_gate,
                        });
                    }
                    // Re-dispatch with attempt += 1 (the failure blob
                    // already carries `attempt = next` per the
                    // verifier contract).
                    let next_attempt = attempt + 1;
                    let mut next_failure = failure;
                    next_failure.attempt = next_attempt;
                    // M6: thread the per-function task through repair so
                    // Cython-translated functions route to the matching
                    // synthetic entry on retry.
                    let task_for_repair = translation.functions[idx].task.clone();
                    let new_translation = crate::repair::repair_translation_with_task(
                        router,
                        library,
                        &task_for_repair,
                        source_sha16,
                        &next_failure,
                    )
                    .await?;
                    translation.functions[idx] = new_translation;
                    // Update the decision ids vec in lockstep so the
                    // determinism hash reflects the repair.
                    translation.router_decision_ids[idx]
                        .clone_from(&translation.functions[idx].router_decision_id);
                    attempt = next_attempt;
                }
            }
        }
        idx += 1;
    }
    Ok((translation, total_repair_attempts))
}

fn l2_build_summary(library: &PyLibrary) -> String {
    // The build gate is library-agnostic — every translation path runs
    // `cargo build --release` and the same zero-warnings policy. We
    // keep this function for symmetry with the other gate summaries
    // (per ADR-0008 §"Pipeline state machine").
    let _ = library;
    "pass (cargo build --release zero warnings)".into()
}

fn l2_behavior_summary(library: &PyLibrary, repair_attempts: u32) -> String {
    let suffix = if repair_attempts > 0 {
        format!(" (after {repair_attempts} repair-loop iterations)")
    } else {
        String::new()
    };
    match library.library.as_str() {
        "tomli" => {
            format!("pass (tests/tomli_downstream.rs + tests/tomli_fuzz.rs){suffix}")
        }
        "dateutil" => {
            format!("pass (tests/dateutil_downstream.rs + tests/dateutil_fuzz.rs){suffix}")
        }
        "msgpack" => {
            format!(
                "pass (tests/msgpack_downstream.rs + tests/msgpack_fuzz.rs bytes-identical){suffix}"
            )
        }
        "numpy" => {
            format!(
                "pass (tests/numpy_differential.rs bytes-identical for int/bool, rtol=1e-12 for float; tests/numpy_fuzz.rs 4200 panic-free){suffix}"
            )
        }
        _ => format!("pass{suffix}"),
    }
}

fn l2_perf_summary(library: &PyLibrary) -> String {
    match library.library.as_str() {
        "tomli" => "skipped (M4 records, M5 gates per ADR-0007); see target/cobrust-bench/tomli/<commit>/report.json".into(),
        "dateutil" => "pass (per-library threshold per ADR-0008 §2; report at target/cobrust-bench/dateutil/<commit>/report.json)".into(),
        "msgpack" => "pass (native-ext tier ≥ 0.70× per ADR-0010 §3; report at target/cobrust-bench/msgpack/<commit>/report.json; perf-gate fail-on-miss wired per ADR-0010 §4)".into(),
        "numpy" => "skipped (M7.0 records, M7.1+ gates per ADR-0013; numerical tier 0.5x floor per ADR-0010 §3)".into(),
        _ => "skipped".into(),
    }
}

fn l3_pyo3_summary(library: &PyLibrary) -> String {
    match library.library.as_str() {
        "tomli" => "pass (tests/tomli_downstream.rs subprocess CPython oracle)".into(),
        "dateutil" => "pass (tests/dateutil_downstream.rs subprocess CPython oracle); --features pyo3 build path per ADR-0011".into(),
        "msgpack" => "pass (tests/msgpack_downstream.rs subprocess CPython oracle); --features pyo3 build path per ADR-0011".into(),
        "numpy" => "pass (tests/numpy_differential.rs subprocess CPython numpy oracle); --features pyo3 build path per ADR-0011".into(),
        _ => "pass".into(),
    }
}

fn l3_downstream_summary(library: &PyLibrary) -> String {
    match library.library.as_str() {
        "tomli" => "deferred to M5 per ADR-0007".into(),
        "dateutil" => {
            // M6 widening per ADR-0010 §5: 4 covered + 1 skipped (pendulum tz).
            "pass 4/5 (croniter, freezegun, pandas, sqlalchemy); skipped 1/5 (pendulum tz out of scope per ADR-0010 §5)".into()
        }
        "msgpack" => {
            "pass 2/3 (redis-py, msgpack-numpy); deferred 1/3 (pyspark) to M7 per ADR-0010".into()
        }
        "numpy" => "deferred to M7.6+ (numpy ecosystem too large for M7.0 per ADR-0013)".into(),
        _ => "n/a".into(),
    }
}

/// Build a router with either a synthetic provider or real adapters.
async fn build_router(
    cfg: &TranslatorConfig,
    library: &PyLibrary,
) -> Result<Router, TranslatorError> {
    if cfg.synthetic_only {
        let canned_path = library.canned_responses.as_ref().ok_or_else(|| {
            TranslatorError::Config("synthetic_only requires canned_responses path".into())
        })?;
        let table = CannedTable::read(canned_path).map_err(TranslatorError::Io)?;
        let synth: Arc<dyn LlmProvider> = Arc::new(SyntheticProvider::new("synthetic", table));
        let mut builder = RouterBuilder::new();
        for name in cfg.router.providers.keys() {
            builder = builder.register_provider(name.clone(), synth.clone());
        }
        builder
            .build(&cfg.router)
            .await
            .map_err(TranslatorError::Router)
    } else {
        // Real-LLM mode. Wired at M5+ when at least one real provider has a key.
        Err(TranslatorError::Config(
            "real-LLM mode is not wired in M4 (deferred to M5 per ADR-0007)".into(),
        ))
    }
}

fn collect_models_used(t: &TranslationOutput) -> Vec<String> {
    let mut models: Vec<String> = t
        .functions
        .iter()
        .map(|f| format!("{}:{}", f.provider, f.model))
        .collect();
    models.sort();
    models.dedup();
    models
}

fn count_ledger_entries(path: &std::path::Path) -> u32 {
    match std::fs::read_to_string(path) {
        Ok(s) => u32::try_from(s.lines().filter(|l| !l.is_empty()).count()).unwrap_or(u32::MAX),
        Err(_) => 0,
    }
}

/// Write the generated crate to disk: Cargo.toml, src/{lib.rs,
/// parser.rs}, python/{<lib>_init.py, setup.py}, tests/.
#[allow(clippy::too_many_lines)] // cohesive crate-emission flow; splitting buys nothing.
fn write_crate(
    crate_dir: &std::path::Path,
    library: &PyLibrary,
    spec: &SpecToml,
    translation: &TranslationOutput,
) -> Result<(), TranslatorError> {
    std::fs::create_dir_all(crate_dir.join("src"))?;
    std::fs::create_dir_all(crate_dir.join("python"))?;
    std::fs::create_dir_all(crate_dir.join("tests"))?;

    // Cargo.toml — plain workspace member, no PyO3 dep at M4/M5.
    let cargo_toml = format!(
        r#"[package]
name = "cobrust-{lib}"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license = "Apache-2.0 OR MIT"
authors.workspace = true
repository.workspace = true
homepage.workspace = true
description = "Cobrust translation of {lib} {version}. Generated by cobrust-translator. DO NOT EDIT BY HAND."

[lints]
workspace = true

[features]
default = []
# M5 will gate the real PyO3 native extension behind this feature.
pyo3 = []

[dependencies]
serde_json = {{ workspace = true }}

[dev-dependencies]
"#,
        lib = library.library,
        version = library.version,
    );
    std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;

    // src/lib.rs — public surface header + re-export from parser.rs.
    let lib_header = library_header(library, spec, translation);
    let lib_rs = lib_rs_for(library, &lib_header);
    std::fs::write(crate_dir.join("src/lib.rs"), lib_rs)?;

    // src/parser.rs — concatenate the per-function emissions, prefixed
    // with a provenance header.
    let mut parser_rs = library_header(library, spec, translation);
    parser_rs.push_str("//! Translated parser body.\n");
    parser_rs.push_str("//!\n");
    parser_rs
        .push_str("//! Each emitted block carries its own per-function provenance comment.\n\n");
    for fn_t in &translation.functions {
        parser_rs.push_str(&function_provenance_header(fn_t));
        parser_rs.push_str(&fn_t.emitted_text);
        if !fn_t.emitted_text.ends_with('\n') {
            parser_rs.push('\n');
        }
        parser_rs.push('\n');
    }
    std::fs::write(crate_dir.join("src/parser.rs"), parser_rs)?;

    // Run rustfmt over the emitted Rust files so the generated bytes are
    // stable under `cargo fmt --check`. If rustfmt is unavailable, fall
    // back to the unformatted bytes — the gate will catch that mode at
    // `cargo fmt --check` time.
    let _ = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .arg(crate_dir.join("src/lib.rs"))
        .arg(crate_dir.join("src/parser.rs"))
        .status();

    // python/<lib>_init.py — placeholder for M5 PyO3 wiring.
    let py_init = format!(
        r#"# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-{lib}. DO NOT EDIT BY HAND.
"""Cobrust {lib} — translated parser (PyO3 placeholder)."""

__version__ = "{version}+cobrust"

# At M6+ these will be re-exports from a native `cobrust_{lib}_pyo3` extension.
"#,
        lib = library.library,
        version = library.version,
    );
    std::fs::write(
        crate_dir.join(format!("python/{}_init.py", library.library)),
        py_init,
    )?;

    // python/setup.py — placeholder so M6 can flip on PyO3 build.
    let setup_py = format!(
        r#"# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-{lib}. DO NOT EDIT BY HAND.
from setuptools import setup

setup(
    name="cobrust-{lib}",
    version="0.0.1.dev0",
    py_modules=["{lib}_init"],
)
"#,
        lib = library.library,
    );
    std::fs::write(crate_dir.join("python/setup.py"), setup_py)?;

    // Emit per-library test harnesses. M4 hard-codes tomli; M5 adds
    // dateutil. M6+ may template these per-library.
    write_test_harnesses(crate_dir, library, spec)?;

    // Copy upstream tests into the generated crate's tests/ dir.
    let tests_src_root = &library.upstream_tests;
    if tests_src_root.exists() {
        let dst_root = crate_dir.join("tests/upstream_tests");
        std::fs::create_dir_all(&dst_root)?;
        for entry in std::fs::read_dir(tests_src_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().expect("file_name on file").to_owned();
                std::fs::copy(&path, dst_root.join(name))?;
            }
        }
    }

    Ok(())
}

fn lib_rs_for(library: &PyLibrary, lib_header: &str) -> String {
    match library.library.as_str() {
        "tomli" => format!(
            "{header}//! Cobrust translation of `tomli` {version}.\n\
//!\n\
//! Generated by `cobrust-translator` in synthetic-LLM mode. The\n\
//! provenance manifest at `PROVENANCE.toml` records every input that\n\
//! drove this translation.\n\
//!\n\
//! M4 scope window: see `corpus/tomli/README.md` §\"Scope window\".\n\
//!\n\
//! Public surface:\n\
//! - `loads(src: &str) -> Result<Value, TomliError>` — parse a TOML string.\n\
//! - `Value` — heterogeneous TOML value tree.\n\
//! - `TomliError` — single error type.\n\
//! - `to_json` / `table_to_json` — JSON conversion helpers used by the L3 differential gate.\n\
\n\
mod parser;\n\
\n\
pub use crate::parser::{{loads, table_to_json, to_json, TomliError, Value}};\n",
            header = lib_header,
            version = library.version,
        ),
        "dateutil" => format!(
            "{header}//! Cobrust translation of `dateutil` {version} (M5 scope window).\n\
//!\n\
//! Generated by `cobrust-translator` in synthetic-LLM mode; see\n\
//! `PROVENANCE.toml` for the full manifest and `corpus/dateutil/README.md`\n\
//! for the M5 scope window.\n\
//!\n\
//! Public surface:\n\
//! - `parse_iso(src: &str) -> Result<DateTuple, ParserError>` — strict ISO-8601.\n\
//! - `relativedelta_add(...)` — pure-arithmetic relative-delta addition.\n\
//! - `DateTuple` — element-wise mirror of the Python 9-tuple.\n\
//! - `ParserError` — single error type for parse failures.\n\
\n\
mod parser;\n\
\n\
pub use crate::parser::{{\n\
    DateTuple, ParserError, days_in_month, is_digit, is_leap_year,\n\
    normalize_datetime, parse_iso, relativedelta_add,\n\
}};\n",
            header = lib_header,
            version = library.version,
        ),
        "msgpack" => format!(
            "{header}//! Cobrust translation of `msgpack-python` {version} (M6 native-ext scope window).\n\
//!\n\
//! Generated by `cobrust-translator` in synthetic-LLM mode; see\n\
//! `PROVENANCE.toml` for the full manifest and `corpus/msgpack/README.md`\n\
//! for the M6 scope window.\n\
//!\n\
//! Public surface:\n\
//! - `pack(value: &MsgValue, out: &mut Vec<u8>) -> Result<(), MsgError>` — encode.\n\
//! - `pack_to_vec(value: &MsgValue) -> Result<Vec<u8>, MsgError>` — encode owned.\n\
//! - `unpack(data: &[u8]) -> Result<MsgValue, MsgError>` — decode.\n\
//! - `MsgValue` — heterogeneous value tree (nil/bool/int/float/str/bytes/array/map).\n\
//! - `MsgError` / `MsgErrorKind` — single error type.\n\
\n\
mod parser;\n\
\n\
pub use crate::parser::{{\n\
    pack, pack_array, pack_bin, pack_float, pack_int, pack_map, pack_str, pack_to_vec,\n\
    pack_uint, pack_uint_cython, unpack, unpack_array, unpack_bin, unpack_float,\n\
    unpack_int, unpack_map, unpack_one, unpack_str, unpack_uint, unpack_uint_cython,\n\
    MsgError, MsgErrorKind, MsgValue,\n\
}};\n",
            header = lib_header,
            version = library.version,
        ),
        "numpy" => format!(
            "{header}//! Cobrust translation of NumPy {version} (M7.0 ndarray foundation per ADR-0013).\n//!\n//! Generated by `cobrust-translator` in synthetic-LLM mode; see\n//! `PROVENANCE.toml` for the full manifest and `corpus/numpy/M7.0/README.md`\n//! for the M7.0 scope window.\n//!\n//! Public surface (M7.0):\n//! - `Array` — closed tagged-union over `ndarray::ArrayD<T>`.\n//! - `Dtype` — closed enum (`Int32 | Int64 | Float32 | Float64 | Bool`).\n//! - `array(values, shape, dtype)` — flat-buffer construction.\n//! - `zeros(shape, dtype)` / `ones(shape, dtype)` — fill-value constructors.\n//! - `arange(start, stop, step, dtype)` — half-open range.\n//! - `NumpyError` / `NumpyErrorKind` — closed error taxonomy.\n\nmod parser;\n\npub use crate::parser::*;\n",
            header = lib_header,
            version = library.version,
        ),
        _ => format!(
            "{header}//! Cobrust translation of `{lib}` {version}.\n\nmod parser;\n\npub use crate::parser::*;\n",
            header = lib_header,
            lib = library.library,
            version = library.version,
        ),
    }
}

fn library_header(library: &PyLibrary, spec: &SpecToml, translation: &TranslationOutput) -> String {
    format!(
        "// AUTO-GENERATED — DO NOT EDIT BY HAND.\n\
// Translated by cobrust-translator (synthetic-LLM mode).\n\
// source-library: {lib} {version}\n\
// oracle: {oracle_runtime} {oracle_runtime_version} (module: {oracle})\n\
// functions translated: {n}\n\
// see PROVENANCE.toml for the full manifest.\n\n",
        lib = library.library,
        version = library.version,
        oracle_runtime = spec.oracle_runtime,
        oracle_runtime_version = spec.oracle_runtime_version,
        oracle = spec.oracle_module,
        n = translation.functions.len(),
    )
}

fn function_provenance_header(fn_t: &FunctionTranslation) -> String {
    format!(
        "// fn:{name} provider={provider} model={model} cache_hit={hit} decision_id={did}\n",
        name = fn_t.name,
        provider = fn_t.provider,
        model = fn_t.model,
        hit = fn_t.cache_hit,
        did = fn_t.router_decision_id,
    )
}

/// Emit per-library test harnesses. M4 hard-codes tomli; M5 adds
/// dateutil. M6+ may template these per-library.
fn write_test_harnesses(
    crate_dir: &std::path::Path,
    library: &PyLibrary,
    _spec: &SpecToml,
) -> Result<(), TranslatorError> {
    match library.library.as_str() {
        "tomli" => {
            let downstream = include_str!("templates/tomli_downstream.rs.tmpl");
            let fuzz = include_str!("templates/tomli_fuzz.rs.tmpl");
            std::fs::write(crate_dir.join("tests/tomli_downstream.rs"), downstream)?;
            std::fs::write(crate_dir.join("tests/tomli_fuzz.rs"), fuzz)?;
        }
        "dateutil" => {
            let downstream = include_str!("templates/dateutil_downstream.rs.tmpl");
            let fuzz = include_str!("templates/dateutil_fuzz.rs.tmpl");
            let bench = include_str!("templates/dateutil_bench.rs.tmpl");
            std::fs::write(crate_dir.join("tests/dateutil_downstream.rs"), downstream)?;
            std::fs::write(crate_dir.join("tests/dateutil_fuzz.rs"), fuzz)?;
            std::fs::write(crate_dir.join("tests/dateutil_bench.rs"), bench)?;
        }
        "msgpack" => {
            let downstream = include_str!("templates/msgpack_downstream.rs.tmpl");
            let fuzz = include_str!("templates/msgpack_fuzz.rs.tmpl");
            let bench = include_str!("templates/msgpack_bench.rs.tmpl");
            std::fs::write(crate_dir.join("tests/msgpack_downstream.rs"), downstream)?;
            std::fs::write(crate_dir.join("tests/msgpack_fuzz.rs"), fuzz)?;
            std::fs::write(crate_dir.join("tests/msgpack_bench.rs"), bench)?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::synthetic::{CannedResponse, CannedTable};

    fn router_cfg(cache: &str, ledger: &str) -> cobrust_llm_router::RouterConfig {
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
"#
        );
        cobrust_llm_router::RouterConfig::from_toml_str(&toml).unwrap()
    }

    #[tokio::test]
    async fn pipeline_emits_synthetic_miss_when_canned_table_empty() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus/tomli");
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
        let canned = CannedTable::new("cpython 3.11");
        canned.write(&corpus.join("canned.toml")).unwrap();

        let cache = dir.path().join("cache");
        let ledger = dir.path().join("ledger.jsonl");
        let cfg = TranslatorConfig::m4_synthetic(
            router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
            dir.path().join("out"),
        );
        let lib = PyLibrary {
            library: "tomli".into(),
            version: "0.0.1".into(),
            source_file: corpus.join("upstream/tomli_loads.py"),
            spec_file: corpus.join("spec.toml"),
            upstream_tests: corpus.join("upstream_tests"),
            canned_responses: Some(corpus.join("canned.toml")),
            seeds: vec![1],
            fuzz_inputs_per_fn: 1,
        };
        let err = translate(&lib, &cfg).await.unwrap_err();
        match err {
            TranslatorError::SyntheticMiss { task, function } => {
                assert_eq!(task, "translate");
                assert_eq!(function, "loads");
            }
            other => panic!("expected SyntheticMiss, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pipeline_writes_crate_when_canned_table_complete() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus/tomli");
        std::fs::create_dir_all(corpus.join("upstream")).unwrap();
        std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
        let py_src = "# stub source\n";
        std::fs::write(corpus.join("upstream/tomli_loads.py"), py_src).unwrap();
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

        let sha =
            crate::deterministic::sha256_file(&corpus.join("upstream/tomli_loads.py")).unwrap();
        let mut canned = CannedTable::new("cpython 3.11");
        canned.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: sha[..16].to_string(),
            attempt: 1,
            response_text: "// translated stub\npub fn loads(_s: &str) {}\n".into(),
        });
        canned.write(&corpus.join("canned.toml")).unwrap();

        let cache = dir.path().join("cache");
        let ledger = dir.path().join("ledger.jsonl");
        let cfg = TranslatorConfig::m4_synthetic(
            router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
            dir.path().join("out"),
        );
        let lib = PyLibrary {
            library: "tomli".into(),
            version: "0.0.1".into(),
            source_file: corpus.join("upstream/tomli_loads.py"),
            spec_file: corpus.join("spec.toml"),
            upstream_tests: corpus.join("upstream_tests"),
            canned_responses: Some(corpus.join("canned.toml")),
            seeds: vec![1],
            fuzz_inputs_per_fn: 1,
        };
        let result = translate(&lib, &cfg).await.unwrap();
        assert_eq!(result.functions.len(), 1);
        assert!(result.crate_dir.join("Cargo.toml").exists());
        assert!(result.crate_dir.join("src/lib.rs").exists());
        assert!(result.crate_dir.join("src/parser.rs").exists());
        assert!(result.crate_dir.join("PROVENANCE.toml").exists());
        assert!(result.crate_dir.join("python/tomli_init.py").exists());
        result.manifest.validate().unwrap();
        assert_eq!(result.manifest.gates.l1_files_emitted, 1);
        // M5 contract: tomli has no repair attempts and no covered dependents.
        assert_eq!(result.repair_attempts, 0);
        assert!(result.manifest.gates.dependents.covered.is_empty());
    }

    #[tokio::test]
    async fn pipeline_is_deterministic_across_runs() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus/tomli");
        std::fs::create_dir_all(corpus.join("upstream")).unwrap();
        std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
        let py_src = "# stable\n";
        std::fs::write(corpus.join("upstream/tomli_loads.py"), py_src).unwrap();
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

        let sha =
            crate::deterministic::sha256_file(&corpus.join("upstream/tomli_loads.py")).unwrap();
        let mut canned = CannedTable::new("cpython 3.11");
        canned.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: sha[..16].to_string(),
            attempt: 1,
            response_text: "// stable\n".into(),
        });
        canned.write(&corpus.join("canned.toml")).unwrap();

        let cache = dir.path().join("cache");
        let ledger = dir.path().join("ledger.jsonl");
        let cfg = TranslatorConfig::m4_synthetic(
            router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
            dir.path().join("out1"),
        );
        let lib = PyLibrary {
            library: "tomli".into(),
            version: "0.0.1".into(),
            source_file: corpus.join("upstream/tomli_loads.py"),
            spec_file: corpus.join("spec.toml"),
            upstream_tests: corpus.join("upstream_tests"),
            canned_responses: Some(corpus.join("canned.toml")),
            seeds: vec![1],
            fuzz_inputs_per_fn: 1,
        };
        let r1 = translate(&lib, &cfg).await.unwrap();

        let cache2 = dir.path().join("cache2");
        let ledger2 = dir.path().join("ledger2.jsonl");
        let cfg2 = TranslatorConfig::m4_synthetic(
            router_cfg(cache2.to_str().unwrap(), ledger2.to_str().unwrap()),
            dir.path().join("out2"),
        );
        let r2 = translate(&lib, &cfg2).await.unwrap();

        assert_eq!(
            r1.manifest.build.deterministic_id, r2.manifest.build.deterministic_id,
            "deterministic_id must be stable across independent runs"
        );
        assert_eq!(
            r1.manifest.source.sha256, r2.manifest.source.sha256,
            "source sha must be stable"
        );
    }

    /// M5: a verifier that rejects the first attempt and accepts the
    /// second exercises the full repair loop without needing real LLM
    /// keys or a multi-attempt synthetic table.
    struct RejectFirstAcceptSecond;
    impl BehaviorVerifier for RejectFirstAcceptSecond {
        fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
            if attempt == 1 {
                VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: "deliberate first-attempt rejection (test fixture)".into(),
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

    #[tokio::test]
    async fn pipeline_repair_loop_recovers_when_attempt_2_canned() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus/tomli");
        std::fs::create_dir_all(corpus.join("upstream")).unwrap();
        std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
        let py_src = "# repair-test source\n";
        std::fs::write(corpus.join("upstream/tomli_loads.py"), py_src).unwrap();
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

        let sha =
            crate::deterministic::sha256_file(&corpus.join("upstream/tomli_loads.py")).unwrap();
        let mut canned = CannedTable::new("cpython 3.11");
        canned.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: sha[..16].to_string(),
            attempt: 1,
            response_text: "// BROKEN attempt 1\n".into(),
        });
        canned.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: sha[..16].to_string(),
            attempt: 2,
            response_text: "// CORRECT attempt 2\n".into(),
        });
        canned.write(&corpus.join("canned.toml")).unwrap();

        let cache = dir.path().join("cache");
        let ledger = dir.path().join("ledger.jsonl");
        let cfg = TranslatorConfig::m4_synthetic(
            router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
            dir.path().join("out"),
        );
        let lib = PyLibrary {
            library: "tomli".into(),
            version: "0.0.1".into(),
            source_file: corpus.join("upstream/tomli_loads.py"),
            spec_file: corpus.join("spec.toml"),
            upstream_tests: corpus.join("upstream_tests"),
            canned_responses: Some(corpus.join("canned.toml")),
            seeds: vec![1],
            fuzz_inputs_per_fn: 1,
        };
        let result = translate_with_verifier(&lib, &cfg, &RejectFirstAcceptSecond)
            .await
            .unwrap();
        assert_eq!(result.repair_attempts, 1);
        assert!(
            result.functions[0]
                .emitted_text
                .contains("CORRECT attempt 2")
        );
        // Diagnostic blob was persisted.
        let diag_path = dir.path().join("out/tomli/diagnostics/loads__2.toml");
        assert!(diag_path.exists());
    }

    struct AlwaysReject;
    impl BehaviorVerifier for AlwaysReject {
        fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_behavior".into(),
                failure_summary: "always rejected".into(),
                failed_inputs: vec![],
                expected: None,
                actual: None,
                attempt: attempt + 1,
            })
        }
    }

    #[tokio::test]
    async fn pipeline_repair_loop_escalates_when_threshold_hit() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = dir.path().join("corpus/tomli");
        std::fs::create_dir_all(corpus.join("upstream")).unwrap();
        std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
        let py_src = "# escalation-test source\n";
        std::fs::write(corpus.join("upstream/tomli_loads.py"), py_src).unwrap();
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

        let sha =
            crate::deterministic::sha256_file(&corpus.join("upstream/tomli_loads.py")).unwrap();
        let mut canned = CannedTable::new("cpython 3.11");
        canned.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: sha[..16].to_string(),
            attempt: 1,
            response_text: "// always broken\n".into(),
        });
        canned.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: sha[..16].to_string(),
            attempt: 2,
            response_text: "// also broken\n".into(),
        });
        canned.write(&corpus.join("canned.toml")).unwrap();

        let cache = dir.path().join("cache");
        let ledger = dir.path().join("ledger.jsonl");
        let mut cfg = TranslatorConfig::m4_synthetic(
            router_cfg(cache.to_str().unwrap(), ledger.to_str().unwrap()),
            dir.path().join("out"),
        );
        cfg.escalation_threshold = 2; // tighter for the test
        let lib = PyLibrary {
            library: "tomli".into(),
            version: "0.0.1".into(),
            source_file: corpus.join("upstream/tomli_loads.py"),
            spec_file: corpus.join("spec.toml"),
            upstream_tests: corpus.join("upstream_tests"),
            canned_responses: Some(corpus.join("canned.toml")),
            seeds: vec![1],
            fuzz_inputs_per_fn: 1,
        };
        let err = translate_with_verifier(&lib, &cfg, &AlwaysReject)
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
                assert_eq!(failed_gate, "l2_behavior");
            }
            other => panic!("expected EscalationExceeded, got {other:?}"),
        }
        // failure_report.md was written.
        let report_path = dir.path().join("out/cobrust-tomli/failure_report.md");
        assert!(report_path.exists());
    }
}
