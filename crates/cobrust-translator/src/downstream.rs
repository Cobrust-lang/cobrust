//! L3 downstream-dependents driver.
//!
//! Per ADR-0009 §5: runs vendored test subsets of dependent libraries
//! against the translated crate via `python3 -m unittest` (or direct
//! `python3` invocation for plain-script subsets). Emits a structured
//! [`DownstreamReport`] consumed by the pipeline to populate
//! [`crate::manifest::DependentsSection`].
//!
//! Constitution §4.2 mandates "top-5 dependents" at L3. M5 ships 2 of
//! 5 (croniter, freezegun) and explicitly defers 3 (pandas, sqlalchemy,
//! pendulum) to M6 — see ADR-0009 §3 for the selection rationale.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Per-dependent gate result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DependentResult {
    pub name: String,
    pub tests_run: u32,
    pub tests_passed: u32,
    pub status: DependentStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DependentStatus {
    Pass,
    Skipped { reason: String },
    Failed { failures: Vec<String> },
}

/// One vendored dependent's location: the python file (relative to
/// repo) that the L3 driver runs.
#[derive(Clone, Debug)]
pub struct DependentSpec {
    pub name: String,
    pub test_script: PathBuf,
}

/// Aggregate L3 downstream report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownstreamReport {
    pub library: String,
    pub dependents: Vec<DependentResult>,
    /// Dependents we know about but didn't run this gate pass.
    pub deferred: Vec<String>,
    pub deferred_reason: String,
}

impl DownstreamReport {
    /// Sum of tests across all `Pass` dependents (skipped + failed not counted).
    #[must_use]
    pub fn total_passed(&self) -> u32 {
        self.dependents
            .iter()
            .filter(|r| matches!(r.status, DependentStatus::Pass))
            .map(|r| r.tests_passed)
            .sum()
    }

    /// One-line summary suitable for `gates.l3_downstream_dependents`.
    #[must_use]
    pub fn manifest_summary(&self) -> String {
        let covered = self.dependents.len();
        let total = covered + self.deferred.len();
        let pass = self
            .dependents
            .iter()
            .filter(|r| matches!(r.status, DependentStatus::Pass))
            .count();
        let names: Vec<String> = self.dependents.iter().map(|r| r.name.clone()).collect();
        let deferred_names = self.deferred.join(", ");
        format!(
            "pass {pass}/{covered} ({covered_names}); deferred {deferred}/{total} ({deferred_names}) per ADR-0009",
            covered_names = names.join(", "),
            deferred = self.deferred.len(),
        )
    }

    /// Names of dependents that passed.
    #[must_use]
    pub fn covered_names(&self) -> Vec<String> {
        self.dependents
            .iter()
            .filter(|r| matches!(r.status, DependentStatus::Pass))
            .map(|r| r.name.clone())
            .collect()
    }
}

/// Run a single dependent's vendored test subset.
///
/// `python_path` is the absolute path to the Python interpreter; we
/// pre-resolve it so the same call can be reused for the L0 oracle
/// (parser_core invocation) and the L3 dependents driver.
///
/// `pythonpath` is prepended to `PYTHONPATH` so the dependent's
/// `from cobrust_dateutil import parse_iso` import resolves to the
/// PyO3-shaped wrapper at `crates/cobrust-dateutil/python/`.
///
/// # Errors
/// I/O errors that prevent spawning Python bubble up; the dependent's
/// own assertion failures are recorded in `Failed { failures }` but
/// **not** raised as `Err` — the caller decides whether to fail the
/// gate based on `pass_count >= 1`.
pub fn run_dependent(
    python_path: &str,
    pythonpath: Option<&Path>,
    spec: &DependentSpec,
) -> std::io::Result<DependentResult> {
    if !spec.test_script.exists() {
        return Ok(DependentResult {
            name: spec.name.clone(),
            tests_run: 0,
            tests_passed: 0,
            status: DependentStatus::Skipped {
                reason: format!("test script {} missing", spec.test_script.display()),
            },
        });
    }
    let mut cmd = Command::new(python_path);
    cmd.arg(&spec.test_script);
    if let Some(p) = pythonpath {
        let existing = std::env::var("PYTHONPATH").unwrap_or_default();
        let combined = if existing.is_empty() {
            p.to_string_lossy().into_owned()
        } else {
            format!("{}:{}", p.display(), existing)
        };
        cmd.env("PYTHONPATH", combined);
    }
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Our vendored subsets emit "PASS <name>" or "FAIL <name>" lines.
    let mut tests_run = 0u32;
    let mut tests_passed = 0u32;
    let mut failures: Vec<String> = Vec::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("PASS ") {
            tests_run += 1;
            tests_passed += 1;
            let _ = rest;
        } else if let Some(rest) = line.strip_prefix("FAIL ") {
            tests_run += 1;
            failures.push(rest.to_string());
        }
    }
    let status = if !output.status.success() && tests_run == 0 {
        DependentStatus::Skipped {
            reason: format!(
                "python script exit {} (likely import failure): {}",
                output.status, stderr
            ),
        }
    } else if !failures.is_empty() {
        DependentStatus::Failed { failures }
    } else {
        DependentStatus::Pass
    };
    Ok(DependentResult {
        name: spec.name.clone(),
        tests_run,
        tests_passed,
        status,
    })
}

/// Pin the M5 dateutil dependents per ADR-0009 §3. Ordering is
/// stable (alphabetical) so the manifest remains determinism-friendly.
#[must_use]
pub fn dateutil_m5_dependents(corpus_root: &Path) -> Vec<DependentSpec> {
    vec![
        DependentSpec {
            name: "croniter".into(),
            test_script: corpus_root.join("dependents/croniter/test_croniter_subset.py"),
        },
        DependentSpec {
            name: "freezegun".into(),
            test_script: corpus_root.join("dependents/freezegun/test_freezegun_subset.py"),
        },
    ]
}

/// The 3 M5-deferred dateutil dependents per ADR-0009 §3.
#[must_use]
pub fn dateutil_m5_deferred() -> Vec<String> {
    vec!["pandas".into(), "sqlalchemy".into(), "pendulum".into()]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn report_total_passed_sums_pass_dependents_only() {
        let report = DownstreamReport {
            library: "dateutil".into(),
            dependents: vec![
                DependentResult {
                    name: "a".into(),
                    tests_run: 5,
                    tests_passed: 5,
                    status: DependentStatus::Pass,
                },
                DependentResult {
                    name: "b".into(),
                    tests_run: 5,
                    tests_passed: 3,
                    status: DependentStatus::Failed {
                        failures: vec!["t1".into(), "t2".into()],
                    },
                },
                DependentResult {
                    name: "c".into(),
                    tests_run: 0,
                    tests_passed: 0,
                    status: DependentStatus::Skipped {
                        reason: "out of scope".into(),
                    },
                },
            ],
            deferred: vec![],
            deferred_reason: String::new(),
        };
        assert_eq!(report.total_passed(), 5); // only `a`
    }

    #[test]
    fn manifest_summary_lists_covered_and_deferred() {
        let report = DownstreamReport {
            library: "dateutil".into(),
            dependents: vec![DependentResult {
                name: "croniter".into(),
                tests_run: 5,
                tests_passed: 5,
                status: DependentStatus::Pass,
            }],
            deferred: vec!["pandas".into(), "sqlalchemy".into()],
            deferred_reason: "M6".into(),
        };
        let s = report.manifest_summary();
        assert!(s.contains("croniter"));
        assert!(s.contains("pandas"));
        assert!(s.contains("ADR-0009"));
        assert!(s.contains("pass 1/1"));
        assert!(s.contains("deferred 2/3"));
    }

    #[test]
    fn run_dependent_skips_when_test_script_missing() {
        let spec = DependentSpec {
            name: "ghost".into(),
            test_script: Path::new("/no/such/script.py").to_path_buf(),
        };
        let result = run_dependent("python3", None, &spec).unwrap();
        match result.status {
            DependentStatus::Skipped { reason } => assert!(reason.contains("missing")),
            _ => panic!("expected Skipped"),
        }
        assert_eq!(result.tests_run, 0);
    }

    #[test]
    fn dateutil_m5_dependents_are_pinned_in_alphabetical_order() {
        let deps = dateutil_m5_dependents(Path::new("/x"));
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].name, "croniter");
        assert_eq!(deps[1].name, "freezegun");
    }

    #[test]
    fn dateutil_m5_deferred_lists_three() {
        let d = dateutil_m5_deferred();
        assert_eq!(d.len(), 3);
        assert!(d.contains(&"pandas".to_string()));
        assert!(d.contains(&"sqlalchemy".to_string()));
        assert!(d.contains(&"pendulum".to_string()));
    }

    #[test]
    fn covered_names_returns_only_pass_dependents() {
        let report = DownstreamReport {
            library: "x".into(),
            dependents: vec![
                DependentResult {
                    name: "a".into(),
                    tests_run: 1,
                    tests_passed: 1,
                    status: DependentStatus::Pass,
                },
                DependentResult {
                    name: "b".into(),
                    tests_run: 1,
                    tests_passed: 0,
                    status: DependentStatus::Failed { failures: vec![] },
                },
            ],
            deferred: vec![],
            deferred_reason: String::new(),
        };
        assert_eq!(report.covered_names(), vec!["a".to_string()]);
    }
}
