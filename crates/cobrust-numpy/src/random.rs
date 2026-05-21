// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy.random)
// scope: M7.5 random per ADR-0018.
// see PROVENANCE.toml for the full manifest.

//! Random surface — `Generator` + `default_rng` + `seed` + `integers` +
//! `random` + `normal` + `uniform` + `choice` per ADR-0018.
//!
//! Per ADR-0018 §1 the Generator is a closed newtype struct around
//! `rand_pcg::Pcg64` (constitution §2.2: no `dyn`). Per ADR-0018 §2 the
//! PRNG backend is PCG64 — matches numpy's `default_rng()` algorithm
//! family and is deterministic across host architectures (algebraic
//! transition function, no host-endianness state). Per ADR-0018 §3 the
//! seed parameter is `Option<u64>` — `None` OS-seeds; `Some(s)` is
//! reproducible across runs of the same binary. Per ADR-0018 §4 the
//! distribution surface is closed at seven methods. Per ADR-0018 §5
//! the acceptance gate is KS-test agreement at p > 0.01 vs numpy 2.0.2
//! for continuous distributions; chi-square / mean-bin agreement for
//! discrete.
//!
//! **Bit-identical reproducibility against numpy is NOT a hard
//! requirement** per ADR-0018 §2. numpy uses a specific SeedSequence
//! layout that we don't replicate. What we promise:
//! - Within Cobrust: same seed → identical stream, every time, on
//!   every host.
//! - Vs numpy: distribution-level agreement (KS-test, χ², mean-bin).

// CQ P1-4: consolidated from 18 separate inner attrs; translator-template fix deferred per F37.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::imprecise_flops,
    clippy::suboptimal_flops,
    clippy::module_name_repetitions,
    clippy::similar_names,
    clippy::uninlined_format_args,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::explicit_iter_loop
)]

use ndarray::{ArrayD, IxDyn};
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Normal, Uniform};
use rand_pcg::Pcg64;

use crate::array::Array;
use crate::error::{NumpyError, NumpyErrorKind};

/// Random number generator. Wraps `rand_pcg::Pcg64` (matches numpy's
/// `default_rng()` algorithm family). Per ADR-0018 §1 this is a closed
/// newtype struct — no `dyn` (constitution §2.2). Same seed → identical
/// stream on any host architecture.
///
/// The `seed_value` field is preserved for diagnostic / repro purposes
/// (numpy exposes `Generator.bit_generator.state`; we expose
/// `seed_value()`).
#[derive(Clone, Debug)]
pub struct Generator {
    rng: Pcg64,
    seed_value: Option<u64>,
}

impl Generator {
    /// Re-seed in place. Subsequent samples follow the new stream.
    pub fn seed(&mut self, seed: u64) {
        self.rng = Pcg64::seed_from_u64(seed);
        self.seed_value = Some(seed);
    }

    /// Last seed used (or `None` if OS-seeded).
    #[must_use]
    pub fn seed_value(&self) -> Option<u64> {
        self.seed_value
    }

    /// Uniform integers in `[low, high)`. Returns an `Int64` Array of
    /// shape `size`.
    ///
    /// # Errors
    /// `NumpyError::InvalidIntegerRange` if `low >= high`.
    pub fn integers(&mut self, low: i64, high: i64, size: &[usize]) -> Result<Array, NumpyError> {
        validate_int_range(low, high)?;
        let n = shape_size(size);
        let mut data: Vec<i64> = Vec::with_capacity(n);
        // `gen_range` is half-open [low, high) — matches numpy.
        for _ in 0..n {
            data.push(self.rng.gen_range(low..high));
        }
        Ok(Array::Int64(
            ArrayD::from_shape_vec(IxDyn(size), data).expect("size product matches data len"),
        ))
    }

    /// Uniform floats in `[0, 1)`. Returns a `Float64` Array.
    ///
    /// # Errors
    /// Currently total — never errors at the public-API boundary.
    pub fn random(&mut self, size: &[usize]) -> Result<Array, NumpyError> {
        let n = shape_size(size);
        let mut data: Vec<f64> = Vec::with_capacity(n);
        for _ in 0..n {
            // `f64` `Standard` distribution = uniform [0, 1).
            data.push(self.rng.r#gen::<f64>());
        }
        Ok(Array::Float64(
            ArrayD::from_shape_vec(IxDyn(size), data).expect("size product matches data len"),
        ))
    }

    /// Gaussian samples `N(loc, scale²)`. Returns a `Float64` Array.
    /// Uses `rand_distr::Normal` (Box-Muller / Ziggurat under the hood).
    ///
    /// # Errors
    /// `NumpyError::InvalidDistributionParams` if `scale <= 0` or
    /// either parameter is non-finite.
    pub fn normal(&mut self, loc: f64, scale: f64, size: &[usize]) -> Result<Array, NumpyError> {
        validate_distribution_params(Some(scale), None, None)?;
        if !loc.is_finite() {
            return Err(NumpyError {
                kind: NumpyErrorKind::InvalidDistributionParams,
                message: format!("loc must be finite, got {loc}"),
            });
        }
        let dist = Normal::new(loc, scale).map_err(|e| NumpyError {
            kind: NumpyErrorKind::InvalidDistributionParams,
            message: format!("Normal::new failed: {e}"),
        })?;
        let n = shape_size(size);
        let mut data: Vec<f64> = Vec::with_capacity(n);
        for _ in 0..n {
            data.push(dist.sample(&mut self.rng));
        }
        Ok(Array::Float64(
            ArrayD::from_shape_vec(IxDyn(size), data).expect("size product matches data len"),
        ))
    }

    /// Uniform floats in `[low, high)`. Returns a `Float64` Array.
    ///
    /// # Errors
    /// `NumpyError::InvalidDistributionParams` if `low >= high` or any
    /// non-finite bound.
    pub fn uniform(&mut self, low: f64, high: f64, size: &[usize]) -> Result<Array, NumpyError> {
        validate_distribution_params(None, Some(low), Some(high))?;
        let dist = Uniform::new(low, high);
        let n = shape_size(size);
        let mut data: Vec<f64> = Vec::with_capacity(n);
        for _ in 0..n {
            data.push(dist.sample(&mut self.rng));
        }
        Ok(Array::Float64(
            ArrayD::from_shape_vec(IxDyn(size), data).expect("size product matches data len"),
        ))
    }

    /// Sample from `values` with optional probability vector `p`. Per
    /// ADR-0018 §4 'choice constraints':
    /// - `replace=true`: sampling-with-replacement (uniform if `p` is
    ///   None, weighted if `p` provided).
    /// - `replace=false`: requires `size.product() <= values.size()`;
    ///   uses Fisher-Yates partial shuffle.
    /// - `p`: must sum to 1 within `1e-8`, no negative entries,
    ///   length == `values.size()`.
    ///
    /// Returns an Array with the same dtype as `values`.
    ///
    /// # Errors
    /// - `NumpyError::EmptyChoicePopulation` if `values.size() == 0`.
    /// - `NumpyError::InvalidProbabilities` for malformed `p`.
    /// - `NumpyError::InvalidDistributionParams` if `replace=false`
    ///   and `size.product() > values.size()`.
    pub fn choice(
        &mut self,
        values: &Array,
        size: &[usize],
        replace: bool,
        p: Option<&[f64]>,
    ) -> Result<Array, NumpyError> {
        let n_values = values.size();
        if n_values == 0 {
            return Err(NumpyError {
                kind: NumpyErrorKind::EmptyChoicePopulation,
                message: "a must be non-empty".into(),
            });
        }
        if let Some(probs) = p {
            validate_probabilities(probs, n_values)?;
        }
        let n_out = shape_size(size);
        if !replace && n_out > n_values {
            return Err(NumpyError {
                kind: NumpyErrorKind::InvalidDistributionParams,
                message: format!(
                    "cannot take more samples ({n_out}) than values has ({n_values}) when replace=false"
                ),
            });
        }

        // Compute per-element index choices, then materialise per the
        // input dtype.
        let indices = if replace {
            if let Some(probs) = p {
                self.weighted_indices_with_replacement(probs, n_out)
            } else {
                self.uniform_indices_with_replacement(n_values, n_out)
            }
        } else {
            self.indices_without_replacement(n_values, n_out)
        };

        Ok(materialise_choice(values, &indices, size))
    }

    fn uniform_indices_with_replacement(&mut self, n_values: usize, n_out: usize) -> Vec<usize> {
        let mut out = Vec::with_capacity(n_out);
        for _ in 0..n_out {
            out.push(self.rng.gen_range(0..n_values));
        }
        out
    }

    fn weighted_indices_with_replacement(&mut self, p: &[f64], n_out: usize) -> Vec<usize> {
        let mut cdf: Vec<f64> = Vec::with_capacity(p.len());
        let mut running = 0.0_f64;
        for v in p {
            running += v;
            cdf.push(running);
        }
        let mut out = Vec::with_capacity(n_out);
        for _ in 0..n_out {
            let u: f64 = self.rng.r#gen::<f64>();
            let mut idx = cdf.len() - 1;
            for (i, c) in cdf.iter().enumerate() {
                if u <= *c {
                    idx = i;
                    break;
                }
            }
            out.push(idx);
        }
        out
    }

    fn indices_without_replacement(&mut self, n_values: usize, n_out: usize) -> Vec<usize> {
        // Fisher-Yates partial shuffle.
        let mut pool: Vec<usize> = (0..n_values).collect();
        let mut out = Vec::with_capacity(n_out);
        for i in 0..n_out {
            let j = self.rng.gen_range(i..n_values);
            pool.swap(i, j);
            out.push(pool[i]);
        }
        out
    }
}

/// Construct a `Generator` from an optional seed. `None` seeds from
/// the OS; `Some(s)` produces a deterministic stream that is
/// reproducible across runs of the same binary on any host
/// architecture (PCG64 algebraic transition).
#[must_use]
pub fn default_rng(seed: Option<u64>) -> Generator {
    let rng = match seed {
        Some(s) => Pcg64::seed_from_u64(s),
        None => Pcg64::from_entropy(),
    };
    Generator {
        rng,
        seed_value: seed,
    }
}

// ---- Validators ---------------------------------------------------------

fn validate_int_range(low: i64, high: i64) -> Result<(), NumpyError> {
    if low >= high {
        return Err(NumpyError {
            kind: NumpyErrorKind::InvalidIntegerRange,
            message: format!("low >= high (low={low}, high={high})"),
        });
    }
    Ok(())
}

fn validate_distribution_params(
    scale: Option<f64>,
    low: Option<f64>,
    high: Option<f64>,
) -> Result<(), NumpyError> {
    if let Some(s) = scale {
        if !s.is_finite() || s <= 0.0 {
            return Err(NumpyError {
                kind: NumpyErrorKind::InvalidDistributionParams,
                message: format!("scale must be > 0 and finite, got {s}"),
            });
        }
    }
    if let (Some(lo), Some(hi)) = (low, high) {
        if !lo.is_finite() || !hi.is_finite() {
            return Err(NumpyError {
                kind: NumpyErrorKind::InvalidDistributionParams,
                message: format!("low/high must be finite, got low={lo}, high={hi}"),
            });
        }
        if lo >= hi {
            return Err(NumpyError {
                kind: NumpyErrorKind::InvalidDistributionParams,
                message: format!("low >= high (low={lo}, high={hi})"),
            });
        }
    }
    Ok(())
}

fn validate_probabilities(p: &[f64], n: usize) -> Result<(), NumpyError> {
    if p.len() != n {
        return Err(NumpyError {
            kind: NumpyErrorKind::InvalidProbabilities,
            message: format!("p length {} != values length {}", p.len(), n),
        });
    }
    let mut s = 0.0_f64;
    for v in p {
        if !v.is_finite() || *v < 0.0 {
            return Err(NumpyError {
                kind: NumpyErrorKind::InvalidProbabilities,
                message: format!("p contains invalid value {v}"),
            });
        }
        s += v;
    }
    if (s - 1.0).abs() > 1e-8 {
        return Err(NumpyError {
            kind: NumpyErrorKind::InvalidProbabilities,
            message: format!("probabilities do not sum to 1 (sum={s})"),
        });
    }
    Ok(())
}

// ---- Helpers ------------------------------------------------------------

fn shape_size(shape: &[usize]) -> usize {
    let mut n: usize = 1;
    for &d in shape {
        n = n.saturating_mul(d);
    }
    n
}

/// Materialise a `choice` result: gather elements of `values` at
/// per-output index, preserving the input dtype.
fn materialise_choice(values: &Array, indices: &[usize], size: &[usize]) -> Array {
    match values {
        Array::Int32(a) => {
            let flat: Vec<i32> = a.iter().copied().collect();
            let data: Vec<i32> = indices.iter().map(|&i| flat[i]).collect();
            Array::Int32(
                ArrayD::from_shape_vec(IxDyn(size), data).expect("size matches indices.len"),
            )
        }
        Array::Int64(a) => {
            let flat: Vec<i64> = a.iter().copied().collect();
            let data: Vec<i64> = indices.iter().map(|&i| flat[i]).collect();
            Array::Int64(
                ArrayD::from_shape_vec(IxDyn(size), data).expect("size matches indices.len"),
            )
        }
        Array::Float32(a) => {
            let flat: Vec<f32> = a.iter().copied().collect();
            let data: Vec<f32> = indices.iter().map(|&i| flat[i]).collect();
            Array::Float32(
                ArrayD::from_shape_vec(IxDyn(size), data).expect("size matches indices.len"),
            )
        }
        Array::Float64(a) => {
            let flat: Vec<f64> = a.iter().copied().collect();
            let data: Vec<f64> = indices.iter().map(|&i| flat[i]).collect();
            Array::Float64(
                ArrayD::from_shape_vec(IxDyn(size), data).expect("size matches indices.len"),
            )
        }
        Array::Bool(a) => {
            let flat: Vec<bool> = a.iter().copied().collect();
            let data: Vec<bool> = indices.iter().map(|&i| flat[i]).collect();
            Array::Bool(
                ArrayD::from_shape_vec(IxDyn(size), data).expect("size matches indices.len"),
            )
        }
    }
}

// ---- Tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::format_push_string)]
    #![allow(clippy::let_unit_value)]
    #![allow(clippy::ignored_unit_patterns)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::float_cmp)]
    #![allow(clippy::similar_names)]
    #![allow(clippy::imprecise_flops)]
    #![allow(clippy::suboptimal_flops)]
    #![allow(clippy::explicit_iter_loop)]
    use super::*;
    use crate::array_i64;

    #[test]
    fn default_rng_reproducibility() {
        // Same seed → same first integer.
        let mut g1 = default_rng(Some(42));
        let mut g2 = default_rng(Some(42));
        let r1 = g1.integers(0, 1_000_000, &[10]).unwrap();
        let r2 = g2.integers(0, 1_000_000, &[10]).unwrap();
        assert_eq!(r1.to_json(), r2.to_json());
    }

    #[test]
    fn different_seeds_produce_different_streams() {
        let mut g1 = default_rng(Some(42));
        let mut g2 = default_rng(Some(43));
        let r1 = g1.integers(0, 1_000_000, &[10]).unwrap();
        let r2 = g2.integers(0, 1_000_000, &[10]).unwrap();
        assert_ne!(r1.to_json(), r2.to_json());
    }

    #[test]
    fn integers_in_range() {
        let mut g = default_rng(Some(42));
        let r = g.integers(10, 20, &[100]).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64");
        };
        for v in arr.iter() {
            assert!(*v >= 10 && *v < 20);
        }
    }

    #[test]
    fn integers_low_eq_high_errs() {
        let mut g = default_rng(Some(42));
        let err = g.integers(5, 5, &[3]).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidIntegerRange);
    }

    #[test]
    fn random_in_unit_interval() {
        let mut g = default_rng(Some(42));
        let r = g.random(&[1000]).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        for v in arr.iter() {
            assert!(*v >= 0.0 && *v < 1.0);
        }
    }

    #[test]
    fn normal_finite() {
        let mut g = default_rng(Some(42));
        let r = g.normal(0.0, 1.0, &[1000]).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        for v in arr.iter() {
            assert!(v.is_finite());
        }
    }

    #[test]
    fn normal_negative_scale_errs() {
        let mut g = default_rng(Some(42));
        let err = g.normal(0.0, -1.0, &[3]).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
    }

    #[test]
    fn uniform_in_range() {
        let mut g = default_rng(Some(42));
        let r = g.uniform(-5.0, 5.0, &[1000]).unwrap();
        let Array::Float64(arr) = r else {
            panic!("expected Float64");
        };
        for v in arr.iter() {
            assert!(*v >= -5.0 && *v < 5.0);
        }
    }

    #[test]
    fn uniform_low_eq_high_errs() {
        let mut g = default_rng(Some(42));
        let err = g.uniform(2.0, 2.0, &[3]).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
    }

    #[test]
    fn choice_with_replacement() {
        let values = array_i64(&[10, 20, 30, 40], &[4]).unwrap();
        let mut g = default_rng(Some(42));
        let r = g.choice(&values, &[100], true, None).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64 (matches input dtype)");
        };
        for v in arr.iter() {
            assert!([10, 20, 30, 40].contains(v));
        }
    }

    #[test]
    fn choice_without_replacement_unique() {
        let values = array_i64(&[10, 20, 30, 40, 50], &[5]).unwrap();
        let mut g = default_rng(Some(42));
        let r = g.choice(&values, &[3], false, None).unwrap();
        let Array::Int64(arr) = r else {
            panic!("expected Int64");
        };
        let mut sorted: Vec<i64> = arr.iter().copied().collect();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            3,
            "all draws must be unique without replacement"
        );
    }

    #[test]
    fn choice_without_replacement_too_many_errs() {
        let values = array_i64(&[10, 20, 30], &[3]).unwrap();
        let mut g = default_rng(Some(42));
        let err = g.choice(&values, &[5], false, None).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidDistributionParams);
    }

    #[test]
    fn choice_empty_values_errs() {
        let values = array_i64(&[], &[0]).unwrap();
        let mut g = default_rng(Some(42));
        let err = g.choice(&values, &[3], true, None).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::EmptyChoicePopulation);
    }

    #[test]
    fn choice_p_must_sum_to_one() {
        let values = array_i64(&[10, 20, 30], &[3]).unwrap();
        let mut g = default_rng(Some(42));
        let bad_p = vec![0.1, 0.2, 0.3]; // sums to 0.6
        let err = g.choice(&values, &[3], true, Some(&bad_p)).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
    }

    #[test]
    fn choice_p_length_mismatch_errs() {
        let values = array_i64(&[10, 20, 30], &[3]).unwrap();
        let mut g = default_rng(Some(42));
        let bad_p = vec![0.5, 0.5]; // length 2, but values len 3
        let err = g.choice(&values, &[3], true, Some(&bad_p)).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
    }

    #[test]
    fn choice_p_negative_errs() {
        let values = array_i64(&[10, 20, 30], &[3]).unwrap();
        let mut g = default_rng(Some(42));
        let bad_p = vec![-0.1, 0.6, 0.5];
        let err = g.choice(&values, &[3], true, Some(&bad_p)).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::InvalidProbabilities);
    }

    #[test]
    fn seed_re_seeds() {
        let mut g = default_rng(Some(42));
        let _ = g.integers(0, 100, &[5]).unwrap();
        g.seed(42);
        let r2 = g.integers(0, 100, &[5]).unwrap();
        // After re-seed to 42, the next 5 ints match a fresh g(42)'s first 5.
        let mut g_fresh = default_rng(Some(42));
        let r_fresh = g_fresh.integers(0, 100, &[5]).unwrap();
        assert_eq!(r2.to_json(), r_fresh.to_json());
    }

    #[test]
    fn seed_value_round_trip() {
        let g = default_rng(Some(1234));
        assert_eq!(g.seed_value(), Some(1234));
        let g_none = default_rng(None);
        assert_eq!(g_none.seed_value(), None);
    }

    #[test]
    fn shape_2d_ok() {
        let mut g = default_rng(Some(42));
        let r = g.random(&[3, 4]).unwrap();
        assert_eq!(r.shape(), vec![3, 4]);
        assert_eq!(r.size(), 12);
    }

    #[test]
    fn shape_3d_ok() {
        let mut g = default_rng(Some(42));
        let r = g.normal(0.0, 1.0, &[2, 3, 4]).unwrap();
        assert_eq!(r.shape(), vec![2, 3, 4]);
        assert_eq!(r.size(), 24);
    }

    // Distribution sanity: sample mean should be close to `loc` for
    // large N. Not a KS-test (that's tests/random_differential.rs); a
    // unit-level sanity check.
    #[test]
    fn normal_mean_within_three_sigma() {
        let mut g = default_rng(Some(42));
        let r = g.normal(5.0, 2.0, &[10000]).unwrap();
        let Array::Float64(arr) = r else { panic!() };
        let mean: f64 = arr.iter().sum::<f64>() / 10000.0;
        // For N=10k with σ=2, sample-mean SE = 2/sqrt(10000) = 0.02.
        // 3σ = 0.06. Should be well within.
        assert!(
            (mean - 5.0).abs() < 0.1,
            "sample mean {mean} too far from 5.0"
        );
    }

    #[test]
    fn uniform_mean_within_bound() {
        let mut g = default_rng(Some(42));
        let r = g.uniform(0.0, 10.0, &[10000]).unwrap();
        let Array::Float64(arr) = r else { panic!() };
        let mean: f64 = arr.iter().sum::<f64>() / 10000.0;
        // Expected mean = 5.0 for U[0, 10).
        assert!((mean - 5.0).abs() < 0.2);
    }

    #[test]
    fn integers_distribution_sanity() {
        let mut g = default_rng(Some(42));
        let r = g.integers(0, 10, &[10000]).unwrap();
        let Array::Int64(arr) = r else { panic!() };
        let mean: f64 = arr.iter().map(|&v| v as f64).sum::<f64>() / 10000.0;
        // Expected mean = 4.5 for U[0, 10).
        assert!((mean - 4.5).abs() < 0.2);
    }

    // The PCG64 backend yields sequential u64s with weak short-window
    // correlation; the integers() loop's gen_range is supposed to
    // remap. Trip-test: 1024 samples should hit > 80% of bins on
    // U[0, 32) — chi-square / coverage sanity.
    #[test]
    fn integers_coverage_sanity() {
        let mut g = default_rng(Some(42));
        let r = g.integers(0, 32, &[1024]).unwrap();
        let Array::Int64(arr) = r else { panic!() };
        let mut bins = vec![0_u64; 32];
        for &v in arr.iter() {
            bins[v as usize] += 1;
        }
        let coverage = bins.iter().filter(|&&c| c > 0).count();
        assert!(coverage >= 26, "coverage too low: {coverage}/32 bins");
    }
}
