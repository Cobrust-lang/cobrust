//! L2.perf benchmark harness.
//!
//! Pinned by ADR-0008 §1+§2+§7. Hand-rolled timing harness that pairs
//! a Rust closure ("the translation under test") against a CPython
//! subprocess running the same inputs. We report median nanoseconds
//! per call across `n_iters` runs, ratio cobrust/cpython, and emit a
//! JSON report at `target/cobrust-bench/<library>/<commit>/report.json`.
//!
//! Constitution §4.2 perf gate: ≥ 0.8× of original on representative
//! benchmark, configurable per library via `corpus/<lib>/perf.toml`.
//! ADR-0008 §2 chosen variant: per-library `pass_ratio` (fraction of
//! public functions that must meet threshold). Default 1.0; M5 dateutil
//! pins 0.5 because synthetic-mode responses are placeholder-quality.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Per-library perf configuration loaded from
/// `corpus/<lib>/perf.toml` (see ADR-0008 §2).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PerfTarget {
    /// Per-function pass threshold: cobrust_ns ≤ (1/threshold) × cpython_ns.
    /// Default 0.8 ⇒ "cobrust must be ≥ 0.8× as fast as cpython".
    pub threshold: f64,
    /// Fraction of public functions that must meet `threshold`.
    /// Default 1.0 (all). Lower for libraries where synthetic-mode
    /// responses are placeholder-quality.
    pub pass_ratio: f64,
    /// Number of timer rounds per input; we report the median.
    pub n_iters: u32,
    /// Number of distinct inputs the harness fuzzes per public function.
    pub n_inputs: u32,
}

impl Default for PerfTarget {
    fn default() -> Self {
        Self {
            threshold: 0.8,
            pass_ratio: 1.0,
            n_iters: 100,
            n_inputs: 32,
        }
    }
}

impl PerfTarget {
    /// Read a `perf.toml` from disk; fall back to default if absent.
    ///
    /// # Errors
    /// I/O errors that aren't `NotFound` bubble up; malformed TOML bubbles up.
    pub fn read_or_default(path: &Path) -> Result<Self, std::io::Error> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
}

/// Per-function benchmark result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub function: String,
    pub cobrust_ns_median: u64,
    pub cpython_ns_median: u64,
    /// Ratio is `cpython_ns_median / cobrust_ns_median`. ≥ 1.0 means
    /// cobrust is at least as fast as cpython; ≥ 0.8 is the default
    /// pass threshold (cobrust ≥ 0.8× cpython speed).
    pub ratio: f64,
    pub pass: bool,
    pub n_inputs: u32,
    pub n_iters: u32,
}

/// Aggregate report written to disk per ADR-0008 §7.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub library: String,
    pub commit: String,
    pub hardware: String,
    pub rustc: String,
    pub cpython: String,
    pub threshold: f64,
    pub pass_ratio: f64,
    pub results: Vec<BenchmarkResult>,
}

impl BenchmarkReport {
    /// Number of public functions that met the per-function threshold.
    #[must_use]
    pub fn passing_count(&self) -> usize {
        self.results.iter().filter(|r| r.pass).count()
    }

    /// Total number of public functions benchmarked.
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.results.len()
    }

    /// True iff `passing_count / total_count >= pass_ratio`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn meets_pass_ratio(&self) -> bool {
        if self.results.is_empty() {
            return true;
        }
        let ratio = self.passing_count() as f64 / self.total_count() as f64;
        ratio + 1e-9 >= self.pass_ratio
    }

    /// One-line summary suitable for the manifest's `gates.l2_perf` field.
    #[must_use]
    pub fn manifest_summary(&self) -> String {
        if self.results.is_empty() {
            return "skipped (no benchmarks recorded)".into();
        }
        let verdict = if self.meets_pass_ratio() {
            "pass"
        } else {
            "fail"
        };
        format!(
            "{verdict} ({passing}/{total} ≥ {threshold:.2}×; pass_ratio={pass_ratio:.2})",
            passing = self.passing_count(),
            total = self.total_count(),
            threshold = self.threshold,
            pass_ratio = self.pass_ratio,
        )
    }

    /// Persist as JSON at
    /// `target/cobrust-bench/<library>/<commit>/report.json`.
    ///
    /// # Errors
    /// I/O or JSON serialisation failures bubble up.
    pub fn write_json(&self, root: &Path) -> Result<PathBuf, std::io::Error> {
        let dir = root
            .join("cobrust-bench")
            .join(&self.library)
            .join(&self.commit);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("report.json");
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, s)?;
        Ok(path)
    }
}

/// Hardware tag from `uname -srm`. Falls back to a static label on
/// failure (we don't want a missing `uname` to fail the gate).
#[must_use]
pub fn hardware_tag() -> String {
    use std::process::Command;
    Command::new("uname")
        .arg("-srm")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown-hw".into())
}

/// Short HEAD SHA at the time of the harness run. Falls back to
/// `"unversioned"` if `git` is missing or the directory is not a repo.
#[must_use]
pub fn short_commit_sha() -> String {
    use std::process::Command;
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unversioned".into())
}

/// Time a closure `n_iters` times and return the **median** elapsed
/// nanoseconds. We deliberately use the median (not the mean) so a
/// single outlier (GC pause, scheduler hiccup) doesn't poison the
/// number.
///
/// `warmup` runs are executed first and discarded.
pub fn time_median<F>(n_iters: u32, warmup: u32, mut f: F) -> u64
where
    F: FnMut(),
{
    for _ in 0..warmup {
        f();
    }
    let mut samples: Vec<u64> = Vec::with_capacity(n_iters as usize);
    for _ in 0..n_iters {
        let t0 = Instant::now();
        f();
        let t1 = Instant::now();
        samples.push(u64::try_from(t1.duration_since(t0).as_nanos()).unwrap_or(u64::MAX));
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Build a [`BenchmarkResult`] from the two median timings + target.
///
/// Casts to `f64` are intentional and bounded: medians are at most a
/// few seconds in nanoseconds (≤ 5e9), well within `f64`'s precise
/// integer range (2^53).
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn classify_result(
    function: &str,
    cobrust_ns: u64,
    cpython_ns: u64,
    target: &PerfTarget,
    n_inputs: u32,
    n_iters: u32,
) -> BenchmarkResult {
    let ratio = if cobrust_ns == 0 {
        f64::INFINITY
    } else {
        cpython_ns as f64 / cobrust_ns as f64
    };
    let pass = ratio + 1e-9 >= target.threshold;
    BenchmarkResult {
        function: function.into(),
        cobrust_ns_median: cobrust_ns,
        cpython_ns_median: cpython_ns,
        ratio,
        pass,
        n_inputs,
        n_iters,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn perf_target_defaults_match_adr_0008() {
        let t = PerfTarget::default();
        assert!((t.threshold - 0.8).abs() < 1e-9);
        assert!((t.pass_ratio - 1.0).abs() < 1e-9);
        assert_eq!(t.n_iters, 100);
        assert_eq!(t.n_inputs, 32);
    }

    #[test]
    fn perf_target_read_or_default_falls_back_when_absent() {
        let t = PerfTarget::read_or_default(Path::new("/no/such/perf.toml")).unwrap();
        assert!((t.threshold - 0.8).abs() < 1e-9);
    }

    #[test]
    fn perf_target_reads_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("perf.toml");
        std::fs::write(
            &path,
            "threshold = 0.5\npass_ratio = 0.5\nn_iters = 10\nn_inputs = 4\n",
        )
        .unwrap();
        let t = PerfTarget::read_or_default(&path).unwrap();
        assert!((t.threshold - 0.5).abs() < 1e-9);
        assert_eq!(t.n_iters, 10);
    }

    #[test]
    fn classify_result_marks_pass_when_ratio_meets_threshold() {
        let target = PerfTarget {
            threshold: 0.8,
            ..Default::default()
        };
        let r = classify_result("f", 100, 100, &target, 1, 1);
        assert!(r.pass);
        assert!((r.ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn classify_result_marks_fail_when_cobrust_too_slow() {
        let target = PerfTarget {
            threshold: 0.8,
            ..Default::default()
        };
        // cobrust 200ns vs cpython 100ns ⇒ ratio 0.5 < 0.8 ⇒ fail.
        let r = classify_result("f", 200, 100, &target, 1, 1);
        assert!(!r.pass);
        assert!((r.ratio - 0.5).abs() < 1e-9);
    }

    #[test]
    fn classify_result_handles_zero_cobrust_ns_as_infinite_ratio() {
        let target = PerfTarget::default();
        let r = classify_result("f", 0, 100, &target, 1, 1);
        assert!(r.pass);
        assert!(r.ratio.is_infinite());
    }

    #[test]
    fn report_meets_pass_ratio_at_threshold() {
        let target = PerfTarget {
            threshold: 0.8,
            pass_ratio: 0.5,
            ..Default::default()
        };
        let report = BenchmarkReport {
            library: "x".into(),
            commit: "abc".into(),
            hardware: "test".into(),
            rustc: "rustc 1.94.1".into(),
            cpython: "3.11".into(),
            threshold: target.threshold,
            pass_ratio: target.pass_ratio,
            results: vec![
                classify_result("a", 100, 200, &target, 1, 1), // pass (ratio 2.0)
                classify_result("b", 200, 100, &target, 1, 1), // fail (ratio 0.5)
            ],
        };
        // 1/2 = 0.5 ≥ 0.5 ⇒ meets.
        assert!(report.meets_pass_ratio());
        assert_eq!(report.passing_count(), 1);
        assert_eq!(report.total_count(), 2);
    }

    #[test]
    fn report_fails_pass_ratio_when_too_few_pass() {
        let target = PerfTarget {
            threshold: 0.8,
            pass_ratio: 1.0,
            ..Default::default()
        };
        let report = BenchmarkReport {
            library: "x".into(),
            commit: "abc".into(),
            hardware: "test".into(),
            rustc: "rustc".into(),
            cpython: "3.11".into(),
            threshold: target.threshold,
            pass_ratio: target.pass_ratio,
            results: vec![classify_result("a", 200, 100, &target, 1, 1)],
        };
        assert!(!report.meets_pass_ratio());
    }

    #[test]
    fn report_writes_json_under_target_cobrust_bench() {
        let dir = tempfile::tempdir().unwrap();
        let report = BenchmarkReport {
            library: "x".into(),
            commit: "abc".into(),
            hardware: "test".into(),
            rustc: "rustc".into(),
            cpython: "3.11".into(),
            threshold: 0.8,
            pass_ratio: 1.0,
            results: vec![],
        };
        let path = report.write_json(dir.path()).unwrap();
        assert!(path.exists());
        let s = std::fs::read_to_string(path).unwrap();
        assert!(s.contains(r#""library": "x""#));
        assert!(s.contains(r#""commit": "abc""#));
    }

    #[test]
    fn manifest_summary_is_human_readable() {
        let target = PerfTarget {
            threshold: 0.8,
            pass_ratio: 0.5,
            ..Default::default()
        };
        let report = BenchmarkReport {
            library: "x".into(),
            commit: "abc".into(),
            hardware: "test".into(),
            rustc: "rustc".into(),
            cpython: "3.11".into(),
            threshold: target.threshold,
            pass_ratio: target.pass_ratio,
            results: vec![
                classify_result("a", 100, 200, &target, 1, 1),
                classify_result("b", 200, 100, &target, 1, 1),
            ],
        };
        let summary = report.manifest_summary();
        assert!(summary.contains("pass"));
        assert!(summary.contains("1/2"));
        assert!(summary.contains("0.80"));
    }

    #[test]
    fn time_median_gives_nonzero_for_real_work() {
        let mut counter = 0u64;
        let med = time_median(50, 5, || {
            for _ in 0..1000 {
                counter = counter.wrapping_add(1);
            }
        });
        assert!(med > 0);
        assert!(counter > 0);
    }
}
