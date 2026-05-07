//! L0 — spec extraction.
//!
//! The L0 stage reads a `corpus/<library>/spec.toml` (the L0 product
//! per ADR-0007 §1) and surfaces it as a `SpecToml` value. M4 commits
//! the spec.toml manually; M5+ will generate it via LLM dispatch
//! against the upstream source.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// L0 spec error taxonomy.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("spec.toml not found at {0}")]
    NotFound(String),
    #[error("malformed spec.toml: {0}")]
    Malformed(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Top-level shape of `corpus/<lib>/spec.toml`.
///
/// Schema per ADR-0007 §1.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpecToml {
    pub schema_version: u32,
    pub library: String,
    pub upstream_version: String,
    pub oracle_module: String,
    pub oracle_runtime: String,
    pub oracle_runtime_version: String,
    /// Function name → spec body. We use BTreeMap to keep iteration
    /// order stable across machines (deterministic_id depends on it).
    pub function: BTreeMap<String, FunctionSpec>,
    pub verification: VerificationBudget,
}

/// One function's behavior contract.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub qualname: String,
    #[serde(default)]
    pub public: bool,
    pub signature: String,
    pub py_compat: String,
    pub description: String,
    #[serde(default)]
    pub exemplars: Vec<Exemplar>,
    #[serde(default)]
    pub errors_on: Vec<String>,
}

/// One exemplar input/output pair as recorded in the spec.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Exemplar {
    pub input: String,
    /// JSON-serialised dict (we cannot embed Python literal types in
    /// TOML directly). The harness owns the comparison.
    pub output: toml::Value,
}

/// Verification-budget block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationBudget {
    pub seeds: Vec<u64>,
    pub fuzz_inputs_per_fn: u32,
    pub tolerance: String,
}

impl SpecToml {
    /// Read and parse `corpus/<lib>/spec.toml`.
    ///
    /// # Errors
    /// Returns `SpecError::NotFound` if the file is absent and
    /// `SpecError::Malformed` if the TOML fails to parse.
    pub fn read(path: &Path) -> Result<Self, SpecError> {
        if !path.exists() {
            return Err(SpecError::NotFound(path.display().to_string()));
        }
        let bytes = std::fs::read_to_string(path)?;
        let spec: SpecToml =
            toml::from_str(&bytes).map_err(|e| SpecError::Malformed(e.to_string()))?;
        Ok(spec)
    }

    /// Public-function names in deterministic order.
    #[must_use]
    pub fn public_function_names(&self) -> Vec<String> {
        self.function
            .iter()
            .filter(|(_, f)| f.public)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Every function name in deterministic order. M4's translation
    /// layer iterates this to emit canned responses.
    #[must_use]
    pub fn all_function_names(&self) -> Vec<String> {
        self.function.keys().cloned().collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn fixture() -> &'static str {
        r#"
schema_version = 1
library = "tomli"
upstream_version = "2.0.1"
oracle_module = "tomllib"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.loads]
qualname = "tomli_loads.loads"
public = true
signature = "loads(src: str) -> dict"
py_compat = "strict"
description = "Parse TOML."
exemplars = []
errors_on = []

[function.skip_whitespace]
qualname = "tomli_loads._skip_whitespace"
public = false
signature = "..."
py_compat = "strict"
description = "Skip whitespace."

[verification]
seeds = [42]
fuzz_inputs_per_fn = 100
tolerance = "exact"
"#
    }

    #[test]
    fn parses_minimal_spec() {
        let spec: SpecToml = toml::from_str(fixture()).unwrap();
        assert_eq!(spec.schema_version, 1);
        assert_eq!(spec.library, "tomli");
        assert_eq!(spec.function.len(), 2);
        assert_eq!(spec.public_function_names(), vec!["loads"]);
    }

    #[test]
    fn iteration_is_deterministic() {
        let spec: SpecToml = toml::from_str(fixture()).unwrap();
        let names_a = spec.all_function_names();
        let names_b = spec.all_function_names();
        assert_eq!(names_a, names_b);
        // BTreeMap → alphabetical.
        assert_eq!(
            names_a,
            vec!["loads".to_string(), "skip_whitespace".to_string()]
        );
    }

    #[test]
    fn read_reports_not_found_clearly() {
        let err = SpecToml::read(std::path::Path::new("/no/such/path.toml")).unwrap_err();
        match err {
            SpecError::NotFound(s) => assert!(s.contains("/no/such/path.toml")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}
