//! Repair-loop driver for the translator pipeline.
//!
//! When an L2 or L3 gate rejects a translation, the diagnostic blob
//! defined by [`GateFailure`] is shipped back into L1 as a follow-up
//! prompt with `attempt: N+1` in the header (see ADR-0008 §3 + §5).
//! The synthetic provider routes attempt-N to its attempt-N entry;
//! the real-LLM provider sees the diagnostic appended to the prompt
//! body and produces a corrected response.
//!
//! Constitution §4.2 retry threshold: default 50. After that, the
//! pipeline writes a human-readable `failure_report.md` and raises
//! [`crate::error::TranslatorError::EscalationExceeded`].

use std::path::{Path, PathBuf};
use std::str::FromStr;

use cobrust_llm_router::{CompletionRequest, Message, Role, Router, SamplingParams, Task};
use serde::{Deserialize, Serialize};

use std::fmt::{self, Write as _};

use crate::error::TranslatorError;
use crate::synthetic::{PromptHeader, format_prompt_body};
use crate::translate::FunctionTranslation;

/// The L2 verification-gate identity space (constitution §4.2). This is
/// the gate that *caught* a divergence — distinct from the gate
/// *outcome* (`"pass"` / `"skipped"` / `"fail"`) carried by
/// [`crate::manifest`]'s `l2_build` / `l2_behavior` / `l2_perf` fields.
///
/// §2.5 compile-time-catch: an invalid or typo'd gate name is
/// unrepresentable, and every consumer must handle all variants
/// exhaustively (no `_ =>` catch-all anywhere), so adding a fourth gate
/// later fails to compile until each site is updated.
///
/// The on-disk / on-wire representation is the exact gate string
/// (`"l2_build"` / `"l2_behavior"` / `"l2_perf"`), preserved via the
/// [`serde`] attributes below so the TOML diagnostic round-trip and
/// every rendered report stay byte-identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateKind {
    /// L2 build gate — `cargo build --release` must pass warning-clean.
    #[serde(rename = "l2_build")]
    Build,
    /// L2 behavior gate — differential + property + fuzz oracle checks.
    #[serde(rename = "l2_behavior")]
    Behavior,
    /// L2 performance gate — ≥ 0.8× of the original on the benchmark.
    #[serde(rename = "l2_perf")]
    Perf,
}

impl GateKind {
    /// The load-bearing gate string. Exactly `"l2_build"` /
    /// `"l2_behavior"` / `"l2_perf"`; downstream reports + tests pin
    /// these byte-for-byte.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            GateKind::Build => "l2_build",
            GateKind::Behavior => "l2_behavior",
            GateKind::Perf => "l2_perf",
        }
    }
}

impl fmt::Display for GateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a string does not name a known L2 gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseGateKindError(pub String);

impl fmt::Display for ParseGateKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown L2 gate `{}`; expected one of l2_build, l2_behavior, l2_perf",
            self.0
        )
    }
}

impl std::error::Error for ParseGateKindError {}

impl FromStr for GateKind {
    type Err = ParseGateKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "l2_build" => Ok(GateKind::Build),
            "l2_behavior" => Ok(GateKind::Behavior),
            "l2_perf" => Ok(GateKind::Perf),
            other => Err(ParseGateKindError(other.to_string())),
        }
    }
}

/// One gate-failure diagnostic blob, persisted to disk and shipped
/// back to the LLM as the prompt body for the next attempt.
///
/// `failed_inputs`, `expected`, and `actual` are all `String` so the
/// diagnostic remains human-readable (TOML round-trip + LLM-friendly).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateFailure {
    /// Function under repair (matches `FunctionTranslation::name`).
    pub function: String,
    /// Gate that rejected the translation (the gate *identity*, one of
    /// the three L2 gates — see [`GateKind`]).
    pub failed_gate: GateKind,
    /// Single-sentence summary suitable for an LLM prompt.
    pub failure_summary: String,
    /// Minimal failing inputs (or build snippet), one per Vec entry.
    pub failed_inputs: Vec<String>,
    /// Expected output (CPython oracle) for the first failing input,
    /// when available.
    pub expected: Option<String>,
    /// Actual output the translation produced for the first failing
    /// input, when available.
    pub actual: Option<String>,
    /// 1-based attempt counter; first repair = `attempt = 2`.
    pub attempt: u32,
}

impl GateFailure {
    /// Persist the diagnostic blob to
    /// `<out_dir>/<library>/diagnostics/<function>__<attempt>.toml`
    /// per ADR-0008 §7.
    ///
    /// # Errors
    /// I/O errors bubble up.
    pub fn write(&self, out_dir: &Path, library: &str) -> Result<PathBuf, std::io::Error> {
        let dir = out_dir.join(library).join("diagnostics");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!(
            "{}__{}.toml",
            sanitize(&self.function),
            self.attempt
        ));
        let s = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(&path, s)?;
        Ok(path)
    }

    /// Format the diagnostic into the prompt body the LLM (or the
    /// synthetic provider's attempt-N entry) will see.
    #[must_use]
    pub fn to_prompt_body(&self) -> String {
        let mut s = String::new();
        s.push_str("REPAIR REQUEST\n");
        writeln!(s, "function: {}", self.function).expect("writing to String never fails");
        writeln!(s, "failed-gate: {}", self.failed_gate).expect("writing to String never fails");
        writeln!(s, "attempt: {}", self.attempt).expect("writing to String never fails");
        writeln!(s, "summary: {}\n", self.failure_summary).expect("writing to String never fails");
        if !self.failed_inputs.is_empty() {
            s.push_str("failed-inputs:\n");
            for i in &self.failed_inputs {
                writeln!(s, "  - {i:?}").expect("writing to String never fails");
            }
            s.push('\n');
        }
        if let Some(e) = &self.expected {
            writeln!(s, "expected: {e}").expect("writing to String never fails");
        }
        if let Some(a) = &self.actual {
            writeln!(s, "actual: {a}").expect("writing to String never fails");
        }
        s.push_str(
            "\nRe-emit the function so it satisfies the failed gate; \
             keep the public signature unchanged.\n",
        );
        s
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Re-dispatch a translation prompt for the failing function with the
/// diagnostic blob appended, asking for `attempt = failure.attempt`.
///
/// The new [`FunctionTranslation`] has its `cache_hit` flag taken
/// from the router response and a fresh `router_decision_id`.
///
/// # Errors
/// `TranslatorError::SyntheticMiss` when the synthetic provider has
/// no canned response for the requested attempt;
/// `TranslatorError::Router` for other dispatch failures.
pub async fn repair_translation(
    router: &Router,
    library: &str,
    source_sha16: &str,
    failure: &GateFailure,
) -> Result<FunctionTranslation, TranslatorError> {
    repair_translation_with_task(router, library, "translate", source_sha16, failure).await
}

/// M6: per-function-task variant. Used when the failed function was
/// translated under `task = "translate_cython"` so the repair prompt
/// routes to the matching synthetic entry. The default
/// [`repair_translation`] forwards here with `task = "translate"`,
/// preserving M5 behaviour.
///
/// # Errors
/// As [`repair_translation`].
pub async fn repair_translation_with_task(
    router: &Router,
    library: &str,
    task: &str,
    source_sha16: &str,
    failure: &GateFailure,
) -> Result<FunctionTranslation, TranslatorError> {
    let header = PromptHeader {
        task: task.into(),
        function: failure.function.clone(),
        source_sha16: source_sha16.into(),
        attempt: failure.attempt,
    };
    let body = failure.to_prompt_body();
    let prompt = format_prompt_body(&header, &body);
    let req = CompletionRequest {
        model: format!("{library}-canned-v1"),
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
    let resp = router
        .dispatch(Task::Translate, req.clone())
        .await
        .map_err(|e| classify_router_error(task, &failure.function, e))?;
    let decision_id = format!(
        "blake3:{}",
        cobrust_llm_router::CacheKey::compute(&resp.provider, &req).hex()
    );
    Ok(FunctionTranslation {
        name: failure.function.clone(),
        source_sha16: source_sha16.into(),
        provider: resp.provider,
        model: resp.response.model.clone(),
        cache_hit: resp.cache_hit,
        router_decision_id: decision_id,
        emitted_text: resp.response.text,
        task: task.into(),
    })
}

fn classify_router_error(
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

/// Write a human-readable failure report when escalation is hit.
/// Returns the path so the caller can surface it in the error message.
///
/// # Errors
/// I/O errors bubble up.
pub fn write_failure_report(
    crate_dir: &Path,
    function: &str,
    failed_gate: GateKind,
    attempts: u32,
    diagnostics: &[GateFailure],
) -> Result<PathBuf, std::io::Error> {
    let path = crate_dir.join("failure_report.md");
    let mut s = String::new();
    writeln!(s, "# Translation failure report — `{function}`\n")
        .expect("writing to String never fails");
    writeln!(
        s,
        "Escalation threshold hit: {attempts} repair attempts on gate `{failed_gate}`."
    )
    .expect("writing to String never fails");
    s.push('\n');
    s.push_str("Per constitution §4.2 / ADR-0008 §3, the function is marked\n");
    s.push_str("`@py_compat(none)` and removed from the gating set. Manual review\n");
    s.push_str("required.\n\n");
    s.push_str("## Per-attempt diagnostics\n\n");
    for d in diagnostics {
        writeln!(s, "### Attempt {}\n", d.attempt).expect("writing to String never fails");
        writeln!(s, "- failed-gate: `{}`", d.failed_gate).expect("writing to String never fails");
        writeln!(s, "- summary: {}", d.failure_summary).expect("writing to String never fails");
        if !d.failed_inputs.is_empty() {
            s.push_str("- failed-inputs:\n");
            for i in &d.failed_inputs {
                writeln!(s, "  - `{i}`").expect("writing to String never fails");
            }
        }
        if let Some(e) = &d.expected {
            writeln!(s, "- expected: `{e}`").expect("writing to String never fails");
        }
        if let Some(a) = &d.actual {
            writeln!(s, "- actual: `{a}`").expect("writing to String never fails");
        }
        s.push('\n');
    }
    std::fs::write(&path, s)?;
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample_failure() -> GateFailure {
        GateFailure {
            function: "parse_iso".into(),
            failed_gate: GateKind::Behavior,
            failure_summary: "swapped year/month on every input".into(),
            failed_inputs: vec!["2026-04-30".into(), "2026-01-31".into()],
            expected: Some("(2026, 4, 30, ...)".into()),
            actual: Some("(4, 2026, 30, ...)".into()),
            attempt: 2,
        }
    }

    #[test]
    fn diagnostic_to_prompt_body_includes_inputs_and_attempt() {
        let f = sample_failure();
        let s = f.to_prompt_body();
        assert!(s.contains("REPAIR REQUEST"));
        assert!(s.contains("attempt: 2"));
        assert!(s.contains("parse_iso"));
        assert!(s.contains("l2_behavior"));
        assert!(s.contains("2026-04-30"));
        assert!(s.contains("expected:"));
        assert!(s.contains("actual:"));
    }

    #[test]
    fn diagnostic_writes_to_disk_under_diagnostics_dir() {
        let dir = tempfile::tempdir().unwrap();
        let f = sample_failure();
        let path = f.write(dir.path(), "dateutil").unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("diagnostics"));
        assert!(path.to_string_lossy().contains("parse_iso__2.toml"));
        let read_back = std::fs::read_to_string(path).unwrap();
        let parsed: GateFailure = toml::from_str(&read_back).unwrap();
        assert_eq!(parsed.function, "parse_iso");
        assert_eq!(parsed.attempt, 2);
    }

    #[test]
    fn sanitize_replaces_unsafe_path_chars() {
        assert_eq!(sanitize("parse/iso"), "parse_iso");
        assert_eq!(sanitize("normal_name"), "normal_name");
        assert_eq!(sanitize("dotted.name"), "dotted_name");
    }

    #[test]
    fn failure_report_contains_each_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let crate_dir = dir.path().join("cobrust-x");
        std::fs::create_dir_all(&crate_dir).unwrap();
        let mut diagnostics = vec![];
        for i in 1..=3 {
            diagnostics.push(GateFailure {
                attempt: i,
                ..sample_failure()
            });
        }
        let path =
            write_failure_report(&crate_dir, "parse_iso", GateKind::Behavior, 3, &diagnostics)
                .unwrap();
        assert!(path.exists());
        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.contains("Attempt 1"));
        assert!(body.contains("Attempt 2"));
        assert!(body.contains("Attempt 3"));
        assert!(body.contains("parse_iso"));
        assert!(body.contains("@py_compat(none)"));
    }
}
