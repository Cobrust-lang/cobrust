//! `cobrust.llm` — source-level binding to `crates/cobrust-llm-router`.
//!
//! ADR-0048 §"M-AI.0 — cobrust.llm" pins this module; refined by
//! `docs/agent/spike/m-ai-0-cobrust-llm-spike.md` (SHA 705f592) and
//! ratified by `[P10-ALPHA-PHASE-2-RATIFY]` (OQ-1A flat-fn /
//! OQ-2B collect-all-chunks / OQ-3 WRAP).
//!
//! # Architecture
//!
//! Three flat-fn intrinsics live in `PRELUDE`:
//!
//! - `llm_complete(provider, model, prompt) -> str`
//! - `llm_dispatch(task, prompt) -> str`
//! - `llm_stream(provider, model, prompt) -> list[str]`
//!
//! Their callsites lower to C-ABI shims in this module via the
//! `cobrust-cli` intrinsic-rewrite pass (see
//! `crates/cobrust-cli/src/build/intrinsics.rs`). Each shim unwraps
//! the pointer args, dispatches through the existing async router
//! over a process-global `tokio::runtime::Runtime`, and returns the
//! response as a heap-allocated `Str` buffer (or list of `Str`
//! buffers for `llm_stream`).
//!
//! # OQ-3 WRAP option implementation
//!
//! The router crate is frozen for M-AI.0 (per
//! `[P10-ALPHA-PHASE-2-RATIFY]` OQ-3 resolution). To target a
//! specific `(provider, model)` directly for `llm_complete` and
//! `llm_stream` — bypassing the routing table — this module
//! **synthesizes** extra routing-table entries at `RouterConfigBundle`
//! init. For each `[providers.<p>]` declared in `cobrust.toml` with
//! `models = [m1, m2, ...]`:
//!
//! - One synthetic `[routing.llm_complete_<p>_<m_i>]` entry per
//!   `(provider, model)` pair, with `strategy = "quality"` and
//!   `preferred = ["<p>:<m_i>"]`.
//! - One mirror `[routing.llm_stream_<p>_<m_i>]` entry per
//!   `(provider, model)` pair, same shape.
//!
//! At dispatch time `llm_complete(p, m, q)` uses
//! `Task::Custom("llm_complete_<p>_<m>")` — falling cleanly to
//! `RouterError::NoProvider` (→ "") if the requested `(provider,
//! model)` was not declared in the user's `cobrust.toml`. The
//! router's `try_provider` enforces the model from the routing-
//! table entry (see `router.rs:462-465`), so our synthesized
//! `(provider, model)` is honored bit-for-bit. The shape of
//! `llm_dispatch(task, prompt)` is unaffected — it walks the user-
//! declared routing table directly via `Task::Custom(task)`.
//!
//! # Decision references
//!
//! - **Decision 1B** (flat-fn naming): mirrors ADR-0044 `input` /
//!   `argv` / `read_line` precedent. No module-path lowering.
//! - **Decision 3B** (collect-all alpha shim): `llm_stream` currently
//!   returns a `List<Str>` whose contents preserve call-order but do
//!   not expose provider delta events. Today the implementation routes
//!   through non-streaming dispatch and returns either `[]` or a
//!   single-element list containing the full response text.
//! - **Decision 4** (process-global runtime): single
//!   `OnceLock<tokio::runtime::Runtime>` shared across shims —
//!   avoids per-call TCP/TLS setup.
//! - **Decision 5** (config caching): `OnceLock<Option<...>>` reads
//!   `COBRUST_CONFIG` env var (or `./cobrust.toml`) once at first
//!   dispatch.
//! - **Decision 7** (error surface): all shims return `""` / empty
//!   list on any failure. Ledger captures the actual error code.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

use cobrust_llm_router::{
    AnthropicProvider, CompletionRequest, LlmProvider, Message, OpenAiProvider, ProviderKind, Role,
    Router, RouterBuilder, RouterConfig, RoutingEntry, StrategyName, Task,
};

// =====================================================================
// Process-global lazy state (Decisions 4 + 5)
// =====================================================================

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static CONFIG_BUNDLE: OnceLock<Option<RouterConfigBundle>> = OnceLock::new();

/// Cached router + amended config bundle. Built once on first use.
struct RouterConfigBundle {
    router: Router,
}

/// Lazily-initialized multi-threaded `tokio` runtime shared across
/// every C-ABI shim in this module. Decision 4: the runtime owns the
/// connection pool, TLS sessions, cache, and ledger — fresh runtimes
/// per shim call would burn TCP setup time.
///
/// # Panics
///
/// Panics if the runtime fails to build. This is treated as fatal:
/// no Cobrust `llm_*` callsite can recover from an absent runtime.
fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("M-AI.0: tokio runtime init failed")
    })
}

/// Reach the cached router bundle, or `None` if config loading
/// failed (missing file, malformed TOML, validation error, etc.).
fn config_bundle() -> Option<&'static RouterConfigBundle> {
    CONFIG_BUNDLE.get_or_init(|| build_bundle().ok()).as_ref()
}

/// Internal error taxonomy for bundle construction. Never surfaced
/// to Cobrust source — Decision 7 collapses all failures to `""`.
/// Payload fields are kept for `Debug`-printing during local
/// development even though the public surface throws them away;
/// `#[allow(dead_code)]` suppresses the unread-field lint that fires
/// because no caller projects out of the variants.
#[derive(Debug)]
#[allow(dead_code)]
enum BuildErr {
    Io(std::io::Error),
    Parse(String),
    Router(cobrust_llm_router::RouterError),
}

impl From<std::io::Error> for BuildErr {
    fn from(e: std::io::Error) -> Self {
        BuildErr::Io(e)
    }
}

/// Construct the `RouterConfigBundle` from `COBRUST_CONFIG` (env-var
/// override) or `./cobrust.toml`. OQ-3 WRAP: synthesizes
/// `[routing.llm_complete_<p>_<m>]` + `[routing.llm_stream_<p>_<m>]`
/// entries per declared `(provider, model)` so `llm_complete(p, m, q)`
/// and `llm_stream(p, m, q)` resolve through the router without
/// modifying the router crate.
fn build_bundle() -> Result<RouterConfigBundle, BuildErr> {
    let path = std::env::var("COBRUST_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("cobrust.toml"));
    let toml = std::fs::read_to_string(&path)?;
    let mut cfg = RouterConfig::from_toml_str(&toml).map_err(|e| BuildErr::Parse(e.to_string()))?;

    // ----- OQ-3 WRAP: synthesize routing entries per (provider, model) ----
    // For every declared provider, add `[routing.llm_complete_<p>_<m>]` and
    // `[routing.llm_stream_<p>_<m>]` entries. The router's `try_provider`
    // enforces `request.model = pm.model.clone()` (router.rs:462-465), so
    // the synthesized `(provider, model)` pair is honored bit-for-bit.
    let mut synth_entries: Vec<(String, RoutingEntry)> = Vec::new();
    for (pname, pcfg) in &cfg.providers {
        for model in &pcfg.models {
            let preferred = vec![format!("{pname}:{model}")];
            // `llm_complete_<provider>_<model>`
            synth_entries.push((
                format!("llm_complete_{pname}_{model}"),
                RoutingEntry {
                    strategy: StrategyName::Quality,
                    n: None,
                    preferred: preferred.clone(),
                },
            ));
            // `llm_stream_<provider>_<model>` — same shape.
            synth_entries.push((
                format!("llm_stream_{pname}_{model}"),
                RoutingEntry {
                    strategy: StrategyName::Quality,
                    n: None,
                    preferred,
                },
            ));
        }
    }
    for (k, v) in synth_entries {
        // Don't clobber user-declared entries with synthesized ones.
        cfg.routing.entry(k).or_insert(v);
    }

    // ----- Build the router with concrete adapter instances ---------------
    let router = runtime()
        .block_on(async {
            let mut builder = RouterBuilder::new();
            for (name, pcfg) in &cfg.providers {
                let provider: Arc<dyn LlmProvider> = match pcfg.kind {
                    ProviderKind::Anthropic => {
                        let api_key = std::env::var(&pcfg.api_key_env).unwrap_or_default();
                        // Constructor returns Result<_, reqwest::Error>; the
                        // only failure path is reqwest::Client::builder()
                        // which is essentially infallible at this layer. If
                        // it does fail, skip this provider — Decision 7
                        // collapses all surface failures to empty strings.
                        match AnthropicProvider::new(name.clone(), pcfg.base_url.clone(), api_key) {
                            Ok(p) => Arc::new(p),
                            Err(_) => continue,
                        }
                    }
                    ProviderKind::Openai => {
                        let api_key = std::env::var(&pcfg.api_key_env).unwrap_or_default();
                        match OpenAiProvider::new(name.clone(), pcfg.base_url.clone(), api_key) {
                            Ok(p) => Arc::new(p),
                            Err(_) => continue,
                        }
                    }
                    ProviderKind::Synthetic => {
                        // Synthetic providers are an in-process test
                        // double; M-AI.0 production wiring has no way
                        // to materialize one without a `#[cfg(test)]`
                        // seam, which is out of scope per the P7-DEV
                        // dispatch (P7-TEST corpus bodies are commented
                        // out — only `require_impl()` runs after the
                        // impl-landed flag flips). Skip so the
                        // RouterBuilder::build validation doesn't fail
                        // on an unregistered provider.
                        continue;
                    }
                };
                builder = builder.register_provider(name.clone(), provider);
            }
            // Strip synthetic-provider configs from the validation pass:
            // RouterBuilder::build rejects declared-but-unregistered
            // providers (router.rs:243-248). For Synthetic provider
            // entries we couldn't register, drop them from the cfg copy
            // we hand to .build().
            let cfg_for_build = strip_synthetic_providers(&cfg);
            builder.build(&cfg_for_build).await
        })
        .map_err(BuildErr::Router)?;
    Ok(RouterConfigBundle { router })
}

/// Drop `Synthetic`-kind providers (and any routing entries that
/// reference them) from a `RouterConfig` copy, so that
/// `RouterBuilder::build`'s declared-but-unregistered-provider check
/// (router.rs:243-248) passes. The user's on-disk `cobrust.toml` is
/// unchanged; this only affects the in-memory bundle used at build
/// time.
fn strip_synthetic_providers(cfg: &RouterConfig) -> RouterConfig {
    let mut out = cfg.clone();
    let synthetic_names: std::collections::BTreeSet<String> = cfg
        .providers
        .iter()
        .filter(|(_, pc)| matches!(pc.kind, ProviderKind::Synthetic))
        .map(|(n, _)| n.clone())
        .collect();
    if synthetic_names.is_empty() {
        return out;
    }
    out.providers.retain(|n, _| !synthetic_names.contains(n));
    out.routing.retain(|_, entry| {
        entry.preferred.iter().all(|tag| {
            // tag is "<provider>:<model>"
            let provider = tag.split(':').next().unwrap_or("");
            !synthetic_names.contains(provider)
        })
    });
    out
}

// =====================================================================
// Rust-side blocking helpers — the unit-testable counterparts to the
// C-ABI shims. Decision 7: all failures collapse to "" / empty Vec.
// =====================================================================

/// `llm_complete(provider, model, prompt) -> str`. Targets a specific
/// `(provider, model)` via the WRAP-synthesized routing entry. Returns
/// the completion text on success, or `""` on any failure.
#[must_use]
pub fn llm_complete_blocking(provider: &str, model: &str, prompt: &str) -> String {
    let Some(bundle) = config_bundle() else {
        return String::new();
    };
    let req = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.to_string(),
        }],
        params: cobrust_llm_router::SamplingParams::default(),
    };
    let task_key = format!("llm_complete_{provider}_{model}");
    runtime()
        .block_on(bundle.router.dispatch(Task::Custom(task_key), req))
        .map(|r| r.response.text)
        .unwrap_or_default()
}

/// `llm_dispatch(task, prompt) -> str`. Looks up `[routing.<task>]`
/// in the user's `cobrust.toml` (or a WRAP-synthesized entry, if the
/// caller chose a `llm_complete_*` / `llm_stream_*` key). Returns the
/// completion text on success, or `""` on any failure.
#[must_use]
pub fn llm_dispatch_blocking(task: &str, prompt: &str) -> String {
    let Some(bundle) = config_bundle() else {
        return String::new();
    };
    let req = CompletionRequest {
        // For user-declared routes the router overrides this from the
        // routing entry's preferred list. The placeholder satisfies
        // the API shape.
        model: String::new(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.to_string(),
        }],
        params: cobrust_llm_router::SamplingParams::default(),
    };
    runtime()
        .block_on(bundle.router.dispatch(Task::Custom(task.to_string()), req))
        .map(|r| r.response.text)
        .unwrap_or_default()
}

/// `llm_stream(provider, model, prompt) -> list[str]`.
///
/// Alpha honesty contract: this is a collect-all shim, not true token
/// streaming. It uses normal router dispatch and returns either `[]` or
/// a single ordered chunk containing the full response text.
/// Returns an empty `Vec` on any failure (per Decision 7).
#[must_use]
pub fn llm_stream_blocking(provider: &str, model: &str, prompt: &str) -> Vec<String> {
    let Some(bundle) = config_bundle() else {
        return Vec::new();
    };
    let req = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.to_string(),
        }],
        params: cobrust_llm_router::SamplingParams::default(),
    };
    let task_key = format!("llm_stream_{provider}_{model}");
    // The router's public surface exposes only non-streaming dispatch.
    // Rather than pretend to emit provider deltas, the alpha contract is
    // explicit: dispatch once, then wrap the full response text as a
    // single ordered chunk. True delta streaming needs follow-up router
    // surface work.
    let Ok(resp) = runtime().block_on(
        bundle
            .router
            .dispatch(Task::Custom(task_key.clone()), req.clone()),
    ) else {
        // Fall through to direct provider streaming if the routed
        // path failed. This preserves the spike Decision 3B intent:
        // when the user calls `llm_stream(p, m, q)` and `(p, m)` is
        // not in the cobrust.toml routing/providers, return `[]`.
        let _ = (provider, model, prompt, task_key);
        return direct_stream_attempt(provider, model, prompt);
    };
    // Decision 3B collect-all form: emit one chunk per dispatch. If
    // the underlying response is empty, return an empty Vec —
    // matches Tier 1 #11 (`empty_stream_returns_empty_vec`).
    if resp.response.text.is_empty() {
        return Vec::new();
    }
    vec![resp.response.text]
}

/// Fallback path for undeclared `(provider, model)` requests.
///
/// No direct provider streaming is implemented in this alpha shim, so
/// undeclared routes honestly return `Vec::new()`.
fn direct_stream_attempt(_provider: &str, _model: &str, _prompt: &str) -> Vec<String> {
    Vec::new()
}

// =====================================================================
// C-ABI shims (codegen targets these via the intrinsic-rewrite pass)
// =====================================================================

/// Read a heap `Str` pointer as a `String`. Tolerates null + empty
/// buffers (returns empty `String`). Mirrors the
/// `__cobrust_println_str_buf` pattern in `io.rs:243-265`.
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    // SAFETY: caller-attestation per the C-ABI contract — `buf` was
    // produced by `__cobrust_str_new` (and its push helpers) or by
    // codegen's `materialize_str_data` path. Both yield pointers
    // valid until the corresponding `__cobrust_str_drop`.
    unsafe {
        let ptr = crate::fmt::__cobrust_str_ptr(buf);
        let len = crate::fmt::__cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

/// Allocate a heap `Str` buffer carrying `s`. Wraps the existing
/// f-string buffer ABI (`__cobrust_str_new` + `__cobrust_str_push_static`),
/// returning the opaque `*mut u8` pointer codegen passes around.
/// Mirrors `io::alloc_str_buffer` (io.rs:167-178).
fn alloc_str_buffer(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` returns a valid buffer pointer that
    // we immediately populate via `__cobrust_str_push_static`. Both
    // contracts are satisfied — empty strings produce an empty buffer.
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// C-ABI shim for source-level `llm_complete(provider, model, prompt) -> str`.
/// Decision 7: any failure → returns an empty `Str` (non-null pointer to
/// a zero-length buffer); callers detect via `__cobrust_str_len(out) == 0`.
///
/// # Safety
///
/// Each pointer must be either null (signals empty string) or a valid
/// `Str` buffer pointer produced by `__cobrust_str_new` and friends.
/// The returned pointer is heap-owned by the f-string runtime; caller
/// must eventually invoke `__cobrust_str_drop` on it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_complete(
    provider: *mut u8,
    model: *mut u8,
    prompt: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let p = unsafe { read_str_buf(provider) };
    // SAFETY: same.
    let m = unsafe { read_str_buf(model) };
    // SAFETY: same.
    let q = unsafe { read_str_buf(prompt) };
    let result = llm_complete_blocking(&p, &m, &q);
    alloc_str_buffer(&result)
}

/// C-ABI shim for source-level `llm_dispatch(task, prompt) -> str`.
/// Decision 7: any failure → empty `Str`.
///
/// # Safety
///
/// Same contract as [`__cobrust_llm_complete`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_dispatch(task: *mut u8, prompt: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let t = unsafe { read_str_buf(task) };
    // SAFETY: same.
    let q = unsafe { read_str_buf(prompt) };
    let result = llm_dispatch_blocking(&t, &q);
    alloc_str_buffer(&result)
}

/// C-ABI shim for source-level `llm_stream(provider, model, prompt) -> list[str]`.
/// Decision 3B: returns a `__cobrust_list_new`-shaped pointer whose i64
/// slots store heap-`Str` pointers (one per Delta chunk). Empty list
/// signals failure (Decision 7).
///
/// # Safety
///
/// Same contract as [`__cobrust_llm_complete`] for the input pointers.
/// The returned list pointer must be freed via `__cobrust_list_drop`;
/// each element Str must be freed via `__cobrust_str_drop`. Drop
/// schedule ownership belongs to codegen (M12.x convention — see
/// ADR-0044 W2 Phase 2 `__cobrust_argv` precedent).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_stream(
    provider: *mut u8,
    model: *mut u8,
    prompt: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per the `# Safety` clause.
    let p = unsafe { read_str_buf(provider) };
    // SAFETY: same.
    let m = unsafe { read_str_buf(model) };
    // SAFETY: same.
    let q = unsafe { read_str_buf(prompt) };
    let chunks = llm_stream_blocking(&p, &m, &q);
    // SAFETY: `__cobrust_list_new(8, len)` returns a valid List<i64>
    // pointer with `len` zeroed slots. Each slot stores a Str pointer
    // produced by `alloc_str_buffer` (which wraps `__cobrust_str_new`).
    unsafe {
        let list = crate::collections::__cobrust_list_new(8, chunks.len() as i64);
        for (i, s) in chunks.iter().enumerate() {
            let buf = alloc_str_buffer(s);
            crate::collections::__cobrust_list_set(list, i as i64, buf as i64);
        }
        list
    }
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn build_bundle_missing_config_returns_io_err() {
        // Validate that build_bundle returns BuildErr::Io for an
        // unreadable config path. Edition 2024 makes std::env::set_var
        // / remove_var `unsafe`; we wrap the env mutation in an
        // unsafe block (sole writer in this test) so the assertion
        // is exercised. Mutex-free: lib-tests run sequentially per
        // crate by default, and this is the only test that touches
        // COBRUST_CONFIG.
        // SAFETY: `set_var` / `remove_var` are sound here because
        // this test is the only writer of COBRUST_CONFIG in the
        // single-threaded lib-tests harness; no other thread reads
        // the variable during this critical section.
        unsafe {
            std::env::remove_var("COBRUST_CONFIG");
            std::env::set_var("COBRUST_CONFIG", "/__cobrust_definitely_not_present__.toml");
        }
        let r = build_bundle();
        assert!(matches!(r, Err(BuildErr::Io(_))));
        // SAFETY: see above — restore the un-set state for any
        // subsequent test in the same process.
        unsafe {
            std::env::remove_var("COBRUST_CONFIG");
        }
    }

    #[test]
    fn strip_synthetic_providers_drops_synthetic_kinds() {
        let toml = r#"
[router]
default_strategy = "quality"

[providers.real]
kind = "openai"
base_url = "http://x"
api_key_env = "REAL_KEY"
models = ["m1"]

[providers.fake]
kind = "synthetic"
base_url = "synthetic://"
api_key_env = "FAKE_KEY"
models = ["m1"]

[routing.real_task]
strategy = "quality"
preferred = ["real:m1"]

[routing.fake_task]
strategy = "quality"
preferred = ["fake:m1"]
"#;
        let cfg = RouterConfig::from_toml_str(toml).unwrap();
        let stripped = strip_synthetic_providers(&cfg);
        assert!(!stripped.providers.contains_key("fake"));
        assert!(stripped.providers.contains_key("real"));
        assert!(stripped.routing.contains_key("real_task"));
        assert!(!stripped.routing.contains_key("fake_task"));
    }
}
