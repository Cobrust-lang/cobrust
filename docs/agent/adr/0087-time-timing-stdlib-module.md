---
doc_kind: adr
adr_id: 0087
title: time — timing + timestamps stdlib module (import time) via std SystemTime (wall) + a lazy-static Instant origin (monotonic) + thread::sleep, with perf_counter ≡ monotonic and a negative-sleep no-op guard + cobrust-stdlib shims
status: accepted
date: 2026-06-05
last_verified_commit: 7e924d5
supersedes: []
superseded_by: []
---

# ADR-0087: `time` — the timing + timestamps stdlib module (`import time`)

## Context

Reading a clock and pausing a thread are universal in Python, and the
L0–L3 translation pipeline needs them (timestamps, interval benchmarks,
rate-limit back-offs, retry sleeps). `time` is the **4th core stdlib
module** after `math` (ADR-0083), `re` (ADR-0084), and `random`
(ADR-0086), wired through the SAME generic ecosystem-call path.

`time` introduces ONE thing with no precedent in the stdlib modules: a
**lazy-static monotonic origin**. `monotonic` / `perf_counter` measure
*intervals*, which requires a fixed reference point captured once per
process — a `static START: OnceLock<Instant>` initialized on first use.
This is the std-only analogue of `random`'s module-global RNG: ambient
process state, but read-only after the one-time capture.

Everything else is already-shipped:

- The four functions are **scalar-in / scalar-out** — the SIMPLEST
  ecosystem-call shape (no Str/list buffer marshalling, unlike `re`; no
  Buffer, unlike `coil`), identical to `random`.
- No new dependency: `std::time::{SystemTime, Instant, Duration}` +
  `std::thread::sleep` + `std::sync::OnceLock` are all `std`. The
  `Cargo.lock` is unchanged (the `random` precedent pulled `rand_pcg`;
  `time` pulls NOTHING).

### Scope (the timing core — 4 functions)

| `.cb` call | Signature | Semantics |
|---|---|---|
| `time.time()` | `[] -> f64` | current Unix-epoch SECONDS as a float (WALL clock) |
| `time.monotonic()` | `[] -> f64` | process-relative seconds, non-decreasing (interval clock) |
| `time.perf_counter()` | `[] -> f64` | ≡ `monotonic` (the SAME high-res `Instant`) |
| `time.sleep(secs)` | `[Float] -> Int` (sentinel) | suspend the thread `secs` s; `secs <= 0.0` is a no-op |

### Deferred (NOT in this cut)

`time_ns()`, `monotonic_ns()`, `perf_counter_ns()` (the integer-nanosecond
variants), `process_time()` / `thread_time()` (CPU clocks), and the entire
calendar / `struct_time` surface (`gmtime`, `localtime`, `mktime`,
`strftime`, `strptime`, `asctime`, `ctime`). The `*_ns` forms are a thin
follow-up (same shims, `as_nanos() as i64` returns); the calendar machinery
needs a `struct_time` aggregate + timezone handling (a much larger surface).
The manifest returns `None` for all of these, so a `.cb` `time.strftime(...)`
is a compile-time `UnknownName` (§2.5 compile-time-catch), NOT a false-green
binding.

## Options considered

### Q1 — the monotonic origin

1. **`static START: OnceLock<Instant>`, `get_or_init(Instant::now)`**
   (CHOSEN). One process-global origin captured lazily on the FIRST
   `monotonic` / `perf_counter` call. `monotonic()` returns
   `START.elapsed().as_secs_f64()`. `OnceLock` is std (no `once_cell` /
   `lazy_static` dep); its `get_or_init` is race-safe (exactly one
   initializer wins under concurrent first-use), so it is correct for M13
   `task::spawn`ed threads with NO lock on the hot read path.
2. **A `thread_local! { START: Instant }`** (rejected). Per-thread origins
   mean two threads' `monotonic()` values are NOT comparable to each other
   (each measures "since *my* first call"). `Instant` is monotonic
   *per machine*, so a single process-wide origin is the correct model —
   every thread shares one timeline. (Contrast `random`, where a per-thread
   RNG is correct because each thread *wants* an independent stream.)
3. **Capture `START` eagerly at `main` entry** (rejected). Requires a
   codegen-emitted init call or a `ctor`; the lazy `OnceLock` needs neither
   and makes the first `monotonic()` return ≈ 0.0 (a clean "zero at first
   read" semantics) with zero startup cost for programs that never time.

### Q2 — `perf_counter` vs `monotonic`

CPython documents `perf_counter` and `monotonic` as two named clocks (with
possibly different resolutions / epochs on exotic platforms).

1. **`perf_counter` ≡ `monotonic`, one shared `START` `Instant`** (CHOSEN).
   `std::time::Instant` IS the highest-resolution monotonic clock the
   platform offers (`QueryPerformanceCounter` on Windows,
   `clock_gettime(CLOCK_MONOTONIC/RAW)` on Unix, `mach_absolute_time` on
   macOS) — the very thing `perf_counter` is specified to be. A second
   independent clock would be pure ceremony returning the same numbers.
   `perf_counter()` is a thin alias to `monotonic()`.
2. **A second `static START_PERF: OnceLock<Instant>`** (rejected). Two
   origins, two near-identical numbers, zero added value, and a subtle
   footgun (a program mixing `monotonic` and `perf_counter` deltas would
   get values off by the tiny gap between the two first-captures). One
   clock is the honest, surprise-free choice.

### Q3 — the negative `sleep`

`Duration::from_secs_f64(x)` **PANICS** for `x < 0.0`, for `NaN`, and for
`+∞`. CPython's `time.sleep(-1)` raises `ValueError`.

1. **Guard `secs > 0.0 && secs.is_finite()` → a NO-OP** (CHOSEN). A
   non-positive, NaN, or infinite `secs` returns IMMEDIATELY without
   touching `from_secs_f64`, so the shim NEVER panics across the C-ABI. A
   non-positive sleep has no meaningful pause to perform, so returning at
   once is the correct, gentlest behavior. The `secs > 0.0` test is false
   for negatives, zero, AND every NaN (all NaN comparisons are false);
   `is_finite()` additionally filters `+∞`.
2. **Panic / abort on negative** (rejected). Mirrors CPython's `ValueError`
   in *spirit* but as a hard process abort across the C-ABI — far harsher
   than CPython's catchable exception, and a translated program that
   computed a slightly-negative sleep from a subtraction would crash. A
   no-op is the safe, §2.5-friendly choice (the LLM that writes
   `time.sleep(deadline - now())` with a passed deadline gets a no-op, not
   a crash).
3. **Clamp negative to 0 then sleep(0)** (rejected). Identical observable
   behavior to the no-op (a `sleep(0)` is itself a no-op pause), but spends
   a `from_secs_f64(0.0)` + `thread::sleep(ZERO)` syscall for nothing. The
   early-return no-op is the cleaner spelling.

### Q4 — what does `sleep` return?

CPython `time.sleep(secs)` returns `None`.

1. **`Ty::Int` i64 sentinel, discarded** (CHOSEN). The shim returns `0`;
   the `.cb` caller discards it (`let _ = time.sleep(d)`). This is the EXACT
   `random.seed` (ADR-0086) / dora `event.send_output` (ADR-0076) pattern.
2. **`Ty::None`** (rejected). `Ty::None` lowers to `i64` in codegen, so a
   Rust shim returning `()` (void) declared as an i64-returning extern is an
   ABI mismatch. The i64 sentinel is the proven scalar analogue.

## Decision

Add `import time` as a `cobrust-stdlib` module (`src/time.rs`) exposing:

- `__cobrust_time_time() -> f64` —
  `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64()`
  (the `unwrap_or_default` is a safe `0.0` fallback for a pre-1970 system
  clock — never an unwind across the C-ABI).
- `__cobrust_time_monotonic() -> f64` —
  `START.get_or_init(Instant::now).elapsed().as_secs_f64()` over a process-
  global `static START: OnceLock<Instant>`.
- `__cobrust_time_perf_counter() -> f64` — a thin alias to `monotonic`
  (reads the SAME `START`).
- `__cobrust_time_sleep(secs: f64) -> i64` —
  `if secs > 0.0 && secs.is_finite() { thread::sleep(Duration::from_secs_f64(secs)) }`
  then return the `0` sentinel.

The four manifest rows in `cobrust-types::ecosystem` drive the generic
ecosystem-call path (NO new MIR arm); codegen declares the four externs
(`() -> f64` ×3, `(f64) -> i64` ×1). `is_ecosystem_module("time")` returns
`true`. `check.rs` needs no edit — all four are CALLS routed through
`lookup_module_fn` (the `module == "math"` constant-attr branch is
parens-free `math.pi` only, never reached by a `time.*()` call).

### The clock contract (constitution §5.2) — ORDERING / RANGE, not exact values

A clock is **environment state**, NOT reproducible: `time()` advances every
call, `monotonic()`'s origin is process-start, `sleep` is best-effort (the
OS scheduler may oversleep). So a RAW clock read is NOT assertable for an
exact value. What IS assertable is the ORDERING / RANGE — the F36
non-determinism discipline:

- `time()` lands in a SANE post-2023 Unix-epoch window (`> 1.7e9` s, `<
  2e9` s for the lifetime of this code) — a broken clock returning `0`,
  milliseconds (`~1.7e12`), or nanoseconds (`~1.7e18`) falls outside.
- `monotonic()` called twice is non-decreasing (`b >= a`) — its only hard
  guarantee and the whole reason it exists.
- `perf_counter()` taken AFTER a `monotonic()` is `>= it` (one shared
  clock).
- `sleep(d)` then re-reading `monotonic` shows AT LEAST roughly `d` elapsed
  (a LOWER bound — the OS may oversleep, never under-sleep by much).

### `@py_compat` tier: Semantic — clocks are environment state

Cobrust does NOT, and does not try to, reproduce CPython's exact float
values: a different epoch float rounding and a different (process-relative)
monotonic origin mean the numbers differ. The contract is:

- **Clock semantics** — `time()` is a wall clock in Unix-epoch seconds,
  `monotonic()` / `perf_counter()` are a non-decreasing interval clock in
  process-relative seconds, `sleep(secs)` suspends for `secs` seconds.
- **Ordering / range** — the assertable properties above.

NOT — bit-identical agreement with CPython's exact clock readouts (which are
themselves machine-dependent). This is the SAME honest posture `random`
takes vs CPython's Mersenne Twister and `coil` takes vs numpy. Tier
`Semantic` records the environment-dependence explicitly rather than
implying a Strict parity that no clock could hold.

### M13 threading note

The `static START` is PROCESS-global (one `OnceLock`, shared across
threads). This is CORRECT for `monotonic`: `Instant` is monotonic per
machine, so a single process-wide origin gives every thread a consistent,
mutually-comparable timeline. `OnceLock` is `Sync` and `get_or_init` is
race-safe (exactly one initializer wins), so concurrent first-use from M13
`task::spawn`ed threads is sound with no lock on the hot read path. (This is
the deliberate OPPOSITE of `random`'s per-thread RNG: a clock origin SHOULD
be shared, an RNG stream should not.)

## Consequences

- **Positive**
  - 4th core stdlib module ships the universal timing surface; the
    translation pipeline gains `time` (timestamps + interval benchmarks +
    sleeps).
  - ZERO new dependency — `std::time` + `std::thread` + `std::sync::OnceLock`
    only. `Cargo.lock` is unchanged (no F64 lockfile-staging risk).
  - Reuses the proven scalar ecosystem-call path: 0-arg f64 returns mirror
    `random.random` (ADR-0086), the f64-arg shape mirrors `math.sqrt`
    (ADR-0083), the i64-sentinel discard mirrors `random.seed`. No new MIR
    arm, no new codegen fn-type mechanism, no new runtime-link recognizer
    (the stdlib staticlib is always linked).
  - The negative-sleep no-op guard is §2.5-friendly: a translated
    `sleep(deadline - now())` that goes slightly negative is a no-op, not a
    crash.

- **Negative**
  - Clock readouts are NOT CPython-bit-identical (different epoch rounding +
    monotonic origin) — a `time` translation cannot be differential-tested
    against CPython for exact output, only for clock semantics + ordering /
    range. Recorded as the `Semantic` tier.
  - `sleep` is best-effort: the OS scheduler may oversleep (the e2e asserts
    a LOWER bound, never an exact delay). This is inherent to any `sleep`.
  - The `*_ns` integer variants + the calendar / `struct_time` surface are
    deferred — sub-microsecond integer timing and date formatting are not
    yet covered.

- **Neutral / unknown**
  - CPython raises `ValueError` on a negative `sleep`; Cobrust makes it a
    no-op (the gentler safe path, Q3). A `.cb` author relying on the
    exception for control flow (rare, and §2.2 deprecates exceptions-as-
    control-flow anyway) would see different behavior. Documented, not a bug.
  - `time()`'s `< 2e9` sanity ceiling in the test is a 2033 horizon; the
    SHIM itself has no ceiling (it returns the true epoch). Only the test's
    upper bound would need bumping past 2033 — a harmless test edit.

## Evidence

- `crates/cobrust-stdlib/src/time.rs` — the `OnceLock<Instant>` origin + the
  four shims + 12 unit tests (sane epoch range, monotonic non-decreasing +
  process-relative-near-zero, perf_counter shares the clock, sleep delays at
  least its argument, the negative / zero / NaN no-op guards, the i64
  sentinel).
- `crates/cobrust-cli/tests/time_e2e.rs` — 6 compile→link→spawn e2es:
  `time()` in a sane epoch range, `monotonic()` non-decreasing,
  `perf_counter()` shares the clock, `sleep(0.05)` delays ≥ 0.03 s, the
  negative-sleep no-panic (clean exit 0), the zero-sleep no-op.
- `crates/cobrust-types/src/ecosystem.rs` — the four `("time", _)` manifest
  rows + `is_ecosystem_module("time")` + 5 manifest unit tests.
- `crates/cobrust-codegen/src/llvm_backend.rs` — the four extern
  declarations.
- Prior art: ADR-0083 (`math` scalar stdlib + the f64-arg shape), ADR-0084
  (`re` ecosystem-call wiring precedent), ADR-0086 (`random` — the 0-arg f64
  shim + the i64-sentinel-discard + the "raw read non-deterministic; only
  the contract is assertable" Semantic-tier posture, all reused verbatim
  here), ADR-0076 (dora `send_output` i64-sentinel-discard origin).
