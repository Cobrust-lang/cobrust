//! Translator runtime configuration.
//!
//! `TranslatorConfig` aggregates the router config (M3 surface) plus
//! M4-specific knobs (output directory, oracle module, escalation
//! threshold, synthetic-only flag).

use std::path::PathBuf;

use cobrust_llm_router::RouterConfig;

/// Runtime knobs for one translation run.
///
/// Keep small — load-bearing decisions belong in `adr:0007` and the
/// provenance manifest, not here.
#[derive(Clone, Debug)]
pub struct TranslatorConfig {
    /// Resolved router configuration (M3 surface). Carries provider
    /// list, routing table, cache and ledger paths.
    pub router: RouterConfig,
    /// Where the pipeline writes the generated crate. The library
    /// name is appended automatically (`out_dir / cobrust-<library>`).
    pub out_dir: PathBuf,
    /// Oracle runtime label for the manifest (e.g. `"cpython 3.11"`).
    pub oracle_runtime: String,
    /// Oracle module import path (e.g. `"tomllib"`).
    pub oracle_module: String,
    /// Repair-loop escalation threshold per ADR-0007 §"Failure routing".
    /// After this many retries on the same function, the function is
    /// marked `@py_compat(none)` with a human-readable failure report.
    pub escalation_threshold: u32,
    /// When `true`, the pipeline only registers a `SyntheticProvider`
    /// for the configured providers. M4 default is `true`. M5+ flips
    /// to `false` once a real-LLM smoke test is wired up.
    pub synthetic_only: bool,
}

impl TranslatorConfig {
    /// Default escalation threshold per constitution §4.2.
    pub const DEFAULT_ESCALATION_THRESHOLD: u32 = 50;

    /// Build a config with M4 defaults: synthetic-only, default
    /// escalation threshold, oracle = `"cpython 3.11"` / `"tomllib"`.
    #[must_use]
    pub fn m4_synthetic(router: RouterConfig, out_dir: PathBuf) -> Self {
        Self {
            router,
            out_dir,
            oracle_runtime: "cpython 3.11".into(),
            oracle_module: "tomllib".into(),
            escalation_threshold: Self::DEFAULT_ESCALATION_THRESHOLD,
            synthetic_only: true,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn router_cfg() -> RouterConfig {
        let toml = r#"
[router]
default_strategy = "quality"
cache_dir = "/tmp/c"
ledger_path = "/tmp/l"

[providers.synthetic]
kind = "openai"
base_url = "http://x"
api_key_env = "K"
models = ["tomli-canned-v1"]

[routing.translate]
strategy = "quality"
preferred = ["synthetic:tomli-canned-v1"]
"#;
        RouterConfig::from_toml_str(toml).unwrap()
    }

    #[test]
    fn m4_synthetic_uses_constitution_threshold() {
        let cfg = TranslatorConfig::m4_synthetic(router_cfg(), PathBuf::from("/tmp/o"));
        assert_eq!(cfg.escalation_threshold, 50);
        assert!(cfg.synthetic_only);
        assert_eq!(cfg.oracle_module, "tomllib");
    }
}
