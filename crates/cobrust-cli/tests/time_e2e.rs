//! `import time` (timing + timestamps) — `.cb` end-to-end proof for the
//! ADR-0087 addition: std `SystemTime` (wall clock) + a lazy-static
//! `Instant` origin (monotonic) + `thread::sleep`. UNIVERSAL — reading a
//! clock and pausing are two of the most-reached-for Python capabilities,
//! and the L0-L3 translation pipeline needs them. These tests compile to
//! REAL binaries, link, spawn, and assert stdout / exit code, proving the
//! scalar f64 clock reads + the i64-sentinel sleep are usable END-TO-END.
//!
//! ## The four functions (all scalar, NO calendar / struct-time)
//!
//! - `time.time() -> f64` — current Unix-epoch SECONDS as a float (a WALL
//!   clock); 0-arg (the `random.random` 0-arg precedent).
//! - `time.monotonic() -> f64` — process-relative seconds, monotonically
//!   non-decreasing (a lazy-static `Instant`); for MEASURING intervals.
//! - `time.perf_counter() -> f64` — the SAME high-res monotonic clock as
//!   `monotonic` (one shared `START` Instant; ADR-0087 unifies them).
//! - `time.sleep(secs)` — suspend the thread `secs` seconds; `secs <= 0.0`
//!   / NaN is a NO-OP (the shim guards the `from_secs_f64(neg)` panic).
//!   Returns a discarded i64 sentinel (CPython returns None; the
//!   `random.seed` / dora `event.send_output` discard pattern).
//!
//! ## The load-bearing semantics (clocks are NON-deterministic)
//!
//! A clock read advances every call and `monotonic`'s origin is process-
//! start, so an EXACT value is NOT assertable. What IS assertable is the
//! ORDERING / RANGE:
//!
//! - `time()` lands in a SANE post-2023 Unix-epoch window (> 1.7e9 s) — a
//!   broken clock (0, or millis, or nanos) falls outside.
//! - `monotonic()` called twice is NON-decreasing (`b >= a`) — the only
//!   hard guarantee it makes, and the whole reason it exists.
//! - `sleep(d)` then re-reading `monotonic` shows AT LEAST ~`d` elapsed (a
//!   real delay; the OS may oversleep, never under-sleep by much).
//! - `sleep(negative)` is an IMMEDIATE no-op that does NOT panic (exit 0).
//!
//! ## @py_compat tier: Semantic (clocks are environment state)
//!
//! A clock is ENVIRONMENT STATE, NOT reproducible. Cobrust does NOT
//! reproduce CPython's exact float values (different epoch rounding,
//! different monotonic origin); the CONTRACT is the clock SEMANTICS (wall
//! vs monotonic, seconds-as-float, ordering/range), NOT bit-identity
//! (ADR-0087 §"Tier"; mirrors `random`'s honest "raw read non-
//! deterministic; only the contract is assertable" posture).
//!
//! Mirrors the compile->spawn->assert-stdout harness of `random_e2e` /
//! `re_e2e` / `math_e2e`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets. (This `#![allow]` is
// the lesson math-part2 / re / random learned the hard way — the doc-lint on
// the e2e header only surfaces in `-p cobrust-cli --all-targets` clippy.)
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `random_e2e::compile_source`.
fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

/// Spawn a compiled program; return `(stdout, stderr, success)`.
fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe).output().expect("spawn time prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// `time()` is a SANE post-2023 Unix epoch (in SECONDS). The raw value is
// non-deterministic (advances every call), so we assert RANGE membership:
// `time() > 1.7e9`. A broken clock (0, or millis ~1.7e12, or nanos
// ~1.7e18) would print `STALE` instead of `epoch-ok`. The canonical
// Cobrust bool idiom is `if cond: print(1) else: print(0)` — here a
// labelled token for a clearer failure message.
// =====================================================================

/// `time() > 1.7e9` (a post-2023 epoch in seconds): prints `epoch-ok`
/// when the wall clock is sane, `STALE` otherwise. The load-bearing
/// proof that `time.time()` returns Unix-epoch SECONDS end-to-end.
#[test]
fn test_e2e_time_is_a_sane_epoch() {
    let source = concat!(
        "import time\n",
        "\n",
        "fn main() -> i64:\n",
        "    let t: f64 = time.time()\n",
        "    if t > 1.7e9:\n",
        "        print(\"epoch-ok\")\n",
        "    else:\n",
        "        print(\"STALE\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "epoch-ok",
        "time() must be a post-2023 Unix epoch in SECONDS (> 1.7e9); got stdout=\n{stdout}",
    );
}

// =====================================================================
// `monotonic()` is NON-decreasing. Two reads with work between: `b >= a`.
// This is the only hard guarantee `monotonic` makes (the absolute value
// is process-relative and not assertable) and the whole reason it
// exists (interval timing).
// =====================================================================

/// `let a = monotonic(); <work>; let b = monotonic()` => `b >= a`. Prints
/// `mono-ok` iff the clock did not go backwards. THE load-bearing
/// monotonic proof, end-to-end through codegen.
#[test]
fn test_e2e_monotonic_is_non_decreasing() {
    let source = concat!(
        "import time\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: f64 = time.monotonic()\n",
        // A tiny real delay so the clock can advance (and to make the
        // ordering observable, not a vacuous `a == a`).
        "    let _ = time.sleep(0.01)\n",
        "    let b: f64 = time.monotonic()\n",
        "    if b >= a:\n",
        "        print(\"mono-ok\")\n",
        "    else:\n",
        "        print(\"BACKWARDS\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "mono-ok",
        "monotonic() must be non-decreasing (b >= a); got stdout=\n{stdout}",
    );
}

// =====================================================================
// `perf_counter()` shares the monotonic clock. A `perf_counter` taken
// AFTER a `monotonic` is >= it (one shared `START` Instant). Proves the
// `perf_counter ≡ monotonic` unification end-to-end.
// =====================================================================

/// `let m = monotonic(); let p = perf_counter()` => `p >= m` (same
/// clock). Prints `perf-ok` iff they are ordered as one clock would be.
#[test]
fn test_e2e_perf_counter_shares_clock() {
    let source = concat!(
        "import time\n",
        "\n",
        "fn main() -> i64:\n",
        "    let m: f64 = time.monotonic()\n",
        "    let p: f64 = time.perf_counter()\n",
        "    if p >= m:\n",
        "        print(\"perf-ok\")\n",
        "    else:\n",
        "        print(\"PERF-BEFORE-MONO\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "perf-ok",
        "perf_counter() shares the monotonic clock (p >= m); got stdout=\n{stdout}",
    );
}

// =====================================================================
// `sleep(d)` DELAYS at least ~d. `t0 = monotonic(); sleep(0.05);
// t1 = monotonic()` => `t1 - t0 >= 0.03` (a real delay; the OS may
// oversleep, never under-sleep by much, so a LOWER bound below the
// requested 0.05 absorbs timer granularity). A broken sleep (a no-op,
// or wrong units) would elapse ≈ 0 and print `NO-DELAY`.
// =====================================================================

/// `t0 = monotonic(); sleep(0.05); t1 = monotonic()` then check
/// `t1 - t0 >= 0.03`. Prints `slept` iff the pause actually happened.
/// THE load-bearing proof that `sleep` suspends the thread end-to-end.
#[test]
fn test_e2e_sleep_delays() {
    let source = concat!(
        "import time\n",
        "\n",
        "fn main() -> i64:\n",
        "    let t0: f64 = time.monotonic()\n",
        "    let _ = time.sleep(0.05)\n",
        "    let t1: f64 = time.monotonic()\n",
        "    let dt: f64 = t1 - t0\n",
        "    if dt >= 0.03:\n",
        "        print(\"slept\")\n",
        "    else:\n",
        "        print(\"NO-DELAY\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "slept",
        "sleep(0.05) must delay at least ~0.03s on the monotonic clock; got stdout=\n{stdout}",
    );
}

// =====================================================================
// `sleep(negative)` is an IMMEDIATE no-op that does NOT panic. The shim
// guards `Duration::from_secs_f64(neg)` (which PANICS); a negative arg
// returns at once. The proof is a clean exit 0 (a panic would abort with
// a non-zero status). Printing `1` after the negative sleep confirms
// control flow continued past it.
// =====================================================================

/// `let _ = sleep(-1.0); print(1)` — the negative sleep is a no-op (no
/// panic), so `1` prints AND the process exits 0. A regressed guard
/// would abort (non-zero exit) before `print(1)`.
#[test]
fn test_e2e_negative_sleep_does_not_panic() {
    let source = concat!(
        "import time\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = time.sleep(-1.0)\n",
        "    print(1)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "negative sleep must NOT panic (clean exit 0); stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert_eq!(
        stdout.trim(),
        "1",
        "control must continue past a no-op negative sleep; got stdout=\n{stdout}",
    );
}

/// `sleep(0.0)` is likewise an immediate no-op (the boundary of the
/// `secs > 0.0` guard). Prints `zero-ok` and exits 0 — a complement to
/// the negative case, pinning the `<= 0.0` no-op boundary.
#[test]
fn test_e2e_zero_sleep_does_not_panic() {
    let source = concat!(
        "import time\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = time.sleep(0.0)\n",
        "    print(\"zero-ok\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "zero sleep must NOT panic; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "zero-ok",
        "sleep(0.0) must be an immediate no-op; got stdout=\n{stdout}",
    );
}
