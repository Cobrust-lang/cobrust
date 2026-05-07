//! L1 — translation engine.
//!
//! Function-level, bottom-up by dependency graph. Each unit of work is
//! one entry from the L0 spec; the engine dispatches via
//! [`cobrust_llm_router::Router`] using the
//! [`crate::synthetic::PromptHeader`] format so the synthetic provider
//! can route the request without parsing the prompt body.
//!
//! M4 deliberately keeps the prompt template trivial — the canned
//! response *is* the translation. M5+ will introduce a real prompt
//! template, prompt caching by canonical-source hash, and structured
//! response parsing.

use std::path::PathBuf;

use cobrust_llm_router::{CompletionRequest, Message, Role, Router, SamplingParams, Task};
use serde::{Deserialize, Serialize};

use crate::error::TranslatorError;
use crate::spec::{FunctionSpec, SpecToml};
use crate::synthetic::{PromptHeader, format_prompt_body};

/// Plan: ordered list of translation units to execute.
///
/// Order is "all functions in the spec, alphabetically by name". M4
/// does not yet do dependency-graph analysis (the canned responses are
/// trusted to be self-consistent); M5+ will compute the call graph and
/// schedule leaves first.
#[derive(Clone, Debug)]
pub struct TranslationPlan {
    pub library: String,
    pub upstream_version: String,
    /// Source file SHA-256 truncated to first 16 hex chars; matches
    /// the synthetic provider's staleness check.
    pub source_sha16: String,
    pub functions: Vec<FunctionUnit>,
}

#[derive(Clone, Debug)]
pub struct FunctionUnit {
    pub name: String,
    pub spec: FunctionSpec,
}

impl TranslationPlan {
    /// Build a plan from a parsed spec + a source SHA. M4 alphabetises
    /// the function list for determinism; M5+ replaces this with a
    /// dependency-graph schedule.
    #[must_use]
    pub fn from_spec(spec: &SpecToml, source_sha16: String) -> Self {
        let mut functions = Vec::with_capacity(spec.function.len());
        for (name, fn_spec) in &spec.function {
            functions.push(FunctionUnit {
                name: name.clone(),
                spec: fn_spec.clone(),
            });
        }
        Self {
            library: spec.library.clone(),
            upstream_version: spec.upstream_version.clone(),
            source_sha16,
            functions,
        }
    }
}

/// Output of running the L1 engine over a plan.
#[derive(Clone, Debug)]
pub struct TranslationOutput {
    pub library: String,
    pub functions: Vec<FunctionTranslation>,
    /// Router decision ids for the deterministic_id computation.
    pub router_decision_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionTranslation {
    pub name: String,
    pub source_sha16: String,
    /// Provider name + model that served this function (always
    /// `synthetic:tomli-canned-v1` in M4 synthetic mode).
    pub provider: String,
    pub model: String,
    pub cache_hit: bool,
    /// Cache key (`blake3:<hex>`). Doubles as the router decision id
    /// for deterministic-id input.
    pub router_decision_id: String,
    pub emitted_text: String,
}

/// One file the L1 engine writes into the translated crate. M4 emits
/// a single `parser.rs` collecting every function, plus `lib.rs`
/// stitched by the manifest writer.
#[derive(Clone, Debug)]
pub struct EmittedFile {
    pub relative_path: PathBuf,
    pub contents: String,
}

/// Run L1 over a plan using the given router. The router must have at
/// least one provider registered for [`Task::Translate`].
///
/// # Errors
/// `TranslatorError::Router` if dispatch fails permanently;
/// `TranslatorError::SyntheticMiss` (lifted from the router's
/// `Provider { code: "synthetic-miss" }` error) if the synthetic
/// provider has no canned response.
pub async fn run_l1(
    router: &Router,
    plan: &TranslationPlan,
) -> Result<TranslationOutput, TranslatorError> {
    let mut translations = Vec::with_capacity(plan.functions.len());
    let mut decision_ids = Vec::with_capacity(plan.functions.len());
    for unit in &plan.functions {
        let header =
            PromptHeader::first_attempt("translate", unit.name.clone(), plan.source_sha16.clone());
        let body = build_translation_prompt(unit);
        let prompt = format_prompt_body(&header, &body);
        let req = CompletionRequest {
            model: format!("{}-canned-v1", plan.library),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            params: SamplingParams {
                max_tokens: Some(8_192),
                temperature: Some(0.0),
                top_p: None,
                stop: vec![],
            },
        };
        let resp = router.dispatch(Task::Translate, req).await.map_err(|e| {
            // Lift synthetic-miss into a structured TranslatorError.
            translate_router_err(&unit.name, e)
        })?;
        decision_ids.push(format!(
            "blake3:{}",
            cobrust_llm_router::CacheKey::compute(
                &resp.provider,
                &router_request_for_id(&plan.library, &unit.name, &plan.source_sha16),
            )
            .hex()
        ));
        translations.push(FunctionTranslation {
            name: unit.name.clone(),
            source_sha16: plan.source_sha16.clone(),
            provider: resp.provider,
            model: resp.response.model.clone(),
            cache_hit: resp.cache_hit,
            router_decision_id: decision_ids.last().cloned().unwrap_or_default(),
            emitted_text: resp.response.text,
        });
    }
    Ok(TranslationOutput {
        library: plan.library.clone(),
        functions: translations,
        router_decision_ids: decision_ids,
    })
}

/// M4 prompt template. Bare-bones — the synthetic provider pattern-
/// matches on the header (task + function), not the body.
fn build_translation_prompt(unit: &FunctionUnit) -> String {
    let mut s = String::new();
    s.push_str(
        "Translate the following Python function to idiomatic Rust\n\
         using only stable std crate types. The output must compile\n\
         under the workspace lints (deny clippy::pedantic).\n\n",
    );
    s.push_str("Signature: ");
    s.push_str(&unit.spec.signature);
    s.push_str("\nDescription: ");
    s.push_str(&unit.spec.description);
    s.push_str("\nPy-compat tier: ");
    s.push_str(&unit.spec.py_compat);
    s.push('\n');
    s
}

/// Build the canonical "what the cache key would be for this function"
/// request, used solely for `router_decision_id` computation. The body
/// is empty (we do not want the cache key to depend on the prompt
/// template, only on the (function, source SHA) tuple). The library
/// parameter feeds into the model name so determinism stays per-library.
fn router_request_for_id(library: &str, function: &str, source_sha16: &str) -> CompletionRequest {
    let header = PromptHeader::first_attempt("translate", function, source_sha16);
    CompletionRequest {
        model: format!("{library}-canned-v1"),
        messages: vec![Message {
            role: Role::User,
            content: format_prompt_body(&header, ""),
        }],
        params: SamplingParams::default(),
    }
}

fn translate_router_err(function: &str, e: cobrust_llm_router::RouterError) -> TranslatorError {
    if let cobrust_llm_router::RouterError::AllFailed(ref pairs) = e {
        for (_, llm_err) in pairs {
            if let cobrust_llm_router::LlmError::Provider { code, .. } = llm_err
                && code == "synthetic-miss"
            {
                return TranslatorError::SyntheticMiss {
                    task: "translate".into(),
                    function: function.into(),
                };
            }
        }
    }
    TranslatorError::Router(e)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn fixture_spec() -> SpecToml {
        let s = r#"
schema_version = 1
library = "tomli"
upstream_version = "2.0.1"
oracle_module = "tomllib"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.alpha]
qualname = "x.alpha"
public = true
signature = "alpha() -> int"
py_compat = "strict"
description = "First."

[function.zeta]
qualname = "x.zeta"
public = false
signature = "zeta() -> int"
py_compat = "strict"
description = "Last alphabetically."

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#;
        toml::from_str(s).unwrap()
    }

    #[test]
    fn plan_alphabetises_functions_for_determinism() {
        let spec = fixture_spec();
        let plan = TranslationPlan::from_spec(&spec, "abc".into());
        assert_eq!(plan.functions.len(), 2);
        assert_eq!(plan.functions[0].name, "alpha");
        assert_eq!(plan.functions[1].name, "zeta");
    }

    #[test]
    fn build_prompt_includes_signature_and_description() {
        let spec = fixture_spec();
        let plan = TranslationPlan::from_spec(&spec, "abc".into());
        let body = build_translation_prompt(&plan.functions[0]);
        assert!(body.contains("alpha() -> int"));
        assert!(body.contains("First."));
        assert!(body.contains("strict"));
    }

    #[test]
    fn router_request_for_id_is_deterministic() {
        let r1 = router_request_for_id("tomli", "loads", "abc123");
        let r2 = router_request_for_id("tomli", "loads", "abc123");
        assert_eq!(r1, r2);
    }
}
