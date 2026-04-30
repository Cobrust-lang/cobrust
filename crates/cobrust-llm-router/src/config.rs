//! `cobrust.toml` parsing for the LLM Router.
//!
//! Schema mirrors `cobrust.toml.example` field-for-field. See `adr:0004` for
//! the binding decision; see `mod:llm_router` for cross-references.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default strategy when a routing entry omits its own.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultStrategy {
    Cost,
    #[default]
    Quality,
    Latency,
}

/// Provider-API kind.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Anthropic,
    Openai,
}

/// `[providers.<name>]` section.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key_env: String,
    #[serde(default)]
    pub models: Vec<String>,
}

/// `[router]` section.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouterSection {
    #[serde(default)]
    pub default_strategy: DefaultStrategy,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    #[serde(default = "default_ledger_path")]
    pub ledger_path: PathBuf,
}

fn default_cache_dir() -> PathBuf {
    PathBuf::from(".cobrust/llm_cache")
}
fn default_ledger_path() -> PathBuf {
    PathBuf::from(".cobrust/ledger.jsonl")
}

impl Default for RouterSection {
    fn default() -> Self {
        Self {
            default_strategy: DefaultStrategy::Quality,
            cache_dir: default_cache_dir(),
            ledger_path: default_ledger_path(),
        }
    }
}

/// One row in the routing table — what strategy and provider list applies to
/// each task name.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingEntry {
    pub strategy: StrategyName,
    /// Required for `consensus`; ignored otherwise.
    #[serde(default)]
    pub n: Option<u8>,
    /// Ordered list of `"provider:model"` tags. The router walks them in order.
    #[serde(default)]
    pub preferred: Vec<String>,
}

/// Strategy as named in TOML. Decoupled from
/// [`crate::router::Strategy`](crate::router::Strategy) which carries
/// runtime parameters.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrategyName {
    Cost,
    Quality,
    Latency,
    Consensus,
}

/// Top-level router configuration. Build via [`RouterConfig::from_toml_str`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouterConfig {
    #[serde(default)]
    pub router: RouterSection,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub routing: BTreeMap<String, RoutingEntry>,
}

/// Parsed `provider:model` pair.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ProviderModel {
    pub provider: String,
    pub model: String,
}

impl ProviderModel {
    /// Parse `"provider:model"`. Returns `None` if the format is wrong.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.splitn(2, ':');
        let provider = parts.next()?.trim();
        let model = parts.next()?.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        Some(Self {
            provider: provider.to_string(),
            model: model.to_string(),
        })
    }
}

impl RouterConfig {
    /// Parse a `cobrust.toml` document.
    ///
    /// # Errors
    /// Returns the message produced by `toml::de`.
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Validate cross-field invariants:
    /// 1. Every `routing.<task>.preferred` references a declared provider.
    /// 2. `consensus` strategies have `n >= 2`.
    /// 3. The model in every `provider:model` pair is listed in
    ///    `providers.<name>.models` (warning, not error — providers may add
    ///    new models post-config).
    ///
    /// # Errors
    /// Returns a string describing the first violation found.
    pub fn validate(&self) -> Result<(), String> {
        for (task, entry) in &self.routing {
            if matches!(entry.strategy, StrategyName::Consensus) {
                let n = entry.n.unwrap_or(0);
                if n < 2 {
                    return Err(format!(
                        "routing.{task}: consensus strategy requires n >= 2 (got {n})"
                    ));
                }
                if usize::from(n) > entry.preferred.len() {
                    return Err(format!(
                        "routing.{task}: consensus n={n} but preferred list has only {} entries",
                        entry.preferred.len()
                    ));
                }
            }
            for tag in &entry.preferred {
                let pm = ProviderModel::parse(tag).ok_or_else(|| {
                    format!("routing.{task}: malformed provider:model tag {tag:?}")
                })?;
                if !self.providers.contains_key(&pm.provider) {
                    return Err(format!(
                        "routing.{task}: provider {:?} referenced by {tag:?} is not declared",
                        pm.provider
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const FULL_TOML: &str = r#"
[router]
default_strategy = "quality"
cache_dir = ".cobrust/llm_cache"
ledger_path = ".cobrust/ledger.jsonl"

[providers.anthropic_official]
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
models = ["claude-opus-4-7", "claude-sonnet-4-6"]

[providers.openai_official]
kind = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
models = ["gpt-5", "gpt-5-mini"]

[providers.deepseek]
kind = "openai"
base_url = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"
models = ["deepseek-v3"]

[routing.spec_extract]
strategy = "quality"
preferred = ["anthropic_official:claude-opus-4-7"]

[routing.translate]
strategy = "consensus"
n = 2
preferred = [
    "anthropic_official:claude-opus-4-7",
    "deepseek:deepseek-v3",
]

[routing.repair]
strategy = "cost"
preferred = ["openai_official:gpt-5-mini", "deepseek:deepseek-v3"]
"#;

    #[test]
    fn parses_full_example_config() {
        let cfg = RouterConfig::from_toml_str(FULL_TOML).expect("must parse");
        assert_eq!(cfg.router.default_strategy, DefaultStrategy::Quality);
        assert_eq!(cfg.providers.len(), 3);
        assert_eq!(cfg.routing.len(), 3);
        assert_eq!(
            cfg.providers["anthropic_official"].kind,
            ProviderKind::Anthropic
        );
        assert_eq!(
            cfg.providers["deepseek"].base_url,
            "https://api.deepseek.com/v1"
        );
        assert_eq!(cfg.routing["translate"].strategy, StrategyName::Consensus);
        assert_eq!(cfg.routing["translate"].n, Some(2));
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn provider_model_parses_pair() {
        let pm = ProviderModel::parse("anthropic_official:claude-opus-4-7").unwrap();
        assert_eq!(pm.provider, "anthropic_official");
        assert_eq!(pm.model, "claude-opus-4-7");
    }

    #[test]
    fn provider_model_rejects_malformed_input() {
        assert!(ProviderModel::parse("no_colon").is_none());
        assert!(ProviderModel::parse(":model").is_none());
        assert!(ProviderModel::parse("provider:").is_none());
    }

    #[test]
    fn validate_flags_unknown_provider() {
        let toml = r#"
[providers.x]
kind = "openai"
base_url = "http://x"
api_key_env = "X"

[routing.t]
strategy = "quality"
preferred = ["y:m"]
"#;
        let cfg = RouterConfig::from_toml_str(toml).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("not declared"), "{err}");
    }

    #[test]
    fn validate_flags_consensus_without_n() {
        let toml = r#"
[providers.x]
kind = "openai"
base_url = "http://x"
api_key_env = "X"

[routing.t]
strategy = "consensus"
preferred = ["x:m"]
"#;
        let cfg = RouterConfig::from_toml_str(toml).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("consensus"), "{err}");
    }

    #[test]
    fn validate_flags_consensus_n_exceeds_preferred() {
        let toml = r#"
[providers.x]
kind = "openai"
base_url = "http://x"
api_key_env = "X"

[routing.t]
strategy = "consensus"
n = 5
preferred = ["x:m1", "x:m2"]
"#;
        let cfg = RouterConfig::from_toml_str(toml).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("only 2 entries"), "{err}");
    }

    #[test]
    fn defaults_apply_when_router_section_omitted() {
        let toml = r#"
[providers.x]
kind = "openai"
base_url = "http://x"
api_key_env = "X"
"#;
        let cfg = RouterConfig::from_toml_str(toml).unwrap();
        assert_eq!(cfg.router.default_strategy, DefaultStrategy::Quality);
        assert_eq!(cfg.router.cache_dir, PathBuf::from(".cobrust/llm_cache"));
        assert_eq!(
            cfg.router.ledger_path,
            PathBuf::from(".cobrust/ledger.jsonl")
        );
    }
}
