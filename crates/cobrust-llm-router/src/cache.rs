//! Content-addressed on-disk cache.
//!
//! Layout: each cached completion lives at
//! `<root>/<aa>/<bb>/<full-hex>.json`, where `aa` and `bb` are the first two
//! hex byte-pair shards of the BLAKE3 cache key. This avoids hammering a
//! single directory with millions of entries.
//!
//! Canonical key bytes (per `adr:0004`):
//! ```text
//! blake3(
//!     b"cobrust-llm-router/v1\n"
//!         || provider_key      || b"\n"
//!         || model_id          || b"\n"
//!         || canonical_params  || b"\n"
//!         || canonical_messages
//! )
//! ```
//!
//! The provider key is included so two providers serving the same model id
//! never share a cache entry — auth and rate budgets are per-provider.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::provider::{CompletionRequest, CompletionResponse, Message, Role, SamplingParams};

/// Stable, machine-independent fingerprint of a `(provider, request)` pair.
///
/// The wire form is `blake3:<64-hex>`; the on-disk filename is just
/// `<64-hex>.json` under a two-level shard.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CacheKey(String);

impl CacheKey {
    /// Compute the canonical key for this request as seen by `provider_key`.
    #[must_use]
    pub fn compute(provider_key: &str, req: &CompletionRequest) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cobrust-llm-router/v1\n");
        hasher.update(provider_key.as_bytes());
        hasher.update(b"\n");
        hasher.update(req.model.as_bytes());
        hasher.update(b"\n");
        hasher.update(canonical_params(&req.params).as_bytes());
        hasher.update(b"\n");
        hasher.update(canonical_messages(&req.messages).as_bytes());
        Self(hasher.finalize().to_hex().to_string())
    }

    /// Hex-only form (no `blake3:` prefix). Used as filename stem.
    #[must_use]
    pub fn hex(&self) -> &str {
        &self.0
    }

    /// Wire form, suitable for ledger entries.
    #[must_use]
    pub fn wire(&self) -> String {
        format!("blake3:{}", self.0)
    }
}

/// On-disk cache rooted at `root`. Cache misses are not errors — they return
/// `Ok(None)`. Real I/O failures bubble up.
#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    request: CompletionRequest,
    response: CompletionResponse,
}

impl Cache {
    /// Create the cache rooted at `root`. The directory is created if missing.
    ///
    /// # Errors
    /// Returns the underlying I/O error if the root cannot be created.
    pub async fn new(root: PathBuf) -> std::io::Result<Self> {
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    /// Returns the cache root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, key: &CacheKey) -> PathBuf {
        let hex = key.hex();
        // Defensive: BLAKE3 hex is always 64 chars; assert in tests.
        let shard0 = &hex[..2];
        let shard1 = &hex[2..4];
        self.root
            .join(shard0)
            .join(shard1)
            .join(format!("{hex}.json"))
    }

    /// Look up a cached completion by key. `Ok(None)` on miss.
    ///
    /// # Errors
    /// I/O errors other than `NotFound`, or JSON deserialisation failures
    /// (treated as cache poisoning), bubble up.
    pub async fn get(&self, key: &CacheKey) -> std::io::Result<Option<CompletionResponse>> {
        let path = self.path_for(key);
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let entry: CacheEntry = serde_json::from_slice(&bytes).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                })?;
                Ok(Some(entry.response))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Store a completion under the given key.
    ///
    /// # Errors
    /// I/O failures (mkdir, write) bubble up.
    pub async fn put(
        &self,
        key: &CacheKey,
        request: &CompletionRequest,
        response: &CompletionResponse,
    ) -> std::io::Result<()> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let entry = CacheEntry {
            request: request.clone(),
            response: response.clone(),
        };
        let bytes = serde_json::to_vec(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        tokio::fs::write(&path, bytes).await
    }
}

/// Canonicalise sampling params: sorted JSON object, stable across machines.
fn canonical_params(p: &SamplingParams) -> String {
    let mut obj = serde_json::Map::new();
    if let Some(v) = p.max_tokens {
        obj.insert("max_tokens".into(), serde_json::json!(v));
    }
    if let Some(v) = p.temperature {
        obj.insert("temperature".into(), serde_json::json!(v));
    }
    if let Some(v) = p.top_p {
        obj.insert("top_p".into(), serde_json::json!(v));
    }
    if !p.stop.is_empty() {
        obj.insert("stop".into(), serde_json::json!(p.stop));
    }
    // BTreeMap-style sort for deterministic ordering.
    let sorted: std::collections::BTreeMap<String, serde_json::Value> = obj.into_iter().collect();
    // serde_json::to_string preserves BTreeMap iteration order.
    serde_json::to_string(&sorted).unwrap_or_else(|_| "{}".to_string())
}

/// Canonicalise a message list as a JSON array. Submission order is preserved.
fn canonical_messages(msgs: &[Message]) -> String {
    let arr: Vec<serde_json::Value> = msgs
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                "content": m.content,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::provider::{Message, Role, SamplingParams, TokenUsage};

    fn req() -> CompletionRequest {
        CompletionRequest {
            model: "claude-opus-4-7".into(),
            messages: vec![Message {
                role: Role::User,
                content: "hello".into(),
            }],
            params: SamplingParams {
                max_tokens: Some(64),
                temperature: Some(0.2),
                top_p: None,
                stop: vec!["END".into()],
            },
        }
    }

    fn resp() -> CompletionResponse {
        CompletionResponse {
            text: "world".into(),
            model: "claude-opus-4-7".into(),
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 5,
            },
        }
    }

    #[test]
    fn cache_key_is_64_hex_chars() {
        let k = CacheKey::compute("anthropic_official", &req());
        assert_eq!(k.hex().len(), 64);
        assert!(k.hex().chars().all(|c| c.is_ascii_hexdigit()));
        assert!(k.wire().starts_with("blake3:"));
    }

    #[test]
    fn cache_key_is_deterministic_across_calls() {
        let k1 = CacheKey::compute("anthropic_official", &req());
        let k2 = CacheKey::compute("anthropic_official", &req());
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_provider() {
        let k1 = CacheKey::compute("anthropic_official", &req());
        let k2 = CacheKey::compute("openai_official", &req());
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_model() {
        let mut r = req();
        let k1 = CacheKey::compute("p", &r);
        r.model = "different".into();
        let k2 = CacheKey::compute("p", &r);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_message_content() {
        let mut r = req();
        let k1 = CacheKey::compute("p", &r);
        r.messages[0].content = "different".into();
        let k2 = CacheKey::compute("p", &r);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_param_value() {
        let mut r = req();
        let k1 = CacheKey::compute("p", &r);
        r.params.temperature = Some(0.9);
        let k2 = CacheKey::compute("p", &r);
        assert_ne!(k1, k2);
    }

    #[test]
    fn canonical_params_sorts_keys_alphabetically() {
        let p = SamplingParams {
            max_tokens: Some(64),
            temperature: Some(0.2),
            top_p: Some(0.9),
            stop: vec!["X".into()],
        };
        let s = canonical_params(&p);
        let pos_max = s.find("max_tokens").unwrap();
        let pos_stop = s.find("stop").unwrap();
        let pos_temp = s.find("temperature").unwrap();
        let pos_top_p = s.find("top_p").unwrap();
        assert!(pos_max < pos_stop);
        assert!(pos_stop < pos_temp);
        assert!(pos_temp < pos_top_p);
    }

    #[tokio::test]
    async fn cache_put_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf()).await.unwrap();
        let key = CacheKey::compute("p", &req());
        assert!(cache.get(&key).await.unwrap().is_none());
        cache.put(&key, &req(), &resp()).await.unwrap();
        let got = cache.get(&key).await.unwrap().unwrap();
        assert_eq!(got, resp());
    }

    #[tokio::test]
    async fn cache_miss_returns_none_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf()).await.unwrap();
        let key = CacheKey::compute("p", &req());
        assert!(cache.get(&key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn cache_path_uses_two_level_shard() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf()).await.unwrap();
        let key = CacheKey::compute("p", &req());
        cache.put(&key, &req(), &resp()).await.unwrap();
        let hex = key.hex();
        let expected = dir
            .path()
            .join(&hex[..2])
            .join(&hex[2..4])
            .join(format!("{hex}.json"));
        assert!(expected.exists());
    }
}
