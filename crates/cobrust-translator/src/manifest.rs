//! Provenance manifest: schema, builder, writer, verifier.
//!
//! Pinned by ADR-0007 §3 ("Provenance manifest schema"). The on-disk
//! form lives at `<crate_dir>/PROVENANCE.toml` next to the generated
//! crate's `Cargo.toml`.
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
}
