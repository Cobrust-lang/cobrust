//! `std.time` — timing + timestamps (`import time`).
//!
//! ADR-0087 pins this surface. The four functions are the universal
//! timing core of CPython's `time` module — the ones every program
//! reaches for to read a clock or pause, with NO struct-time / strftime
//! calendar machinery (those are a documented follow-up):
//!
//! - `time.time() -> f64` — current Unix-epoch time in SECONDS as a
//!   float (a WALL clock; `SystemTime::now().duration_since(UNIX_EPOCH)`).
//!   0-arg. For a timestamp / "what time is it".
//! - `time.monotonic() -> f64` — seconds from a PROCESS-relative origin,
//!   monotonically non-decreasing (a high-res `Instant`). 0-arg. For
//!   MEASURING an interval (never goes backwards, immune to wall-clock
//!   adjustments / NTP steps / DST).
//! - `time.perf_counter() -> f64` — the SAME high-res `Instant` clock as
//!   `monotonic` (CPython documents them as distinct named clocks, but
//!   on every supported platform both are the highest-resolution
//!   monotonic source; Cobrust unifies them onto ONE `START` `Instant`).
//!   0-arg.
//! - `time.sleep(secs: f64)` — suspend the current thread for `secs`
//!   seconds (`std::thread::sleep(Duration::from_secs_f64(secs))`).
//!   CPython returns `None`; Cobrust returns a discarded i64 SENTINEL
//!   (the `random.seed` / dora `event.send_output` pattern, ADR-0086).
//!
//! **The monotonic origin** (the design crux, ADR-0087 §"Monotonic
//! origin"): a process-global `static START: OnceLock<Instant>`, lazily
//! captured by `Instant::now()` on FIRST use of `monotonic` /
//! `perf_counter`. `monotonic()` then returns `START.elapsed().as_secs_f64()`
//! — seconds since that first call (so the first call returns ≈ 0.0, and
//! every later call returns a larger value). This is PROCESS-relative by
//! design: Python's `monotonic` only promises a *fixed reference point
//! within a process*, never a meaningful absolute. The `OnceLock` is the
//! lazy-static mechanism (no `lazy_static`/`once_cell` dep — std only);
//! `get_or_init(Instant::now)` is the one-time capture.
//!
//! **`perf_counter` ≡ `monotonic`** (ADR-0087 §"perf_counter"): both
//! shims read the SAME `START` `Instant`. `Instant` IS the highest-
//! resolution monotonic clock the platform offers (the very thing
//! `perf_counter` is specified to be), so a second independent clock
//! would be pure ceremony. They are deliberately the same number.
//!
//! **The negative-`sleep` guard** (the safety crux, ADR-0087
//! §"Negative sleep"): `Duration::from_secs_f64(x)` PANICS for `x < 0`
//! (and for NaN / +∞). CPython's `time.sleep(-1)` raises `ValueError`.
//! Cobrust takes the GENTLER, SAFE path: `secs <= 0.0` (covers negative,
//! zero, and — via the `!(secs > 0.0)` form — NaN) is a NO-OP that
//! returns immediately, NEVER a panic across the C-ABI. A non-positive
//! sleep has no meaningful "pause" to perform, so returning at once is
//! the correct, surprise-free behavior.
//!
//! **Determinism + `@py_compat` tier: Semantic** (ADR-0087 §"Tier").
//! A clock is ENVIRONMENT STATE, NOT reproducible: `time.time()` returns
//! a different value every call (wall time advances), `monotonic()`
//! depends on when the process started, and `sleep` is best-effort
//! (the OS scheduler may oversleep). So a RAW clock read is NOT
//! assertable for an exact value; what IS assertable is the ORDERING /
//! RANGE (a sane post-2023 epoch, a non-decreasing monotonic, a sleep
//! that delays AT LEAST roughly its argument). This mirrors `random`'s
//! honest "the raw draw is non-deterministic; only the contract is
//! assertable" posture (ADR-0086 §"Determinism"). Cobrust does NOT
//! reproduce CPython's exact float values (different epoch float
//! rounding, different monotonic origin); the CONTRACT is the clock
//! SEMANTICS (wall vs monotonic, seconds-as-float), not bit-identity.
//!
//! **ABI** — the four `__cobrust_time_*` symbols are scalar-in /
//! scalar-out, the SIMPLEST C-ABI shape (no Str/list buffer
//! marshalling, like `random`): `time` / `monotonic` / `perf_counter`
//! are `() -> f64`, `sleep` is `(f64) -> i64` (the discarded sentinel).
//! The generic ecosystem-call path drives the args + return off the
//! `EcoSig` rows in `cobrust-types`; codegen only declares the externs
//! (NO new MIR arm — 0-arg reuses `random.random`, the f64-arg reuses
//! `math.sqrt`).
//!
//! **M13 threading note** (ADR-0087 §"Threading"): the `static START`
//! is PROCESS-global (one `OnceLock`, shared across threads). This is
//! CORRECT for `monotonic` — `Instant` is monotonic *per machine*, so a
//! single process-wide origin gives every thread a consistent timeline
//! (two threads reading `monotonic()` get values comparable to each
//! other). `OnceLock` is `Sync` and its `get_or_init` is race-safe
//! (exactly one initializer wins), so concurrent first-use from M13
//! `task::spawn`ed threads is sound with no lock on the hot read path.

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The process-global monotonic origin — captured by `Instant::now()`
/// on the FIRST call to [`monotonic`] / [`perf_counter`]. `OnceLock` is
/// the std lazy-static (no `once_cell` dep); `get_or_init` performs the
/// one-time capture race-safely (M13 threading, ADR-0087). Both
/// `monotonic` and `perf_counter` read THIS one `Instant` — they are
/// deliberately the same clock.
static START: OnceLock<Instant> = OnceLock::new();

// =====================================================================
// Rust-side helpers (testable without the C-ABI). The clock reads are
// total (no panic); `sleep` guards its one panic source (a negative /
// NaN `Duration::from_secs_f64`).
// =====================================================================

/// `time.time()` — current Unix-epoch time in SECONDS as an `f64` (a
/// WALL clock). `duration_since(UNIX_EPOCH)` is `Err` only if the system
/// clock is set BEFORE 1970 (a misconfigured host); `unwrap_or_default()`
/// then yields `0.0` rather than panicking across the C-ABI — a safe,
/// non-unwinding fallback. The fractional part carries sub-second
/// precision (`as_secs_f64`), matching CPython `time.time()`.
fn time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// `time.monotonic()` — seconds since the process-relative [`START`]
/// origin, monotonically non-decreasing. The FIRST call captures
/// `START` (and returns ≈ 0.0); every later call returns
/// `START.elapsed()` in seconds, never smaller than a previous call
/// (`Instant` is monotonic by construction — immune to wall-clock
/// steps). For MEASURING intervals.
fn monotonic() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// `time.perf_counter()` — the highest-resolution monotonic clock,
/// reading the SAME [`START`] `Instant` as [`monotonic`]. CPython names
/// them distinctly, but `Instant` already IS the platform's best
/// monotonic source, so the two are deliberately identical (ADR-0087
/// §"perf_counter"). Defined as a thin alias to keep the two named
/// entry points while sharing one origin.
fn perf_counter() -> f64 {
    monotonic()
}

/// `time.sleep(secs)` — suspend the current thread for `secs` seconds.
///
/// The NEGATIVE-GUARD (ADR-0087 §"Negative sleep"): the `secs > 0.0`
/// test is load-bearing. `Duration::from_secs_f64(x)` PANICS for any
/// `x < 0.0`, for `NaN`, and for `+∞`; CPython raises `ValueError` on a
/// negative sleep. The guard makes a non-positive (or NaN) `secs` a
/// NO-OP — it returns IMMEDIATELY, never panicking across the C-ABI.
/// Only a strictly-positive, finite `secs` reaches `thread::sleep`
/// (where `from_secs_f64` is total). The OS may oversleep slightly
/// (best-effort timing — the Semantic-tier honesty).
fn sleep(secs: f64) {
    // `secs > 0.0` is FALSE for negative, zero, AND NaN (every NaN
    // comparison is false), so all three panic-or-no-pause cases skip
    // `from_secs_f64`. A finite `+∞` would also be filtered were it
    // somehow positive-infinite — but `from_secs_f64` only panics on
    // non-finite, and `secs > 0.0` admits `+∞`; guard it explicitly via
    // `is_finite` so an infinite sleep is a no-op too (never a panic).
    if secs > 0.0 && secs.is_finite() {
        std::thread::sleep(Duration::from_secs_f64(secs));
    }
}

// =====================================================================
// C-ABI shims — the `__cobrust_time_*` symbols codegen declares + calls.
// All scalar-in / scalar-out (no buffer marshalling). Each delegates to
// the Rust helper above. NONE unwinds across the C-ABI: the clock reads
// are total (the epoch fallback is `unwrap_or_default`), and `sleep`
// guards its only panic source.
// =====================================================================

/// C-ABI shim for `time.time() -> f64` (0-arg). The `-> f64` lowers to
/// an LLVM `double` return, mirroring `__cobrust_random_random` (the
/// 0-arg scalar precedent). The `_ecoret` Float local at the call site
/// receives the Unix-epoch seconds.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_time_time() -> f64 {
    time()
}

/// C-ABI shim for `time.monotonic() -> f64` (0-arg) — seconds since the
/// process-relative origin, non-decreasing. Same `() -> f64` shape as
/// [`__cobrust_time_time`]; the FIRST call lazily captures [`START`].
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_time_monotonic() -> f64 {
    monotonic()
}

/// C-ABI shim for `time.perf_counter() -> f64` (0-arg) — the SAME
/// high-res monotonic clock as [`__cobrust_time_monotonic`] (reads the
/// shared [`START`] `Instant`). Deliberately equal to `monotonic`.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_time_perf_counter() -> f64 {
    perf_counter()
}

/// C-ABI shim for `time.sleep(secs)`. CPython's `time.sleep` returns
/// `None`; Cobrust types the call `Ty::Int` and this shim returns a `0`
/// SENTINEL the call site discards (`let _ = time.sleep(d)`) — the
/// `random.seed` / dora `event.send_output` discard pattern (ADR-0086).
/// Avoids the `Ty::None -> void` C-ABI mismatch. The pause is the whole
/// payload; the returned value carries no information. A non-positive /
/// NaN `secs` is a NO-OP (the [`sleep`] guard), so this NEVER panics.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_time_sleep(secs: f64) -> i64 {
    sleep(secs);
    0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- time(): a SANE post-2023 Unix-epoch range ----------------------
    // A clock read is non-deterministic (advances every call), so we
    // assert the RANGE, not an exact value: any real run is AFTER 2023
    // (1.7e9 s) and — for the lifetime of this code — BEFORE ~2033
    // (2e9 s). A broken `time()` (returning 0, or milliseconds, or
    // nanoseconds) would fall outside this window.

    #[test]
    fn time_is_a_sane_unix_epoch() {
        let t = time();
        assert!(
            t > 1.7e9,
            "time() must be a post-2023 Unix epoch in SECONDS (> 1.7e9); got {t}",
        );
        assert!(
            t < 2.0e9,
            "time() must be Unix-epoch SECONDS, not millis/nanos (< 2e9); got {t}",
        );
    }

    // -- monotonic(): non-decreasing — the load-bearing contract --------
    // `monotonic` exists to MEASURE intervals: a later call is NEVER
    // smaller than an earlier one. We call it twice with a tiny bit of
    // work between; the second read must be >= the first. This is the
    // only hard guarantee `monotonic` makes (the absolute value is
    // process-relative and not assertable).

    #[test]
    fn monotonic_is_non_decreasing() {
        let a = monotonic();
        // A little work so the clock can advance (and to defeat any
        // constant-fold that would make this vacuous).
        let mut acc = 0u64;
        for i in 0..10_000 {
            acc = acc.wrapping_add(i);
        }
        assert!(acc > 0);
        let b = monotonic();
        assert!(
            b >= a,
            "monotonic() must be non-decreasing; first={a}, second={b}",
        );
    }

    #[test]
    fn monotonic_first_read_is_near_zero() {
        // The process-relative origin means an early read is small (a
        // few ms at most into a test process). Generous upper bound so
        // this is not flaky under a loaded CI box; the point is it is
        // PROCESS-relative seconds, not a Unix epoch (which would be
        // ~1.7e9). If `monotonic` accidentally returned wall time, this
        // would fail by ~9 orders of magnitude.
        let m = monotonic();
        assert!(
            (0.0..3600.0).contains(&m),
            "monotonic() is process-relative seconds (not a Unix epoch); got {m}",
        );
    }

    // -- perf_counter ≡ monotonic (same clock) --------------------------

    #[test]
    fn perf_counter_shares_the_monotonic_clock() {
        // Both read the SAME `START` Instant, so consecutive reads are
        // ordered across the two names exactly as within one: a
        // perf_counter taken AFTER a monotonic is >= it.
        let m = monotonic();
        let p = perf_counter();
        assert!(
            p >= m,
            "perf_counter() and monotonic() share one clock; perf={p} should be >= mono={m}",
        );
    }

    // -- sleep(d) actually delays AT LEAST ~d ---------------------------
    // sleep is best-effort (the OS may oversleep, never UNDER-sleep by
    // much), so we assert a LOWER bound: after sleep(0.05), at least
    // ~0.03 s elapsed on the monotonic clock. A broken sleep (a no-op,
    // or wrong units) would elapse ≈ 0.

    #[test]
    fn sleep_delays_at_least_its_argument() {
        let t0 = monotonic();
        sleep(0.05);
        let t1 = monotonic();
        let dt = t1 - t0;
        assert!(
            dt >= 0.03,
            "sleep(0.05) must delay at least ~0.03s; measured {dt}s",
        );
    }

    // -- the NEGATIVE-GUARD: sleep(<=0) / NaN is an IMMEDIATE no-op ------
    // `Duration::from_secs_f64(negative)` PANICS; the guard turns a
    // non-positive (or NaN) sleep into a no-op that returns at once. If
    // the guard regressed, these would PANIC (test failure) rather than
    // return — proving the guard is doing its job.

    #[test]
    fn sleep_negative_is_a_noop_not_a_panic() {
        // No panic, and effectively instantaneous.
        let t0 = monotonic();
        sleep(-1.0);
        let t1 = monotonic();
        assert!(
            t1 - t0 < 0.5,
            "sleep(-1.0) must be an immediate no-op (no pause, no panic); elapsed {}s",
            t1 - t0,
        );
    }

    #[test]
    fn sleep_zero_is_a_noop() {
        let t0 = monotonic();
        sleep(0.0);
        let t1 = monotonic();
        assert!(
            t1 - t0 < 0.5,
            "sleep(0.0) must return immediately; elapsed {}s",
            t1 - t0,
        );
    }

    #[test]
    fn sleep_nan_is_a_noop_not_a_panic() {
        // NaN > 0.0 is false, so the guard skips `from_secs_f64` (which
        // would otherwise panic on NaN). A no-op, no panic.
        sleep(f64::NAN);
        // Reaching here without a panic is the assertion.
    }

    // -- the C-ABI shims delegate to the helpers (same contract) --------

    #[test]
    fn sleep_shim_returns_zero_sentinel() {
        // The discarded sentinel is always 0 (the value carries no info;
        // the pause is the payload). A non-positive arg is a no-op.
        assert_eq!(__cobrust_time_sleep(-5.0), 0);
        assert_eq!(__cobrust_time_sleep(0.0), 0);
    }

    #[test]
    fn time_shim_is_a_sane_epoch() {
        let t = __cobrust_time_time();
        assert!(
            (1.7e9..2.0e9).contains(&t),
            "time shim epoch range; got {t}"
        );
    }

    #[test]
    fn monotonic_shim_is_non_decreasing() {
        let a = __cobrust_time_monotonic();
        let b = __cobrust_time_monotonic();
        assert!(b >= a, "monotonic shim non-decreasing; a={a}, b={b}");
    }

    #[test]
    fn perf_counter_shim_shares_clock() {
        let m = __cobrust_time_monotonic();
        let p = __cobrust_time_perf_counter();
        assert!(
            p >= m,
            "perf_counter shim shares the monotonic clock; p={p}, m={m}"
        );
    }
}
