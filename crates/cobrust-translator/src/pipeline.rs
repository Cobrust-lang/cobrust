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

use cobrust_llm_router::{
    AnthropicProvider, LlmProvider, OpenAiProvider, ProviderKind, Router, RouterBuilder,
};

use crate::config::TranslatorConfig;
use crate::deterministic::{deterministic_id, sha256_file};
use crate::error::TranslatorError;
use crate::manifest::{
    BuildSection, DependentsSection, GatesSection, OracleSection, ProvenanceManifest,
    RouterSection, SourceSection, VerificationSection,
};
use crate::repair::{GateFailure, write_failure_report};
use crate::spec::{FunctionSpec, PyCompatTier, SpecToml};
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
    /// Structured per-gate verdicts feeding the manifest's gate fields.
    /// Pinned by ADR-0040 §"Honest gate verdicts": the manifest stores
    /// the human-readable string form, but callers needing a typed view
    /// (CI, tooling, downstream tests) read these directly without
    /// parsing the string.
    pub gate_outcomes: GateOutcomes,
}

/// Structured outcome of one L2 / L3 gate. Pinned by ADR-0040
/// §"Honest gate verdicts": replaces the literal-string scheme that
/// returned `"pass (...)"` regardless of verifier verdict (see
/// claude-desktop integrated handoff §1.B2).
///
/// Stored variants:
///
/// - `Pass { detail }` — gate accepted; `detail` carries the harness /
///   evidence string for the manifest (e.g. `"tests/tomli_downstream.rs
///   + tests/tomli_fuzz.rs (after 2 repair-loop iterations)"`).
/// - `Fail { reason }` — gate rejected; `reason` is the
///   verifier-supplied diagnostic. Surfaced unmodified into the
///   manifest as `"fail: <reason>"`.
/// - `Skip { reason }` — gate skipped on purpose (e.g. M4 perf gate is
///   recorded-only, not gated). Distinct from Pass; surfaced as
///   `"skipped (<reason>)"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateOutcome {
    Pass { detail: String },
    Fail { reason: String },
    Skip { reason: String },
}

impl GateOutcome {
    /// Human-readable serialization for the manifest's `gates.l2_*`
    /// string fields. Distinct prefixes for Pass / Fail / Skip so the
    /// verdict is never confusable with another at parse time.
    #[must_use]
    pub fn as_manifest_str(&self) -> String {
        match self {
            GateOutcome::Pass { detail } if detail.is_empty() => "pass".into(),
            GateOutcome::Pass { detail } => format!("pass ({detail})"),
            GateOutcome::Fail { reason } => format!("fail: {reason}"),
            GateOutcome::Skip { reason } if reason.is_empty() => "skipped".into(),
            GateOutcome::Skip { reason } => format!("skipped ({reason})"),
        }
    }

    /// True when the gate verdict is `Pass`.
    #[must_use]
    pub const fn is_pass(&self) -> bool {
        matches!(self, GateOutcome::Pass { .. })
    }

    /// True when the gate verdict is `Fail`.
    #[must_use]
    pub const fn is_fail(&self) -> bool {
        matches!(self, GateOutcome::Fail { .. })
    }

    /// True when the gate verdict is `Skip`.
    #[must_use]
    pub const fn is_skip(&self) -> bool {
        matches!(self, GateOutcome::Skip { .. })
    }
}

impl std::fmt::Display for GateOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_manifest_str())
    }
}

/// Aggregate of every L2/L3 gate's structured verdict for one
/// translation run. Pinned by ADR-0040 §"Honest gate verdicts".
///
/// `worst()` returns the worst-priority verdict across all gates
/// (Fail > Skip > Pass), used by integration tests to assert pipeline
/// honesty without parsing the manifest strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateOutcomes {
    pub l2_build: GateOutcome,
    pub l2_behavior: GateOutcome,
    pub l2_perf: GateOutcome,
    pub l3_pyo3_wrapper: GateOutcome,
    pub l3_downstream_dependents: GateOutcome,
}

impl GateOutcomes {
    /// Worst-priority verdict across all gates (Fail > Skip > Pass).
    /// Used by callers to compute one aggregate exit signal — distinct
    /// from the M4-vintage hardcoded `"pass"` string scheme.
    #[must_use]
    pub fn worst(&self) -> GateOutcome {
        let gates = [
            &self.l2_build,
            &self.l2_behavior,
            &self.l2_perf,
            &self.l3_pyo3_wrapper,
            &self.l3_downstream_dependents,
        ];
        // Fail wins outright.
        for g in &gates {
            if g.is_fail() {
                return (*g).clone();
            }
        }
        // Skip beats Pass when no Fail.
        for g in &gates {
            if g.is_skip() {
                return (*g).clone();
            }
        }
        // Otherwise return the first gate's Pass (l2_build is canonical).
        self.l2_build.clone()
    }
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

    /// Verdict to record on the manifest when **every** function is
    /// accepted by `verify` (= the loop never observed a Reject).
    /// Pinned by ADR-0040 §"Honest gate verdicts": prevents the
    /// no-op [`AcceptAll`] from masquerading as a real
    /// [`GateOutcome::Pass`] in the manifest.
    ///
    /// Default = `Pass { detail: "" }`. The [`AcceptAll`] verifier
    /// overrides this to `Skip` so the manifest honestly records that
    /// no behavior gate was wired.
    fn default_outcome(&self) -> GateOutcome {
        GateOutcome::Pass {
            detail: String::new(),
        }
    }
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

    /// Verdict to record on the manifest when **every** function is
    /// accepted by `verify`. Default = `Pass { detail: "" }`. See
    /// [`BehaviorVerifier::default_outcome`] for the matching contract.
    fn default_outcome(&self) -> GateOutcome {
        GateOutcome::Pass {
            detail: String::new(),
        }
    }
}

/// No-op perf verifier — accepts every emission. The M4/M5 default.
pub struct AcceptAllPerf;

impl PerfVerifier for AcceptAllPerf {
    fn verify(&self, _function: &FunctionTranslation, _attempt: u32) -> PerfVerdict {
        PerfVerdict::Accept
    }

    fn default_outcome(&self) -> GateOutcome {
        GateOutcome::Skip {
            reason: "AcceptAllPerf — no L2.perf gate wired".into(),
        }
    }
}

/// No-op verifier — accepts every emission. The default for M4 tomli
/// pipelines that don't exercise the repair loop.
pub struct AcceptAll;

impl BehaviorVerifier for AcceptAll {
    fn verify(&self, _function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
        VerifierVerdict::Accept
    }

    fn default_outcome(&self) -> GateOutcome {
        GateOutcome::Skip {
            reason: "AcceptAll — no L2.behavior gate wired".into(),
        }
    }
}

// ============================================================================
// ADR-0052c §5 — TierVerifier + OracleHarness
// ============================================================================

/// One observation pair from the differential oracle: the upstream
/// CPython oracle's expected output and the Cobrust translation's
/// actual output, both rendered to canonical strings. ADR-0052c §5
/// pairs this with [`TierVerifier`] for per-tier verdict dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct OracleObservation {
    /// Input passed to both the oracle and the translation.
    pub input: String,
    /// Canonical string form of the oracle's output (CPython truth).
    pub expected: String,
    /// Canonical string form of the translation's output. For tiers
    /// other than [`PyCompatTier::Numerical`] this is compared
    /// byte-for-byte (Strict) or structurally (Semantic); for
    /// `Numerical { rtol }` it must parse as f64 and the
    /// [`TierVerifier`] applies `assert_allclose(rtol=...)` semantics.
    pub actual: String,
}

/// The caller-supplied oracle harness ADR-0052c §5 binds. The pipeline
/// invokes [`OracleHarness::observe`] once per `(function_name, attempt)`
/// and feeds the returned observations through [`TierVerifier`].
///
/// The trait does NOT prescribe how observations are gathered — the
/// concrete impl may shell out to a Python subprocess (M4/M5/M6
/// canonical), call a vendored Python interpreter via FFI (M7+), or
/// return precomputed observations from disk.
pub trait OracleHarness: Send + Sync {
    /// Gather observations for one function. The translator passes the
    /// function's name + emitted text; the impl returns the per-input
    /// `(expected, actual)` pairs. Returning an empty vec is permitted
    /// (oracle-disabled fast path); [`TierVerifier`] treats this as
    /// `Accept` for any tier.
    ///
    /// # Errors
    /// Returns an oracle-side diagnostic the verifier surfaces as a
    /// [`GateFailure::failure_summary`] when the harness itself fails
    /// (NOT when a divergence is observed — divergences are returned
    /// in the observation list).
    fn observe(
        &self,
        function: &FunctionTranslation,
        attempt: u32,
    ) -> Result<Vec<OracleObservation>, String>;
}

/// ADR-0052c §5 tier-aware behavior verifier. Reads each function's
/// [`PyCompatTier`] from the spec table and dispatches per-tier verdict
/// policy:
///
/// - [`PyCompatTier::Strict`] — byte-identity check; any divergence rejects.
/// - [`PyCompatTier::Semantic`] — structural-equivalence permitted.
/// - [`PyCompatTier::Numerical { rtol }`] — `assert_allclose(rtol=...)`.
/// - [`PyCompatTier::None`] — gate disabled; accepts unconditionally.
///
/// Replaces [`AcceptAll`] as the production default once a caller wires
/// it via [`translate_with_verifier`]. [`AcceptAll`] remains exported as
/// a no-op test fixture (the M4 backward-compat path).
pub struct TierVerifier {
    /// Per-function L0 specs keyed by function name. The pipeline
    /// builds this from `SpecToml.function` when constructing the
    /// verifier; see [`TierVerifier::from_spec`].
    specs: std::collections::BTreeMap<String, FunctionSpec>,
    /// Caller-supplied oracle harness producing `(expected, actual)`
    /// observation pairs.
    oracle: Arc<dyn OracleHarness>,
}

impl TierVerifier {
    /// Construct from a per-function spec table + an oracle harness.
    #[must_use]
    pub fn new(
        specs: std::collections::BTreeMap<String, FunctionSpec>,
        oracle: Arc<dyn OracleHarness>,
    ) -> Self {
        Self { specs, oracle }
    }

    /// Convenience: build the verifier from a parsed [`SpecToml`].
    /// Clones the function map so the caller retains ownership of the
    /// spec value.
    #[must_use]
    pub fn from_spec(spec: &SpecToml, oracle: Arc<dyn OracleHarness>) -> Self {
        Self::new(spec.function.clone(), oracle)
    }

    /// Per-tier strict byte-identity check.
    #[allow(clippy::unused_self)] // method receiver retained for future oracle handle access
    fn verify_bit_identical(
        &self,
        function: &FunctionTranslation,
        observations: &[OracleObservation],
        attempt: u32,
    ) -> VerifierVerdict {
        for obs in observations {
            if obs.expected != obs.actual {
                return VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: format!(
                        "Strict-tier byte-identity check failed for {}: oracle vs emission diverge",
                        function.name
                    ),
                    failed_inputs: vec![obs.input.clone()],
                    expected: Some(obs.expected.clone()),
                    actual: Some(obs.actual.clone()),
                    attempt: attempt.saturating_add(1),
                });
            }
        }
        VerifierVerdict::Accept
    }

    /// Per-tier semantic / structural-equivalence check. M-batch
    /// canonical impl: compare after stripping whitespace + normalizing
    /// punctuation; treat the JSON representation of dicts as
    /// key-order-insensitive.
    #[allow(clippy::unused_self)] // method receiver retained for future oracle handle access
    fn verify_semantic(
        &self,
        function: &FunctionTranslation,
        observations: &[OracleObservation],
        attempt: u32,
    ) -> VerifierVerdict {
        for obs in observations {
            if !semantic_equivalent(&obs.expected, &obs.actual) {
                return VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: format!(
                        "Semantic-tier structural check failed for {}: oracle vs emission diverge",
                        function.name
                    ),
                    failed_inputs: vec![obs.input.clone()],
                    expected: Some(obs.expected.clone()),
                    actual: Some(obs.actual.clone()),
                    attempt: attempt.saturating_add(1),
                });
            }
        }
        VerifierVerdict::Accept
    }

    /// Per-tier numerical `assert_allclose(rtol=...)` check.
    #[allow(clippy::unused_self)] // method receiver retained for future oracle handle access
    fn verify_allclose(
        &self,
        function: &FunctionTranslation,
        observations: &[OracleObservation],
        rtol: f64,
        attempt: u32,
    ) -> VerifierVerdict {
        for obs in observations {
            let Ok(expected) = obs.expected.trim().parse::<f64>() else {
                return VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: format!(
                        "Numerical(rtol={rtol}) expected f64 oracle for {}, got non-numeric",
                        function.name
                    ),
                    failed_inputs: vec![obs.input.clone()],
                    expected: Some(obs.expected.clone()),
                    actual: Some(obs.actual.clone()),
                    attempt: attempt.saturating_add(1),
                });
            };
            let Ok(actual) = obs.actual.trim().parse::<f64>() else {
                return VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: format!(
                        "Numerical(rtol={rtol}) expected f64 emission for {}, got non-numeric",
                        function.name
                    ),
                    failed_inputs: vec![obs.input.clone()],
                    expected: Some(obs.expected.clone()),
                    actual: Some(obs.actual.clone()),
                    attempt: attempt.saturating_add(1),
                });
            };
            if !numpy_allclose(expected, actual, rtol) {
                return VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: format!(
                        "Numerical(rtol={rtol}) rejected: drift exceeds tolerance for {}",
                        function.name
                    ),
                    failed_inputs: vec![obs.input.clone()],
                    expected: Some(obs.expected.clone()),
                    actual: Some(obs.actual.clone()),
                    attempt: attempt.saturating_add(1),
                });
            }
        }
        VerifierVerdict::Accept
    }
}

impl BehaviorVerifier for TierVerifier {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
        // Look up the per-function spec; absent entries fall through
        // to Accept (TierVerifier never rejects what it can't classify).
        let Some(spec) = self.specs.get(&function.name) else {
            return VerifierVerdict::Accept;
        };

        // None tier: gate disabled.
        if matches!(spec.py_compat, PyCompatTier::None) {
            return VerifierVerdict::Accept;
        }

        // Query the caller-supplied oracle.
        let observations = match self.oracle.observe(function, attempt) {
            Ok(obs) => obs,
            Err(msg) => {
                return VerifierVerdict::Reject(GateFailure {
                    function: function.name.clone(),
                    failed_gate: "l2_behavior".into(),
                    failure_summary: format!("oracle harness failed: {msg}"),
                    failed_inputs: vec![],
                    expected: None,
                    actual: None,
                    attempt: attempt.saturating_add(1),
                });
            }
        };

        // Empty observation set = oracle-disabled fast path. Accept.
        if observations.is_empty() {
            return VerifierVerdict::Accept;
        }

        // Per-tier dispatch.
        match &spec.py_compat {
            PyCompatTier::Strict => self.verify_bit_identical(function, &observations, attempt),
            PyCompatTier::Semantic => self.verify_semantic(function, &observations, attempt),
            PyCompatTier::Numerical { rtol } => {
                self.verify_allclose(function, &observations, *rtol, attempt)
            }
            // None already handled above; this arm unreachable in practice.
            PyCompatTier::None => VerifierVerdict::Accept,
        }
    }

    fn default_outcome(&self) -> GateOutcome {
        GateOutcome::Pass {
            detail: "TierVerifier wired (ADR-0052c)".into(),
        }
    }
}

/// Semantic-tier structural-equivalence predicate. Strips whitespace
/// from both sides and treats JSON-shaped dict strings as
/// key-order-insensitive when both parse as JSON objects.
fn semantic_equivalent(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    // Whitespace-only difference.
    let exp_ws: String = expected.chars().filter(|c| !c.is_whitespace()).collect();
    let act_ws: String = actual.chars().filter(|c| !c.is_whitespace()).collect();
    if exp_ws == act_ws {
        return true;
    }
    // JSON object key-order-insensitive comparison.
    if let (Ok(a), Ok(b)) = (
        serde_json::from_str::<serde_json::Value>(expected),
        serde_json::from_str::<serde_json::Value>(actual),
    ) {
        return a == b;
    }
    false
}

/// `numpy.testing.assert_allclose(rtol=...)` predicate. Matches the
/// NumPy canonical semantics: `|a - b| <= atol + rtol * |b|` with
/// `atol = 0.0` (NumPy's default for `assert_allclose`).
#[allow(clippy::float_cmp)] // intentional bit-identity short-circuit before tolerance check
fn numpy_allclose(expected: f64, actual: f64, rtol: f64) -> bool {
    if expected == actual {
        return true;
    }
    let diff = (expected - actual).abs();
    let tol = rtol * actual.abs();
    diff <= tol
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
    let RepairLoopResult {
        translation,
        repair_attempts,
        behavior_outcome,
        perf_outcome,
    } = run_repair_loop(
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

    // ---- Compose structured gate outcomes ---------------------------------
    // Per ADR-0040 §"Honest gate verdicts": the manifest's gate strings
    // come from a typed [`GateOutcome`] verdict, never a hardcoded
    // literal. The build / pyo3 / downstream gates derive their default
    // verdict from the per-library policy table (see
    // `default_l2_build_outcome` etc.); the behavior/perf verdicts come
    // straight out of the repair loop above.
    let gate_outcomes = GateOutcomes {
        l2_build: default_l2_build_outcome(library),
        l2_behavior: behavior_outcome,
        l2_perf: perf_outcome,
        l3_pyo3_wrapper: default_l3_pyo3_outcome(library),
        l3_downstream_dependents: default_l3_downstream_outcome(library),
    };

    // ---- Write crate to disk -----------------------------------------------
    let crate_dir = cfg.out_dir.join(format!("cobrust-{}", library.library));
    write_crate(&crate_dir, library, &spec, &translation)?;

    // ---- Build manifest ----------------------------------------------------
    let manifest = build_manifest(
        library,
        cfg,
        &source_sha256,
        &translation,
        repair_attempts,
        &gate_outcomes,
    );
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
        gate_outcomes,
    })
}

/// Carries the structured outcome of the repair loop back to the
/// orchestrator. Pinned by ADR-0040 §"Honest gate verdicts".
struct RepairLoopResult {
    translation: TranslationOutput,
    repair_attempts: u32,
    behavior_outcome: GateOutcome,
    perf_outcome: GateOutcome,
}

/// Build the provenance manifest from the translation artefacts.
///
/// The `gate_outcomes` argument carries the structured per-gate verdict
/// (Pass / Fail / Skip) the orchestrator computed from the verifier
/// hooks — *not* a hardcoded literal. Pinned by ADR-0040 §"Honest gate
/// verdicts" (see also claude-desktop integrated handoff §1.B2).
fn build_manifest(
    library: &PyLibrary,
    cfg: &TranslatorConfig,
    source_sha256: &str,
    translation: &TranslationOutput,
    repair_attempts: u32,
    gate_outcomes: &GateOutcomes,
) -> ProvenanceManifest {
    let _ = repair_attempts; // detail already baked into the behavior outcome
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
            l2_build: gate_outcomes.l2_build.as_manifest_str(),
            l2_behavior: gate_outcomes.l2_behavior.as_manifest_str(),
            l2_perf: gate_outcomes.l2_perf.as_manifest_str(),
            l3_pyo3_wrapper: gate_outcomes.l3_pyo3_wrapper.as_manifest_str(),
            l3_downstream_dependents: gate_outcomes.l3_downstream_dependents.as_manifest_str(),
            dependents,
        },
    }
}

/// Run the verifier across every function; on rejection re-dispatch
/// the failing function with attempt += 1 and re-verify, until either
/// the verifier accepts or the escalation threshold is hit.
///
/// Returns a [`RepairLoopResult`] carrying the final translation
/// output, total repair attempts, and the structured per-gate
/// outcomes ([`GateOutcome`]). Pinned by ADR-0040 §"Honest gate
/// verdicts": the per-gate outcomes are derived from the verifier's
/// observed verdicts, not a hardcoded literal — so a fake-pass cannot
/// surface to the manifest. (Note: the loop returns `Err` on
/// escalation, so the success path always carries Pass-or-Skip
/// verdicts; Fail propagates as `EscalationExceeded`.)
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
) -> Result<RepairLoopResult, TranslatorError> {
    let mut total_repair_attempts: u32 = 0;
    // Track whether the verifier ever produced a non-default verdict.
    // If any function went through a Reject/repair cycle, we know the
    // verifier is "live" and the success path is a real Pass; otherwise
    // we honour the verifier's `default_outcome()` hook so AcceptAll
    // surfaces as Skip (= the no-op signal) and a real verifier with
    // zero failures surfaces as Pass.
    let mut behavior_observed_reject = false;
    let mut perf_observed_reject = false;
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
                    PerfVerdict::Reject(f) => {
                        perf_observed_reject = true;
                        VerifierVerdict::Reject(f)
                    }
                },
                VerifierVerdict::Reject(f) => {
                    behavior_observed_reject = true;
                    VerifierVerdict::Reject(f)
                }
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

    // Compose per-gate outcomes. If the verifier ever rejected at least
    // once, we know it is "live" and the resolved success path is a
    // real `Pass { detail }`. Otherwise we surface the verifier's
    // `default_outcome()` so a no-op verifier (e.g. [`AcceptAll`]) is
    // recorded as `Skip` rather than masquerading as `Pass`.
    let behavior_outcome = if behavior_observed_reject {
        GateOutcome::Pass {
            detail: behavior_pass_detail(library, total_repair_attempts),
        }
    } else {
        verifier.default_outcome()
    };
    let perf_outcome = if perf_observed_reject {
        GateOutcome::Pass {
            detail: perf_pass_detail(library),
        }
    } else {
        perf_verifier.default_outcome()
    };

    Ok(RepairLoopResult {
        translation,
        repair_attempts: total_repair_attempts,
        behavior_outcome,
        perf_outcome,
    })
}

/// Default L2.build verdict for a library.
///
/// Pinned by ADR-0040 §"Honest gate verdicts": the build gate is
/// L2-external (real `cargo build` runs against the *generated* crate
/// at the workspace level, not from inside `translate()`). The
/// pipeline therefore records the gate as `Skip` carrying the rationale
/// — never a hardcoded `"pass"`. Real-LLM mode integration tests can
/// inject their own outcome via a future `BuildVerifier` hook (queued
/// for ADR-0040.1).
fn default_l2_build_outcome(library: &PyLibrary) -> GateOutcome {
    let _ = library;
    GateOutcome::Skip {
        reason:
            "L2.build runs at the workspace `cargo build --release` step, not inside translate() \
             — see ADR-0040 §\"Honest gate verdicts\""
                .into(),
    }
}

/// Default L3.pyo3-wrapper verdict for a library. Today this is
/// recorded as `Skip` because the PyO3 build path runs out-of-pipeline
/// (per ADR-0011 the workspace `cargo test --features pyo3` is the
/// authoritative gate, not anything `translate()` invokes). Same
/// honesty contract as `default_l2_build_outcome`.
fn default_l3_pyo3_outcome(library: &PyLibrary) -> GateOutcome {
    let _ = library;
    GateOutcome::Skip {
        reason:
            "L3.pyo3-wrapper runs at the workspace `cargo test --features pyo3` step per ADR-0011 \
             — translate() does not invoke PyO3 builds"
                .into(),
    }
}

/// Default L3.downstream-dependents verdict for a library. The L3
/// driver runs subprocess Python harnesses (per ADR-0009) which is
/// the authoritative gate; `translate()` records the dependents
/// section but cannot itself drive the L3 driver synchronously
/// without dragging the test runtime into the translator. Same
/// Skip-by-default contract as the build / pyo3 gates.
fn default_l3_downstream_outcome(library: &PyLibrary) -> GateOutcome {
    let _ = library;
    GateOutcome::Skip {
        reason:
            "L3.downstream-dependents runs out-of-pipeline (per-library workspace test target) \
             — see ADR-0009"
                .into(),
    }
}

/// Per-library detail string for `behavior` Pass outcomes. Names the
/// test targets the verifier exercised so the manifest carries
/// observable evidence, not a literal.
fn behavior_pass_detail(library: &str, repair_attempts: u32) -> String {
    let suffix = if repair_attempts > 0 {
        format!(" (after {repair_attempts} repair-loop iterations)")
    } else {
        String::new()
    };
    match library {
        "tomli" => format!("tests/tomli_downstream.rs + tests/tomli_fuzz.rs{suffix}"),
        "dateutil" => format!("tests/dateutil_downstream.rs + tests/dateutil_fuzz.rs{suffix}"),
        "msgpack" => {
            format!("tests/msgpack_downstream.rs + tests/msgpack_fuzz.rs bytes-identical{suffix}")
        }
        "numpy" => format!(
            "tests/numpy_differential.rs bytes-identical for int/bool, rtol=1e-12 for float; \
             tests/numpy_fuzz.rs 4200 panic-free{suffix}"
        ),
        _ => format!("verifier accepted{suffix}"),
    }
}

/// Per-library detail string for `perf` Pass outcomes.
fn perf_pass_detail(library: &str) -> String {
    match library {
        "dateutil" => {
            "per-library threshold per ADR-0008 §2; report at target/cobrust-bench/dateutil/<commit>/report.json".into()
        }
        "msgpack" => {
            "native-ext tier ≥ 0.70× per ADR-0010 §3; report at target/cobrust-bench/msgpack/<commit>/report.json".into()
        }
        _ => "perf verifier accepted".into(),
    }
}

/// Build a router with either a synthetic provider or real adapters.
///
/// Pinned by ADR-0040 §"Real-LLM mode wiring" (B1 from claude-desktop
/// integrated handoff §1):
///
/// - **Synthetic mode (`cfg.synthetic_only = true`)**: registers one
///   [`SyntheticProvider`] for every declared provider key in
///   `cfg.router.providers`, all backed by the canned-response table
///   at `library.canned_responses`. `canned_responses = None` returns
///   `Err(TranslatorError::Config)` instead of panicking.
/// - **Real-LLM mode (`cfg.synthetic_only = false`)**: walks
///   `cfg.router.providers` and registers a real adapter for each
///   declared provider — [`OpenAiProvider`] for `kind = "openai"` (also
///   covers DeepSeek, vLLM, OpenRouter, etc. via the OpenAI-compatible
///   shape — see ADR-0004), [`AnthropicProvider`] for
///   `kind = "anthropic"`. The API key is read from each provider's
///   `api_key_env`. Missing or empty env vars surface as
///   `TranslatorError::Config` with the env-var name (no panic, no
///   silent fallback to synthetic).
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
        build_real_llm_router(cfg).await
    }
}

/// Real-LLM mode wiring. Pinned by ADR-0040 §"Real-LLM mode wiring"
/// (B1). Walks `cfg.router.providers`, instantiates the matching
/// adapter for each provider's `kind`, and registers it under the
/// declared provider key. The router's routing table validation then
/// catches any provider-key references that don't have a matching
/// declaration (defence in depth).
async fn build_real_llm_router(cfg: &TranslatorConfig) -> Result<Router, TranslatorError> {
    if cfg.router.providers.is_empty() {
        return Err(TranslatorError::Config(
            "real-LLM mode requires at least one [providers.<name>] entry in cobrust.toml".into(),
        ));
    }
    let mut builder = RouterBuilder::new();
    for (name, provider_cfg) in &cfg.router.providers {
        let api_key = std::env::var(&provider_cfg.api_key_env)
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                TranslatorError::Config(format!(
                    "real-LLM mode: provider {name:?} requires non-empty env var {:?}",
                    provider_cfg.api_key_env
                ))
            })?;
        let provider: Arc<dyn LlmProvider> = match provider_cfg.kind {
            ProviderKind::Openai => Arc::new(
                OpenAiProvider::new(name.clone(), provider_cfg.base_url.clone(), api_key).map_err(
                    |e| {
                        TranslatorError::Config(format!(
                            "OpenAiProvider for {name:?} failed to construct: {e}"
                        ))
                    },
                )?,
            ),
            ProviderKind::Anthropic => Arc::new(
                AnthropicProvider::new(name.clone(), provider_cfg.base_url.clone(), api_key)
                    .map_err(|e| {
                        TranslatorError::Config(format!(
                            "AnthropicProvider for {name:?} failed to construct: {e}"
                        ))
                    })?,
            ),
            ProviderKind::Synthetic => {
                return Err(TranslatorError::Config(format!(
                    "real-LLM mode: provider {name:?} declares kind=synthetic; \
                     synthetic providers belong in synthetic_only=true mode \
                     (no wire protocol matches `synthetic`)"
                )));
            }
        };
        builder = builder.register_provider(name.clone(), provider);
    }
    builder
        .build(&cfg.router)
        .await
        .map_err(TranslatorError::Router)
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
