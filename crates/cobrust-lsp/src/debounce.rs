//! Per-URI bounded debounce per ADR-0057b §3.5.
//!
//! The LSP `did_change` handler spawns one `tokio::task` per event;
//! each task records its `(uri, version)` in [`DebounceTokens`] and
//! sleeps for the debounce window. After waking, the task checks
//! whether its version is still the latest recorded; if a newer
//! `did_change` arrived during the window, the older task self-cancels
//! and the newer task runs the pipeline.
//!
//! The net effect: N events arriving within the window collapse to one
//! pipeline re-run + one `publish_diagnostics` emission. Production
//! default is 100ms (per LSP best practice and ADR-0057a §9 Risk 3);
//! tests pass `0` to bypass entirely.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;
use tower_lsp::lsp_types::Url;

/// Default debounce window for LSP `did_change` per ADR-0057b §3.5.
pub const DEFAULT_DEBOUNCE_MS: u64 = 100;

/// One entry in the [`DebounceTokens`] map. Each `did_change` records a
/// version + a `Notify` handle; the spawned task waits on the `Notify`
/// (or the sleep timer, whichever fires first).
#[derive(Debug)]
struct UriDebounceEntry {
    /// Latest version observed for this URI. Spawned tasks compare
    /// their own version against this; if their version is lower they
    /// self-cancel.
    latest_version: i32,
}

/// Per-URI debounce state shared across the [`crate::Backend`].
///
/// One `DebounceTokens` per `Backend`. Thread-safe via interior `Mutex`.
#[derive(Debug)]
pub struct DebounceTokens {
    /// Debounce window. `Duration::ZERO` disables debouncing (tests).
    window: Duration,
    /// URI → latest scheduled version. Updated under the mutex.
    inner: Mutex<HashMap<Url, UriDebounceEntry>>,
}

/// Opaque token returned by [`DebounceTokens::schedule`]. Hand it to
/// [`wait_for_token`] to block for the debounce window before proceeding.
#[derive(Debug, Clone)]
pub struct DebounceToken {
    /// Sleep duration before the task should wake.
    pub(crate) window: Duration,
}

impl DebounceTokens {
    /// Construct a new debounce-token store with the given window.
    /// Pass `Duration::ZERO` to bypass debouncing entirely.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Record a new `(uri, version)` scheduling and return a token the
    /// spawned task uses to wait out the debounce window. Subsequent
    /// calls for the same URI overwrite the recorded version; older
    /// tasks waking up after their version is overtaken self-cancel
    /// (see [`Self::is_latest`]).
    #[must_use]
    pub fn schedule(&self, uri: Url, version: i32) -> DebounceToken {
        let mut map = self.inner.lock().expect("debounce inner poisoned");
        map.insert(
            uri,
            UriDebounceEntry {
                latest_version: version,
            },
        );
        DebounceToken {
            window: self.window,
        }
    }

    /// Returns `true` if `version` matches the latest recorded version
    /// for `uri`. Spawned tasks call this after their debounce sleep
    /// to decide whether to run the pipeline.
    #[must_use]
    pub fn is_latest(&self, uri: &Url, version: i32) -> bool {
        let map = self.inner.lock().expect("debounce inner poisoned");
        map.get(uri)
            .is_some_and(|entry| entry.latest_version == version)
    }

    /// Forget the latest-version record for `uri` (called by tests
    /// asserting on debounce coalescing).
    pub fn forget(&self, uri: &Url) {
        let mut map = self.inner.lock().expect("debounce inner poisoned");
        map.remove(uri);
    }

    /// Window duration (for diagnostics).
    #[must_use]
    pub fn window(&self) -> Duration {
        self.window
    }
}

impl Default for DebounceTokens {
    fn default() -> Self {
        Self::new(Duration::from_millis(DEFAULT_DEBOUNCE_MS))
    }
}

/// Wait for the debounce window. Inlined into the spawned `did_change`
/// task per ADR-0057b §3.5.
pub async fn wait_for_token(token: DebounceToken) {
    if token.window.is_zero() {
        // Test path — pass through.
        // Yield once so the calling task can be scheduled out.
        tokio::task::yield_now().await;
        return;
    }
    tokio::time::sleep(token.window).await;
}

// `Notify` is imported but currently unused; kept for a follow-up
// sub-ADR that promotes the simple version-bump cancellation to a true
// async-aware cancel. Suppress the dead-import lint.
#[allow(dead_code)]
fn _notify_keepalive() -> Notify {
    Notify::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("static URL parses")
    }

    #[test]
    fn schedule_and_is_latest_round_trip() {
        let store = DebounceTokens::new(Duration::from_millis(50));
        let u = url("file:///a.cb");
        let _t = store.schedule(u.clone(), 1);
        assert!(store.is_latest(&u, 1));
        assert!(!store.is_latest(&u, 0));
        let _t2 = store.schedule(u.clone(), 2);
        assert!(store.is_latest(&u, 2));
        assert!(!store.is_latest(&u, 1));
    }

    #[test]
    fn forget_clears_entry() {
        let store = DebounceTokens::new(Duration::from_millis(50));
        let u = url("file:///b.cb");
        let _t = store.schedule(u.clone(), 7);
        store.forget(&u);
        assert!(!store.is_latest(&u, 7));
    }

    #[test]
    fn zero_window_uses_pass_through() {
        let store = DebounceTokens::new(Duration::ZERO);
        let token = store.schedule(url("file:///z.cb"), 42);
        assert!(token.window.is_zero());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wait_for_token_returns_quickly_under_zero_window() {
        let token = DebounceToken {
            window: Duration::ZERO,
        };
        let started = std::time::Instant::now();
        wait_for_token(token).await;
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wait_for_token_respects_short_window() {
        // Use a short (15ms) real window to keep the test cheap while
        // still validating that wait_for_token sleeps. test-util's
        // `start_paused` is gated behind a feature we don't enable.
        let token = DebounceToken {
            window: Duration::from_millis(15),
        };
        let started = std::time::Instant::now();
        wait_for_token(token).await;
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(10),
            "expected ≥10ms, got {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "expected <500ms, got {elapsed:?}"
        );
    }
}
