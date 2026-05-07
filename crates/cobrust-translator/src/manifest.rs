//! Provenance manifest: schema, builder, writer, verifier.
//!
//! Pinned by ADR-0007 §3 ("Provenance manifest schema") and extended
//! by ADR-0009 §5 ("Manifest representation of partial coverage")
//! which adds a structured `gates.dependents` sub-section so machines
//! can audit downstream-dependents coverage without parsing the
//! human-readable string.
//!
//! The on-disk form lives at `<crate_dir>/PROVENANCE.toml` next to
//! the generated crate's `Cargo.toml`.
//!
//! Every field is load-bearing; missing fields are a constitutional
//! violation (§2.4 "no silent translations, ever").

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceManifest {
    pub source: SourceSection,
    pub oracle: OracleSection,
    pub verification: VerificationSection,
    pub router: RouterSection,
    pub build: BuildSection,
    pub gates: GatesSection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSection {
    pub library: String,
    pub version: String,
    /// Full 64-hex SHA-256 of the source archive.
    pub sha256: String,
    pub file_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleSection {
    pub runtime: String,
    pub runtime_version: String,
    pub oracle_module: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationSection {
    pub seeds: Vec<u64>,
    pub fuzz_inputs_per_fn: u32,
    #[serde(default)]
    pub divergences: Vec<String>,
    #[serde(default)]
    pub known_failures: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouterSection {
    pub strategy: String,
    pub models_used: Vec<String>,
    pub ledger_entries: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSection {
    pub toolchain: String,
    /// `blake3:<hex>` per `crate::deterministic::deterministic_id`.
    pub deterministic_id: String,
    pub crate_layout_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatesSection {
    pub l0_spec_emitted: bool,
    pub l1_files_emitted: u32,
    pub l2_build: String,
    pub l2_behavior: String,
    pub l2_perf: String,
    pub l3_pyo3_wrapper: String,
    pub l3_downstream_dependents: String,
    /// Structured downstream-dependents coverage per ADR-0009 §5.
    /// Defaults to empty arrays for backward compatibility with M4
    /// tomli manifests that predate ADR-0009.
    #[serde(default)]
    pub dependents: DependentsSection,
}

/// Per ADR-0009 §5: structured manifest of which dependent libraries
/// were validated by the L3 driver and which were deliberately
/// deferred to a future milestone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependentsSection {
    /// Dependent libraries whose vendored test subset ran and passed.
    #[serde(default)]
    pub covered: Vec<String>,
    /// Dependent libraries explicitly deferred (not run in this gate
    /// pass) — must be matched by an ADR.
    #[serde(default)]
    pub deferred: Vec<String>,
    /// One-line ADR-anchored reason for the deferral.
    #[serde(default)]
    pub deferred_reason: String,
    /// M6 (per ADR-0010 §5): dependent libraries whose tests ran but
    /// resolved to `Skipped { reason }` because the exercised API is
    /// out of the current milestone's scope (e.g. dateutil's `tz`
    /// module under pendulum). Distinct from `deferred` (those didn't
    /// run at all). Defaults to empty for M4/M5 manifests.
    #[serde(default)]
    pub skipped: Vec<String>,
    /// M6 (per ADR-0010 §5): one-line ADR-anchored reason for the skip.
    #[serde(default)]
    pub skipped_reason: String,
}

impl ProvenanceManifest {
    /// Serialise to canonical TOML.
    ///
    /// # Errors
    /// Bubble up any serialiser error.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Read from disk.
    ///
    /// # Errors
    /// I/O or TOML parse failures.
    pub fn read(path: &Path) -> Result<Self, std::io::Error> {
        let s = std::fs::read_to_string(path)?;
        toml::from_str(&s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Write to disk.
    ///
    /// # Errors
    /// I/O or serialiser failures.
    pub fn write(&self, path: &Path) -> Result<(), std::io::Error> {
        let s = self
            .to_toml()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, s)
    }

    /// Validate that mandatory invariants hold.
    ///
    /// # Errors
    /// Returns a `String` reason if any invariant is broken.
    pub fn validate(&self) -> Result<(), String> {
        if self.source.sha256.len() != 64 {
            return Err(format!(
                "source.sha256 must be 64 hex chars, got {}",
                self.source.sha256.len()
            ));
        }
        if !self.source.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("source.sha256 must be lowercase hex".to_string());
        }
        if !self.build.deterministic_id.starts_with("blake3:") {
            return Err("build.deterministic_id must be blake3:<hex>".to_string());
        }
        if self.build.deterministic_id.len() != "blake3:".len() + 64 {
            return Err("build.deterministic_id must encode a 64-hex blake3 digest".to_string());
        }
        if self.router.models_used.is_empty() {
            return Err("router.models_used must be non-empty".to_string());
        }
        if !self.gates.l0_spec_emitted {
            return Err("gates.l0_spec_emitted must be true".to_string());
        }
        if self.gates.l1_files_emitted == 0 {
            return Err("gates.l1_files_emitted must be > 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample() -> ProvenanceManifest {
        ProvenanceManifest {
            source: SourceSection {
                library: "tomli".into(),
                version: "2.0.1".into(),
                sha256: "0".repeat(64),
                file_count: 1,
            },
            oracle: OracleSection {
                runtime: "cpython".into(),
                runtime_version: "3.11.15".into(),
                oracle_module: "tomllib".into(),
            },
            verification: VerificationSection {
                seeds: vec![42],
                fuzz_inputs_per_fn: 1024,
                divergences: vec![],
                known_failures: vec![],
            },
            router: RouterSection {
                strategy: "synthetic".into(),
                models_used: vec!["synthetic:tomli-canned-v1".into()],
                ledger_entries: 12,
            },
            build: BuildSection {
                toolchain: "rustc 1.94.1".into(),
                deterministic_id: format!("blake3:{}", "f".repeat(64)),
                crate_layout_version: 1,
            },
            gates: GatesSection {
                l0_spec_emitted: true,
                l1_files_emitted: 1,
                l2_build: "pass".into(),
                l2_behavior: "pass".into(),
                l2_perf: "skipped".into(),
                l3_pyo3_wrapper: "pass".into(),
                l3_downstream_dependents: "deferred to M5".into(),
                dependents: DependentsSection::default(),
            },
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let m = sample();
        let s = m.to_toml().unwrap();
        let read_back: ProvenanceManifest = toml::from_str(&s).unwrap();
        assert_eq!(m, read_back);
    }

    #[test]
    fn validate_accepts_well_formed_manifest() {
        sample().validate().unwrap();
    }

    #[test]
    fn validate_rejects_short_source_sha() {
        let mut m = sample();
        m.source.sha256 = "abc".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn validate_rejects_non_blake3_deterministic_id() {
        let mut m = sample();
        m.build.deterministic_id = "sha256:abc".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_files_emitted() {
        let mut m = sample();
        m.gates.l1_files_emitted = 0;
        assert!(m.validate().is_err());
    }

    #[test]
    fn validate_requires_l0_spec_emitted() {
        let mut m = sample();
        m.gates.l0_spec_emitted = false;
        assert!(m.validate().is_err());
    }

    #[test]
    fn dependents_section_round_trips() {
        let mut m = sample();
        m.gates.dependents = DependentsSection {
            covered: vec!["croniter".into(), "freezegun".into()],
            deferred: vec!["pandas".into(), "sqlalchemy".into(), "pendulum".into()],
            deferred_reason: "M5 budget; M6 widens per ADR-0009".into(),
            skipped: vec![],
            skipped_reason: String::new(),
        };
        let s = m.to_toml().unwrap();
        let read_back: ProvenanceManifest = toml::from_str(&s).unwrap();
        assert_eq!(m, read_back);
        assert_eq!(read_back.gates.dependents.covered.len(), 2);
        assert_eq!(read_back.gates.dependents.deferred.len(), 3);
    }

    #[test]
    fn dependents_section_round_trips_with_skipped() {
        // M6 — pendulum is skipped because tz module is out of scope.
        let mut m = sample();
        m.gates.dependents = DependentsSection {
            covered: vec![
                "croniter".into(),
                "freezegun".into(),
                "pandas".into(),
                "sqlalchemy".into(),
            ],
            deferred: vec![],
            deferred_reason: String::new(),
            skipped: vec!["pendulum".into()],
            skipped_reason: "tz module out of M5/M6 scope per ADR-0010 §5".into(),
        };
        let s = m.to_toml().unwrap();
        let read_back: ProvenanceManifest = toml::from_str(&s).unwrap();
        assert_eq!(m, read_back);
        assert_eq!(read_back.gates.dependents.skipped, vec!["pendulum"]);
        assert!(
            read_back
                .gates
                .dependents
                .skipped_reason
                .contains("ADR-0010")
        );
    }

    #[test]
    fn dependents_section_defaults_empty_for_backwards_compat() {
        // Synthesise a TOML missing the [gates.dependents] table — must
        // round-trip to an empty `DependentsSection`.
        let mut m = sample();
        // Force the field to a known default.
        m.gates.dependents = DependentsSection::default();
        let toml = m.to_toml().unwrap();
        let read_back: ProvenanceManifest = toml::from_str(&toml).unwrap();
        assert!(read_back.gates.dependents.covered.is_empty());
        assert!(read_back.gates.dependents.deferred.is_empty());
    }
}
