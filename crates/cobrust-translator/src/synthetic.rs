//! Synthetic-LLM provider for the translator gate path.
//!
//! Implements [`cobrust_llm_router::LlmProvider`] backed by a
//! pre-recorded TOML response table. Used when the translator runs
//! without API keys (the default M4/M5 path) and as a deterministic
//! test double in CI.
//!
//! The lookup key is `(task, function, attempt)`, extracted from the
//! [`CompletionRequest`]'s **last user message** which the translator
//! emits in a stable header form (M5 — version 1.1; see ADR-0008 §5):
//!
//! ```text
//! cobrust-translator/v1
//! task: <task>
//! function: <function>
//! source-sha256: <16-hex>
//! attempt: <N>            (optional; defaults to 1)
//! ---
//! <prompt body>
//! ```
//!
//! This is a deliberate departure from prompt-hash keying — see
//! `adr:0007` §"Synthetic-LLM mode" for rationale (we want responses
//! that are reviewable and human-editable; raw cache hashes are not).
//!
//! M5 added the optional `attempt` field so the same `(task, function)`
//! pair can carry multiple canned responses, one per repair attempt
//! (per `adr:0008` §5). Attempt-1 with no header line is the M4
//! default; attempt-2+ requires both a header line and a matching
//! `attempt = N` field on the canned entry.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::pin::Pin;
use std::sync::Mutex;

use async_trait::async_trait;
use cobrust_llm_router::{
    Chunk, CompletionRequest, CompletionResponse, LlmError, LlmProvider, TokenUsage,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};

/// Header marker the translator stamps onto every prompt body so the
/// synthetic provider can route the request without parsing the prompt.
pub const PROMPT_HEADER_MARKER: &str = "cobrust-translator/v1";
/// Exact end-of-header sentinel.
pub const PROMPT_HEADER_DELIMITER: &str = "\n---\n";

/// Default attempt number when neither the prompt nor the entry sets one.
pub const DEFAULT_ATTEMPT: u32 = 1;

/// One canned response, keyed by `(task, function, attempt)`. The
/// `attempt` field is optional on disk and defaults to 1 — keeping the
/// M4 tomli table valid without modification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CannedResponse {
    pub task: String,
    pub function: String,
    /// `source_sha256` from the corpus, truncated to the first 16 hex
    /// chars. Used as a staleness check: if the upstream source SHA
    /// changes, the canned response is treated as a miss.
    pub source_sha16: String,
    /// Repair-loop attempt number (1-based). Defaults to 1 when
    /// omitted on disk; matches the prompt header's `attempt:` line
    /// when present.
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    pub response_text: String,
}

const fn default_attempt() -> u32 {
    DEFAULT_ATTEMPT
}

/// Top-level on-disk file shape.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CannedTable {
    pub schema_version: u32,
    pub oracle_runtime: String,
    #[serde(default)]
    pub entry: Vec<CannedResponse>,
}

impl CannedTable {
    /// Read and parse a canned-response file.
    ///
    /// # Errors
    /// I/O or TOML parse failures bubble up.
    pub fn read(path: &Path) -> Result<Self, std::io::Error> {
        let s = fs::read_to_string(path)?;
        toml::from_str(&s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Write to disk in the canonical format (sorted by
    /// (task, function, attempt)).
    ///
    /// # Errors
    /// I/O or TOML serialisation failures bubble up.
    pub fn write(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut sorted = self.clone();
        sorted.entry.sort_by(|a, b| {
            a.task
                .cmp(&b.task)
                .then(a.function.cmp(&b.function))
                .then(a.attempt.cmp(&b.attempt))
        });
        let s = toml::to_string_pretty(&sorted)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(path, s)
    }

    /// Build an empty table with the given oracle label.
    #[must_use]
    pub fn new(oracle_runtime: &str) -> Self {
        Self {
            schema_version: 1,
            oracle_runtime: oracle_runtime.into(),
            entry: Vec::new(),
        }
    }

    /// Add an entry. Replaces any existing one with the same
    /// `(task, function, attempt)` key.
    pub fn insert(&mut self, response: CannedResponse) {
        self.entry.retain(|e| {
            !(e.task == response.task
                && e.function == response.function
                && e.attempt == response.attempt)
        });
        self.entry.push(response);
    }
}

/// Synthetic provider implementing [`LlmProvider`] from a canned table.
pub struct SyntheticProvider {
    name: String,
    /// Keyed by `(task, function, attempt)` for O(log n) lookup.
    table: Mutex<BTreeMap<(String, String, u32), CannedResponse>>,
}

impl std::fmt::Debug for SyntheticProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.table.lock().expect("synthetic table poisoned");
        f.debug_struct("SyntheticProvider")
            .field("name", &self.name)
            .field("entries", &g.len())
            .finish()
    }
}

impl SyntheticProvider {
    /// Build from a [`CannedTable`]. Provider name is recorded so the
    /// caller can register multiple synthetic providers if desired.
    #[must_use]
    pub fn new(name: impl Into<String>, table: CannedTable) -> Self {
        let mut map = BTreeMap::new();
        for entry in table.entry {
            map.insert(
                (entry.task.clone(), entry.function.clone(), entry.attempt),
                entry,
            );
        }
        Self {
            name: name.into(),
            table: Mutex::new(map),
        }
    }

    /// Convenience: load from disk.
    ///
    /// # Errors
    /// File or TOML parse failures bubble up.
    pub fn from_canned_toml(name: impl Into<String>, path: &Path) -> Result<Self, std::io::Error> {
        let table = CannedTable::read(path)?;
        Ok(Self::new(name, table))
    }

    /// Record (or replace) one entry. Used by the recording binary
    /// and integration tests.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned (i.e. another thread
    /// panicked while holding it).
    pub fn record(&self, response: CannedResponse) {
        let mut g = self.table.lock().expect("synthetic table poisoned");
        g.insert(
            (
                response.task.clone(),
                response.function.clone(),
                response.attempt,
            ),
            response,
        );
    }

    /// Number of entries currently registered. For diagnostics only.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.table.lock().expect("synthetic table poisoned").len()
    }

    /// Snapshot the current table for serialisation.
    ///
    /// # Panics
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn snapshot(&self) -> CannedTable {
        let g = self.table.lock().expect("synthetic table poisoned");
        CannedTable {
            schema_version: 1,
            oracle_runtime: String::new(),
            entry: g.values().cloned().collect(),
        }
    }
}

#[async_trait]
impl LlmProvider for SyntheticProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> cobrust_llm_router::ProviderKind {
        cobrust_llm_router::ProviderKind::Synthetic
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let header = extract_header(&req).ok_or_else(|| LlmError::Provider {
            code: "synthetic-no-header".into(),
            message: "synthetic provider requires translator header on user message".into(),
        })?;
        let g = self.table.lock().expect("synthetic table poisoned");
        let key = (header.task.clone(), header.function.clone(), header.attempt);
        let Some(entry) = g.get(&key) else {
            return Err(LlmError::Provider {
                code: "synthetic-miss".into(),
                message: format!(
                    "no canned response for task={} function={} attempt={}",
                    header.task, header.function, header.attempt
                ),
            });
        };
        if entry.source_sha16 != header.source_sha16 {
            return Err(LlmError::Provider {
                code: "synthetic-stale".into(),
                message: format!(
                    "stale canned response for task={} function={} attempt={} (have sha16={}, asked={})",
                    header.task,
                    header.function,
                    header.attempt,
                    entry.source_sha16,
                    header.source_sha16
                ),
            });
        }
        Ok(CompletionResponse {
            text: entry.response_text.clone(),
            model: req.model,
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: u32::try_from(entry.response_text.len()).unwrap_or(u32::MAX),
            },
        })
    }

    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>> {
        let outcome = futures::executor::block_on(self.complete(req));
        match outcome {
            Ok(resp) => {
                let chunks = vec![Chunk::Delta(resp.text), Chunk::Done(resp.usage)];
                Box::pin(stream::iter(chunks.into_iter().map(Ok)))
            }
            Err(err) => Box::pin(stream::once(async move { Err(err) })),
        }
    }
}

/// Header parsed out of the translator's stable user-message format.
#[derive(Clone, Debug)]
pub struct PromptHeader {
    pub task: String,
    pub function: String,
    pub source_sha16: String,
    /// Repair-loop attempt; defaults to 1 when the header line is
    /// absent. M5 added this field; M4 prompts continue to roundtrip
    /// because absence is treated as `attempt = 1`.
    pub attempt: u32,
}

impl PromptHeader {
    /// Build a header for an attempt-1 prompt (M4/M5 default).
    #[must_use]
    pub fn first_attempt(
        task: impl Into<String>,
        function: impl Into<String>,
        source_sha16: impl Into<String>,
    ) -> Self {
        Self {
            task: task.into(),
            function: function.into(),
            source_sha16: source_sha16.into(),
            attempt: DEFAULT_ATTEMPT,
        }
    }
}

/// Build a translator-formatted user-message body. The synthetic
/// provider extracts the header back out via [`extract_header`].
///
/// When `attempt == 1` (the default M4 path) the header line is
/// **omitted** so M4-vintage tomli prompts hash-roundtrip exactly the
/// same as before — preserving the M4 cache key + ledger entries.
#[must_use]
pub fn format_prompt_body(header: &PromptHeader, body: &str) -> String {
    if header.attempt == DEFAULT_ATTEMPT {
        format!(
            "{marker}\ntask: {task}\nfunction: {function}\nsource-sha256: {sha}{delim}{body}",
            marker = PROMPT_HEADER_MARKER,
            task = header.task,
            function = header.function,
            sha = header.source_sha16,
            delim = PROMPT_HEADER_DELIMITER,
            body = body,
        )
    } else {
        format!(
            "{marker}\ntask: {task}\nfunction: {function}\nsource-sha256: {sha}\nattempt: {attempt}{delim}{body}",
            marker = PROMPT_HEADER_MARKER,
            task = header.task,
            function = header.function,
            sha = header.source_sha16,
            attempt = header.attempt,
            delim = PROMPT_HEADER_DELIMITER,
            body = body,
        )
    }
}

/// Extract the header from a request's last user message.
#[must_use]
pub fn extract_header(req: &CompletionRequest) -> Option<PromptHeader> {
    let last_user = req
        .messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, cobrust_llm_router::Role::User))?;
    parse_header(&last_user.content)
}

/// Parse a header out of a raw prompt body. Public so the recording
/// binary and tests can roundtrip without going through `complete()`.
#[must_use]
pub fn parse_header(body: &str) -> Option<PromptHeader> {
    let body = body.strip_prefix(PROMPT_HEADER_MARKER)?;
    let body = body.strip_prefix('\n')?;
    let (header, _) = body.split_once(PROMPT_HEADER_DELIMITER)?;
    let mut task = None;
    let mut function = None;
    let mut sha = None;
    let mut attempt = DEFAULT_ATTEMPT;
    for line in header.lines() {
        if let Some(v) = line.strip_prefix("task: ") {
            task = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("function: ") {
            function = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("source-sha256: ") {
            sha = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("attempt: ") {
            attempt = v.trim().parse::<u32>().unwrap_or(DEFAULT_ATTEMPT);
        }
    }
    Some(PromptHeader {
        task: task?,
        function: function?,
        source_sha16: sha?,
        attempt,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use cobrust_llm_router::{Message, Role, SamplingParams};

    fn req_with_attempt(
        task: &str,
        function: &str,
        sha: &str,
        attempt: u32,
        body: &str,
    ) -> CompletionRequest {
        let header = PromptHeader {
            task: task.into(),
            function: function.into(),
            source_sha16: sha.into(),
            attempt,
        };
        CompletionRequest {
            model: "synthetic:tomli-canned-v1".into(),
            messages: vec![Message {
                role: Role::User,
                content: format_prompt_body(&header, body),
            }],
            params: SamplingParams::default(),
        }
    }

    fn req_with(task: &str, function: &str, sha: &str, body: &str) -> CompletionRequest {
        req_with_attempt(task, function, sha, DEFAULT_ATTEMPT, body)
    }

    #[test]
    fn header_round_trips() {
        let header = PromptHeader::first_attempt("translate", "loads", "abc123def456789a");
        let body = format_prompt_body(&header, "Translate this Python function:");
        let parsed = parse_header(&body).unwrap();
        assert_eq!(parsed.task, "translate");
        assert_eq!(parsed.function, "loads");
        assert_eq!(parsed.source_sha16, "abc123def456789a");
        assert_eq!(parsed.attempt, 1);
    }

    #[test]
    fn attempt_header_round_trips() {
        let header = PromptHeader {
            task: "translate".into(),
            function: "parse_iso".into(),
            source_sha16: "187586aad2a69e52".into(),
            attempt: 2,
        };
        let body = format_prompt_body(&header, "diagnostic + retry");
        let parsed = parse_header(&body).unwrap();
        assert_eq!(parsed.attempt, 2);
        // Confirm the literal `attempt: 2` line is in the body when attempt > 1.
        assert!(body.contains("\nattempt: 2\n"));
    }

    #[test]
    fn attempt_1_prompt_body_omits_attempt_line() {
        // M4 backward-compat: attempt=1 prompts must hash identically
        // to the M4 format (no `attempt:` line present).
        let header = PromptHeader::first_attempt("translate", "loads", "abc");
        let body = format_prompt_body(&header, "");
        assert!(!body.contains("attempt:"));
    }

    #[tokio::test]
    async fn synthetic_returns_canned_response_on_match() {
        let mut table = CannedTable::new("cpython 3.11");
        table.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: "deadbeef00000000".into(),
            attempt: 1,
            response_text: "fn loads() -> Dict { ... }".into(),
        });
        let provider = SyntheticProvider::new("synthetic", table);
        let req = req_with("translate", "loads", "deadbeef00000000", "translate this");
        let resp = provider.complete(req).await.unwrap();
        assert_eq!(resp.text, "fn loads() -> Dict { ... }");
    }

    #[tokio::test]
    async fn synthetic_routes_attempt_2_to_attempt_2_entry() {
        let mut table = CannedTable::new("cpython 3.11");
        table.insert(CannedResponse {
            task: "translate".into(),
            function: "parse_iso".into(),
            source_sha16: "187586aad2a69e52".into(),
            attempt: 1,
            response_text: "// BROKEN".into(),
        });
        table.insert(CannedResponse {
            task: "translate".into(),
            function: "parse_iso".into(),
            source_sha16: "187586aad2a69e52".into(),
            attempt: 2,
            response_text: "// CORRECT".into(),
        });
        let provider = SyntheticProvider::new("synthetic", table);
        let r1 = provider
            .complete(req_with_attempt(
                "translate",
                "parse_iso",
                "187586aad2a69e52",
                1,
                "first try",
            ))
            .await
            .unwrap();
        assert_eq!(r1.text, "// BROKEN");
        let r2 = provider
            .complete(req_with_attempt(
                "translate",
                "parse_iso",
                "187586aad2a69e52",
                2,
                "second try",
            ))
            .await
            .unwrap();
        assert_eq!(r2.text, "// CORRECT");
    }

    #[tokio::test]
    async fn synthetic_returns_synthetic_miss_on_unknown_function() {
        let table = CannedTable::new("cpython 3.11");
        let provider = SyntheticProvider::new("synthetic", table);
        let req = req_with("translate", "unknown", "deadbeef00000000", "??");
        let err = provider.complete(req).await.unwrap_err();
        match err {
            LlmError::Provider { code, message } => {
                assert_eq!(code, "synthetic-miss");
                assert!(message.contains("unknown"));
            }
            other => panic!("expected Provider error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn synthetic_returns_synthetic_stale_on_sha_mismatch() {
        let mut table = CannedTable::new("cpython 3.11");
        table.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: "olddeadbeef00000".into(),
            attempt: 1,
            response_text: "stale".into(),
        });
        let provider = SyntheticProvider::new("synthetic", table);
        let req = req_with("translate", "loads", "newhash000000000", "??");
        let err = provider.complete(req).await.unwrap_err();
        match err {
            LlmError::Provider { code, .. } => assert_eq!(code, "synthetic-stale"),
            other => panic!("expected Provider error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn synthetic_returns_synthetic_no_header_when_marker_missing() {
        let table = CannedTable::new("cpython 3.11");
        let provider = SyntheticProvider::new("synthetic", table);
        let req = CompletionRequest {
            model: "synthetic".into(),
            messages: vec![Message {
                role: Role::User,
                content: "free-form prompt without translator header".into(),
            }],
            params: SamplingParams::default(),
        };
        let err = provider.complete(req).await.unwrap_err();
        match err {
            LlmError::Provider { code, .. } => assert_eq!(code, "synthetic-no-header"),
            other => panic!("expected Provider error, got {other:?}"),
        }
    }

    #[test]
    fn canned_table_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("canned.toml");
        let mut table = CannedTable::new("cpython 3.11");
        table.insert(CannedResponse {
            task: "translate".into(),
            function: "loads".into(),
            source_sha16: "feedface00000000".into(),
            attempt: 1,
            response_text: "// emitted Rust\n".into(),
        });
        table.write(&path).unwrap();
        let read_back = CannedTable::read(&path).unwrap();
        assert_eq!(read_back.entry.len(), 1);
        assert_eq!(read_back.entry[0].function, "loads");
        assert_eq!(read_back.entry[0].source_sha16, "feedface00000000");
        assert_eq!(read_back.entry[0].attempt, 1);
    }

    #[test]
    fn canned_table_default_attempt_is_one_when_omitted_on_disk() {
        // Construct TOML on disk without an `attempt` field — must default to 1.
        let toml_src = r#"
schema_version = 1
oracle_runtime = "cpython 3.11"

[[entry]]
task = "translate"
function = "legacy"
source_sha16 = "0000000000000000"
response_text = "// legacy"
"#;
        let parsed: CannedTable = toml::from_str(toml_src).unwrap();
        assert_eq!(parsed.entry.len(), 1);
        assert_eq!(parsed.entry[0].attempt, 1);
    }
}
