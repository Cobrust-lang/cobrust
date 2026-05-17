//! L0 — spec extraction.
//!
//! The L0 stage reads a `corpus/<library>/spec.toml` (the L0 product
//! per ADR-0007 §1) and surfaces it as a `SpecToml` value. M4 commits
//! the spec.toml manually; M5+ will generate it via LLM dispatch
//! against the upstream source.
//!
//! ADR-0052c §4 (Wave-2) replaces the raw `String` `py_compat` field
//! with the typed [`PyCompatTier`] enum via a custom serde Deserialize
//! impl on [`FunctionSpec`]. The custom impl accepts the canonical
//! tier strings (`"strict"`, `"semantic"`, `"numerical(rtol=…)"`,
//! `"none"`) and ALSO the M7+ numpy-corpus sidecar form
//! (`py_compat = "numerical"` + `py_compat_rtol = 1e-7`). Malformed
//! strings reject at parse time per the §2.5 compile-time-catch
//! contract.

use std::collections::BTreeMap;
use std::path::Path;

use serde::de;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

/// `@py_compat` tier per ADR-0052c §3 matrix.
///
/// Tier semantics:
///
/// - [`PyCompatTier::Strict`] — byte-identical oracle output; any
///   divergence rejects. Canonical strict equality.
/// - [`PyCompatTier::Semantic`] — structural-equivalence permitted
///   (dict key order, error-message text drift OK provided error
///   *kind* matches).
/// - [`PyCompatTier::Numerical`] — `assert_allclose(rtol=…)` semantics
///   for f64 comparisons. The `rtol` payload binds the tolerance.
/// - [`PyCompatTier::None`] — gate disabled; verifier accepts
///   unconditionally + manifest records
///   [`crate::pipeline::GateOutcome::Skip`] honestly per ADR-0040.
///
/// The string form of each variant matches the existing corpus TOML
/// strings (backward-compat with the `M4..M7` corpus PROVENANCEs):
///
/// - `"strict"` ↔ [`PyCompatTier::Strict`]
/// - `"semantic"` ↔ [`PyCompatTier::Semantic`]
/// - `"numerical(rtol=1e-7)"` ↔ `PyCompatTier::Numerical { rtol: 1e-7 }`
/// - `"numerical"` (bare) + sibling `py_compat_rtol = 1e-7` (M7+
///   numpy-corpus sidecar form) ↔ `PyCompatTier::Numerical { rtol: 1e-7 }`
/// - `"numerical"` bare with no sidecar ↔ defaults to `rtol = 1e-7`
/// - `"none"` ↔ [`PyCompatTier::None`]
#[derive(Clone, Debug, PartialEq)]
pub enum PyCompatTier {
    /// Byte-identical oracle output required.
    Strict,
    /// Structural equivalence permitted.
    Semantic,
    /// `assert_allclose(rtol=...)` semantics for f64 comparisons.
    Numerical {
        /// Relative tolerance for `assert_allclose`. Mirrors the NumPy
        /// `numpy.testing.assert_allclose(rtol=...)` canonical idiom.
        rtol: f64,
    },
    /// Gate disabled. Verifier accepts unconditionally; manifest
    /// records [`crate::pipeline::GateOutcome::Skip`] per ADR-0040.
    None,
}

/// Default rtol for the bare `"numerical"` form (matches existing
/// `corpus/numpy/M7.1/spec.toml` sidecar value).
const DEFAULT_NUMERICAL_RTOL: f64 = 1e-7;

impl PyCompatTier {
    /// Parse the canonical string form per ADR-0052c §4.
    ///
    /// Accepts:
    /// - `"strict"` → `Strict`
    /// - `"semantic"` → `Semantic`
    /// - `"numerical(rtol=<value>)"` → `Numerical { rtol: <value> }`
    /// - `"none"` → `None`
    ///
    /// Does NOT accept the bare `"numerical"` form (handled at
    /// `FunctionSpec` deserialize time so the sibling `py_compat_rtol`
    /// field is available). Bare `"numerical"` passed here returns
    /// `Err`; pass it through the `FunctionSpec` deserialize path
    /// instead.
    ///
    /// # Errors
    /// Returns a diagnostic naming the offending input and listing the
    /// expected variants per §2.5 compile-time-catch contract.
    pub fn parse_strict_string(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        match trimmed {
            "strict" => Ok(Self::Strict),
            "semantic" => Ok(Self::Semantic),
            "none" => Ok(Self::None),
            "numerical" => Err(format!(
                "py_compat: bare \"numerical\" needs explicit rtol; use \
                 \"numerical(rtol=<value>)\" or supply sibling \
                 py_compat_rtol = <value> per ADR-0052c §4 (got {trimmed:?})"
            )),
            other if other.starts_with("numerical(") && other.ends_with(')') => {
                let inside = &other["numerical(".len()..other.len() - 1];
                let payload = inside.trim();
                let rest = payload.strip_prefix("rtol").ok_or_else(|| {
                    format!(
                        "py_compat: malformed \"numerical(...)\" — expected \
                         numerical(rtol=<value>); got {other:?}"
                    )
                })?;
                let rest = rest.trim_start();
                let rest = rest.strip_prefix('=').ok_or_else(|| {
                    format!(
                        "py_compat: malformed \"numerical(...)\" — missing \
                         '=' after rtol; got {other:?}"
                    )
                })?;
                let value_text = rest.trim();
                if value_text.is_empty() {
                    return Err(format!(
                        "py_compat: malformed \"numerical(rtol=<value>)\" — \
                         rtol value is empty (got {other:?})"
                    ));
                }
                let rtol: f64 = value_text.parse().map_err(|e| {
                    format!(
                        "py_compat: malformed \"numerical(rtol=<value>)\" — \
                         could not parse rtol={value_text:?} as f64: {e}"
                    )
                })?;
                if !rtol.is_finite() || rtol <= 0.0 {
                    return Err(format!(
                        "py_compat: numerical rtol must be finite and > 0 \
                         (got {rtol})"
                    ));
                }
                Ok(Self::Numerical { rtol })
            }
            other => Err(format!(
                "py_compat: unknown tier {other:?}; expected \
                 strict|semantic|numerical(rtol=…)|none"
            )),
        }
    }

    /// Canonical string form for serialization. Round-trips through
    /// [`Self::parse_strict_string`] (modulo the bare `"numerical"`
    /// shorthand which canonicalises to `"numerical(rtol=…)"`).
    #[must_use]
    pub fn canonical_string(&self) -> String {
        match self {
            Self::Strict => "strict".to_string(),
            Self::Semantic => "semantic".to_string(),
            Self::Numerical { rtol } => format!("numerical(rtol={rtol})"),
            Self::None => "none".to_string(),
        }
    }
}

impl std::fmt::Display for PyCompatTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical_string())
    }
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
///
/// Per ADR-0052c §4, `py_compat` is the typed [`PyCompatTier`] enum.
/// The custom serde [`Deserialize`] / [`Serialize`] impls on
/// `FunctionSpec` accept the canonical TOML string forms AND the M7+
/// numpy-corpus sidecar form (`py_compat = "numerical"` +
/// `py_compat_rtol = <value>`) for backward-compat.
#[derive(Clone, Debug)]
pub struct FunctionSpec {
    pub qualname: String,
    pub public: bool,
    pub signature: String,
    pub py_compat: PyCompatTier,
    pub description: String,
    pub exemplars: Vec<Exemplar>,
    pub errors_on: Vec<String>,
    /// M6 (per ADR-0010 §2): translation task this function uses.
    /// Defaults to "translate" (pure-Python). Set to
    /// "translate_cython" for entries derived from .pyx sources.
    /// Backward-compatible: M4 tomli + M5 dateutil specs omit the
    /// field; serde defaults preserve their behaviour.
    pub task: String,
}

impl FunctionSpec {
    /// Construct a minimal `FunctionSpec` with default fields. Used by
    /// callers that need a literal-constructor form (the audit-3a
    /// stateful test builds a `FunctionUnit` programmatically rather
    /// than via TOML parse, so it needs this constructor after the
    /// `String` → [`PyCompatTier`] type migration).
    #[must_use]
    pub fn new(
        qualname: impl Into<String>,
        signature: impl Into<String>,
        py_compat: PyCompatTier,
        description: impl Into<String>,
    ) -> Self {
        Self {
            qualname: qualname.into(),
            public: false,
            signature: signature.into(),
            py_compat,
            description: description.into(),
            exemplars: Vec::new(),
            errors_on: Vec::new(),
            task: "translate".to_string(),
        }
    }
}

/// Intermediate raw deserialize struct — matches the on-disk TOML
/// schema directly (including the sidecar `py_compat_rtol` field used
/// by M7+ numpy corpus PROVENANCEs).
#[derive(Deserialize)]
struct FunctionSpecRaw {
    qualname: String,
    #[serde(default)]
    public: bool,
    signature: String,
    py_compat: String,
    /// M7+ numpy-corpus sidecar form: `py_compat = "numerical"` +
    /// `py_compat_rtol = 1e-7`. Optional; when present and the
    /// `py_compat` string is bare `"numerical"`, the rtol payload is
    /// read from here.
    #[serde(default)]
    py_compat_rtol: Option<f64>,
    description: String,
    #[serde(default)]
    exemplars: Vec<Exemplar>,
    #[serde(default)]
    errors_on: Vec<String>,
    #[serde(default = "default_task")]
    task: String,
}

impl<'de> Deserialize<'de> for FunctionSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = FunctionSpecRaw::deserialize(deserializer)?;
        let py_compat =
            resolve_py_compat(&raw.py_compat, raw.py_compat_rtol).map_err(de::Error::custom)?;
        Ok(Self {
            qualname: raw.qualname,
            public: raw.public,
            signature: raw.signature,
            py_compat,
            description: raw.description,
            exemplars: raw.exemplars,
            errors_on: raw.errors_on,
            task: raw.task,
        })
    }
}

impl Serialize for FunctionSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("qualname", &self.qualname)?;
        map.serialize_entry("public", &self.public)?;
        map.serialize_entry("signature", &self.signature)?;
        map.serialize_entry("py_compat", &self.py_compat.canonical_string())?;
        // Numerical tier also emits the sidecar rtol so M7+ readers
        // that consume the bare form still see the value.
        if let PyCompatTier::Numerical { rtol } = &self.py_compat {
            map.serialize_entry("py_compat_rtol", rtol)?;
        }
        map.serialize_entry("description", &self.description)?;
        map.serialize_entry("exemplars", &self.exemplars)?;
        map.serialize_entry("errors_on", &self.errors_on)?;
        map.serialize_entry("task", &self.task)?;
        map.end()
    }
}

/// Resolve the `py_compat` string + optional sidecar `py_compat_rtol`
/// into a typed [`PyCompatTier`] per ADR-0052c §4.
fn resolve_py_compat(tier_string: &str, sidecar_rtol: Option<f64>) -> Result<PyCompatTier, String> {
    let trimmed = tier_string.trim();
    if trimmed == "numerical" {
        // Bare numerical: read the sidecar if present, else default to
        // 1e-7 (matches existing M7+ corpus baseline).
        let rtol = sidecar_rtol.unwrap_or(DEFAULT_NUMERICAL_RTOL);
        if !rtol.is_finite() || rtol <= 0.0 {
            return Err(format!(
                "py_compat: numerical rtol must be finite and > 0 (got {rtol})"
            ));
        }
        return Ok(PyCompatTier::Numerical { rtol });
    }
    // For non-bare-numerical tiers the sidecar is ignored.
    PyCompatTier::parse_strict_string(trimmed)
}

fn default_task() -> String {
    "translate".to_string()
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
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
        assert!(matches!(
            spec.function["loads"].py_compat,
            PyCompatTier::Strict
        ));
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

    // ---- ADR-0052c §4 PyCompatTier parser unit tests ---------------------

    #[test]
    fn parse_strict_string_strict() {
        assert!(matches!(
            PyCompatTier::parse_strict_string("strict").unwrap(),
            PyCompatTier::Strict
        ));
    }

    #[test]
    fn parse_strict_string_semantic() {
        assert!(matches!(
            PyCompatTier::parse_strict_string("semantic").unwrap(),
            PyCompatTier::Semantic
        ));
    }

    #[test]
    fn parse_strict_string_none() {
        assert!(matches!(
            PyCompatTier::parse_strict_string("none").unwrap(),
            PyCompatTier::None
        ));
    }

    #[test]
    fn parse_strict_string_numerical_with_rtol() {
        let t = PyCompatTier::parse_strict_string("numerical(rtol=1e-7)").unwrap();
        match t {
            PyCompatTier::Numerical { rtol } => assert_eq!(rtol, 1e-7),
            other => panic!("expected Numerical, got {other:?}"),
        }
    }

    #[test]
    fn parse_strict_string_numerical_rejects_empty_rtol() {
        let err = PyCompatTier::parse_strict_string("numerical(rtol=)").unwrap_err();
        assert!(err.contains("rtol") || err.contains("numerical"));
    }

    #[test]
    fn parse_strict_string_rejects_typo() {
        let err = PyCompatTier::parse_strict_string("strikt").unwrap_err();
        assert!(err.contains("strikt"));
        assert!(err.contains("strict"));
    }

    #[test]
    fn parse_strict_string_rejects_bare_numerical() {
        // Bare "numerical" without a sidecar must go through the
        // FunctionSpec deserialize path (which has access to the
        // sibling py_compat_rtol field). Calling parse_strict_string
        // directly with "numerical" must Err.
        let err = PyCompatTier::parse_strict_string("numerical").unwrap_err();
        assert!(err.contains("numerical"));
    }

    #[test]
    fn canonical_string_round_trips() {
        let cases = [
            PyCompatTier::Strict,
            PyCompatTier::Semantic,
            PyCompatTier::Numerical { rtol: 1e-7 },
            PyCompatTier::None,
        ];
        for case in cases {
            let s = case.canonical_string();
            let parsed = PyCompatTier::parse_strict_string(&s).unwrap();
            assert_eq!(parsed, case, "round-trip failed for {case:?}");
        }
    }

    #[test]
    fn bare_numerical_with_sidecar_rtol_loads() {
        // Mirrors `corpus/numpy/M7.1/spec.toml` exactly.
        let toml = r#"
schema_version = 1
library = "numpy"
upstream_version = "2.0.2"
oracle_module = "numpy"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.add]
qualname = "ufunc_core.add"
public = true
signature = "add(a, b) -> tuple"
py_compat = "numerical"
py_compat_rtol = 1e-7
description = "Element-wise add."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
        let spec: SpecToml = toml::from_str(toml).expect("bare numerical + sidecar must parse");
        let add = spec.function.get("add").expect("add exists");
        match &add.py_compat {
            PyCompatTier::Numerical { rtol } => assert_eq!(*rtol, 1e-7),
            other => panic!("expected Numerical, got {other:?}"),
        }
    }

    #[test]
    fn bare_numerical_without_sidecar_defaults_rtol() {
        let toml = r#"
schema_version = 1
library = "numpy"
upstream_version = "2.0.2"
oracle_module = "numpy"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.bare]
qualname = "x.bare"
public = true
signature = "bare()"
py_compat = "numerical"
description = "Bare numerical."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
        let spec: SpecToml =
            toml::from_str(toml).expect("bare numerical without sidecar must parse");
        match &spec.function["bare"].py_compat {
            PyCompatTier::Numerical { rtol } => assert_eq!(*rtol, 1e-7),
            other => panic!("expected Numerical, got {other:?}"),
        }
    }

    #[test]
    fn strikt_typo_rejected_with_diagnostic() {
        let toml = r#"
schema_version = 1
library = "x"
upstream_version = "0.0.1"
oracle_module = "x"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.f]
qualname = "x.f"
public = true
signature = "f()"
py_compat = "strikt"
description = "Typo."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
        let err = toml::from_str::<SpecToml>(toml).unwrap_err().to_string();
        assert!(err.contains("strikt"));
        assert!(err.contains("strict"));
    }
}
