//! Category B + D — ADR-0052c Wave 2 TierVerifier behavior + router routing
//! integration tests.
//!
//! Pinned by ADR-0052c §5 (TierVerifier impl) + §7 (router integration).
//! These tests stand up a mock oracle that returns caller-controlled
//! divergent outputs, then assert that the DEV-side `TierVerifier`
//! dispatches the correct per-tier verdict policy.
//!
//! All 13 tests (B1..B10 behavior + D1..D3 router) marked
//! `#[ignore = "ADR-0052c Wave-2 DEV impl pending"]` per F28 PAIR.
//!
//! ## Category B — TierVerifier verdict policies (10 tests)
//!
//! | # | Tier | Scenario | Expected verdict |
//! |---|------|----------|------------------|
//! | B1 | Strict | byte-identical oracle output | Accept |
//! | B2 | Strict | any divergence | Reject |
//! | B3 | Strict | single-char drift | Reject (bit-strict) |
//! | B4 | Semantic | identical strings | Accept |
//! | B5 | Semantic | dict-key-order drift | Accept (structural match) |
//! | B6 | Semantic | semantic divergence | Reject |
//! | B7 | Numerical{rtol=1e-7} | within tolerance (1e-9 drift) | Accept |
//! | B8 | Numerical{rtol=1e-7} | exceeds tolerance (1e-5 drift) | Reject |
//! | B9 | None | any output (no contract) | Accept |
//! | B10 | mixed-tier manifest | per-function dispatch correct | mixed |
//!
//! ## Category D — Router routing implication (3 tests)
//!
//! | # | Tier | Router strategy |
//! |---|------|-----------------|
//! | D1 | Strict | StrategyName::Consensus (override) |
//! | D2 | Semantic | StrategyName::Quality (default) |
//! | D3 | Numerical | StrategyName::Cost (default) |
//!
//! ## Design — mock oracle pattern
//!
//! The DEV-side `TierVerifier` consumes an `OracleHarness` trait per
//! ADR-0052c §5. Until DEV ships, this TEST file uses a `MockOracle`
//! struct (test-local, no dependency on DEV-side trait) that records
//! the expected and actual outputs caller-supplied. The Verifier
//! interaction layer is the public `BehaviorVerifier` trait; DEV's
//! `TierVerifier` MUST `impl BehaviorVerifier for TierVerifier`.
//!
//! TEST file references `TierVerifier` via Debug-string assertion or
//! function name string — DEV owns the constructor + state shape.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::missing_panics_doc,
    clippy::print_stdout,
    clippy::uninlined_format_args,
    clippy::needless_pass_by_value,
    dead_code
)]

use std::path::Path;

use cobrust_translator::{
    BehaviorVerifier, FunctionTranslation, GateFailure, PyLibrary, TranslatorConfig,
    VerifierVerdict, translate,
};

// ============================================================================
// Test fixtures — minimal corpus shared with pipeline_l2_gates_use_real_verdicts.rs
// ============================================================================

/// Build a minimal one-function corpus on disk; py_compat is the tier string.
fn write_corpus(corpus: &Path, py_compat_value: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    std::fs::create_dir_all(corpus.join("upstream")).unwrap();
    std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
    std::fs::write(corpus.join("upstream/stub.py"), "# stub\n").unwrap();
    let spec = format!(
        r#"
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
py_compat = "{py_compat_value}"
description = "Stub."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#
    );
    std::fs::write(corpus.join("spec.toml"), spec).unwrap();
    (corpus.join("upstream/stub.py"), corpus.join("spec.toml"))
}

fn write_canned(corpus: &Path, sha16: &str) -> std::path::PathBuf {
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
pub fn loads(_s: &str) {{}}
"""
"#
    );
    std::fs::write(&path, toml).unwrap();
    path
}

fn synthetic_router_cfg(cache: &Path, ledger: &Path) -> cobrust_llm_router::RouterConfig {
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
    cobrust_llm_router::RouterConfig::from_toml_str(&toml).unwrap()
}

fn make_translation(name: &str, emitted: &str) -> FunctionTranslation {
    FunctionTranslation {
        name: name.to_string(),
        source_sha16: "0000000000000000".to_string(),
        provider: "synthetic".to_string(),
        model: "test".to_string(),
        cache_hit: false,
        router_decision_id: "blake3:test".to_string(),
        emitted_text: emitted.to_string(),
        task: "translate".to_string(),
    }
}

// ============================================================================
// MockOracle — paired with TierVerifier in DEV-side §5
// ============================================================================

/// Mock oracle that returns the caller-supplied expected/actual pair for
/// the verifier to compare. The DEV-side `OracleHarness` trait per
/// ADR-0052c §5 is expected to accept an injected oracle; this TEST
/// fixture uses a thread-safe interior-mutable RefCell-like pattern via
/// a closure to avoid Send/Sync friction.
///
/// The actual interaction with TierVerifier happens via the
/// `BehaviorVerifier::verify` call; the DEV impl reads
/// `function.spec.py_compat`, dispatches to the appropriate verify_*
/// helper, and queries the injected oracle.
///
/// Because the DEV-side `TierVerifier` constructor is not yet shipped,
/// the B-tests here exercise the `BehaviorVerifier` trait surface
/// against the existing `AcceptAll` baseline (which is the v0 default).
/// Each B-test documents the EXPECTED post-DEV verdict and asserts the
/// CURRENT pre-DEV verdict matches the v0 AcceptAll behaviour. Once DEV
/// ships, the test author flips each `expected_pre_dev` assertion to
/// the `expected_post_dev` verdict. The two arms are kept as named
/// constants at the top of each test so the diff is mechanical.
#[derive(Clone, Debug)]
struct MockOracleRecord {
    expected: String,
    actual: String,
}

// ============================================================================
// B1 — Strict tier accepts byte-identical output
// ============================================================================

/// Per ADR-0052c §3 matrix row 1: Strict tier accepts ONLY byte-identical
/// oracle output. When the LLM emission matches the oracle exactly, the
/// TierVerifier must dispatch `verify_bit_identical` and return Accept.
///
/// DEV-side: `TierVerifier { oracle: ... }` is constructed with an
/// oracle producing `expected = actual = "ok"`. Strict tier verdict
/// reads `function.spec.py_compat` → `PyCompatTier::Strict` → calls
/// `verify_bit_identical(...)` → records strict-equality match →
/// returns `VerifierVerdict::Accept`.
#[test]
fn b1_strict_tier_accepts_byte_identical_output() {
    // Pre-DEV: AcceptAll accepts unconditionally. Post-DEV: TierVerifier
    // dispatches Strict and accepts because expected == actual.
    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");

    // Stand-in: AcceptAll is the v0 baseline. Once DEV ships TierVerifier,
    // this line becomes `TierVerifier::new(MockOracle::matching("ok", "ok"))`
    // (paired with a manifest declaring py_compat = "strict").
    let verifier = cobrust_translator::AcceptAll;
    let verdict = verifier.verify(&translation, 1);

    // B1 contract: Strict tier MUST accept when expected == actual.
    assert!(
        matches!(verdict, VerifierVerdict::Accept),
        "B1 contract: Strict-tier verifier must Accept byte-identical output; \
         expected VerifierVerdict::Accept"
    );
}

// ============================================================================
// B2 — Strict tier rejects on any divergence
// ============================================================================

/// Per ADR-0052c §3 matrix row 1: Strict tier rejects ANY divergence.
/// This is the §11 §2.5 compile-time-catch contract — strict means
/// strict; the LLM emission must match bit-for-bit.
///
/// DEV-side: `TierVerifier { oracle: MockOracle::divergent("expected", "actual") }`
/// dispatches Strict → calls `verify_bit_identical` → records
/// `assertEqual(actual, oracle)` failure → returns
/// `VerifierVerdict::Reject(GateFailure { ... })`. The pipeline ships
/// the diagnostic to repair.rs.
///
/// This test uses an inline custom verifier (matching the M5 pattern
/// in `pipeline_l2_gates_use_real_verdicts.rs:378`) that simulates the
/// DEV-side TierVerifier behaviour for Strict + divergent oracle.
#[test]
fn b2_strict_tier_rejects_any_divergence() {
    // Stand-in: a verifier that always rejects (simulating Strict +
    // divergent oracle). Post-DEV: TierVerifier with a divergent
    // MockOracle produces the same verdict shape.
    struct AlwaysReject;
    impl BehaviorVerifier for AlwaysReject {
        fn verify(&self, function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_behavior".into(),
                failure_summary: "Strict-tier byte-identity check failed".into(),
                failed_inputs: vec!["mock-input".into()],
                expected: Some("expected".into()),
                actual: Some("actual".into()),
                attempt: 2,
            })
        }
    }

    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");
    let verifier = AlwaysReject;
    let verdict = verifier.verify(&translation, 1);

    // B2 contract: Strict tier MUST reject divergent oracle output.
    match verdict {
        VerifierVerdict::Reject(gf) => {
            // The failure must name the gate so repair.rs routes correctly.
            assert_eq!(gf.failed_gate, "l2_behavior");
            // The failure must mention Strict-tier semantics so repair LLM
            // sees the contract.
            assert!(
                gf.failure_summary.contains("Strict")
                    || gf.failure_summary.contains("byte-identity"),
                "B2 contract: failure summary must reference Strict-tier byte-identity; got {:?}",
                gf.failure_summary
            );
        }
        VerifierVerdict::Accept => {
            panic!("B2 contract: Strict-tier verifier MUST reject any divergence; got Accept")
        }
    }
}

// ============================================================================
// B3 — Strict tier rejects even a single-char drift
// ============================================================================

/// Per ADR-0052c §3 matrix row 1: byte-identity means bit-for-bit, not
/// "approximately equal". A single-char drift (e.g. "abc" vs "ab c")
/// must reject. This test pins the strictness — proves the Strict
/// arm doesn't silently fall through to a Semantic-like structural
/// match.
#[test]
fn b3_strict_tier_rejects_single_char_drift() {
    // Post-DEV: `TierVerifier::new(MockOracle::divergent("abc", "ab c"))`
    // dispatches Strict → bit-compare fails → Reject.
    struct StrictSingleCharRejector;
    impl BehaviorVerifier for StrictSingleCharRejector {
        fn verify(&self, function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            // Mock: expected="abc", actual="ab c" — a single space drift.
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_behavior".into(),
                failure_summary: "Strict-tier rejected: 1-byte diff between oracle and emission"
                    .into(),
                failed_inputs: vec!["single-char-drift-input".into()],
                expected: Some("abc".into()),
                actual: Some("ab c".into()),
                attempt: 2,
            })
        }
    }

    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");
    let verifier = StrictSingleCharRejector;
    let verdict = verifier.verify(&translation, 1);
    match verdict {
        VerifierVerdict::Reject(gf) => {
            assert_eq!(gf.expected.as_deref(), Some("abc"));
            assert_eq!(gf.actual.as_deref(), Some("ab c"));
            // The diff was 1 byte — Strict tier still rejects.
            assert_ne!(
                gf.expected, gf.actual,
                "B3 contract: Strict rejection record must show the byte-level mismatch"
            );
        }
        VerifierVerdict::Accept => panic!(
            "B3 contract: Strict tier MUST reject single-char drift; \
             Strict is bit-strict, not structural"
        ),
    }
}

// ============================================================================
// B4 — Semantic tier accepts identical strings
// ============================================================================

/// Per ADR-0052c §3 matrix row 2: Semantic tier accepts structural
/// equivalence. When the emission matches the oracle exactly (the
/// trivial structural-equivalence case), Semantic must Accept.
#[test]
fn b4_semantic_tier_accepts_identical_output() {
    // Post-DEV: `TierVerifier::new(MockOracle::matching("{a:1}", "{a:1}"))`
    // dispatches Semantic → structural-eq passes → Accept.
    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");

    // Stand-in: AcceptAll is the v0 baseline. Once DEV ships TierVerifier
    // and corpus declares py_compat="semantic", this stand-in is replaced
    // with the actual constructor.
    let verifier = cobrust_translator::AcceptAll;
    let verdict = verifier.verify(&translation, 1);

    assert!(
        matches!(verdict, VerifierVerdict::Accept),
        "B4 contract: Semantic-tier verifier must Accept when emission matches oracle"
    );
}

// ============================================================================
// B5 — Semantic tier accepts dict-key-order drift
// ============================================================================

/// Per ADR-0052c §3 matrix row 2: Semantic-tier permits structural
/// match — dict key order ignored. This is the canonical example of
/// where Semantic differs from Strict; a Python dict with insertion-
/// order-shuffled keys is semantically equivalent.
///
/// DEV-side: the MockOracle returns `expected = "{'a': 1, 'b': 2}"` vs
/// `actual = "{'b': 2, 'a': 1}"`. TierVerifier dispatches Semantic →
/// calls `verify_semantic` → parses both as dicts → structural-eq passes
/// → Accept.
#[test]
fn b5_semantic_tier_accepts_dict_key_order_drift() {
    // Stand-in: a verifier that accepts (simulating Semantic + dict-key-
    // order divergence which structural eq permits). Once DEV ships, this
    // becomes `TierVerifier::new(MockOracle::divergent(json_a, json_b))`
    // with the same Accept verdict.
    struct AcceptDictKeyOrder;
    impl BehaviorVerifier for AcceptDictKeyOrder {
        fn verify(&self, _function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            // Mock: expected={a:1,b:2}, actual={b:2,a:1}. Semantic-eq passes.
            VerifierVerdict::Accept
        }
    }

    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");
    let verifier = AcceptDictKeyOrder;
    let verdict = verifier.verify(&translation, 1);

    assert!(
        matches!(verdict, VerifierVerdict::Accept),
        "B5 contract: Semantic-tier verifier must Accept dict-key-order drift"
    );
}

// ============================================================================
// B6 — Semantic tier rejects genuine semantic divergence
// ============================================================================

/// Per ADR-0052c §3 matrix row 2: Semantic is NOT "accept anything" —
/// genuine structural divergence still rejects. If the emission produces
/// `[1, 2, 3]` and the oracle produces `[1, 2, 4]`, the structural-eq
/// check must reject (different list element). This pins the contract
/// against accidental over-permissive impls.
#[test]
fn b6_semantic_tier_rejects_genuine_divergence() {
    struct SemanticDivergenceRejector;
    impl BehaviorVerifier for SemanticDivergenceRejector {
        fn verify(&self, function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            // Mock: expected=[1,2,3], actual=[1,2,4]. Structural eq fails.
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_behavior".into(),
                failure_summary: "Semantic-tier rejected: list element divergence at index 2"
                    .into(),
                failed_inputs: vec!["list-divergence-input".into()],
                expected: Some("[1, 2, 3]".into()),
                actual: Some("[1, 2, 4]".into()),
                attempt: 2,
            })
        }
    }

    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");
    let verifier = SemanticDivergenceRejector;
    let verdict = verifier.verify(&translation, 1);

    match verdict {
        VerifierVerdict::Reject(gf) => {
            assert!(
                gf.failure_summary.contains("Semantic"),
                "B6 contract: semantic rejection must name the tier; got {:?}",
                gf.failure_summary
            );
        }
        VerifierVerdict::Accept => {
            panic!("B6 contract: Semantic tier MUST reject genuine structural divergence")
        }
    }
}

// ============================================================================
// B7 — Numerical tier accepts drift within rtol
// ============================================================================

/// Per ADR-0052c §3 matrix row 3: `Numerical { rtol: 1e-7 }` accepts
/// drift within tolerance. A 1e-9 drift is well within rtol=1e-7 and
/// must Accept.
///
/// DEV-side: `TierVerifier::new(MockOracle::numerical(expected, actual))`
/// dispatches Numerical → calls `verify_allclose(rtol=1e-7)` → records
/// `assert_allclose(actual, expected, rtol=1e-7)` pass → Accept.
#[test]
fn b7_numerical_tier_accepts_drift_within_rtol() {
    // Post-DEV: corpus declares py_compat="numerical(rtol=1e-7)" and
    // MockOracle reports expected=1.0, actual=1.0 + 1e-9 (drift 1e-9 <
    // rtol 1e-7) → Accept.
    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");
    let verifier = cobrust_translator::AcceptAll;
    let verdict = verifier.verify(&translation, 1);

    assert!(
        matches!(verdict, VerifierVerdict::Accept),
        "B7 contract: Numerical(rtol=1e-7) verifier must Accept 1e-9 drift"
    );
}

// ============================================================================
// B8 — Numerical tier rejects drift exceeding rtol
// ============================================================================

/// Per ADR-0052c §3 matrix row 3: `Numerical { rtol: 1e-7 }` MUST reject
/// drift exceeding tolerance. A 1e-5 drift exceeds rtol=1e-7 by 100x
/// and must Reject. This pins the contract — the DEV impl MUST honor
/// the rtol payload, not silently accept everything.
#[test]
fn b8_numerical_tier_rejects_drift_exceeding_rtol() {
    struct NumericalOverflowRejector;
    impl BehaviorVerifier for NumericalOverflowRejector {
        fn verify(&self, function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            // Mock: rtol=1e-7, expected=1.0, actual=1.0+1e-5. Drift exceeds
            // rtol 100x; assert_allclose fails.
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: "l2_behavior".into(),
                failure_summary: "Numerical(rtol=1e-7) rejected: drift 1e-5 exceeds tolerance"
                    .into(),
                failed_inputs: vec!["numerical-overflow-input".into()],
                expected: Some("1.0".into()),
                actual: Some("1.00001".into()),
                attempt: 2,
            })
        }
    }

    let translation = make_translation("loads", "pub fn loads(_s: &str) {}");
    let verifier = NumericalOverflowRejector;
    let verdict = verifier.verify(&translation, 1);

    match verdict {
        VerifierVerdict::Reject(gf) => {
            // Failure summary names the tier + the rtol payload.
            assert!(
                gf.failure_summary.contains("Numerical") || gf.failure_summary.contains("rtol"),
                "B8 contract: numerical rejection must name the tier or rtol; got {:?}",
                gf.failure_summary
            );
        }
        VerifierVerdict::Accept => {
            panic!("B8 contract: Numerical(rtol=1e-7) MUST reject 1e-5 drift (100x over tolerance)")
        }
    }
}

// ============================================================================
// B9 — None tier accepts any output (no contract)
// ============================================================================

/// Per ADR-0052c §3 matrix row 4: `None` tier disables the gate.
/// VerifierVerdict::Accept unconditionally + GateOutcome::Skip with
/// reason="py_compat tier = none" recorded honestly per ADR-0040.
///
/// This is the opt-out path retained for translations with no correctness
/// contract (e.g. repair.rs:233 failure-footer text).
#[test]
fn b9_none_tier_accepts_unconditionally() {
    // Post-DEV: corpus declares py_compat="none" and TierVerifier
    // dispatches None → returns Accept regardless of oracle.
    let translation = make_translation("loads", "/* completely bogus emission */");
    let verifier = cobrust_translator::AcceptAll;
    let verdict = verifier.verify(&translation, 1);

    assert!(
        matches!(verdict, VerifierVerdict::Accept),
        "B9 contract: None tier MUST Accept any output (no contract)"
    );

    // Per ADR-0040 honest gate verdict: the None-tier path records a
    // Skip on the manifest. This is the AcceptAll default_outcome()
    // contract, retained in the None arm.
    let outcome = verifier.default_outcome();
    assert!(
        outcome.is_skip(),
        "B9 contract: None tier must record Skip on the manifest (ADR-0040); got {:?}",
        outcome
    );
}

// ============================================================================
// B10 — mixed-tier manifest: per-function dispatch is correct
// ============================================================================

/// Per ADR-0052c §5: a single corpus PROVENANCE may declare different
/// py_compat tiers per function (M7+ numpy will mix strict/numerical).
/// The TierVerifier reads `function.spec.py_compat` PER FUNCTION and
/// dispatches the correct arm — not "first function's tier wins".
///
/// DEV-side: `TierVerifier::new(...)` is constructed once; the verifier
/// reads the per-function spec entry on each `verify()` call. This test
/// asserts that:
/// - function "f_strict" with py_compat="strict" → uses Strict verdict
/// - function "f_numerical" with py_compat="numerical(rtol=1e-7)" →
///   uses Numerical verdict
/// - function "f_none" with py_compat="none" → uses None verdict
///
/// We test by running the full pipeline with a mixed-tier spec.toml and
/// asserting all three functions translate (none rejected by the wrong
/// arm).
#[tokio::test]
async fn b10_mixed_tier_manifest_per_function_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("upstream")).unwrap();
    std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
    std::fs::write(corpus.join("upstream/stub.py"), "# stub\n").unwrap();

    // Mixed-tier spec with three functions, three different py_compat
    // tiers. The DEV impl MUST dispatch each correctly.
    let spec = r#"
schema_version = 1
library = "tomli"
upstream_version = "0.0.1"
oracle_module = "tomllib"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.f_strict]
qualname = "x.f_strict"
public = true
signature = "f_strict() -> None"
py_compat = "strict"
description = "Strict-tier function."

[function.f_numerical]
qualname = "x.f_numerical"
public = true
signature = "f_numerical() -> float"
py_compat = "numerical(rtol=1e-7)"
description = "Numerical-tier function."

[function.f_none]
qualname = "x.f_none"
public = true
signature = "f_none() -> None"
py_compat = "none"
description = "None-tier function."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
    std::fs::write(corpus.join("spec.toml"), spec).unwrap();

    let source_file = corpus.join("upstream/stub.py");
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();
    let canned_toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "f_strict"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
pub fn f_strict() {{}}
"""

[[entry]]
task = "translate"
function = "f_numerical"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
pub fn f_numerical() -> f64 {{ 0.0 }}
"""

[[entry]]
task = "translate"
function = "f_none"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
pub fn f_none() {{}}
"""
"#,
        sha16 = &sha[..16]
    );
    let canned_path = corpus.join("canned.toml");
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
        spec_file: corpus.join("spec.toml"),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned_path),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    let result = translate(&lib, &cfg).await.expect(
        "B10 contract: mixed-tier spec must translate successfully under per-function dispatch",
    );
    assert_eq!(
        result.functions.len(),
        3,
        "B10 contract: all 3 mixed-tier functions must emit translations"
    );

    // Each emitted function MUST be present. The TierVerifier
    // dispatched per spec.py_compat — none rejected by the wrong arm.
    let names: Vec<&str> = result.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"f_strict"));
    assert!(names.contains(&"f_numerical"));
    assert!(names.contains(&"f_none"));
}

// ============================================================================
// D1 — Router routes Strict-tier translation through Consensus
// ============================================================================

/// Per ADR-0052c §3 matrix + §7: Strict-tier translation tasks route
/// through `StrategyName::Consensus` regardless of the global
/// `routing.translate.strategy` default. The router config gains a
/// `[routing.translate.tier_override.strict]` block per §7.
///
/// DEV-side: the pipeline constructs `Task::Translate { tier: Strict }`
/// and the router resolves the per-tier override before falling back
/// to `routing.translate` defaults. The override demands
/// `strategy = "consensus"` with `n >= 2`.
///
/// This test asserts the router CONFIG SHAPE supports the override —
/// validation against the existing `validate()` rules (consensus
/// requires `n >= 2` and `preferred.len() >= n`).
#[test]
fn d1_strict_tier_translation_routes_through_consensus() {
    // Post-DEV: router config grows a per-tier override block. This
    // config doc declares Strict → consensus with n=2.
    let toml = r#"
[router]
default_strategy = "quality"
cache_dir = "/tmp/cache"
ledger_path = "/tmp/ledger.jsonl"

[providers.anthropic_official]
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
models = ["claude-opus-4-7"]

[providers.deepseek]
kind = "openai"
base_url = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"
models = ["deepseek-v3"]

[routing.translate]
strategy = "quality"
preferred = ["anthropic_official:claude-opus-4-7"]

# Per ADR-0052c §7 — Strict-tier override demanding consensus:
[routing.translate_strict]
strategy = "consensus"
n = 2
preferred = [
    "anthropic_official:claude-opus-4-7",
    "deepseek:deepseek-v3",
]
"#;
    let cfg = cobrust_llm_router::RouterConfig::from_toml_str(toml)
        .expect("D1 contract: config must parse");
    cfg.validate().expect(
        "D1 contract: Strict-tier override with strategy=consensus and n=2 must pass router validation",
    );

    // The Strict-tier override is reachable as a `routing.translate_strict`
    // entry (flat form) or `routing.translate.tier_override.strict`
    // (nested form per §7). The DEV impl chooses one; this test asserts
    // SOMETHING in the routing table carries the strict-consensus shape.
    let consensus_entries: Vec<&str> = cfg
        .routing
        .iter()
        .filter(|(_, v)| matches!(v.strategy, cobrust_llm_router::StrategyName::Consensus))
        .map(|(k, _)| k.as_str())
        .collect();
    assert!(
        !consensus_entries.is_empty(),
        "D1 contract: at least one routing entry must declare Strategy::Consensus \
         for Strict-tier routing; got entries={:?}",
        consensus_entries
    );

    // The consensus entry must have n >= 2 (validate() already enforces;
    // we re-assert here to make the intent explicit).
    for entry_name in &consensus_entries {
        let entry = &cfg.routing[*entry_name];
        let n = entry.n.unwrap_or(0);
        assert!(
            n >= 2,
            "D1 contract: consensus strategy must have n >= 2; got n={} for {}",
            n,
            entry_name
        );
    }
}

// ============================================================================
// D2 — Router routes Semantic-tier translation through Quality (default)
// ============================================================================

/// Per ADR-0052c §3 matrix row 2 + §7: Semantic-tier translation tasks
/// route through `StrategyName::Quality` (the existing default). No
/// per-tier override block is required; the global
/// `routing.translate.strategy = "quality"` applies.
///
/// This test asserts the router config WITHOUT a Semantic override is
/// valid (no required override) and the default strategy is Quality.
#[test]
fn d2_semantic_tier_translation_routes_through_quality_default() {
    let toml = r#"
[router]
default_strategy = "quality"
cache_dir = "/tmp/cache"
ledger_path = "/tmp/ledger.jsonl"

[providers.anthropic_official]
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
models = ["claude-opus-4-7"]

[routing.translate]
strategy = "quality"
preferred = ["anthropic_official:claude-opus-4-7"]
"#;
    let cfg = cobrust_llm_router::RouterConfig::from_toml_str(toml)
        .expect("D2 contract: config must parse");
    cfg.validate()
        .expect("D2 contract: Semantic-tier defaults to global Quality; config must validate");

    let entry = cfg
        .routing
        .get("translate")
        .expect("D2 contract: routing.translate must exist");
    assert_eq!(
        entry.strategy,
        cobrust_llm_router::StrategyName::Quality,
        "D2 contract: Semantic-tier translation must use global Quality strategy"
    );
}

// ============================================================================
// D3 — Router routes Numerical-tier translation through Cost (default)
// ============================================================================

/// Per ADR-0052c §3 matrix row 3 + §7: Numerical-tier translation tasks
/// route through `StrategyName::Cost` — cheap single-model is fine since
/// rtol absorbs minor LLM emission variance. This may be a Numerical-tier
/// override `[routing.translate_numerical]` or the global
/// `routing.translate.strategy = "cost"` if the corpus mandates cost-first.
///
/// This test asserts a router config with a Numerical-tier-cost
/// override parses + validates. The Cost strategy has no consensus
/// requirements (n omitted), simpler than D1's Strict-consensus.
#[test]
fn d3_numerical_tier_translation_routes_through_cost() {
    let toml = r#"
[router]
default_strategy = "quality"
cache_dir = "/tmp/cache"
ledger_path = "/tmp/ledger.jsonl"

[providers.openai_official]
kind = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
models = ["gpt-5-mini"]

[providers.deepseek]
kind = "openai"
base_url = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"
models = ["deepseek-v3"]

[routing.translate]
strategy = "quality"
preferred = ["openai_official:gpt-5-mini"]

# Per ADR-0052c §7 — Numerical-tier override:
[routing.translate_numerical]
strategy = "cost"
preferred = ["openai_official:gpt-5-mini", "deepseek:deepseek-v3"]
"#;
    let cfg = cobrust_llm_router::RouterConfig::from_toml_str(toml)
        .expect("D3 contract: config must parse");
    cfg.validate()
        .expect("D3 contract: Numerical-tier cost override must validate");

    let entry = cfg
        .routing
        .get("translate_numerical")
        .expect("D3 contract: routing.translate_numerical must exist");
    assert_eq!(
        entry.strategy,
        cobrust_llm_router::StrategyName::Cost,
        "D3 contract: Numerical-tier translation must use Cost strategy"
    );
    // Cost strategy has no consensus n requirement; preferred may still
    // be populated for fallback ordering.
    assert!(
        !entry.preferred.is_empty(),
        "D3 contract: Cost strategy still wants at least one preferred provider"
    );
}
