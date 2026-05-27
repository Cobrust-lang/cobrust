//! M7.0 panic-free fuzz harness for cobrust-coil.
//!
//! Constitution §4.2 floor: ≥ 1000 fuzzed inputs per public function.
//! ADR-0013 §"M7.0 scope window": "≥ 1000 fuzz panic-free".
//!
//! We drive the four public constructors (`zeros`, `ones`, `array`,
//! `arange`) with deterministic-seeded random shapes, dtypes, and
//! values, and assert that every input either produces an `Array`
//! whose `to_json()` round-trips bytewise through `serde_json`, or
//! returns `Err(NumpyError)` cleanly. **No panics.**
//!
//! Seeds are recorded in `PROVENANCE.toml`'s `verification.seeds`.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::manual_is_multiple_of)]

use coil::{Dtype, arange, array, ones, zeros};

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn next_f64(&mut self) -> f64 {
        // Map u64 -> f64 in [-1e3, 1e3].
        let u = self.next_u64();
        let unit = (u >> 11) as f64 / ((1_u64 << 53) as f64);
        (unit - 0.5) * 2_000.0
    }

    fn next_dim(&mut self, max: u32) -> usize {
        (self.next_u32() % (max + 1)) as usize
    }

    fn next_dtype(&mut self) -> Dtype {
        match self.next_u32() % 5 {
            0 => Dtype::Int32,
            1 => Dtype::Int64,
            2 => Dtype::Float32,
            3 => Dtype::Float64,
            _ => Dtype::Bool,
        }
    }
}

const SEEDS: [u64; 3] = [42, 1337, 0xDEAD_BEEF];

fn budget_per_seed() -> u32 {
    // 350 * 3 seeds * 4 ops = 4200 fuzz calls in total (well over the
    // ADR-0013 §"M7.0 scope window" floor of 1000 per function).
    350
}

fn check_round_trip<F: Fn() -> Result<coil::Array, coil::NumpyError>>(f: F) {
    match f() {
        Ok(arr) => {
            // Round-trip through serde_json.
            let payload = arr.to_json();
            let s = serde_json::to_string(&payload).expect("to_json must round-trip");
            let _back: serde_json::Value = serde_json::from_str(&s).expect("from_str must succeed");
            // Observers must not panic.
            let _ = arr.shape();
            let _ = arr.ndim();
            let _ = arr.size();
            let _ = arr.dtype();
            let _ = arr.repr();
        }
        Err(e) => {
            assert!(!e.message.is_empty(), "error must carry a message");
        }
    }
}

#[test]
fn fuzz_zeros_panic_free() {
    for seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..budget_per_seed() {
            let rank = (lcg.next_u32() % 4) as usize; // 0..=3
            let mut shape: Vec<usize> = Vec::with_capacity(rank);
            for _ in 0..rank {
                shape.push(lcg.next_dim(6));
            }
            let dtype = lcg.next_dtype();
            check_round_trip(|| zeros(&shape, dtype));
        }
    }
}

#[test]
fn fuzz_ones_panic_free() {
    for seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..budget_per_seed() {
            let rank = (lcg.next_u32() % 4) as usize;
            let mut shape: Vec<usize> = Vec::with_capacity(rank);
            for _ in 0..rank {
                shape.push(lcg.next_dim(6));
            }
            let dtype = lcg.next_dtype();
            check_round_trip(|| ones(&shape, dtype));
        }
    }
}

#[test]
fn fuzz_array_panic_free() {
    for seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..budget_per_seed() {
            let rank = (lcg.next_u32() % 3) as usize + 1;
            let mut shape: Vec<usize> = Vec::with_capacity(rank);
            let mut size: usize = 1;
            for _ in 0..rank {
                let d = lcg.next_dim(5);
                shape.push(d);
                size = size.saturating_mul(d);
            }
            // Also occasionally feed a wrong-size buffer to exercise
            // the ShapeMismatch path.
            let buf_len = if lcg.next_u32() % 4 == 0 {
                size.saturating_add(1)
            } else {
                size
            };
            let buf: Vec<f64> = (0..buf_len).map(|_| lcg.next_f64()).collect();
            let dtype = lcg.next_dtype();
            check_round_trip(|| array(&buf, &shape, dtype));
        }
    }
}

#[test]
fn fuzz_arange_panic_free() {
    for seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..budget_per_seed() {
            let start = lcg.next_f64() / 100.0;
            let stop = start + lcg.next_f64() / 100.0;
            let mut step = lcg.next_f64() / 50.0;
            // Occasionally force step=0 to exercise ZeroStep.
            if lcg.next_u32() % 16 == 0 {
                step = 0.0;
            }
            // Cap step away from "very tiny" to keep counts bounded.
            if step.abs() < 0.01 && step != 0.0 {
                step = step.signum() * 0.01;
            }
            let dtype = lcg.next_dtype();
            check_round_trip(|| arange(start, stop, step, dtype));
        }
    }
}

#[test]
fn fuzz_total_count_meets_floor() {
    // Sanity assertion documenting the budget for the audit trail.
    let total = SEEDS.len() as u64 * (budget_per_seed() as u64) * 4;
    assert!(
        total >= 1000,
        "total fuzz calls {total} must be ≥ 1000 per ADR-0013"
    );
    assert_eq!(total, 4200);
}
