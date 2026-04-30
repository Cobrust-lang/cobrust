//! Router — strategy + dispatch + retry + consensus tie-breaking.
//!
//! All semantics pinned by `adr:0004`. Cache and ledger are first-class
//! members of the router so every dispatch is observable post-hoc.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Instant;
use unicode_normalization::UnicodeNormalization;

use crate::cache::{Cache, CacheKey};
use crate::config::{DefaultStrategy, ProviderModel, RouterConfig, StrategyName};
use crate::ledger::{Ledger, LedgerEntry, now_rfc3339};
use crate::provider::{CompletionRequest, CompletionResponse, LlmError, LlmProvider};

/// Logical task tag. Maps to a `[routing.<task>]` section in `cobrust.toml`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Task {
    SpecExtract,
    Translate,
    Repair,
    Custom(String),
}

impl Task {
    /// String form used as the routing-table key and the ledger task name.
    #[must_use]
    pub fn as_key(&self) -> &str {
        match self {
            Task::SpecExtract => "spec_extract",
            Task::Translate => "translate",
            Task::Repair => "repair",
            Task::Custom(s) => s.as_str(),
        }
    }
}

/// Strategy with runtime parameters. Constructed from the TOML
/// [`StrategyName`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Strategy {
    Cost,
    Quality,
    Latency,
    Consensus { n: u8 },
}

impl Strategy {
    fn from_table(name: StrategyName, n: Option<u8>) -> Self {
        match name {
            StrategyName::Cost => Strategy::Cost,
            StrategyName::Quality => Strategy::Quality,
            StrategyName::Latency => Strategy::Latency,
            StrategyName::Consensus => Strategy::Consensus { n: n.unwrap_or(2) },
        }
    }

    fn from_default(d: DefaultStrategy) -> Self {
        match d {
            DefaultStrategy::Cost => Strategy::Cost,
            DefaultStrategy::Quality => Strategy::Quality,
            DefaultStrategy::Latency => Strategy::Latency,
        }
    }
}

/// Resolved routing entry — strategy + ordered (provider, model) pairs.
#[derive(Clone, Debug)]
struct ResolvedRoute {
    strategy: Strategy,
    preferred: Vec<ProviderModel>,
}

/// Successful dispatch result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterResponse {
    pub response: CompletionResponse,
    pub provider: String,
    pub cache_hit: bool,
}

/// Router-level errors. `LlmError`s from individual provider attempts are
/// rolled into `AllFailed` once the table is exhausted.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("config: {0}")]
    Config(String),
    #[error("no provider configured for task {0:?}")]
    NoProvider(String),
    #[error("all providers failed: {0:?}")]
    AllFailed(Vec<(String, LlmError)>),
    #[error("consensus quorum lost (need {need}, got {got})")]
    ConsensusQuorumLost { need: u8, got: u8 },
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for RouterError {
    fn from(e: std::io::Error) -> Self {
        RouterError::Io(e.to_string())
    }
}

/// Retry budget per `adr:0004`: 5 attempts, 30 s elapsed cap, 250 ms base,
/// factor 2, full jitter, honour `Retry-After`.
#[derive(Copy, Clone, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_delay_ms: u64,
    pub factor: f64,
    pub max_total_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay_ms: 250,
            factor: 2.0,
            max_total_ms: 30_000,
        }
    }
}

impl RetryPolicy {
    /// Compute the next sleep duration for `attempt` (1-indexed). When the
    /// error carries a `Retry-After`, that value overrides the computed delay.
    fn next_delay_ms(&self, attempt: u8, err: &LlmError) -> u64 {
        if let LlmError::RateLimit { retry_after_ms } = err
            && *retry_after_ms > 0
        {
            return *retry_after_ms;
        }
        let exp = attempt.saturating_sub(1);
        let pow = self.factor.powi(i32::from(exp));
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let max = (self.base_delay_ms as f64) * pow;
        if max <= 0.0 {
            return 0;
        }
        // Full-jitter: uniform [0, max].
        let mut rng = rand::thread_rng();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let delay = rng.gen_range(0.0..=max) as u64;
        delay
    }
}

/// In-memory EWMA latency tracker for `Strategy::Latency`. Keys are
/// `provider:model` tags.
#[derive(Default, Debug)]
struct LatencyTracker {
    inner: HashMap<String, f64>,
}

impl LatencyTracker {
    const ALPHA: f64 = 0.2;

    fn observe(&mut self, key: &str, latency_ms: u64) {
        #[allow(clippy::cast_precision_loss)]
        let v = latency_ms as f64;
        let entry = self.inner.entry(key.to_string()).or_insert(v);
        *entry = Self::ALPHA.mul_add(v, (1.0 - Self::ALPHA) * *entry);
    }

    fn get(&self, key: &str) -> Option<f64> {
        self.inner.get(key).copied()
    }
}

/// Router. Holds the provider registry, routing table, cache, ledger, and
/// retry policy.
pub struct Router {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    routing: HashMap<String, ResolvedRoute>,
    default_strategy: Strategy,
    cache: Cache,
    ledger: Ledger,
    retry: RetryPolicy,
    latency: Arc<AsyncMutex<LatencyTracker>>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .field("routing", &self.routing.keys().collect::<Vec<_>>())
            .field("default_strategy", &self.default_strategy)
            .finish_non_exhaustive()
    }
}

/// Builder for the router. Concrete adapters or test doubles are registered
/// via [`RouterBuilder::register_provider`]; the table is fixed by the parsed
/// [`RouterConfig`].
#[derive(Default)]
pub struct RouterBuilder {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    retry: Option<RetryPolicy>,
}

impl RouterBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a concrete provider under the given key. The key must match a
    /// `[providers.<key>]` section in the config.
    #[must_use]
    pub fn register_provider(
        mut self,
        key: impl Into<String>,
        provider: Arc<dyn LlmProvider>,
    ) -> Self {
        self.providers.insert(key.into(), provider);
        self
    }

    /// Override the default retry policy.
    #[must_use]
    pub fn retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Build the router from the resolved config + registered providers.
    ///
    /// # Errors
    /// Returns [`RouterError::Config`] if the config fails to validate or
    /// references unregistered providers, or [`RouterError::Io`] if the
    /// cache/ledger paths cannot be opened.
    pub async fn build(self, cfg: &RouterConfig) -> Result<Router, RouterError> {
        cfg.validate().map_err(RouterError::Config)?;
        for name in cfg.providers.keys() {
            if !self.providers.contains_key(name) {
                return Err(RouterError::Config(format!(
                    "provider {name:?} declared in config but not registered with the builder"
                )));
            }
        }
        // Build resolved routing table.
        let mut routing = HashMap::new();
        for (task, entry) in &cfg.routing {
            let mut preferred = Vec::with_capacity(entry.preferred.len());
            for tag in &entry.preferred {
                let pm = ProviderModel::parse(tag).ok_or_else(|| {
                    RouterError::Config(format!(
                        "routing.{task}: malformed provider:model tag {tag:?}"
                    ))
                })?;
                preferred.push(pm);
            }
            routing.insert(
                task.clone(),
                ResolvedRoute {
                    strategy: Strategy::from_table(entry.strategy, entry.n),
                    preferred,
                },
            );
        }
        let cache = Cache::new(cfg.router.cache_dir.clone()).await?;
        let ledger = Ledger::open(cfg.router.ledger_path.clone()).await?;
        Ok(Router {
            providers: self.providers,
            routing,
            default_strategy: Strategy::from_default(cfg.router.default_strategy),
            cache,
            ledger,
            retry: self.retry.unwrap_or_default(),
            latency: Arc::new(AsyncMutex::new(LatencyTracker::default())),
        })
    }
}

impl Router {
    /// Convenience: build directly from config; assumes providers are
    /// registered via [`RouterBuilder`].
    #[must_use]
    pub fn builder() -> RouterBuilder {
        RouterBuilder::new()
    }

    /// Dispatch one task. Honours the resolved strategy, retries transient
    /// errors per the retry policy, falls through to the next preferred
    /// provider on permanent failure, writes one ledger entry per attempt,
    /// and reads/writes the on-disk cache.
    ///
    /// # Errors
    /// See [`RouterError`] variants.
    pub async fn dispatch(
        &self,
        task: Task,
        req: CompletionRequest,
    ) -> Result<RouterResponse, RouterError> {
        let key = task.as_key();
        let Some(route) = self.routing.get(key).cloned() else {
            return Err(RouterError::NoProvider(key.to_string()));
        };
        let strategy = route.strategy;
        let preferred = route.preferred;

        match strategy {
            Strategy::Quality | Strategy::Cost | Strategy::Latency => {
                let order = self.order_preferred(strategy, &preferred).await;
                self.dispatch_ordered(task.clone(), &req, &order, None)
                    .await
            }
            Strategy::Consensus { n } => {
                self.dispatch_consensus(task.clone(), &req, &preferred, n)
                    .await
            }
        }
    }

    async fn order_preferred(
        &self,
        strategy: Strategy,
        preferred: &[ProviderModel],
    ) -> Vec<ProviderModel> {
        match strategy {
            Strategy::Latency => {
                let tracker = self.latency.lock().await;
                let mut paired: Vec<(f64, ProviderModel)> = preferred
                    .iter()
                    .map(|pm| {
                        let key = format!("{}:{}", pm.provider, pm.model);
                        let latency = tracker.get(&key).unwrap_or(f64::INFINITY);
                        (latency, pm.clone())
                    })
                    .collect();
                drop(tracker);
                paired.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                paired.into_iter().map(|(_, pm)| pm).collect()
            }
            // Quality, Cost, Consensus all walk the table in submitted order.
            _ => preferred.to_vec(),
        }
    }

    async fn dispatch_ordered(
        &self,
        task: Task,
        req: &CompletionRequest,
        order: &[ProviderModel],
        consensus_group: Option<String>,
    ) -> Result<RouterResponse, RouterError> {
        let mut errors: Vec<(String, LlmError)> = Vec::new();
        for pm in order {
            match self
                .try_provider(&task, req, pm, consensus_group.clone())
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    errors.push((pm.provider.clone(), err));
                }
            }
        }
        Err(RouterError::AllFailed(errors))
    }

    async fn dispatch_consensus(
        &self,
        task: Task,
        req: &CompletionRequest,
        preferred: &[ProviderModel],
        n: u8,
    ) -> Result<RouterResponse, RouterError> {
        let group = uuid::Uuid::new_v4().to_string();
        let take_n = usize::from(n).min(preferred.len());
        let shards = &preferred[..take_n];
        // Spawn parallel attempts per shard.
        let mut joins = Vec::with_capacity(shards.len());
        for pm in shards {
            let pm = pm.clone();
            let task = task.clone();
            let req = req.clone();
            let group = group.clone();
            let this = self.handle();
            joins.push(tokio::spawn(async move {
                let mut req_for_shard = req;
                req_for_shard.model = pm.model.clone();
                this.try_provider(&task, &req_for_shard, &pm, Some(group))
                    .await
            }));
        }
        let mut successes: Vec<RouterResponse> = Vec::new();
        for j in joins {
            if let Ok(Ok(r)) = j.await {
                successes.push(r);
            }
        }
        let need_quorum = u8::try_from((take_n / 2) + (take_n % 2)).unwrap_or(u8::MAX);
        if u8::try_from(successes.len()).unwrap_or(u8::MAX) < need_quorum {
            return Err(RouterError::ConsensusQuorumLost {
                need: need_quorum,
                got: u8::try_from(successes.len()).unwrap_or(u8::MAX),
            });
        }
        Ok(consensus_pick(&successes, preferred))
    }

    fn handle(&self) -> RouterHandle {
        RouterHandle {
            providers: self.providers.clone(),
            cache: self.cache.clone(),
            ledger: self.ledger.clone(),
            retry: self.retry,
            latency: self.latency.clone(),
        }
    }

    async fn try_provider(
        &self,
        task: &Task,
        req: &CompletionRequest,
        pm: &ProviderModel,
        consensus_group: Option<String>,
    ) -> Result<RouterResponse, LlmError> {
        self.handle()
            .try_provider(task, req, pm, consensus_group)
            .await
    }
}

/// Lightweight cloneable handle for spawned consensus shards.
#[derive(Clone)]
struct RouterHandle {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    cache: Cache,
    ledger: Ledger,
    retry: RetryPolicy,
    latency: Arc<AsyncMutex<LatencyTracker>>,
}

impl RouterHandle {
    #[allow(clippy::too_many_lines)]
    async fn try_provider(
        &self,
        task: &Task,
        req: &CompletionRequest,
        pm: &ProviderModel,
        consensus_group: Option<String>,
    ) -> Result<RouterResponse, LlmError> {
        let provider = self
            .providers
            .get(&pm.provider)
            .ok_or_else(|| LlmError::Provider {
                code: "unknown_provider".into(),
                message: format!("provider {} not registered", pm.provider),
            })?;

        // Enforce the model from the routing table; the caller's `req.model`
        // may have a different value (e.g. for consensus the router sets it).
        let mut request = req.clone();
        request.model = pm.model.clone();

        let key = CacheKey::compute(&pm.provider, &request);
        // Cache lookup.
        if let Some(resp) = self
            .cache
            .get(&key)
            .await
            .map_err(|e| LlmError::Decode(e.to_string()))?
        {
            self.ledger
                .append(&LedgerEntry::ok(
                    now_rfc3339(),
                    task.as_key(),
                    pm.provider.clone(),
                    pm.model.clone(),
                    key.wire(),
                    true,
                    resp.usage,
                    0,
                    1,
                    consensus_group.clone(),
                ))
                .await
                .map_err(|e| LlmError::Decode(e.to_string()))?;
            return Ok(RouterResponse {
                response: resp,
                provider: pm.provider.clone(),
                cache_hit: true,
            });
        }

        // Live dispatch with retry on transient errors.
        let mut attempt: u8 = 1;
        let total_start = Instant::now();
        loop {
            let call_start = Instant::now();
            let outcome = provider.complete(request.clone()).await;
            let elapsed_ms = u32::try_from(call_start.elapsed().as_millis()).unwrap_or(u32::MAX);
            match outcome {
                Ok(resp) => {
                    self.cache
                        .put(&key, &request, &resp)
                        .await
                        .map_err(|e| LlmError::Decode(e.to_string()))?;
                    {
                        let latency_key = format!("{}:{}", pm.provider, pm.model);
                        let mut tracker = self.latency.lock().await;
                        tracker.observe(&latency_key, u64::from(elapsed_ms));
                    }
                    self.ledger
                        .append(&LedgerEntry::ok(
                            now_rfc3339(),
                            task.as_key(),
                            pm.provider.clone(),
                            pm.model.clone(),
                            key.wire(),
                            false,
                            resp.usage,
                            elapsed_ms,
                            attempt,
                            consensus_group.clone(),
                        ))
                        .await
                        .map_err(|e| LlmError::Decode(e.to_string()))?;
                    return Ok(RouterResponse {
                        response: resp,
                        provider: pm.provider.clone(),
                        cache_hit: false,
                    });
                }
                Err(err) => {
                    let transient = err.is_transient();
                    self.ledger
                        .append(&LedgerEntry::err(
                            now_rfc3339(),
                            task.as_key(),
                            pm.provider.clone(),
                            pm.model.clone(),
                            key.wire(),
                            elapsed_ms,
                            attempt,
                            err.code(),
                            transient,
                            consensus_group.clone(),
                        ))
                        .await
                        .map_err(|e| LlmError::Decode(e.to_string()))?;

                    if !transient || attempt >= self.retry.max_attempts {
                        return Err(err);
                    }
                    let total_elapsed_ms =
                        u64::try_from(total_start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    if total_elapsed_ms >= self.retry.max_total_ms {
                        return Err(err);
                    }
                    let delay = self
                        .retry
                        .next_delay_ms(attempt, &err)
                        .min(self.retry.max_total_ms.saturating_sub(total_elapsed_ms));
                    if delay > 0 {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }
}

/// Pick the consensus winner: largest group on `BLAKE3(NFC(text))`, then
/// lexicographic-smallest hash, then preferred-list index ascending.
fn consensus_pick(successes: &[RouterResponse], preferred: &[ProviderModel]) -> RouterResponse {
    debug_assert!(!successes.is_empty(), "caller checks quorum");

    let preferred_index = |provider: &str, model: &str| -> usize {
        preferred
            .iter()
            .position(|pm| pm.provider == provider && pm.model == model)
            .unwrap_or(usize::MAX)
    };

    // Hash NFC-normalised text → group.
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, r) in successes.iter().enumerate() {
        let normalised: String = r.response.text.nfc().collect();
        let h = blake3::hash(normalised.as_bytes());
        let key = h.to_hex().as_str()[..32].to_string();
        groups.entry(key).or_default().push(idx);
    }
    // Find largest group; tie-break by lexicographically smallest hash.
    let mut group_summaries: Vec<(usize, String)> = groups
        .iter()
        .map(|(h, idxs)| (idxs.len(), h.clone()))
        .collect();
    group_summaries.sort_by(|a, b| match b.0.cmp(&a.0) {
        std::cmp::Ordering::Equal => a.1.cmp(&b.1),
        o => o,
    });
    let winning_hash = group_summaries[0].1.clone();
    let winning_idxs = &groups[&winning_hash];

    // Within the winning group, pick the shard whose preferred index is smallest.
    let mut best_idx = winning_idxs[0];
    let mut best_pref = preferred_index(
        &successes[best_idx].provider,
        &successes[best_idx].response.model,
    );
    for &idx in winning_idxs.iter().skip(1) {
        let cand_pref = preferred_index(&successes[idx].provider, &successes[idx].response.model);
        if cand_pref < best_pref {
            best_pref = cand_pref;
            best_idx = idx;
        }
    }
    successes[best_idx].clone()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;
    use crate::provider::TokenUsage;

    fn pm(p: &str, m: &str) -> ProviderModel {
        ProviderModel {
            provider: p.into(),
            model: m.into(),
        }
    }

    fn ok(provider: &str, model: &str, text: &str) -> RouterResponse {
        RouterResponse {
            response: CompletionResponse {
                text: text.into(),
                model: model.into(),
                usage: TokenUsage::default(),
            },
            provider: provider.into(),
            cache_hit: false,
        }
    }

    #[test]
    fn task_as_key_matches_toml_section_names() {
        assert_eq!(Task::SpecExtract.as_key(), "spec_extract");
        assert_eq!(Task::Translate.as_key(), "translate");
        assert_eq!(Task::Repair.as_key(), "repair");
        assert_eq!(Task::Custom("xyz".into()).as_key(), "xyz");
    }

    #[test]
    fn consensus_picks_majority_group() {
        let preferred = vec![pm("a", "m1"), pm("b", "m2"), pm("c", "m3")];
        let successes = vec![
            ok("a", "m1", "answer-A"),
            ok("b", "m2", "answer-A"),
            ok("c", "m3", "answer-B"),
        ];
        let winner = consensus_pick(&successes, &preferred);
        assert_eq!(winner.response.text, "answer-A");
        assert_eq!(
            winner.provider, "a",
            "first preferred wins ties within group"
        );
    }

    #[test]
    fn consensus_tie_breaks_on_smaller_hash_then_preferred_index() {
        let preferred = vec![pm("a", "m1"), pm("b", "m2")];
        let successes = vec![ok("a", "m1", "answer-A"), ok("b", "m2", "answer-B")];
        // Two singleton groups → smaller hash wins → call should be deterministic.
        let w1 = consensus_pick(&successes, &preferred);
        let w2 = consensus_pick(&successes, &preferred);
        assert_eq!(w1, w2, "tie-break must be deterministic across calls");
    }

    #[test]
    fn consensus_tie_break_prefers_lower_index_in_winning_group() {
        let preferred = vec![pm("b", "m2"), pm("a", "m1")];
        let successes = vec![ok("a", "m1", "same-text"), ok("b", "m2", "same-text")];
        let winner = consensus_pick(&successes, &preferred);
        assert_eq!(
            winner.provider, "b",
            "preferred[0]=b wins inside same group"
        );
    }

    #[test]
    fn consensus_normalises_unicode_for_grouping() {
        // Two NFC-equivalent forms of "café" should hash identically.
        let preferred = vec![pm("a", "m1"), pm("b", "m2")];
        let nfc = "caf\u{00e9}"; // é precomposed
        let nfd = "cafe\u{0301}"; // e + combining acute
        let successes = vec![ok("a", "m1", nfc), ok("b", "m2", nfd)];
        let winner = consensus_pick(&successes, &preferred);
        // Both shards land in the same group (size 2); tie-break: smaller
        // preferred index. preferred[0] is "a", so "a" wins.
        assert_eq!(winner.provider, "a");
    }

    #[test]
    fn retry_policy_honours_retry_after() {
        let p = RetryPolicy::default();
        let err = LlmError::RateLimit {
            retry_after_ms: 7777,
        };
        assert_eq!(p.next_delay_ms(1, &err), 7777);
    }

    #[test]
    fn retry_policy_jitter_within_bound() {
        let p = RetryPolicy::default();
        let err = LlmError::Server {
            status: 503,
            body: String::new(),
        };
        for attempt in 1..=4 {
            let exp = i32::from(attempt - 1);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let upper = (p.base_delay_ms as f64 * p.factor.powi(exp)) as u64;
            for _ in 0..50 {
                let d = p.next_delay_ms(attempt, &err);
                assert!(d <= upper, "delay {d} should be <= {upper}");
            }
        }
    }

    #[test]
    fn latency_tracker_ewma_converges() {
        let mut t = LatencyTracker::default();
        let key = "p:m";
        for _ in 0..50 {
            t.observe(key, 100);
        }
        let v = t.get(key).expect("must exist");
        assert!((v - 100.0).abs() < 0.5, "EWMA should converge: {v}");
    }
}
