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

use std::fmt::Write as _;
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
    /// M6 (per ADR-0010 §2): translation task this entry was served
    /// under (`"translate"` for pure-Python, `"translate_cython"` for
    /// Cython). Defaults to `"translate"` when deserialised from M4/M5
    /// vintage data.
    #[serde(default = "default_translation_task")]
    pub task: String,
}

fn default_translation_task() -> String {
    "translate".to_string()
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
        let task = unit.spec.task.clone();
        let header =
            PromptHeader::first_attempt(task.clone(), unit.name.clone(), plan.source_sha16.clone());
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
            translate_router_err(&task, &unit.name, e)
        })?;
        decision_ids.push(format!(
            "blake3:{}",
            cobrust_llm_router::CacheKey::compute(
                &resp.provider,
                &router_request_for_id(&plan.library, &task, &unit.name, &plan.source_sha16),
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
            task: task.clone(),
        });
    }
    Ok(TranslationOutput {
        library: plan.library.clone(),
        functions: translations,
        router_decision_ids: decision_ids,
    })
}

/// M4 prompt template. Bare-bones — the synthetic provider pattern-
/// matches on the header (task + function), not the body. Retained as
/// the no-context fallback per `adr:0036` Option 2; new callers should
/// prefer [`build_translation_prompt_rich`] with a [`WorkspaceContext`].
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
    s.push_str(&tier_prompt_instruction(&unit.spec.py_compat));
    s.push('\n');
    s
}

/// ADR-0052c §6 tier-aware prompt instruction block. Renders the
/// per-tier directive the L1 translation LLM consumes.
///
/// - `Strict` → bit-identity instruction.
/// - `Numerical { rtol }` → `assert_allclose(rtol=...)` instruction.
/// - `Semantic` → structural-match instruction.
/// - `None` → no-gate disclosure.
#[must_use]
pub fn tier_prompt_instruction(tier: &crate::spec::PyCompatTier) -> String {
    match tier {
        crate::spec::PyCompatTier::Strict => "strict (output MUST be bit-identical to the \
             CPython oracle on all exemplars; any divergence fails the gate)"
            .to_string(),
        crate::spec::PyCompatTier::Numerical { rtol } => format!(
            "numerical(rtol={rtol}) (output MUST satisfy \
             numpy.assert_allclose(rtol={rtol}) vs the oracle; small float drift OK)"
        ),
        crate::spec::PyCompatTier::Semantic => "semantic (output MUST match the oracle \
             structurally; dict key order and error message text may differ provided the \
             error kind matches)"
            .to_string(),
        crate::spec::PyCompatTier::None => "none (no L2 gate; emit the most faithful \
             translation you can)"
            .to_string(),
    }
}

/// Workspace-context bundle the rich prompt builder consumes, per
/// `adr:0036` Option 2. Library authors construct one of these per
/// library (e.g. one for tomli, one for dateutil) and pass it on every
/// L1 dispatch that wants the audit-1-validated rich prompt design.
///
/// Per `adr:0036 §"Decision"`, the bundle carries:
/// - the library's already-translated module preamble (e.g. tomli's
///   `Value` enum + `TomliError` constructor + `State` struct),
/// - one or more already-translated functions presented as few-shot
///   examples of workspace-style,
/// - the verbatim Python source of the target function (kept here so
///   `spec.toml`'s schema does not have to grow),
/// - the Cobrust idiomatic return-type contract for this function,
/// - the Cobrust idiomatic error-construction contract.
///
/// The structural shape of the emitted prompt mirrors the audit-1
/// authoritative `build_rich_prompt` (test-side) verbatim — that
/// design has empirical PASS data on `tomli_loads._parse_bool` (12/12
/// strict tier; see `findings/audit-1-tomli-real-llm-result.md`).
#[derive(Clone, Debug, Default)]
pub struct WorkspaceContext {
    /// Library's already-translated common types. Inserted verbatim
    /// into the rich prompt's "Workspace API contract" section. The
    /// LLM is told these are already in scope and MUST NOT be
    /// redefined.
    pub module_preamble: String,
    /// Already-translated functions presented as few-shot examples.
    /// Each entry is `(function_name, full Rust source)`. N=1 is the
    /// audit-1-validated baseline; N>1 is supported additively.
    pub fewshot_examples: Vec<(String, String)>,
    /// Verbatim Python source of the target function (copied from the
    /// upstream corpus). Empty string is permitted for the bare-fallback
    /// case but the rich variant strongly recommends populating it —
    /// audit-1 PASS data depended on the LLM seeing the source.
    pub target_python_source: String,
    /// Cobrust idiomatic return-type contract for the target function,
    /// e.g. `"Result<bool, TomliError>"`. Inserted as a numbered
    /// directive in the prompt's "Output requirements" section.
    pub return_type_contract: String,
    /// Cobrust idiomatic error-construction contract, e.g.
    /// `"Err(TomliError::new(\"…\", state.pos))"`. Forbids `panic!()`
    /// and `unwrap()` per audit-1's failure-mode analysis (sonnet
    /// branch produced `panic!` without this directive).
    pub error_construction_contract: String,
}

impl WorkspaceContext {
    /// Construct an empty context. Library authors fill in the
    /// library-specific fields. Never returns `Err`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Rich-prompt builder per `adr:0036` Option 2. Bridges an existing
/// [`FunctionUnit`] (from `spec.toml`) and a library-specific
/// [`WorkspaceContext`] into a prompt that carries the audit-1 design
/// (target Python source → workspace API contract → few-shot example →
/// numbered output requirements). The structural shape is verbatim
/// the same as `tests/audit_1_tomli_real_llm.rs::build_rich_prompt`,
/// which has empirical PASS data on the leaf `parse_bool`.
///
/// The bare [`build_translation_prompt`] stays available as the
/// no-context fallback for M4..M-batch synthetic-mode pipelines.
#[must_use]
pub fn build_translation_prompt_rich(unit: &FunctionUnit, ctx: &WorkspaceContext) -> String {
    let mut s = String::new();

    // Header: identifies the library + target. The LLM understands
    // this framing best when it appears before any code.
    s.push_str(
        "You are translating a Python function into idiomatic Rust\n\
         for the Cobrust workspace. The output must compile under\n\
         the workspace lints (deny clippy::pedantic).\n\n",
    );

    // Section 1: target Python source verbatim (if available). Audit-1
    // PASS data showed the LLM follows the source's branch shape much
    // more reliably when given the body, not just the signature.
    if !ctx.target_python_source.trim().is_empty() {
        s.push_str("# Target function (Python source, verbatim)\n\n");
        s.push_str("```python\n");
        s.push_str(ctx.target_python_source.trim_end());
        s.push_str("\n```\n\n");
    }

    // Section 2: workspace API contract — the already-translated types
    // the emission MUST NOT redefine. This is the load-bearing fix vs
    // the audit-1 sonnet PARTIAL-FAIL (which hallucinated field names).
    if !ctx.module_preamble.trim().is_empty() {
        s.push_str(
            "# Workspace API contract (already in scope; do NOT redefine)\n\n\
             The translated function will be glued onto a module that\n\
             already defines the types below. Use these exact field\n\
             names and constructor signatures.\n\n",
        );
        s.push_str("```rust\n");
        s.push_str(ctx.module_preamble.trim_end());
        s.push_str("\n```\n\n");
    }

    // Section 3: few-shot examples — already-translated workspace
    // helpers of the same shape. N=1 is audit-1-validated; we emit
    // every entry caller provides.
    if !ctx.fewshot_examples.is_empty() {
        s.push_str("# Few-shot examples: workspace helpers of the same shape\n\n");
        s.push_str(
            "Match their style: byte-level operations on `state.bytes`\n\
             (not `state.src` chars), `Result<T, ErrType>` return,\n\
             `state.expect(...)?` for required characters, error\n\
             constructor over `panic!`.\n\n",
        );
        for (name, source) in &ctx.fewshot_examples {
            writeln!(s, "## `{name}`\n").expect("write to String never fails");
            s.push_str("```rust\n");
            s.push_str(source.trim_end());
            s.push_str("\n```\n\n");
        }
    }

    // Section 4: output requirements — numbered, explicit, terminal.
    // This is the directive block the audit-1 sonnet PARTIAL-FAIL
    // diagnosis identified as missing.
    s.push_str("# Output requirements\n\n");
    s.push_str(
        "Emit ONLY the Rust function body, no module preamble, no\n\
         imports, no comments outside the function. Specifically:\n\n",
    );
    let return_contract = if ctx.return_type_contract.trim().is_empty() {
        "Result<T, ErrType>"
    } else {
        ctx.return_type_contract.trim()
    };
    writeln!(s, "1. Function signature MUST return `{return_contract}`.")
        .expect("write to String never fails");
    if ctx.error_construction_contract.trim().is_empty() {
        s.push_str(
            "2. Use the workspace error constructor for errors. Do NOT\n   use `panic!()` or `unwrap()`.\n",
        );
    } else {
        let err_contract = ctx.error_construction_contract.trim();
        writeln!(
            s,
            "2. Use `{err_contract}` for errors. Do NOT use `panic!()` or\n   `unwrap()`."
        )
        .expect("write to String never fails");
    }
    s.push_str(
        "3. Do NOT redefine types in the workspace API contract — they\n   are already in scope.\n\
         4. Do NOT wrap the output in markdown code fences.\n",
    );
    let desc = &unit.spec.description;
    writeln!(s, "5. Spec description (from L0): {desc}").expect("write to String never fails");
    let tier_block = tier_prompt_instruction(&unit.spec.py_compat);
    writeln!(s, "6. Py-compat tier: {tier_block}").expect("write to String never fails");
    let sig = &unit.spec.signature;
    writeln!(s, "7. Spec signature (Python): {sig}").expect("write to String never fails");

    s
}

/// Build the canonical "what the cache key would be for this function"
/// request, used solely for `router_decision_id` computation. The body
/// is empty (we do not want the cache key to depend on the prompt
/// template, only on the (function, source SHA) tuple). The library
/// parameter feeds into the model name so determinism stays per-library.
fn router_request_for_id(
    library: &str,
    task: &str,
    function: &str,
    source_sha16: &str,
) -> CompletionRequest {
    let header = PromptHeader::first_attempt(task, function, source_sha16);
    CompletionRequest {
        model: format!("{library}-canned-v1"),
        messages: vec![Message {
            role: Role::User,
            content: format_prompt_body(&header, ""),
        }],
        params: SamplingParams::default(),
    }
}

fn translate_router_err(
    task: &str,
    function: &str,
    e: cobrust_llm_router::RouterError,
) -> TranslatorError {
    if let cobrust_llm_router::RouterError::AllFailed(ref pairs) = e {
        for (_, llm_err) in pairs {
            if let cobrust_llm_router::LlmError::Provider { code, .. } = llm_err
                && code == "synthetic-miss"
            {
                return TranslatorError::SyntheticMiss {
                    task: task.into(),
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
        let r1 = router_request_for_id("tomli", "translate", "loads", "abc123");
        let r2 = router_request_for_id("tomli", "translate", "loads", "abc123");
        assert_eq!(r1, r2);
    }

    #[test]
    fn router_request_for_id_distinguishes_translate_from_translate_cython() {
        let pure = router_request_for_id("msgpack", "translate", "pack", "abc123");
        let cy = router_request_for_id("msgpack", "translate_cython", "pack", "abc123");
        assert_ne!(pure, cy, "task must change request canonical form");
    }

    #[test]
    fn rich_prompt_includes_every_workspace_context_section() {
        // Per `adr:0036` Decision: the rich prompt must reproduce
        // the audit-1 PASS-validated structural shape.
        let spec = fixture_spec();
        let plan = TranslationPlan::from_spec(&spec, "abc".into());
        let ctx = WorkspaceContext {
            module_preamble: "pub struct Demo;".to_string(),
            fewshot_examples: vec![(
                "demo_helper".to_string(),
                "fn demo_helper() -> Result<i64, DemoError> { Ok(0) }".to_string(),
            )],
            target_python_source: "def alpha():\n    return 0\n".to_string(),
            return_type_contract: "Result<i64, DemoError>".to_string(),
            error_construction_contract: "Err(DemoError::new(\"msg\", pos))".to_string(),
        };
        let body = build_translation_prompt_rich(&plan.functions[0], &ctx);

        // Every audit-1-validated section anchor must appear.
        assert!(body.contains("# Target function"));
        assert!(body.contains("def alpha():"));
        assert!(body.contains("# Workspace API contract"));
        assert!(body.contains("pub struct Demo;"));
        assert!(body.contains("# Few-shot examples"));
        assert!(body.contains("`demo_helper`"));
        assert!(body.contains("# Output requirements"));
        assert!(body.contains("Result<i64, DemoError>"));
        assert!(body.contains("Err(DemoError::new"));
        assert!(body.contains("Do NOT use `panic!()`"));
        assert!(body.contains("First."));
        assert!(body.contains("strict"));
    }

    #[test]
    fn rich_prompt_with_empty_context_still_yields_directive_block() {
        // Defensive: an empty WorkspaceContext should still produce a
        // valid prompt (defaults in the directive block) so callers
        // that haven't fully populated the bundle don't crash.
        let spec = fixture_spec();
        let plan = TranslationPlan::from_spec(&spec, "abc".into());
        let body = build_translation_prompt_rich(&plan.functions[0], &WorkspaceContext::new());
        assert!(body.contains("# Output requirements"));
        assert!(body.contains("Result<T, ErrType>"));
        // No "Target function" / "Workspace API" / "Few-shot" sections
        // when the context's corresponding fields are empty.
        assert!(!body.contains("# Target function"));
        assert!(!body.contains("# Workspace API contract"));
        assert!(!body.contains("# Few-shot examples"));
    }
}
