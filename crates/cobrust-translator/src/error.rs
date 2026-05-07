//! Translator error taxonomy. Pinned by `adr:0007` §"Public surface"
//! and extended by `adr:0008` §"`TranslatorError` extension".

use cobrust_llm_router::RouterError;

/// All errors the translation pipeline can surface.
///
/// `SyntheticMiss` is the canonical failure mode for synthetic-LLM
/// operation against an unrecorded prompt. It is permanent; the caller
/// must add the entry to the canned-response table or switch to
/// real-LLM mode.
#[derive(Debug, thiserror::Error)]
pub enum TranslatorError {
    /// L0 spec extraction failed.
    #[error("L0 spec extraction failed: {0}")]
    SpecExtraction(String),

    /// L1 translation failed for a specific function.
    #[error("L1 translation failed for {function}: {message}")]
    Translation { function: String, message: String },

    /// L2 build gate failed.
    #[error("L2 build gate failed: {0}")]
    BuildGate(String),

    /// L2 behavior gate failed.
    #[error("L2 behavior gate failed: {0}")]
    BehaviorGate(String),

    /// L2 performance gate failed (per ADR-0008 §2).
    #[error("L2 perf gate failed: {0}")]
    PerfGate(String),

    /// L3 downstream gate failed.
    #[error("L3 downstream gate failed: {0}")]
    DownstreamGate(String),

    /// Synthetic provider received a prompt it had no recorded response
    /// for. The pipeline must add the response or run real-LLM mode.
    #[error("synthetic-miss for task {task} function {function}: no canned response")]
    SyntheticMiss { task: String, function: String },

    /// Repair-loop hit `escalation_threshold` retries on one function
    /// (per ADR-0008 §3 / constitution §4.2). `failure_report.md` is
    /// written next to the manifest before this error is raised.
    #[error(
        "escalation: {function} hit {attempts} repair attempts on gate {failed_gate}; failure_report.md written"
    )]
    EscalationExceeded {
        function: String,
        attempts: u32,
        failed_gate: String,
    },

    /// I/O failure (file read/write, mkdir).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Router-level failure surfacing through the dispatch layer.
    #[error("router: {0}")]
    Router(#[from] RouterError),

    /// Manifest validation failure: schema or determinism violation.
    #[error("manifest: {0}")]
    Manifest(String),

    /// Configuration parse failure.
    #[error("config: {0}")]
    Config(String),

    /// Generic decode failure (TOML, JSON, etc.).
    #[error("decode: {0}")]
    Decode(String),
}

impl From<toml::de::Error> for TranslatorError {
    fn from(e: toml::de::Error) -> Self {
        TranslatorError::Decode(e.to_string())
    }
}

impl From<toml::ser::Error> for TranslatorError {
    fn from(e: toml::ser::Error) -> Self {
        TranslatorError::Decode(e.to_string())
    }
}

impl From<serde_json::Error> for TranslatorError {
    fn from(e: serde_json::Error) -> Self {
        TranslatorError::Decode(e.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_miss_message_is_actionable() {
        let e = TranslatorError::SyntheticMiss {
            task: "translate".into(),
            function: "loads".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("translate"));
        assert!(msg.contains("loads"));
        assert!(msg.contains("synthetic-miss"));
    }

    #[test]
    fn translation_message_carries_function_and_reason() {
        let e = TranslatorError::Translation {
            function: "parse_int".into(),
            message: "expected i64".into(),
        };
        assert!(e.to_string().contains("parse_int"));
        assert!(e.to_string().contains("expected i64"));
    }

    #[test]
    fn escalation_exceeded_message_names_function_and_gate() {
        let e = TranslatorError::EscalationExceeded {
            function: "parse_iso".into(),
            attempts: 50,
            failed_gate: "l2_behavior".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("parse_iso"));
        assert!(msg.contains("50"));
        assert!(msg.contains("l2_behavior"));
        assert!(msg.contains("failure_report.md"));
    }

    #[test]
    fn perf_gate_carries_message() {
        let e = TranslatorError::PerfGate("3/12 below 0.8x threshold".into());
        let msg = e.to_string();
        assert!(msg.contains("perf"));
        assert!(msg.contains("3/12"));
    }
}
