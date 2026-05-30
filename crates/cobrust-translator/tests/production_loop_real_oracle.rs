//! Production translation closed-loop firing on a **REAL** CPython
//! differential, recorded honestly in the manifest.
//!
//! Closes defect (a) of the M4 follow-up finding
//! `docs/agent/findings/m4-tomli-real-llm-end-to-end-2026-05-30.md`
//! Follow-up #2: the production `pipeline::translate_with_verifiers`
//! repair loop had never been exercised against a *real* differential
//! oracle. The default `AcceptAll` behavior verifier degrades to
//! `Skip`, and the existing `TierVerifier` coverage only ever fed it
//! canned/synthetic `OracleObservation`s. This test wires the
//! production `TierVerifier` to an oracle that:
//!
//!   - computes `expected` by really running CPython 3.11
//!     (`/opt/homebrew/bin/python3.11 -c ...`), the project's pinned
//!     oracle; and
//!   - computes `actual` by really compiling the translation's
//!     `emitted_text` with `rustc` (single-file, no cargo — dodges the
//!     F72 cargo-in-test flakiness) and running the binary.
//!
//! No values are hardcoded: the divergence is *genuinely observed*.
//! The function under test is the trivial `incr(n) -> n + 1` so the
//! proof is about the LOOP MECHANISM + REAL ORACLE, not library
//! complexity (real-library parity is already proven by the bespoke
//! `full_pipeline_tomli_real_llm.rs` harness — not duplicated here).
//!
//! Flow exercised end-to-end through the production entrypoint:
//!   1. L1 dispatch serves the canned attempt-1 emission — a
//!      COMPILABLE-but-WRONG `incr` that computes `n + 2`.
//!   2. The `TierVerifier` (strict tier) queries the oracle: CPython
//!      says `n + 1`, the compiled emission says `n + 2` → byte-identity
//!      fails → `Reject` → repair loop fires.
//!   3. Re-dispatch (attempt 2) serves the CORRECT `n + 1` emission;
//!      the oracle now matches → `Accept`.
//!   4. The manifest's `verification.divergences` (ADR-0082) honestly
//!      records the broken `actual` vs the CPython `expected`.
//!
//! SCOPE / CI HONESTY (do not over-trust this as CI-enforced): the
//! pinned `PYTHON_PATH` is Homebrew-only, so on every non-macOS CI
//! runner the guard below returns early and this test asserts NOTHING —
//! it is a **macOS-developer-local** proof of the real-CPython-vs-real-
//! `rustc` differential. The loop + ADR-0082 *mechanism* it exercises is
//! independently CI-guarded (pure synthetic, no `python3.11`) by two
//! unit tests in `src/pipeline.rs`:
//! `pipeline_repair_loop_recovers_when_attempt_2_canned` (positive: one
//! Reject ⇒ one divergence record) and
//! `pipeline_is_deterministic_across_runs` (negative: clean run records
//! none). This test's unique added value is the live real-oracle
//! observation, not the loop wiring.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use cobrust_llm_router::RouterConfig;
use cobrust_translator::{
    AcceptAllPerf, FunctionTranslation, OracleHarness, OracleObservation, PyLibrary, SpecToml,
    TierVerifier, TranslatorConfig, translate_with_verifiers,
};

/// The project's pinned CPython oracle (the established constant in the
/// sibling `dateutil_pipeline.rs` / `msgpack_pipeline.rs` tests).
const PYTHON_PATH: &str = "/opt/homebrew/bin/python3.11";

/// Fixed input set fed to BOTH oracles. Includes zero, a small
/// positive, and a negative so a constant-offset bug (`n + 2`) is
/// unambiguously caught on every input.
const INPUTS: [i64; 5] = [0, 1, 5, -3, 100];

// ---------------------------------------------------------------------------
// Oracle harness: REAL CPython (expected) vs REAL compiled emission (actual).
// ---------------------------------------------------------------------------

/// Runs CPython 3.11 to produce `incr(n) = n + 1` truth and compiles +
/// runs the translation's `emitted_text` to produce its actual output.
struct CpythonOracleHarness;

impl CpythonOracleHarness {
    /// Run CPython, printing `n + 1` for each input, one per line.
    /// Returns the stdout lines (string form, matching the Rust side's
    /// integer `Display`). Errors on any oracle-side failure.
    fn cpython_expected() -> Result<Vec<String>, String> {
        // Build a Python list literal from the fixed inputs; the script
        // prints `n + 1` per element, one per line, with no decoration.
        let list = INPUTS
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let script = format!("for n in [{list}]:\n    print(n + 1)");
        let out = Command::new(PYTHON_PATH)
            .arg("-c")
            .arg(&script)
            .output()
            .map_err(|e| format!("failed to spawn python3.11: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "python3.11 oracle exited non-zero: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    /// Compile `emitted_text` + a generated `main` that prints
    /// `incr(n)` for each input (one per line), then run the binary and
    /// return its stdout lines. Single-file `rustc`, no cargo, no deps.
    /// A fresh temp dir per call (RAII-cleaned) avoids cross-attempt
    /// contamination.
    ///
    /// SAFETY PIN: this compiles + RUNS `emitted_text`, so it must ONLY
    /// ever be fed TRUSTED in-test `CannedTable` emissions — never live
    /// untrusted LLM/router output. A real-LLM oracle would need the
    /// emission sandboxed before this point (do not copy this harness
    /// verbatim into a real-LLM pipeline).
    fn emission_actual(emitted_text: &str) -> Result<Vec<String>, String> {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
        let src = dir.path().join("emission.rs");
        let bin = dir.path().join("emission_bin");

        // Generate the driver `main`. The fixed inputs are baked in as a
        // Rust array literal so we observe exactly the same domain as
        // the CPython side. `incr` is supplied by `emitted_text`.
        let list = INPUTS
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let program = format!(
            "{emitted_text}\n\nfn main() {{\n    for n in [{list}] {{\n        \
             println!(\"{{}}\", incr(n));\n    }}\n}}\n"
        );
        std::fs::write(&src, program).map_err(|e| format!("write emission.rs failed: {e}"))?;

        let compile = Command::new("rustc")
            .arg(&src)
            .arg("-o")
            .arg(&bin)
            .output()
            .map_err(|e| format!("failed to spawn rustc: {e}"))?;
        if !compile.status.success() {
            return Err(format!(
                "rustc failed to compile emission: {}",
                String::from_utf8_lossy(&compile.stderr)
            ));
        }
        let run = Command::new(&bin)
            .output()
            .map_err(|e| format!("failed to run compiled emission: {e}"))?;
        if !run.status.success() {
            return Err(format!(
                "compiled emission exited non-zero: {}",
                String::from_utf8_lossy(&run.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&run.stdout)
            .lines()
            .map(str::to_string)
            .collect())
        // `dir` drops here → temp files cleaned (F63 RAII).
    }
}

impl OracleHarness for CpythonOracleHarness {
    fn observe(
        &self,
        function: &FunctionTranslation,
        _attempt: u32,
    ) -> Result<Vec<OracleObservation>, String> {
        let expected = Self::cpython_expected()?;
        let actual = Self::emission_actual(&function.emitted_text)?;
        if expected.len() != INPUTS.len() {
            return Err(format!(
                "CPython oracle returned {} lines, expected {}",
                expected.len(),
                INPUTS.len()
            ));
        }
        if actual.len() != INPUTS.len() {
            return Err(format!(
                "compiled emission returned {} lines, expected {}",
                actual.len(),
                INPUTS.len()
            ));
        }
        // One observation per input: (input, oracle-truth, emission-output).
        Ok(INPUTS
            .iter()
            .zip(expected.iter())
            .zip(actual.iter())
            .map(|((n, exp), act)| OracleObservation {
                input: n.to_string(),
                expected: exp.clone(),
                actual: act.clone(),
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Fixtures: corpus (spec + source), canned broken→fixed table, router cfg.
// ---------------------------------------------------------------------------

/// Write the L0 corpus: a stub Python source + a `spec.toml` declaring
/// the single `incr` function at the `strict` (byte-identity) tier.
/// Returns `(source_file, spec_file)`.
fn write_incr_corpus(corpus: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    std::fs::create_dir_all(corpus.join("upstream")).unwrap();
    std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
    // The synthetic provider routes on the prompt header, not the body,
    // so the source content only needs to be stable (it seeds the SHA).
    std::fs::write(
        corpus.join("upstream/incr.py"),
        "def incr(n):\n    return n + 1\n",
    )
    .unwrap();
    let spec = r#"
schema_version = 1
library = "incr"
upstream_version = "0.0.1"
oracle_module = "builtins"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.incr]
qualname = "incr.incr"
public = true
signature = "incr(n) -> int"
py_compat = "strict"
description = "Return n + 1."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
    std::fs::write(corpus.join("spec.toml"), spec).unwrap();
    (corpus.join("upstream/incr.py"), corpus.join("spec.toml"))
}

/// Write the broken→fixed canned table. Both `response_text`s are
/// self-contained, compilable Rust fns (the synthetic path copies
/// `response_text` verbatim into `emitted_text`, with no provenance
/// header prepended — see `translate.rs::run_l1`, `emitted_text:
/// resp.response.text`). Attempt 1 is COMPILABLE-but-WRONG (`n + 2`);
/// attempt 2 is CORRECT (`n + 1`).
fn write_canned_broken_then_fixed(corpus: &Path, sha16: &str) -> std::path::PathBuf {
    let path = corpus.join("canned.toml");
    let toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "incr"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
pub fn incr(n: i64) -> i64 {{ n + 2 }}
"""

[[entry]]
task = "translate"
function = "incr"
source_sha16 = "{sha16}"
attempt = 2
response_text = """
pub fn incr(n: i64) -> i64 {{ n + 1 }}
"""
"#
    );
    std::fs::write(&path, toml).unwrap();
    path
}

/// Synthetic-mode router config. The model name must be
/// `<library>-canned-v1` (`incr-canned-v1`) to match the dispatch model
/// string built in `run_l1` / `repair_translation_with_task`. Only the
/// default `routing.translate` is wired, so the strict tier falls back
/// to `Task::Translate` (no `translate_strict` override exists).
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
models = ["incr-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:incr-canned-v1"]
"#,
        cache = cache.display(),
        ledger = ledger.display(),
    );
    RouterConfig::from_toml_str(&toml).unwrap()
}

// ---------------------------------------------------------------------------
// The test.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn production_loop_fires_on_real_cpython_differential() {
    // Self-skip if the pinned CPython oracle is absent (rustc is always
    // present in this toolchain). Mirrors the sibling-test skip style.
    if !Path::new(PYTHON_PATH).exists() {
        eprintln!(
            "production_loop_real_oracle: skipping — {PYTHON_PATH} not present (CPython oracle)"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus/incr");
    let (source_file, spec_file) = write_incr_corpus(&corpus);

    // Canned entries are keyed by the real source SHA16 (the synthetic
    // provider rejects a stale SHA), exactly as the production path
    // computes it.
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();
    let canned = write_canned_broken_then_fixed(&corpus, &sha[..16]);

    let cache = dir.path().join("cache");
    let ledger = dir.path().join("ledger.jsonl");
    let cfg = TranslatorConfig::m4_synthetic(
        synthetic_router_cfg(&cache, &ledger),
        dir.path().join("out"),
    );
    let lib = PyLibrary {
        library: "incr".into(),
        version: "0.0.1".into(),
        source_file,
        spec_file: spec_file.clone(),
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    // Build the PRODUCTION TierVerifier from the parsed spec + the REAL
    // oracle harness, then drive the PRODUCTION entrypoint.
    let spec = SpecToml::read(&spec_file).unwrap();
    let verifier = TierVerifier::from_spec(&spec, Arc::new(CpythonOracleHarness));
    let result = translate_with_verifiers(&lib, &cfg, &verifier, &AcceptAllPerf)
        .await
        .expect("production pipeline must converge on attempt 2");

    // --- The repair loop fired on the REAL divergence. ---
    assert!(
        result.repair_attempts >= 1,
        "repair loop must fire at least once on the real CPython differential; got {}",
        result.repair_attempts
    );

    // --- The final emission is the corrected body. ---
    let final_emission = &result.functions[0].emitted_text;
    assert!(
        final_emission.contains("n + 1"),
        "final emission must carry the corrected body (n + 1); got: {final_emission:?}"
    );
    // Sanity: the broken body must NOT survive in the final emission.
    assert!(
        !final_emission.contains("n + 2"),
        "broken body (n + 2) must not survive in the final emission; got: {final_emission:?}"
    );

    // --- The manifest honestly recorded the divergence (ADR-0082). ---
    let divs = &result.manifest.verification.divergences;
    assert!(
        !divs.is_empty(),
        "manifest must record the observed divergence; got empty"
    );
    let rec = &divs[0];
    assert!(rec.contains("incr"), "divergence names the function: {rec}");
    assert!(
        rec.contains("l2_behavior"),
        "divergence names the gate: {rec}"
    );
    // The record carries the genuinely-observed broken value. The strict
    // byte-identity check reports the FIRST diverging input, which (in
    // INPUTS order) is `0`: CPython expected "1" (n + 1) while the broken
    // emission produced "2" (n + 2). These exact strings come from REALLY
    // running each oracle — not from any hardcoded constant — so their
    // presence proves the differential was observed, not asserted.
    assert!(
        rec.contains("input=\"0\""),
        "divergence must name the real first-diverging input (0): {rec}"
    );
    assert!(
        rec.contains("expected=\"1\""),
        "divergence must carry the CPython-expected value \"1\" for input 0: {rec}"
    );
    assert!(
        rec.contains("actual=\"2\""),
        "divergence must carry the broken emission value \"2\" for input 0: {rec}"
    );

    // Surface the verbatim record so the auditor can read the real
    // differential that was observed and recorded.
    eprintln!("repair_attempts = {}", result.repair_attempts);
    eprintln!("divergences[0]  = {rec}");
}
