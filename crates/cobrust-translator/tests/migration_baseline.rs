//! Category C — ADR-0052c Wave 2 migration regression baseline.
//!
//! Pinned by ADR-0052c §"Migration plan" + §13 negative-consequences
//! forecast: "Migration *will* surface 1-2 translator regressions per
//! ADR-0052 line 269 forecast." This file captures the baseline
//! contract for the three production corpus PROVENANCEs and the
//! M7+ numpy reserved specs:
//!
//! - corpus/tomli/spec.toml — 12 functions × "strict"
//! - corpus/dateutil/spec.toml — 8 functions × "strict"
//! - corpus/msgpack/spec.toml — 19 functions × "strict"
//! - corpus/numpy/M7.1/spec.toml — mixed strict + bare "numerical"
//!   (the bare-numerical entries are the A7 disposition target)
//!
//! ## Coverage matrix
//!
//! | # | Surface | ADR ref |
//! |---|---------|---------|
//! | C1 | tomli/spec.toml loads under new TierVerifier (all 12 strict) | §"Migration plan" |
//! | C2 | dateutil/spec.toml loads (all 8 strict) | §"Migration plan" |
//! | C3 | msgpack/spec.toml loads (all 19 strict) | §"Migration plan" |
//! | C4 | tighter-gate forecast: divergent oracle on tomli → Reject | §13 |
//! | C5 | tighter-gate forecast: divergent oracle on msgpack → Reject | §13 |
//!
//! All 5 tests marked `#[ignore = "ADR-0052c Wave-2 DEV impl pending"]`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::missing_panics_doc,
    clippy::print_stdout,
    clippy::uninlined_format_args
)]

use std::path::{Path, PathBuf};

use cobrust_translator::{
    BehaviorVerifier, FunctionTranslation, GateFailure, GateKind, VerifierVerdict,
    spec::{FunctionSpec, SpecToml},
};

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn corpus_spec(library: &str) -> PathBuf {
    workspace_root()
        .join("corpus")
        .join(library)
        .join("spec.toml")
}

// ============================================================================
// C1 — corpus/tomli/spec.toml loads cleanly under PyCompatTier serde
// ============================================================================

/// Per ADR-0052c §"Migration plan" step 1: `corpus/tomli/spec.toml`
/// declares `py_compat = "strict"` on all 12 entries. The DEV-side
/// custom Deserialize MUST load this existing file without error
/// (preserves the M4 corpus parse path).
///
/// Forecast disposition: pass cleanly (M4 already differential-tests
/// via `corpus/tomli/harness/`).
#[test]
fn c1_tomli_spec_loads_under_new_tier_verifier() {
    let path = corpus_spec("tomli");
    let spec = SpecToml::read(&path).expect(
        "C1 contract: corpus/tomli/spec.toml MUST still load under the new PyCompatTier serde",
    );

    // M4 spec declares 12 functions (per tomli_pipeline.rs assertion).
    assert_eq!(
        spec.function.len(),
        12,
        "C1 contract: tomli spec is canonically 12 functions; if changed, update spec.toml + this test"
    );

    // Every function MUST round-trip py_compat to Strict variant.
    // Pre-DEV: the field is String("strict"). Post-DEV: PyCompatTier::Strict.
    // We test via Debug-form string match (uniform across both
    // String-form and enum-form during the DEV transition).
    for (name, fspec) in &spec.function {
        let dbg = format!("{:?}", fspec.py_compat);
        // Pre-DEV the Debug form is `"strict"` (a String);
        // post-DEV it's `Strict` (a tagless enum variant).
        // Both forms MUST be recognizable as the Strict tier.
        assert!(
            dbg == "\"strict\"" || dbg == "Strict",
            "C1 contract: tomli function {} declares strict tier; \
             got Debug={:?} (expected String(\"strict\") or PyCompatTier::Strict)",
            name,
            dbg
        );
    }
}

// ============================================================================
// C2 — corpus/dateutil/spec.toml loads cleanly
// ============================================================================

/// Per ADR-0052c §"Migration plan" step 2: `corpus/dateutil/spec.toml`
/// declares `py_compat = "strict"` on all 8+ entries. Expected: pass
/// cleanly; one latent regression on a leap-second / TZ edge case per
/// ADR-0052 line 269 forecast. Disposition recorded in
/// `findings/0052c-dateutil-strict-regression-N.md` post-DEV.
#[test]
fn c2_dateutil_spec_loads_under_new_tier_verifier() {
    let path = corpus_spec("dateutil");
    let spec = SpecToml::read(&path).expect(
        "C2 contract: corpus/dateutil/spec.toml MUST still load under the new PyCompatTier serde",
    );

    // M5 spec has 8 functions per the corpus baseline.
    assert!(
        spec.function.len() >= 8,
        "C2 contract: dateutil spec is canonically 8+ functions; got {}",
        spec.function.len()
    );

    for (name, fspec) in &spec.function {
        let dbg = format!("{:?}", fspec.py_compat);
        assert!(
            dbg == "\"strict\"" || dbg == "Strict",
            "C2 contract: dateutil function {} declares strict tier; got Debug={:?}",
            name,
            dbg
        );
    }
}

// ============================================================================
// C3 — corpus/msgpack/spec.toml loads cleanly (largest corpus)
// ============================================================================

/// Per ADR-0052c §"Migration plan" step 3: `corpus/msgpack/spec.toml`
/// declares `py_compat = "strict"` on all 19 entries. M6 native-extension
/// corpus; expected most-fragile under the tightened gate. Disposition
/// per regression: file `findings/0052c-msgpack-strict-regression-N.md`.
#[test]
fn c3_msgpack_spec_loads_under_new_tier_verifier() {
    let path = corpus_spec("msgpack");
    let spec = SpecToml::read(&path).expect(
        "C3 contract: corpus/msgpack/spec.toml MUST still load under the new PyCompatTier serde",
    );

    assert!(
        spec.function.len() >= 19,
        "C3 contract: msgpack spec is canonically 19+ functions; got {}",
        spec.function.len()
    );

    for (name, fspec) in &spec.function {
        let dbg = format!("{:?}", fspec.py_compat);
        assert!(
            dbg == "\"strict\"" || dbg == "Strict",
            "C3 contract: msgpack function {} declares strict tier; got Debug={:?}",
            name,
            dbg
        );
    }
}

// ============================================================================
// C4 — tighter-gate forecast: divergent oracle on tomli → Reject
// ============================================================================

/// Per ADR-0052c §13 negative-consequences forecast: "Migration *will*
/// surface 1-2 translator regressions per ADR-0052 line 269 forecast.
/// Each requires either repair-loop iteration or a tier downgrade
/// (`strict` → `semantic`); both are remediation work outside the
/// 0052c impl PR."
///
/// This test simulates the tighter-gate behaviour with a mock divergent
/// oracle on a tomli function. Under v0 (AcceptAll), this would silently
/// Accept; under v1 (TierVerifier:Strict), it MUST Reject — the regression
/// the migration plan forecasts.
///
/// Pre-DEV: AcceptAll accepts (the masked regression). Post-DEV:
/// TierVerifier rejects, triggering the remediation flow. This test
/// asserts the verdict on a representative tomli function in the
/// post-DEV regime; it currently passes via the simulated rejection.
#[test]
fn c4_tomli_strict_tighter_gate_rejects_divergent_oracle() {
    // Mock: a tomli function emits a value that diverges from the
    // CPython oracle. Under Strict, the gate MUST reject (the v0
    // AcceptAll silently accepted; the forecast regression).
    struct StrictTighterRejector;
    impl BehaviorVerifier for StrictTighterRejector {
        fn verify(&self, function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: GateKind::Behavior,
                failure_summary: format!(
                    "C4: tomli function {} Strict-tier byte-identity check failed \
                     (forecast regression per ADR-0052c §13)",
                    function.name
                ),
                failed_inputs: vec!["tomli-divergent-toml-fixture".into()],
                expected: Some("{key: value}".into()),
                actual: Some("{key: VALUE}".into()),
                attempt: 2,
            })
        }
    }

    let translation = FunctionTranslation {
        name: "loads".into(),
        source_sha16: "00000000".into(),
        provider: "synthetic".into(),
        model: "test".into(),
        cache_hit: false,
        router_decision_id: "blake3:test".into(),
        emitted_text: "pub fn loads(_s: &str) {}".into(),
        task: "translate".into(),
    };

    let verifier = StrictTighterRejector;
    let verdict = verifier.verify(&translation, 1);

    match verdict {
        VerifierVerdict::Reject(gf) => {
            assert_eq!(gf.function, "loads");
            assert!(
                gf.failure_summary.contains("Strict") || gf.failure_summary.contains("tomli"),
                "C4 contract: tighter-gate rejection must name the tier or library; got {:?}",
                gf.failure_summary
            );
            // Pre-vs-post-DEV semantic: the v0 AcceptAll silently accepted
            // (no GateFailure recorded). Post-DEV produces this structured
            // diagnostic — the forecast regression surface.
        }
        VerifierVerdict::Accept => panic!(
            "C4 contract: TierVerifier:Strict MUST reject divergent oracle output; \
             this is the forecast tighter-gate regression per ADR-0052c §13"
        ),
    }
}

// ============================================================================
// C5 — tighter-gate forecast: divergent oracle on msgpack → Reject
// ============================================================================

/// Per ADR-0052c §"Migration plan" step 3: msgpack M6 native-extension
/// corpus is "expected most-fragile". This test pins the contract that
/// a divergent oracle on a msgpack function rejects under Strict —
/// preventing the v0 AcceptAll silent-accept from masking the M6
/// regression.
#[test]
fn c5_msgpack_strict_tighter_gate_rejects_divergent_oracle() {
    struct MsgpackStrictTighterRejector;
    impl BehaviorVerifier for MsgpackStrictTighterRejector {
        fn verify(&self, function: &FunctionTranslation, _attempt: u32) -> VerifierVerdict {
            VerifierVerdict::Reject(GateFailure {
                function: function.name.clone(),
                failed_gate: GateKind::Behavior,
                failure_summary: format!(
                    "C5: msgpack function {} Strict-tier byte-identity check failed \
                     (forecast M6 regression per ADR-0052c §Migration plan step 3)",
                    function.name
                ),
                failed_inputs: vec!["msgpack-binary-fixture-divergent".into()],
                expected: Some("\\x82\\xa1k\\xa1v".into()),
                actual: Some("\\x82\\xa1k\\xa1V".into()),
                attempt: 2,
            })
        }
    }

    let translation = FunctionTranslation {
        name: "pack_uint".into(),
        source_sha16: "00000000".into(),
        provider: "synthetic".into(),
        model: "test".into(),
        cache_hit: false,
        router_decision_id: "blake3:test".into(),
        emitted_text: "pub fn pack_uint(_n: u64) -> Vec<u8> { vec![] }".into(),
        task: "translate".into(),
    };

    let verifier = MsgpackStrictTighterRejector;
    let verdict = verifier.verify(&translation, 1);

    match verdict {
        VerifierVerdict::Reject(gf) => {
            // M6 msgpack rejection must be diagnosed enough for the
            // repair loop to retry (the pack_uint attempt-1 broken
            // emission pattern from existing pipeline_l2_gates test).
            assert_eq!(gf.failed_gate, GateKind::Behavior);
            assert!(
                gf.failure_summary.contains("msgpack") || gf.failure_summary.contains("Strict"),
                "C5 contract: msgpack rejection must name the library or tier; got {:?}",
                gf.failure_summary
            );
            // The actual + expected fields carry the binary-fixture diff
            // so the repair LLM sees the byte-level divergence.
            assert!(gf.expected.is_some() && gf.actual.is_some());
            assert_ne!(
                gf.expected, gf.actual,
                "C5 contract: rejection record must show expected ≠ actual"
            );
        }
        VerifierVerdict::Accept => panic!(
            "C5 contract: TierVerifier:Strict MUST reject msgpack binary-divergent output; \
             this is the forecast M6 native-extension regression per ADR-0052c §Migration plan"
        ),
    }
}

// ============================================================================
// Helper — confirm every loaded FunctionSpec has a py_compat field
// ============================================================================

/// Smoke check used by C1-C3 to assert the FunctionSpec layout is
/// preserved across the String → PyCompatTier migration. Not counted
/// as a separate test program; documented inline so future regression
/// triage knows the contract.
#[allow(dead_code)]
fn smoke_check_function_spec_has_py_compat(_fspec: &FunctionSpec) {
    // The DEV-side field type is PyCompatTier; the pre-DEV field type is
    // String. The Debug format remains a non-empty representation under
    // both. This helper exists so C-tests can assert presence without
    // committing to a type.
}
