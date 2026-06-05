---
doc_kind: module
module_id: mod:time
crate: none
last_verified_commit: 7e924d5
dependencies: [mod:types, mod:codegen, mod:stdlib]
---

# Module: time (timing + timestamps stdlib surface)

## Purpose

`import time` — the timing + timestamps Python stdlib module wired into
Cobrust (per ADR-0087). Timestamps / interval benchmarks / sleeps:
`time.time()`, `time.monotonic()`, `time.perf_counter()`,
`time.sleep(secs)`.

NOT a crate. There is no `cobrust-time`; `time` is a compiler surface — a
manifest in `cobrust-types` + four `__cobrust_time_*` shims in
`cobrust-stdlib/src/time.rs` (backed by std `SystemTime` / `Instant` /
`thread::sleep`) + the extern decls in `cobrust-codegen`.

The ONE novelty vs `math`/`re`/`random`: a **lazy-static monotonic
origin** (`static START: OnceLock<Instant>`), captured once per process on
first use, so `monotonic` / `perf_counter` measure intervals from a fixed
reference. The std-only analogue of `random`'s module-global RNG — but
shared process-wide (a clock origin SHOULD be common across threads,
unlike an RNG stream). Everything else is scalar-in / scalar-out, the
SIMPLEST ecosystem-call shape — no new MIR arm, no new codegen fn-type.

## Status

- **ADR-0087 — delivered.** 4 scalar functions (time / monotonic /
  perf_counter / sleep). 12 `cobrust-stdlib` shim unit tests + 5
  `cobrust-types` manifest tests + 6 `.cb` e2e tests green.
- ZERO new dependency — `std::time` + `std::thread` + `std::sync::OnceLock`
  only. `Cargo.lock` unchanged.
- The `*_ns` integer-nanosecond variants (`time_ns` / `monotonic_ns` /
  `perf_counter_ns`), the CPU clocks (`process_time` / `thread_time`), and
  the entire calendar / `struct_time` surface (`gmtime` / `localtime` /
  `strftime` / `strptime` / …) are documented follow-ups.

## Public surface — `lookup_module_fn("time", _)`

| `.cb` form | signature | runtime symbol | tier |
|---|---|---|---|
| `time.time()` | `[] -> Float` | `__cobrust_time_time` | Semantic |
| `time.monotonic()` | `[] -> Float` | `__cobrust_time_monotonic` | Semantic |
| `time.perf_counter()` | `[] -> Float` | `__cobrust_time_perf_counter` | Semantic |
| `time.sleep(secs)` | `[Float] -> Int` (sentinel) | `__cobrust_time_sleep` | Semantic |

`is_ecosystem_module("time") == true`. `check.rs` needs no edit (all four
are CALLS routed through `lookup_module_fn`; the `module == "math"`
constant-attr branch is parens-free `math.pi` only, never reached).

## Semantics

- `time.time()` — current Unix-epoch time in **SECONDS as a float** (a
  WALL clock). `SystemTime::now().duration_since(UNIX_EPOCH)
  .unwrap_or_default().as_secs_f64()`. The `unwrap_or_default` is a safe
  `0.0` fallback for a pre-1970 system clock (never an unwind across the
  C-ABI). 0-arg. For "what time is it" / a timestamp.
- `time.monotonic()` — seconds from a **PROCESS-relative origin**,
  monotonically **non-decreasing** (immune to wall-clock adjustments / NTP
  steps / DST). 0-arg. For MEASURING an interval. The first call returns
  ≈ 0.0 (it captures the origin); later calls return larger values.
- `time.perf_counter()` — **≡ `monotonic`** — the SAME high-res `Instant`
  clock (reads the shared `START`). CPython names them distinctly, but
  `Instant` already IS the platform's best monotonic source, so the two
  are deliberately equal. 0-arg.
- `time.sleep(secs)` — suspend the current thread for `secs` seconds
  (`thread::sleep(Duration::from_secs_f64(secs))`). `secs <= 0.0` / NaN /
  `+∞` is a **NO-OP** (the guard — see below). CPython returns `None`;
  Cobrust returns a discarded **i64 sentinel** (`0`). The `.cb` form is
  `let _ = time.sleep(d)`.

### The monotonic origin (the design crux)

A process-global `static START: OnceLock<Instant>` in
`cobrust-stdlib/src/time.rs`:

- **Lazily captured** on the FIRST `monotonic` / `perf_counter` call via
  `START.get_or_init(Instant::now)` — no startup cost for a program that
  never times.
- `monotonic()` = `START.get_or_init(Instant::now).elapsed().as_secs_f64()`
  — seconds since that capture, never smaller than a previous call
  (`Instant` is monotonic by construction).
- `OnceLock` is the **std lazy-static** (no `once_cell` / `lazy_static`
  dep). Its `get_or_init` is **race-safe** (exactly one initializer wins),
  so concurrent first-use from M13 `task::spawn`ed threads is sound with NO
  lock on the hot read path.

### `perf_counter` ≡ `monotonic`

Both shims read the SAME `START` `Instant`. `perf_counter()` is a thin
alias to `monotonic()`. `Instant` is the highest-resolution monotonic
clock the platform offers (the very thing `perf_counter` is specified to
be), so a second independent clock would be pure ceremony returning the
same numbers.

### The negative-`sleep` guard (the safety crux)

`Duration::from_secs_f64(x)` **PANICS** for `x < 0.0`, for `NaN`, and for
`+∞`. CPython's `time.sleep(-1)` raises `ValueError`. Cobrust takes the
GENTLER, SAFE path: the shim guards `if secs > 0.0 && secs.is_finite()`,
so a non-positive / NaN / infinite `secs` is a **NO-OP** that returns
immediately — NEVER a panic across the C-ABI. (`secs > 0.0` is false for
negatives, zero, AND every NaN; `is_finite()` filters `+∞`.) A non-positive
sleep has no meaningful pause to perform, so returning at once is correct.

### Determinism contract (§5.2) — ORDERING / RANGE, not exact values

A clock is **environment state**, NOT reproducible: `time()` advances every
call, `monotonic()`'s origin is process-start, `sleep` is best-effort (the
OS may oversleep). So a RAW read is NOT assertable for an exact value
(F36); what IS assertable:

- `time()` in a SANE post-2023 Unix-epoch window (`> 1.7e9` s, `< 2e9` s) —
  a broken clock (`0`, millis `~1.7e12`, nanos `~1.7e18`) falls outside.
- `monotonic()` called twice is non-decreasing (`b >= a`).
- `perf_counter()` after a `monotonic()` is `>= it` (one shared clock).
- `sleep(d)` then re-reading `monotonic` shows AT LEAST roughly `d` elapsed
  (a LOWER bound — the OS may oversleep).

### `@py_compat` tier: Semantic (clocks are environment state)

Cobrust does NOT reproduce CPython's exact float values (different epoch
rounding + a different process-relative monotonic origin). The CONTRACT is
the **clock semantics** (wall vs monotonic, seconds-as-float) + **ordering
/ range**, NOT bit-identity. Same honest posture `random` takes vs CPython's
Mersenne Twister and `coil` takes vs numpy. Tier `Semantic`.

## ABI

The four `__cobrust_time_*` symbols are scalar-in / scalar-out (no Str/list
buffer marshalling, like `random`):

| symbol | C-ABI type |
|---|---|
| `__cobrust_time_time` | `() -> f64` (LLVM `double`) |
| `__cobrust_time_monotonic` | `() -> f64` |
| `__cobrust_time_perf_counter` | `() -> f64` |
| `__cobrust_time_sleep` | `(f64) -> i64` (discarded sentinel) |

The generic ecosystem-call path (`cobrust-mir` ecosystem-call lowering →
`emit_ecosystem_call`) drives args + return off the `EcoSig`; the
destination `_ecoret` local carries the manifest return type. Codegen only
declares the externs (the stdlib staticlib is always linked, so no
runtime-link recognizer is needed). The 0-arg f64 returns reuse the
`random.random` precedent; the f64-arg shape reuses `math.sqrt`; the
i64-sentinel side effect reuses `random.seed` — all lower through the
EXISTING path → **`'none'`** new MIR code.

## `sleep` return-type rationale

CPython `time.sleep` returns `None`. Cobrust types it `Ty::Int` (an i64
sentinel `0`), NOT `Ty::None`, because `Ty::None` lowers to `i64` in
codegen (`lower_ty`) while a Rust shim returning `()` would be an ABI
mismatch. The i64 sentinel is exactly the `random.seed` / dora
`event.send_output` discard pattern. The caller discards it:
`let _ = time.sleep(d)`.

## M13 threading note

The `static START` is PROCESS-global (one `OnceLock`, shared across
threads) — CORRECT for `monotonic`: `Instant` is monotonic per machine, so
a single process-wide origin gives every thread a mutually-comparable
timeline. This is the deliberate OPPOSITE of `random`'s per-thread RNG: a
clock origin SHOULD be shared, an RNG stream should not.

## Deferred

- `time_ns` / `monotonic_ns` / `perf_counter_ns` — integer-nanosecond
  variants (a thin follow-up: same shims, `as_nanos() as i64`).
- `process_time` / `thread_time` — CPU clocks.
- `gmtime` / `localtime` / `mktime` / `strftime` / `strptime` / `asctime` /
  `ctime` — the calendar / `struct_time` surface (needs a `struct_time`
  aggregate + timezone handling). The manifest returns `None`, so a `.cb`
  `time.strftime(...)` is a compile-time `UnknownName` (§2.5), not a
  false-green binding.

## Tests

- `crates/cobrust-stdlib/src/time.rs` `#[cfg(test)]` — 12 tests: `time()`
  in a sane epoch range, `monotonic()` non-decreasing + process-relative-
  near-zero, `perf_counter` shares the clock, `sleep(0.05)` delays ≥ 0.03 s,
  the negative / zero / NaN no-op guards (no panic), the shim delegation +
  i64 sentinel.
- `crates/cobrust-cli/tests/time_e2e.rs` — 6 compile→link→spawn e2es:
  `time()` sane-epoch, `monotonic()` non-decreasing, `perf_counter` shares
  the clock, `sleep` delays, negative-sleep no-panic (exit 0), zero-sleep
  no-op.
- `crates/cobrust-types/src/ecosystem.rs` `#[cfg(test)]` — 5 manifest tests
  (each row's symbol + types + Semantic tier; `time` is a known module; the
  deferred fns resolve to `None`).

## See also

- `docs/agent/adr/0087-time-timing-stdlib-module.md` — the design ADR.
- `docs/human/en/import-time.md` / `docs/human/zh/import-time.md` —
  human-facing usage.
- `docs/agent/modules/stdlib.md` — the host crate (`cobrust-stdlib`).
- ADR-0083 (`math`), ADR-0084 (`re`), ADR-0086 (`random` — the 0-arg f64
  shim + i64-sentinel-discard + Semantic-tier non-determinism posture
  reused here).
