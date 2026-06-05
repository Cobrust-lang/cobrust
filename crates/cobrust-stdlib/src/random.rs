//! `std.random` — pseudo-random sampling (`import random`).
//!
//! ADR-0086 pins this surface. The four functions are the universal
//! scalar core of CPython's `random` module — the ones that take only
//! scalars (or nothing) and return a scalar, with NO list argument and
//! NO list mutation (`choice` / `shuffle` / `sample` are a documented
//! follow-up):
//!
//! - `random.random() -> f64` — a uniform float in `[0, 1)` (0-arg).
//! - `random.randint(a, b) -> int` — a uniform int in `[a, b]`,
//!   INCLUSIVE on BOTH ends (CPython `randint(1, 6)` can return 6 —
//!   the half-open `[a, b)` form is WRONG, see [`randint`]).
//! - `random.uniform(a, b) -> f64` — a uniform float in `[a, b]`.
//! - `random.seed(n)` — re-seed the global RNG; the SAME seed yields an
//!   IDENTICAL subsequent stream (reproducible — the whole point of
//!   `seed`). CPython returns `None`; Cobrust returns a discarded i64
//!   sentinel (mirrors dora `event.send_output`, ADR-0086 §"seed
//!   return").
//!
//! **The global RNG** (the design crux, ADR-0086 §"Global RNG"): a
//! process-`thread_local!` `RefCell<rand_pcg::Pcg64>`, OS-seeded on
//! FIRST use via `Pcg64::from_entropy()` (getrandom) and re-seeded by
//! `random.seed(n)`. This is MODULE-GLOBAL like Python's `random`
//! (whose module-level functions share one hidden `Random` instance) —
//! distinct from `coil.random`'s explicit `Generator` HANDLE. Each
//! extern-C shim does `RNG.with(|cell| cell.borrow_mut().<draw>())`:
//! single-threaded per call (one borrow, released before return), so
//! there is no double-borrow. `Pcg64` reuses `coil.random`'s backend
//! (`rand_pcg`, already a workspace dep).
//!
//! **Determinism** (constitution §5.2, ADR-0086 §"Determinism"): the
//! seed makes the stream reproducible — `seed(k); x; seed(k); y` gives
//! `x == y`, every time, on every host (`Pcg64`'s transition function
//! is algebraic with no host-endianness state, so the same seed yields
//! the same bytes cross-platform). WITHOUT a seed the RNG is OS-entropy
//! seeded (non-deterministic — the feature). A RAW draw is therefore
//! NOT assertable; the seed-reproducibility EQUALITY is (the load-
//! bearing `.cb` e2e).
//!
//! **`@py_compat` tier: Semantic** (ADR-0086 §"Divergence"). CPython's
//! `random` uses the Mersenne Twister (MT19937); Cobrust uses `Pcg64`.
//! The two produce DIFFERENT streams for the same seed — Cobrust does
//! NOT reproduce CPython's exact values. The CONTRACT is the
//! DISTRIBUTION (uniform on the stated interval) + Cobrust-internal
//! seed-reproducibility, NOT bit-identical agreement with CPython. This
//! mirrors `coil.random`'s honest "distribution-level, not bit-
//! identical vs numpy" posture (`coil/src/random.rs` §doc).
//!
//! **ABI** — the four `__cobrust_random_*` symbols are scalar-in /
//! scalar-out, the SIMPLEST C-ABI shape (no Str/list buffer marshalling,
//! unlike `re`): `random` is `() -> f64`, `randint` is `(i64, i64) ->
//! i64`, `uniform` is `(f64, f64) -> f64`, `seed` is `(i64) -> i64`
//! (the discarded sentinel). The generic ecosystem-call path drives the
//! args + return off the `EcoSig` rows in `cobrust-types`; codegen only
//! declares the externs (NO new MIR arm).

use std::cell::RefCell;

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;

thread_local! {
    /// The process-global PRNG behind the module-level `random.*`
    /// functions, mirroring CPython's hidden module-level `Random`
    /// instance. Lazily OS-seeded on first borrow (`Pcg64::from_entropy`,
    /// getrandom-backed) so an un-seeded program is non-deterministic;
    /// `random.seed(n)` replaces it in place for reproducibility.
    ///
    /// `thread_local!` (not a `static Mutex`) because every shim runs
    /// single-threaded per call — Cobrust's codegen-emitted `main` and
    /// its callbacks invoke these on the calling thread, one borrow at a
    /// time. This keeps the borrow contention-free and matches the §2.2
    /// no-GIL / no-global-lock posture (no cross-thread lock on a draw).
    static RNG: RefCell<Pcg64> = RefCell::new(Pcg64::from_entropy());
}

// =====================================================================
// Rust-side helpers (testable without the C-ABI). Each borrows the
// thread-local RNG exactly once and releases it before returning, so
// there is never a re-entrant double `borrow_mut`.
// =====================================================================

/// `random.random()` — a uniform float in `[0, 1)`. Half-open: `0.0` is
/// attainable, `1.0` is NOT (the standard unit-interval convention,
/// matching CPython `random.random` and `rand`'s `Rng::gen::<f64>()`).
fn random() -> f64 {
    // `r#gen` (raw identifier) — `gen` is a reserved keyword in edition
    // 2024; the `f64` `Standard` distribution is uniform `[0, 1)`. Matches
    // `coil.random`'s `self.rng.r#gen::<f64>()` (coil/src/random.rs:114).
    RNG.with(|cell| cell.borrow_mut().r#gen::<f64>())
}

/// `random.randint(a, b)` — a uniform int in `[a, b]`, INCLUSIVE on BOTH
/// ends. CPython `randint(1, 6)` can return 6; `randint(5, 5)` always
/// returns 5. The `..=` (inclusive range) is load-bearing: a half-open
/// `a..b` would NEVER yield `b`, the classic off-by-one that silently
/// biases a dice roll. `a > b` is a caller error CPython raises on; here
/// `gen_range` panics (a clean trap, never a silent wrong value).
fn randint(a: i64, b: i64) -> i64 {
    RNG.with(|cell| cell.borrow_mut().gen_range(a..=b))
}

/// `random.uniform(a, b)` — a uniform float in `[a, b]`. Unlike
/// [`random`]'s half-open unit interval, `uniform` is closed on both
/// ends (`rand`'s `gen_range(a..=b)` for floats). Matches CPython
/// `random.uniform` (which is `a + (b - a) * random()`; the endpoint
/// inclusion is a rounding nicety either way).
fn uniform(a: f64, b: f64) -> f64 {
    RNG.with(|cell| cell.borrow_mut().gen_range(a..=b))
}

/// `random.seed(n)` — replace the global RNG with `Pcg64::seed_from_u64(n)`.
/// The SAME `n` yields an IDENTICAL subsequent stream (reproducible);
/// this is the determinism contract (constitution §5.2). The `as u64`
/// reinterprets the i64 seed bit-for-bit (a negative seed is a distinct
/// valid stream, not an error). Returns nothing meaningful — the caller
/// discards the [`__cobrust_random_seed`] i64 sentinel.
fn seed(n: i64) {
    RNG.with(|cell| *cell.borrow_mut() = Pcg64::seed_from_u64(n as u64));
}

// =====================================================================
// C-ABI shims — the `__cobrust_random_*` symbols codegen declares +
// calls. All scalar-in / scalar-out (no buffer marshalling). Each
// delegates to the Rust helper above (one borrow per call). None of
// these unwinds across the C-ABI: `gen` / `gen_range` are total for a
// valid range, and a malformed `randint(a, b)` with `a > b` panics
// inside Rust (a clean abort, never UB across the boundary).
// =====================================================================

/// C-ABI shim for `random.random() -> f64` (0-arg). The `-> f64` lowers
/// to an LLVM `double` return, mirroring `math::__cobrust_math_sqrt`'s
/// scalar return (here with NO argument — the first 0-arg scalar stdlib
/// shim). The `_ecoret` Float local at the call site receives it.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_random_random() -> f64 {
    random()
}

/// C-ABI shim for `random.randint(a, b) -> i64` — INCLUSIVE `[a, b]`.
/// The `(i64, i64) -> i64` shape mirrors `math`'s int-returning shims;
/// the inclusivity lives in the Rust [`randint`] helper (`a..=b`).
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_random_randint(a: i64, b: i64) -> i64 {
    randint(a, b)
}

/// C-ABI shim for `random.uniform(a, b) -> f64` — uniform float in
/// `[a, b]`. The `(f64, f64) -> f64` shape mirrors `math::__cobrust_math_pow`.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_random_uniform(a: f64, b: f64) -> f64 {
    uniform(a, b)
}

/// C-ABI shim for `random.seed(n)`. CPython's `random.seed` returns
/// `None`; Cobrust types the call `Ty::Int` and this shim returns a `0`
/// SENTINEL the call site discards (`let _ = random.seed(n)` or a bare
/// expression statement) — the dora `event.send_output` discard pattern
/// (ADR-0086 §"seed return"). Avoids the `Ty::None -> void` C-ABI
/// mismatch (a `Ty::None` eco-return would lower its destination to i64
/// while the shim returns unit). The reseed side effect is the whole
/// payload; the returned value carries no information.
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_random_seed(n: i64) -> i64 {
    seed(n);
    0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- seed reproducibility: the LOAD-BEARING determinism contract ----
    // `seed(k); x; seed(k); y` => x == y, every time. This is the only
    // assertable property of an RNG (a raw draw is not), and it is what
    // `random.seed` exists for (constitution §5.2). Without this, `seed`
    // is a no-op and the whole module is untestable.

    #[test]
    fn seed_makes_random_reproducible() {
        seed(42);
        let a = random();
        seed(42);
        let b = random();
        assert_eq!(a, b, "same seed must reproduce the same random() draw");
    }

    #[test]
    fn seed_reproduces_a_whole_sequence() {
        // Not just the first draw — the entire stream is reproducible.
        seed(7);
        let first: Vec<f64> = (0..16).map(|_| random()).collect();
        seed(7);
        let second: Vec<f64> = (0..16).map(|_| random()).collect();
        assert_eq!(first, second, "same seed must reproduce the FULL stream");
    }

    #[test]
    fn different_seeds_give_different_streams() {
        // The contract's converse: distinct seeds almost surely diverge
        // (a Pcg64 collision on the first f64 across two seeds is ~2^-53).
        seed(1);
        let a = random();
        seed(2);
        let b = random();
        assert_ne!(a, b, "distinct seeds should produce distinct draws");
    }

    #[test]
    fn seed_reproduces_randint_and_uniform_too() {
        seed(99);
        let i1 = randint(0, 1_000_000);
        let u1 = uniform(-10.0, 10.0);
        seed(99);
        let i2 = randint(0, 1_000_000);
        let u2 = uniform(-10.0, 10.0);
        assert_eq!(i1, i2, "randint stream is reproducible under a fixed seed");
        assert_eq!(u1, u2, "uniform stream is reproducible under a fixed seed");
    }

    // -- random() in [0, 1): half-open over many draws ------------------

    #[test]
    fn random_is_in_unit_interval() {
        seed(123);
        for _ in 0..10_000 {
            let x = random();
            assert!(
                (0.0..1.0).contains(&x),
                "random() must be in [0, 1); got {x}",
            );
        }
    }

    // -- randint INCLUSIVE on BOTH ends — the off-by-one guard ----------
    // CPython randint(5, 5) == 5; randint(0, 1) yields BOTH 0 and 1 over
    // many draws. A half-open [a, b) would NEVER produce b — these tests
    // would FAIL on that bug.

    #[test]
    fn randint_single_point_is_that_point() {
        seed(0);
        for _ in 0..100 {
            assert_eq!(randint(5, 5), 5, "randint(5,5) must always be 5");
        }
    }

    #[test]
    fn randint_includes_both_endpoints() {
        // Over many draws of randint(0, 1), BOTH 0 and 1 must appear.
        // The upper endpoint 1 appearing is the inclusive-range proof: a
        // half-open `0..1` would only ever yield 0.
        seed(2024);
        let mut saw_low = false;
        let mut saw_high = false;
        for _ in 0..1000 {
            match randint(0, 1) {
                0 => saw_low = true,
                1 => saw_high = true,
                other => panic!("randint(0,1) out of range: {other}"),
            }
        }
        assert!(saw_low, "randint(0,1) must be able to yield 0");
        assert!(
            saw_high,
            "randint(0,1) must be able to yield 1 (the INCLUSIVE upper end; \
             a half-open [a,b) bug would never reach it)",
        );
    }

    #[test]
    fn randint_stays_within_inclusive_range() {
        seed(555);
        for _ in 0..10_000 {
            let x = randint(-3, 7);
            assert!(
                (-3..=7).contains(&x),
                "randint(-3,7) must be in [-3, 7] inclusive; got {x}",
            );
        }
    }

    // -- uniform in [a, b] ----------------------------------------------

    #[test]
    fn uniform_is_within_bounds() {
        seed(314);
        for _ in 0..10_000 {
            let x = uniform(2.5, 9.5);
            assert!(
                (2.5..=9.5).contains(&x),
                "uniform(2.5, 9.5) must be in [2.5, 9.5]; got {x}",
            );
        }
    }

    #[test]
    fn uniform_handles_negative_range() {
        seed(271);
        for _ in 0..10_000 {
            let x = uniform(-5.0, -1.0);
            assert!(
                (-5.0..=-1.0).contains(&x),
                "uniform(-5, -1) must be in [-5, -1]; got {x}",
            );
        }
    }

    // -- the C-ABI shims delegate to the helpers (same contract) --------

    #[test]
    fn seed_shim_returns_zero_sentinel() {
        // The discarded sentinel is always 0 (the value carries no info;
        // the reseed side effect is the payload).
        assert_eq!(__cobrust_random_seed(1), 0);
    }

    #[test]
    fn shims_are_seed_reproducible() {
        __cobrust_random_seed(2026);
        let r1 = __cobrust_random_random();
        let i1 = __cobrust_random_randint(1, 6);
        let u1 = __cobrust_random_uniform(0.0, 100.0);
        __cobrust_random_seed(2026);
        let r2 = __cobrust_random_random();
        let i2 = __cobrust_random_randint(1, 6);
        let u2 = __cobrust_random_uniform(0.0, 100.0);
        assert_eq!(r1, r2, "random shim reproducible under a fixed seed");
        assert_eq!(i1, i2, "randint shim reproducible under a fixed seed");
        assert_eq!(u1, u2, "uniform shim reproducible under a fixed seed");
        // randint(1, 6) is a die roll: always in [1, 6] inclusive.
        assert!(
            (1..=6).contains(&i1),
            "randint(1,6) shim in [1,6]; got {i1}"
        );
    }
}
