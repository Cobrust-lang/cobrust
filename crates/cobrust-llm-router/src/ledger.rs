//! Append-only JSONL token ledger.
//!
//! Schema (per `adr:0004`):
//! ```json
//! {
//!   "ts":               "2026-04-30T01:23:45.678Z",
//!   "task":             "translate",
//!   "provider":         "anthropic_official",
//!   "model":            "claude-opus-4-7",
//!   "cache_key":        "blake3:<hex>",
//!   "cache_hit":        false,
//!   "prompt_tokens":    123,
//!   "completion_tokens":456,
//!   "total_tokens":     579,
//!   "latency_ms":       1234,
//!   "attempt":          1,
//!   "outcome":          "ok",
//!   "error_code":       null,
//!   "consensus_group":  null
//! }
//! ```
//!
//! Writes go through a single `tokio::sync::Mutex<File>` opened with
//! `O_APPEND`, ensuring no two writers tear a line. Each line is terminated
//! by `\n`. Readers must tolerate at most one trailing partial line in case
//! of crash mid-write.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::provider::TokenUsage;

/// Outcome label for a completion attempt.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    ErrorTransient,
    ErrorPermanent,
}

/// One record in the JSONL ledger.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub ts: String,
    pub task: String,
    pub provider: String,
    pub model: String,
    pub cache_key: String,
    pub cache_hit: bool,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub latency_ms: u32,
    pub attempt: u8,
    pub outcome: Outcome,
    pub error_code: Option<String>,
    pub consensus_group: Option<String>,
}

impl LedgerEntry {
    /// Construct a successful entry from the usage data.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn ok(
        ts: String,
        task: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        cache_key: impl Into<String>,
        cache_hit: bool,
        usage: TokenUsage,
        latency_ms: u32,
        attempt: u8,
        consensus_group: Option<String>,
    ) -> Self {
        Self {
            ts,
            task: task.into(),
            provider: provider.into(),
            model: model.into(),
            cache_key: cache_key.into(),
            cache_hit,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total(),
            latency_ms,
            attempt,
            outcome: Outcome::Ok,
            error_code: None,
            consensus_group,
        }
    }

    /// Construct an error entry. `transient` chooses between
    /// [`Outcome::ErrorTransient`] and [`Outcome::ErrorPermanent`].
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn err(
        ts: String,
        task: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        cache_key: impl Into<String>,
        latency_ms: u32,
        attempt: u8,
        error_code: impl Into<String>,
        transient: bool,
        consensus_group: Option<String>,
    ) -> Self {
        Self {
            ts,
            task: task.into(),
            provider: provider.into(),
            model: model.into(),
            cache_key: cache_key.into(),
            cache_hit: false,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            latency_ms,
            attempt,
            outcome: if transient {
                Outcome::ErrorTransient
            } else {
                Outcome::ErrorPermanent
            },
            error_code: Some(error_code.into()),
            consensus_group,
        }
    }
}

/// Append-only ledger handle. Cheap to clone; all clones share the same
/// underlying mutex so concurrent writers serialise.
#[derive(Clone, Debug)]
pub struct Ledger {
    path: PathBuf,
    file: Arc<Mutex<tokio::fs::File>>,
}

impl Ledger {
    /// Open or create the JSONL file at `path` for append-only writes.
    /// The parent directory is created if missing.
    ///
    /// # Errors
    /// I/O failures bubble up.
    pub async fn open(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    /// Returns the ledger path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one JSONL record. The line is followed by `\n`.
    ///
    /// # Errors
    /// I/O or JSON serialisation failures bubble up.
    pub async fn append(&self, entry: &LedgerEntry) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        line.push(b'\n');
        let mut guard = self.file.lock().await;
        guard.write_all(&line).await?;
        guard.flush().await?;
        Ok(())
    }
}

/// Format the current UTC instant as RFC3339 with millisecond precision.
#[must_use]
pub fn now_rfc3339() -> String {
    let now = time::OffsetDateTime::now_utc();
    // RFC3339 with millisecond precision and `Z` suffix.
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::provider::TokenUsage;

    fn make_ok() -> LedgerEntry {
        LedgerEntry::ok(
            "2026-04-30T01:23:45.678Z".to_string(),
            "translate",
            "anthropic_official",
            "claude-opus-4-7",
            "blake3:abcd",
            false,
            TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 200,
            },
            1234,
            1,
            None,
        )
    }

    #[test]
    fn ok_entry_computes_total() {
        let e = make_ok();
        assert_eq!(e.total_tokens, 300);
        assert!(matches!(e.outcome, Outcome::Ok));
    }

    #[test]
    fn err_entry_marks_outcome_correctly() {
        let transient = LedgerEntry::err(
            "ts".into(),
            "translate",
            "p",
            "m",
            "blake3:00",
            5,
            1,
            "rate-limit",
            true,
            None,
        );
        assert!(matches!(transient.outcome, Outcome::ErrorTransient));
        let permanent = LedgerEntry::err(
            "ts".into(),
            "translate",
            "p",
            "m",
            "blake3:00",
            5,
            1,
            "auth",
            false,
            None,
        );
        assert!(matches!(permanent.outcome, Outcome::ErrorPermanent));
    }

    #[tokio::test]
    async fn ledger_appends_jsonl_line_per_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let ledger = Ledger::open(path.clone()).await.unwrap();
        ledger.append(&make_ok()).await.unwrap();
        ledger.append(&make_ok()).await.unwrap();
        let bytes = tokio::fs::read(&path).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let lines: Vec<&str> = text.split('\n').filter(|s| !s.is_empty()).collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let parsed: LedgerEntry = serde_json::from_str(line).unwrap();
            assert_eq!(parsed, make_ok());
        }
    }

    #[tokio::test]
    async fn ledger_is_append_only_across_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let ledger1 = Ledger::open(path.clone()).await.unwrap();
        ledger1.append(&make_ok()).await.unwrap();
        drop(ledger1);
        let ledger2 = Ledger::open(path.clone()).await.unwrap();
        ledger2.append(&make_ok()).await.unwrap();
        let bytes = tokio::fs::read(&path).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let line_count = text.split('\n').filter(|s| !s.is_empty()).count();
        assert_eq!(line_count, 2, "second open must NOT truncate");
    }

    #[tokio::test]
    async fn ledger_concurrent_writes_do_not_tear_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let ledger = Ledger::open(path.clone()).await.unwrap();
        let mut joins = Vec::new();
        for _ in 0..32 {
            let l = ledger.clone();
            joins.push(tokio::spawn(async move {
                for _ in 0..10 {
                    l.append(&make_ok()).await.unwrap();
                }
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        let bytes = tokio::fs::read(&path).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let lines: Vec<&str> = text.split('\n').filter(|s| !s.is_empty()).collect();
        assert_eq!(lines.len(), 320, "expected 320 well-formed lines");
        for line in lines {
            let _: LedgerEntry = serde_json::from_str(line).expect("every line must be valid JSON");
        }
    }

    #[test]
    fn now_rfc3339_is_well_formed() {
        let s = now_rfc3339();
        // Must end with Z (UTC) and contain a date-time delimiter.
        assert!(s.ends_with('Z'), "{s}");
        assert!(s.contains('T'), "{s}");
    }
}
