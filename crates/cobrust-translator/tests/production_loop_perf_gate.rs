//! Production translation closed-loop firing on a **REAL** L2.perf
//! differential — the perf-gate sibling of
//! `tests/production_loop_real_oracle.rs`.
//!
//! Activates the last stub of the L2 verification trio (task #164). The
//! behavior gate already has a real test-wired oracle
//! (`CpythonOracleHarness` in `production_loop_real_oracle.rs`); the perf
//! gate's production default is still `AcceptAllPerf` (a no-op `Skip`),
//! and the only `PerfVerifier` exercise so far — msgpack's
//! `PackUintPerfBrokenVerifier` — FAKES it via a string-marker
//! (`contains("PERF-BROKEN")`), never measuring time. This test makes
//! the perf gate REAL: a test-wired [`BenchmarkPerfVerifier`] that
//! genuinely TIMES the translated `incr` against CPython and gates on
//! the constitution §4.2 / ADR-0008 `PerfTarget(0.8)` threshold.
//!
//! Symmetric with the behavior oracle (do NOT over-trust as CI-enforced):
//!
//!   - `expected` (the bar): really run CPython 3.11
//!     (`/opt/homebrew/bin/python3.11`, the pinned `PYTHON_PATH`) timing
//!     `incr(n) = n + 1` warm-up + N iterations → median ns/op.
//!   - `actual` (under test): really compile the translation's
//!     `emitted_text` with single-file `rustc` (no cargo — dodges the
//!     F72 cargo-in-test flakiness, exactly like `emission_actual`) into
//!     a driver that itself runs warm-up + N timed iterations and prints
//!     its OWN median ns/op. We time the function *inside* the compiled
//!     binary (not the process wall-clock) so process/interpreter
//!     start-up never pollutes the per-op number.
//!
//! The ratio is `cpython_ns / cobrust_ns` (the `bench.rs` convention):
//! ≥ 0.8 ⇒ cobrust is at least 0.8× CPython speed ⇒ PASS. We reuse
//! `bench.rs`'s [`PerfTarget`] (`threshold = 0.8`), [`classify_result`],
//! and [`BenchmarkResult`] verbatim — the gate logic under test is the
//! shipped one, not a re-implementation.
//!
//! Flow exercised end-to-end through the production entrypoint
//! `translate_with_verifiers(&lib, &cfg, &AcceptAll, &BenchmarkPerfVerifier)`:
//!   1. L1 dispatch serves the canned attempt-1 emission — a
//!      CORRECT-but-DELIBERATELY-SLOW `incr` (computes `n + 1` after a
//!      ~200us blocking `sleep`). Behavior accepts (the result is right);
//!      the perf gate TIMES it, finds it many× slower than CPython, and
//!      `Reject`s with `GateKind::Perf` → repair fires.
//!   2. Re-dispatch (attempt 2) serves the FAST clean `incr` (`n + 1`,
//!      no sleep); the timed gate now measures it far FASTER than
//!      CPython → `Accept`.
//!   3. The manifest's `gates.l2_perf` records the pass.
//!
//! NON-FLAKY (and WHY NOT A BUSY-LOOP): the slow emission is slow BY
//! CONSTRUCTION via a `std::thread::sleep` — a syscall the optimiser CANNOT
//! elide. An earlier version used a ~5M-iteration `std::hint::black_box`
//! accumulator busy-loop, but `black_box` is only a *best-effort* barrier
//! (no correctness guarantee) and `rustc -O` INTERMITTENTLY DCE'd the loop
//! (its `acc` was ultimately discarded), collapsing the slow `incr` to
//! `n + 1` → median 0 ns → ratio +∞ → the slow path flakily PASSED the gate
//! (a reliability audit caught the wrong verdict on 6+ runs; cf. F75). The
//! sleep removes that nondeterminism: ~200us/call is DECISIVELY > CPython's
//! ~tens-of-ns `n + 1` (ratio ~0.0004 ≪ 0.8, ~2000× margin) on EVERY build.
//! The fast emission is a single integer add the optimiser shrinks below
//! the `Instant` resolution → median 0 ns → ratio = +∞ ≫ 0.8. Median (not
//! mean) + warm-up; the margin is ~3 orders of magnitude on the slow side
//! and an unbounded ratio on the fast side, so jitter can never flip either.
//!
//! SCOPE / CI HONESTY: the pinned `PYTHON_PATH` is Homebrew-only and
//! `rustc` must be present, so on every non-macOS CI runner the guard
//! below returns early and this test asserts NOTHING — it is a
//! **macOS-developer-local** proof of the real-CPython-vs-real-`rustc`
//! perf differential, NOT a CI gate. The production default perf
//! verifier stays [`AcceptAllPerf`] (`Skip`); this test does not change
//! it. The repair-loop *mechanism* it exercises is independently
//! CI-guarded (pure synthetic, no `python3.11`) by the msgpack pipeline
//! unit tests + `src/pipeline.rs`'s loop tests.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]

use std::path::Path;
use std::process::Command;

use cobrust_llm_router::RouterConfig;
use cobrust_translator::{
    AcceptAll, BenchmarkResult, FunctionTranslation, GateKind, PerfTarget, PerfVerdict,
    PerfVerifier, PyLibrary, TranslatorConfig, classify_result, translate_with_verifiers,
};

/// The project's pinned CPython oracle — the established constant in the
/// sibling `production_loop_real_oracle.rs` / `msgpack_pipeline.rs`
/// tests. Reused verbatim; do NOT invent a different path.
const PYTHON_PATH: &str = "/opt/homebrew/bin/python3.11";

/// Warm-up iterations discarded before timing (both tiers). Enough to
/// settle CPU frequency + branch predictor + the CPython bytecode warm
/// path; the orders-of-magnitude slow/fast gap means we don't need many.
const WARMUP: u32 = 50;

/// Timed iterations whose median ns/op we report (both tiers). The
/// median of this many samples is rock-stable for the ~3-orders-of-
/// magnitude gap we measure; kept modest so the slow emission's
/// per-call ~200us don't blow up this local-only test's wall-time.
const N_ITERS: u32 = 101;

/// Single fixed input fed to BOTH tiers for the timed op. The op is
/// `incr(n) = n + 1`; the value is immaterial to the timing (the slow
/// emission's cost is its sleep, not the arithmetic).
const TIMED_INPUT: i64 = 7;

// ---------------------------------------------------------------------------
// TIMED harness: REAL CPython (the bar) vs REAL compiled emission (under
// test). Reuses the `production_loop_real_oracle.rs` compile-via-single-
// file-`rustc` pattern, but the driver is TIMED: it runs the op
// warm-up + N times and prints its own median ns/op to stdout.
// ---------------------------------------------------------------------------

/// Parse the single `u64` (median ns/op) a timed driver prints on its
/// last non-empty stdout line.
fn parse_median_ns(stdout: &str) -> Result<u64, String> {
    let line = stdout
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .ok_or_else(|| format!("timed driver produced no stdout lines; got {stdout:?}"))?;
    line.trim()
        .parse::<u64>()
        .map_err(|e| format!("could not parse median ns/op from {line:?}: {e}"))
}

/// Time CPython 3.11 running `incr(n) = n + 1` for `TIMED_INPUT`,
/// warm-up + `N_ITERS` rounds, and return the **median** ns/op. The
/// Python script does its OWN per-call timing with `time.perf_counter_ns`
/// and `statistics.median`, so process start-up is excluded — only the
/// `incr(n)` call is measured, the same op the Rust side times.
fn cpython_median_ns() -> Result<u64, String> {
    // The op is defined as a real Python function so we time a genuine
    // call (matching the Rust side timing a genuine `incr(n)` call),
    // not a folded constant. `black_box`-equivalent: `incr` is opaque to
    // the interpreter (no constant-folding across the call boundary).
    let script = format!(
        "import time, statistics\n\
         def incr(n):\n    return n + 1\n\
         N = {N_ITERS}\n\
         WARMUP = {WARMUP}\n\
         x = {TIMED_INPUT}\n\
         for _ in range(WARMUP):\n    incr(x)\n\
         samples = []\n\
         for _ in range(N):\n    \
         t0 = time.perf_counter_ns()\n    incr(x)\n    \
         t1 = time.perf_counter_ns()\n    samples.append(t1 - t0)\n\
         print(int(statistics.median(samples)))\n",
    );
    let out = Command::new(PYTHON_PATH)
        .arg("-c")
        .arg(&script)
        .output()
        .map_err(|e| format!("failed to spawn python3.11: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "python3.11 perf oracle exited non-zero: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    parse_median_ns(&String::from_utf8_lossy(&out.stdout))
}

/// Compile `emitted_text` (which must define `pub fn incr(n: i64) ->
/// i64`) + a generated TIMED `main`, then run the binary and return the
/// median ns/op it prints. Single-file `rustc -O`, no cargo, no deps.
///
/// The driver mirrors `bench::time_median`: `WARMUP` discarded rounds,
/// then `N_ITERS` `Instant`-bracketed calls, median of the samples. We
/// wrap the input AND the result in `std::hint::black_box` so the
/// optimiser cannot hoist the call out of the loop or const-fold it —
/// the slow emission's sleep must actually execute on every iteration.
///
/// SAFETY PIN (same as `emission_actual` in the sibling test): this
/// compiles + RUNS `emitted_text`, so it must ONLY ever be fed TRUSTED
/// in-test emissions — never live untrusted LLM/router output.
fn emission_median_ns(emitted_text: &str) -> Result<u64, String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let src = dir.path().join("perf_emission.rs");
    let bin = dir.path().join("perf_emission_bin");

    // `-O` (release-equivalent) so the FAST emission is genuinely fast
    // (a single add). The SLOW emission's cost is a `sleep` syscall the
    // optimiser cannot elide; the driver-loop `black_box` pins the CALL
    // so it is not hoisted / const-folded out of the timed loop.
    let program = format!(
        "{emitted_text}\n\n\
         use std::hint::black_box;\n\
         use std::time::Instant;\n\
         fn main() {{\n    \
         let x: i64 = {TIMED_INPUT};\n    \
         for _ in 0..{WARMUP} {{ black_box(incr(black_box(x))); }}\n    \
         let mut samples: Vec<u64> = Vec::with_capacity({N_ITERS});\n    \
         for _ in 0..{N_ITERS} {{\n        \
         let t0 = Instant::now();\n        \
         black_box(incr(black_box(x)));\n        \
         let t1 = Instant::now();\n        \
         samples.push(t1.duration_since(t0).as_nanos() as u64);\n    \
         }}\n    \
         samples.sort_unstable();\n    \
         println!(\"{{}}\", samples[samples.len() / 2]);\n}}\n",
    );
    std::fs::write(&src, program).map_err(|e| format!("write perf_emission.rs failed: {e}"))?;

    let compile = Command::new("rustc")
        .arg("-O")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .map_err(|e| format!("failed to spawn rustc: {e}"))?;
    if !compile.status.success() {
        return Err(format!(
            "rustc failed to compile perf emission: {}",
            String::from_utf8_lossy(&compile.stderr)
        ));
    }
    let run = Command::new(&bin)
        .output()
        .map_err(|e| format!("failed to run compiled perf emission: {e}"))?;
    if !run.status.success() {
        return Err(format!(
            "compiled perf emission exited non-zero: {}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    parse_median_ns(&String::from_utf8_lossy(&run.stdout))
    // `dir` drops here → temp files cleaned (F63 RAII).
}

// ---------------------------------------------------------------------------
// The REAL perf verifier: times emission-vs-CPython, gates on 0.8×.
// ---------------------------------------------------------------------------

/// A test-wired [`PerfVerifier`] that genuinely benchmarks the
/// translated `incr` against CPython and applies the constitution §4.2
/// `PerfTarget(0.8)` threshold via `bench.rs`'s shipped
/// [`classify_result`]. This is the REAL counterpart to msgpack's
/// string-marker `PackUintPerfBrokenVerifier` — symmetric with how
/// `production_loop_real_oracle.rs`'s `CpythonOracleHarness` is the real
/// counterpart to the synthetic behavior oracle.
///
/// `verify`:
///   1. times CPython's `incr` → `cpython_ns` (the bar);
///   2. compiles + times the emission's `incr` → `cobrust_ns`;
///   3. `classify_result` computes `ratio = cpython_ns / cobrust_ns` and
///      `pass = ratio >= threshold (0.8)`;
///   4. `pass` ⇒ [`PerfVerdict::Accept`]; else
///      [`PerfVerdict::Reject`] with `failed_gate: GateKind::Perf`
///      (the enum from dac5fb5, not a string) carrying the genuinely-
///      measured medians + ratio.
///
/// On any harness-side error (rustc/python failure) it `Reject`s with a
/// `GateKind::Perf` failure naming the error — a perf gate that cannot
/// measure must not silently pass.
struct BenchmarkPerfVerifier {
    target: PerfTarget,
    /// CPython's median ns/op for `incr`, measured once and cached so
    /// every attempt is judged against the SAME bar (and we don't pay
    /// the CPython subprocess cost per attempt). `incr` is identical
    /// across attempts on the CPython side — only the Rust emission
    /// changes — so caching the bar is correct.
    cpython_ns: u64,
}

impl BenchmarkPerfVerifier {
    /// Construct with the default `PerfTarget` (threshold 0.8) and a
    /// freshly-measured CPython bar.
    fn new() -> Result<Self, String> {
        Ok(Self {
            target: PerfTarget::default(),
            cpython_ns: cpython_median_ns()?,
        })
    }

    /// Time the emission and classify it against the cached CPython bar.
    /// Exposed for the direct (non-pipeline) verifier proof test.
    fn benchmark(&self, function: &FunctionTranslation) -> Result<BenchmarkResult, String> {
        let cobrust_ns = emission_median_ns(&function.emitted_text)?;
        Ok(classify_result(
            &function.name,
            cobrust_ns,
            self.cpython_ns,
            &self.target,
            1,
            N_ITERS,
        ))
    }
}

impl PerfVerifier for BenchmarkPerfVerifier {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> PerfVerdict {
        // Only `incr` carries a timed contract here; anything else (none
        // in this corpus) passes untouched, matching the msgpack shape.
        if function.name != "incr" {
            return PerfVerdict::Accept;
        }
        match self.benchmark(function) {
            Ok(result) => {
                eprintln!(
                    "[perf-gate] {fname} attempt {attempt}: cobrust_ns={cob} cpython_ns={cpy} \
                     ratio={ratio:.4} (threshold {thr:.2}×) → {verdict}",
                    fname = function.name,
                    cob = result.cobrust_ns_median,
                    cpy = result.cpython_ns_median,
                    ratio = result.ratio,
                    thr = self.target.threshold,
                    verdict = if result.pass { "ACCEPT" } else { "REJECT" },
                );
                if result.pass {
                    PerfVerdict::Accept
                } else {
                    PerfVerdict::Reject(cobrust_translator::GateFailure {
                        function: function.name.clone(),
                        failed_gate: GateKind::Perf,
                        failure_summary: format!(
                            "{fname} fails the {thr:.2}× perf gate: cobrust median {cob}ns vs \
                             CPython median {cpy}ns (ratio {ratio:.4} < {thr:.2})",
                            fname = function.name,
                            thr = self.target.threshold,
                            cob = result.cobrust_ns_median,
                            cpy = result.cpython_ns_median,
                            ratio = result.ratio,
                        ),
                        failed_inputs: vec![format!("incr({TIMED_INPUT})")],
                        expected: Some(format!(
                            "ratio (cpython_ns / cobrust_ns) ≥ {thr:.2} \
                             (CPython median {cpy}ns is the bar)",
                            thr = self.target.threshold,
                            cpy = result.cpython_ns_median,
                        )),
                        actual: Some(format!(
                            "median ns/op = {cob} (ratio {ratio:.4})",
                            cob = result.cobrust_ns_median,
                            ratio = result.ratio,
                        )),
                        attempt: attempt.saturating_add(1),
                    })
                }
            }
            Err(msg) => PerfVerdict::Reject(cobrust_translator::GateFailure {
                function: function.name.clone(),
                failed_gate: GateKind::Perf,
                failure_summary: format!("perf harness failed for {}: {msg}", function.name),
                failed_inputs: vec![],
                expected: None,
                actual: None,
                attempt: attempt.saturating_add(1),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Canned emissions: both compute `n + 1` (behavior always passes); they
// differ ONLY in runtime. The synthetic path copies `response_text`
// verbatim into `emitted_text` (no provenance header), exactly as in the
// sibling tests.
// ---------------------------------------------------------------------------

/// A CORRECT-but-DELIBERATELY-SLOW `incr`: returns `n + 1`, but only
/// after a blocking `sleep`. The result is genuinely `n + 1`, so the
/// BEHAVIOR gate would accept it — it is ONLY the perf gate that fires.
///
/// WHY A SLEEP, NOT A BUSY-LOOP: an earlier version used a ~5M-iteration
/// `std::hint::black_box` accumulator loop whose result was discarded
/// (`black_box(acc);` then `n + 1`). `black_box` is a *best-effort*
/// optimisation barrier with NO correctness guarantee, and `rustc -O`
/// INTERMITTENTLY dead-code-eliminated the whole loop (acc was ultimately
/// unused) — collapsing `incr` to `n + 1`, median 0 ns, ratio = +∞, so the
/// "slow" emission flakily PASSED the gate (the reliability audit caught a
/// wrong verdict on 6+ runs; FAILED runs measured 0 ns / 0.5 s wall vs
/// PASSED runs 10 ms / 6 s — proof the loop was compiled away). A
/// `std::thread::sleep` is a syscall the optimiser CANNOT elide, so the
/// slow path is slow BY CONSTRUCTION on every build, deterministically.
const SLOW_INCR: &str = r"pub fn incr(n: i64) -> i64 {
    // ~200us/call via a real sleep syscall — DECISIVELY > CPython's
    // ~tens-of-ns `n + 1` (ratio ~0.0004 << 0.8, ~2000x margin),
    // optimiser-proof (unlike a black_box busy-loop, which -O may DCE).
    std::thread::sleep(std::time::Duration::from_micros(200));
    n + 1
}";

/// The FAST clean `incr`: a single integer add. Under `-O` it shrinks
/// below the `Instant` resolution → median 0 ns → `classify_result`
/// yields ratio = +∞ ≫ 0.8 (decisively faster than CPython). The repair
/// target.
const FAST_INCR: &str = "pub fn incr(n: i64) -> i64 { n + 1 }";

/// Build a [`FunctionTranslation`] for `incr` carrying `emitted_text`.
/// Mirrors the per-attempt canned-emission shape the synthetic pipeline
/// supplies; used directly by the verifier-proof test.
fn incr_translation(emitted_text: &str) -> FunctionTranslation {
    FunctionTranslation {
        name: "incr".into(),
        source_sha16: "0000000000000000".into(),
        provider: "synthetic".into(),
        model: "incr-canned-v1".into(),
        cache_hit: false,
        router_decision_id: "test".into(),
        emitted_text: emitted_text.into(),
        task: "translate".into(),
    }
}

// ---------------------------------------------------------------------------
// Corpus + canned-table + router fixtures (mirrors
// production_loop_real_oracle.rs).
// ---------------------------------------------------------------------------

/// Write the L0 corpus: a stub `incr.py` + a `spec.toml` declaring the
/// single `incr` function. We mark it `none` tier so the production
/// behavior verifier is a guaranteed no-op (`AcceptAll` is wired anyway,
/// but `none` makes the intent explicit: this test isolates the PERF
/// gate). Returns `(source_file, spec_file)`.
fn write_incr_corpus(corpus: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    std::fs::create_dir_all(corpus.join("upstream")).unwrap();
    std::fs::create_dir_all(corpus.join("upstream_tests")).unwrap();
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
py_compat = "none"
description = "Return n + 1 (perf-gate fixture; behavior gate disabled)."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
    std::fs::write(corpus.join("spec.toml"), spec).unwrap();
    (corpus.join("upstream/incr.py"), corpus.join("spec.toml"))
}

/// Write the slow→fast canned table. Attempt 1 is the SLOW `incr` (perf
/// gate rejects); attempt 2 is the FAST `incr` (perf gate accepts). Both
/// are self-contained compilable Rust fns copied verbatim into
/// `emitted_text`. Keyed by the real source SHA16 (the synthetic
/// provider rejects a stale SHA).
fn write_canned_slow_then_fast(corpus: &Path, sha16: &str) -> std::path::PathBuf {
    let path = corpus.join("canned.toml");
    // The canned-table TOML uses `"""` multi-line strings; the bodies
    // contain no `"""` so no escaping is needed.
    let toml = format!(
        r#"schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "incr"
source_sha16 = "{sha16}"
attempt = 1
response_text = """
{SLOW_INCR}
"""

[[entry]]
task = "translate"
function = "incr"
source_sha16 = "{sha16}"
attempt = 2
response_text = """
{FAST_INCR}
"""
"#,
    );
    std::fs::write(&path, toml).unwrap();
    path
}

/// Synthetic-mode router config. Model name must be `incr-canned-v1`
/// (the dispatch model string `run_l1` / `repair_translation_with_task`
/// build). Mirrors `production_loop_real_oracle.rs`.
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

/// Shared CI/local skip guard: the pinned CPython oracle + `rustc` must
/// both be present. Returns `true` (and prints a skip marker) when the
/// test must self-skip so CI (no python3.11) stays green.
fn must_skip(test_name: &str) -> bool {
    if !Path::new(PYTHON_PATH).exists() {
        eprintln!("{test_name}: skipping — {PYTHON_PATH} not present (CPython perf oracle)");
        return true;
    }
    let rustc_ok = Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !rustc_ok {
        eprintln!("{test_name}: skipping — rustc not available (perf emission compiler)");
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Test 1 — the REAL verifier, exercised directly: slow → Reject(Perf),
// fast → Accept. Proves the timing + threshold logic in isolation,
// before wiring it through the pipeline.
// ---------------------------------------------------------------------------

#[test]
fn benchmark_perf_verifier_rejects_slow_accepts_fast() {
    if must_skip("production_loop_perf_gate::verifier_direct") {
        return;
    }

    let verifier = BenchmarkPerfVerifier::new().expect("CPython perf bar must measure");
    eprintln!(
        "[perf-gate] CPython bar: incr median = {}ns/op (threshold {:.2}×)",
        verifier.cpython_ns, verifier.target.threshold
    );

    // --- The SLOW emission: genuinely measured ≫ CPython → Reject. ---
    let slow = incr_translation(SLOW_INCR);
    let slow_result = verifier.benchmark(&slow).expect("slow benchmark runs");
    eprintln!(
        "[perf-gate] SLOW incr: cobrust={}ns cpython={}ns ratio={:.6} pass={}",
        slow_result.cobrust_ns_median,
        slow_result.cpython_ns_median,
        slow_result.ratio,
        slow_result.pass
    );
    assert!(
        !slow_result.pass,
        "SLOW incr must FAIL the 0.8× gate; got ratio {:.6} (cobrust {}ns vs cpython {}ns)",
        slow_result.ratio, slow_result.cobrust_ns_median, slow_result.cpython_ns_median
    );
    // Non-flaky margin: the slow ratio must be DECISIVELY below 0.8 —
    // assert it is below half the threshold, with vast headroom (real
    // value ≈ 0.001). Jitter cannot lift a ~0.001 ratio past 0.4.
    assert!(
        slow_result.ratio < 0.4,
        "SLOW incr ratio must be decisively < 0.8 (asserting < 0.4 for margin); got {:.6}",
        slow_result.ratio
    );
    match verifier.verify(&slow, 1) {
        PerfVerdict::Reject(f) => {
            assert_eq!(
                f.failed_gate,
                GateKind::Perf,
                "slow emission must Reject on the Perf gate (the dac5fb5 enum), got {:?}",
                f.failed_gate
            );
            assert_eq!(f.function, "incr");
            assert_eq!(f.attempt, 2, "reject stamps the next attempt number");
        }
        PerfVerdict::Accept => panic!("SLOW incr must be Rejected by the perf gate"),
    }

    // --- The FAST emission: genuinely measured ≪ CPython → Accept. ---
    let fast = incr_translation(FAST_INCR);
    let fast_result = verifier.benchmark(&fast).expect("fast benchmark runs");
    eprintln!(
        "[perf-gate] FAST incr: cobrust={}ns cpython={}ns ratio={:.6} pass={}",
        fast_result.cobrust_ns_median,
        fast_result.cpython_ns_median,
        fast_result.ratio,
        fast_result.pass
    );
    assert!(
        fast_result.pass,
        "FAST incr must PASS the 0.8× gate; got ratio {:.6} (cobrust {}ns vs cpython {}ns)",
        fast_result.ratio, fast_result.cobrust_ns_median, fast_result.cpython_ns_median
    );
    // Non-flaky margin: the fast ratio must be DECISIVELY at/above 0.8 —
    // assert ≥ 1.0 (cobrust at least as fast as CPython), with vast
    // headroom (real value ≳ 10). Jitter cannot drop a ~10 ratio to 1.0.
    assert!(
        fast_result.ratio >= 1.0,
        "FAST incr ratio must be decisively ≥ 0.8 (asserting ≥ 1.0 for margin); got {:.6}",
        fast_result.ratio
    );
    assert!(
        matches!(verifier.verify(&fast, 1), PerfVerdict::Accept),
        "FAST incr must be Accepted by the perf gate"
    );

    // The two medians are unambiguously ordered — the gap is the whole
    // point of the non-flaky design.
    assert!(
        slow_result.cobrust_ns_median > fast_result.cobrust_ns_median * 100,
        "SLOW must be ≫100× the FAST emission (slow {}ns, fast {}ns)",
        slow_result.cobrust_ns_median,
        fast_result.cobrust_ns_median
    );
}

// ---------------------------------------------------------------------------
// Test 2 — the full production repair loop: slow attempt-1 Rejected on
// l2_perf, loop repairs, fast attempt-2 Accepted, manifest records the
// pass. Drives the SHIPPED `translate_with_verifiers` entrypoint.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn production_loop_perf_gate_repairs_slow_to_fast() {
    if must_skip("production_loop_perf_gate::repair_loop") {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus/incr");
    let (source_file, spec_file) = write_incr_corpus(&corpus);

    // Canned entries are keyed by the real source SHA16, exactly as the
    // production path computes it (the synthetic provider rejects stale).
    let sha = cobrust_translator::deterministic::sha256_file(&source_file).unwrap();
    let canned = write_canned_slow_then_fast(&corpus, &sha[..16]);

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
        spec_file,
        upstream_tests: corpus.join("upstream_tests"),
        canned_responses: Some(canned),
        seeds: vec![1],
        fuzz_inputs_per_fn: 1,
    };

    // Behavior gate = AcceptAll (no-op); the REAL perf gate is the
    // BenchmarkPerfVerifier — symmetric with how the behavior-oracle
    // test wires TierVerifier + AcceptAllPerf. The CPython bar is
    // measured once at construction and reused across both attempts.
    let perf = BenchmarkPerfVerifier::new().expect("CPython perf bar must measure");
    let cpython_bar = perf.cpython_ns;
    let result = translate_with_verifiers(&lib, &cfg, &AcceptAll, &perf)
        .await
        .expect("production pipeline must converge on the fast attempt 2");

    // --- The perf repair loop fired exactly once. ---
    assert_eq!(
        result.repair_attempts, 1,
        "perf gate must reject the slow attempt-1 exactly once and converge on attempt 2; got {}",
        result.repair_attempts
    );

    // --- The final emission is the FAST clean body. ---
    let final_emission = &result.functions[0].emitted_text;
    assert!(
        final_emission.contains("n + 1"),
        "final emission must carry the fast body (n + 1); got: {final_emission:?}"
    );
    // The slow body (the `sleep`) must NOT survive into the accepted
    // emission — the fast `n + 1` body has no sleep, so its presence would
    // prove the repair did not replace the slow attempt.
    assert!(
        !final_emission.contains("sleep"),
        "slow `sleep` body must not survive in the final emission; got: {final_emission:?}"
    );
    assert!(
        !final_emission.contains("from_micros"),
        "slow-emission marker (from_micros) must not survive in the final emission; got: {final_emission:?}"
    );

    // --- The diagnostic blob for the rejected attempt-1 was persisted,
    //     naming the perf gate. ---
    let diag = dir.path().join("out/incr/diagnostics/incr__2.toml");
    assert!(
        diag.exists(),
        "perf-reject diagnostic not persisted at {diag:?}"
    );
    let diag_text = std::fs::read_to_string(&diag).unwrap();
    assert!(
        diag_text.contains("l2_perf"),
        "diagnostic must name the perf gate (l2_perf); got: {diag_text}"
    );

    // --- The structured perf gate outcome is Pass (the live gate
    //     rejected at least once, so it is NOT the AcceptAllPerf Skip). ---
    assert!(
        result.gate_outcomes.l2_perf.is_pass(),
        "l2_perf gate outcome must be Pass after a real reject→repair→accept; got {:?}",
        result.gate_outcomes.l2_perf
    );

    // --- The manifest's l2_perf records the pass. ---
    result.manifest.validate().unwrap();
    let l2_perf = &result.manifest.gates.l2_perf;
    assert!(
        l2_perf.starts_with("pass"),
        "manifest l2_perf must record the pass; got {l2_perf:?}"
    );

    eprintln!("[perf-gate] CPython bar reused across attempts = {cpython_bar}ns/op");
    eprintln!("[perf-gate] repair_attempts = {}", result.repair_attempts);
    eprintln!("[perf-gate] manifest l2_perf = {l2_perf:?}");
    eprintln!("[perf-gate] final emission   = {final_emission:?}");
}
