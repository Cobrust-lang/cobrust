//! M7.5 well-typed random programs (per ADR-0018).
//!
//! ≥ 50 well-typed random programs covering the M7.5 surface
//! (default_rng, seed, integers, random, normal, uniform, choice).
//! Every program either succeeds or returns a documented `Err(...)`
//! per ADR-0018.

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
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unusual_byte_groupings)]

use coil::{Array, array_f64, array_i64, default_rng};

#[test]
fn well_typed_01_default_rng_some_42() {
    let g = default_rng(Some(42));
    assert_eq!(g.seed_value(), Some(42));
}

#[test]
fn well_typed_02_default_rng_none() {
    let g = default_rng(None);
    assert_eq!(g.seed_value(), None);
}

#[test]
fn well_typed_03_integers_basic() {
    let mut g = default_rng(Some(42));
    let r = g.integers(0, 10, &[5]).unwrap();
    assert_eq!(r.shape(), vec![5]);
    let Array::Int64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= 0 && *v < 10);
    }
}

#[test]
fn well_typed_04_integers_negative_range() {
    let mut g = default_rng(Some(42));
    let r = g.integers(-100, -50, &[20]).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= -100 && *v < -50);
    }
}

#[test]
fn well_typed_05_integers_large_range() {
    let mut g = default_rng(Some(42));
    let r = g.integers(0, 1_000_000_000, &[100]).unwrap();
    assert_eq!(r.size(), 100);
}

#[test]
fn well_typed_06_integers_2d_shape() {
    let mut g = default_rng(Some(42));
    let r = g.integers(0, 100, &[3, 4]).unwrap();
    assert_eq!(r.shape(), vec![3, 4]);
    assert_eq!(r.size(), 12);
}

#[test]
fn well_typed_07_integers_3d_shape() {
    let mut g = default_rng(Some(42));
    let r = g.integers(0, 50, &[2, 3, 4]).unwrap();
    assert_eq!(r.shape(), vec![2, 3, 4]);
}

#[test]
fn well_typed_08_random_basic() {
    let mut g = default_rng(Some(42));
    let r = g.random(&[10]).unwrap();
    assert!(matches!(r, Array::Float64(_)));
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= 0.0 && *v < 1.0);
    }
}

#[test]
fn well_typed_09_random_2d() {
    let mut g = default_rng(Some(42));
    let r = g.random(&[5, 5]).unwrap();
    assert_eq!(r.shape(), vec![5, 5]);
}

#[test]
fn well_typed_10_normal_unit() {
    let mut g = default_rng(Some(42));
    let r = g.normal(0.0, 1.0, &[100]).unwrap();
    assert!(matches!(r, Array::Float64(_)));
}

#[test]
fn well_typed_11_normal_loc_5_scale_2() {
    let mut g = default_rng(Some(42));
    let r = g.normal(5.0, 2.0, &[10000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    let mean: f64 = arr.iter().sum::<f64>() / 10000.0;
    assert!((mean - 5.0).abs() < 0.2);
}

#[test]
fn well_typed_12_normal_negative_loc() {
    let mut g = default_rng(Some(42));
    let r = g.normal(-3.0, 0.5, &[1000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(v.is_finite());
    }
}

#[test]
fn well_typed_13_uniform_basic() {
    let mut g = default_rng(Some(42));
    let r = g.uniform(0.0, 1.0, &[100]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= 0.0 && *v < 1.0);
    }
}

#[test]
fn well_typed_14_uniform_negative_low() {
    let mut g = default_rng(Some(42));
    let r = g.uniform(-10.0, 10.0, &[1000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= -10.0 && *v < 10.0);
    }
}

#[test]
fn well_typed_15_choice_int_with_replacement() {
    let values = array_i64(&[10, 20, 30, 40], &[4]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[100], true, None).unwrap();
    assert!(matches!(r, Array::Int64(_)));
}

#[test]
fn well_typed_16_choice_int_without_replacement() {
    let values = array_i64(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[3], false, None).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    let mut sorted: Vec<i64> = arr.iter().copied().collect();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 3);
}

#[test]
fn well_typed_17_choice_float_with_p() {
    let values = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.5, 0.3, 0.2];
    let r = g.choice(&values, &[1000], true, Some(&p)).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    // value 1.0 should appear most often (~50%).
    let count_one = arr.iter().filter(|&&v| v == 1.0).count();
    assert!(count_one > 400 && count_one < 600, "count_one={count_one}");
}

#[test]
fn well_typed_18_seed_reproducibility() {
    let mut g1 = default_rng(Some(42));
    let mut g2 = default_rng(Some(42));
    let r1 = g1.normal(0.0, 1.0, &[100]).unwrap();
    let r2 = g2.normal(0.0, 1.0, &[100]).unwrap();
    assert_eq!(r1.to_json(), r2.to_json());
}

#[test]
fn well_typed_19_re_seed() {
    let mut g = default_rng(Some(1));
    let _drain = g.random(&[10]).unwrap();
    g.seed(42);
    let r = g.random(&[5]).unwrap();
    let mut g_fresh = default_rng(Some(42));
    let r_fresh = g_fresh.random(&[5]).unwrap();
    assert_eq!(r.to_json(), r_fresh.to_json());
}

#[test]
fn well_typed_20_seed_value_after_seed() {
    let mut g = default_rng(Some(1));
    g.seed(99);
    assert_eq!(g.seed_value(), Some(99));
}

#[test]
fn well_typed_21_integers_singleton() {
    let mut g = default_rng(Some(42));
    let r = g.integers(5, 6, &[10]).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert_eq!(*v, 5);
    }
}

#[test]
fn well_typed_22_random_zero_size() {
    let mut g = default_rng(Some(42));
    let r = g.random(&[0]).unwrap();
    assert_eq!(r.size(), 0);
}

#[test]
fn well_typed_23_normal_large_n() {
    let mut g = default_rng(Some(42));
    let r = g.normal(0.0, 1.0, &[100000]).unwrap();
    assert_eq!(r.size(), 100000);
}

#[test]
fn well_typed_24_uniform_high_precision() {
    let mut g = default_rng(Some(42));
    let r = g.uniform(0.999999, 1.0, &[100]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= 0.999999 && *v < 1.0);
    }
}

#[test]
fn well_typed_25_choice_single_size() {
    let values = array_i64(&[7], &[1]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[5], true, None).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert_eq!(*v, 7);
    }
}

#[test]
fn well_typed_26_choice_2d_output() {
    let values = array_i64(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[3, 3], true, None).unwrap();
    assert_eq!(r.shape(), vec![3, 3]);
}

#[test]
fn well_typed_27_choice_uniform_p() {
    let values = array_i64(&[10, 20, 30, 40], &[4]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.25, 0.25, 0.25, 0.25];
    let r = g.choice(&values, &[1000], true, Some(&p)).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    // Each value should appear ~250 times.
    for target in [10_i64, 20, 30, 40] {
        let count = arr.iter().filter(|&&v| v == target).count();
        assert!(count > 150 && count < 350, "count for {target} = {count}");
    }
}

#[test]
fn well_typed_28_normal_zero_size() {
    let mut g = default_rng(Some(42));
    let r = g.normal(0.0, 1.0, &[0]).unwrap();
    assert_eq!(r.size(), 0);
}

#[test]
fn well_typed_29_uniform_zero_size() {
    let mut g = default_rng(Some(42));
    let r = g.uniform(0.0, 1.0, &[0]).unwrap();
    assert_eq!(r.size(), 0);
}

#[test]
fn well_typed_30_integers_zero_size() {
    let mut g = default_rng(Some(42));
    let r = g.integers(0, 100, &[0]).unwrap();
    assert_eq!(r.size(), 0);
}

#[test]
fn well_typed_31_seed_zero() {
    let mut g1 = default_rng(Some(0));
    let mut g2 = default_rng(Some(0));
    assert_eq!(
        g1.random(&[10]).unwrap().to_json(),
        g2.random(&[10]).unwrap().to_json()
    );
}

#[test]
fn well_typed_32_seed_max_u64() {
    let mut g1 = default_rng(Some(u64::MAX));
    let mut g2 = default_rng(Some(u64::MAX));
    assert_eq!(
        g1.random(&[10]).unwrap().to_json(),
        g2.random(&[10]).unwrap().to_json()
    );
}

#[test]
fn well_typed_33_normal_unit_variance() {
    let mut g = default_rng(Some(42));
    let r = g.normal(0.0, 1.0, &[10000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    let mean: f64 = arr.iter().sum::<f64>() / 10000.0;
    let var: f64 = arr.iter().map(|x| (*x - mean).powi(2)).sum::<f64>() / 10000.0;
    // Sample variance for N(0,1) at N=10k should be ~1 ± 0.05.
    assert!((var - 1.0).abs() < 0.1, "var={var}");
}

#[test]
fn well_typed_34_uniform_distribution_mean() {
    let mut g = default_rng(Some(42));
    let r = g.uniform(2.0, 8.0, &[10000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    let mean: f64 = arr.iter().sum::<f64>() / 10000.0;
    // Expected mean = (2+8)/2 = 5.0
    assert!((mean - 5.0).abs() < 0.15);
}

#[test]
fn well_typed_35_integers_consume_then_continue() {
    let mut g = default_rng(Some(42));
    let r1 = g.integers(0, 100, &[5]).unwrap();
    let r2 = g.integers(0, 100, &[5]).unwrap();
    // Sequential calls should NOT match (different stream positions).
    assert_ne!(r1.to_json(), r2.to_json());
}

#[test]
fn well_typed_36_choice_p_sums_to_one_floor() {
    let values = array_i64(&[1, 2], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    // Sum exactly 1.0 — should accept.
    let p = vec![0.7, 0.3];
    let r = g.choice(&values, &[100], true, Some(&p)).unwrap();
    assert_eq!(r.size(), 100);
}

#[test]
fn well_typed_37_choice_p_within_rtol() {
    let values = array_i64(&[1, 2], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    // Within 1e-8 of 1.0 — should accept.
    let p = vec![0.5_f64, 0.5_f64 + 1e-9];
    let r = g.choice(&values, &[10], true, Some(&p)).unwrap();
    assert_eq!(r.size(), 10);
}

#[test]
fn well_typed_38_normal_extreme_loc() {
    let mut g = default_rng(Some(42));
    let r = g.normal(1e6, 1.0, &[100]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    let mean: f64 = arr.iter().sum::<f64>() / 100.0;
    assert!((mean - 1e6).abs() < 1.0);
}

#[test]
fn well_typed_39_normal_small_scale() {
    let mut g = default_rng(Some(42));
    let r = g.normal(0.0, 1e-9, &[100]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(v.abs() < 1e-7);
    }
}

#[test]
fn well_typed_40_random_distribution_quantiles() {
    let mut g = default_rng(Some(42));
    let r = g.random(&[10000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    let mut sorted: Vec<f64> = arr.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q25 = sorted[2500];
    let q75 = sorted[7500];
    assert!((q25 - 0.25).abs() < 0.05);
    assert!((q75 - 0.75).abs() < 0.05);
}

#[test]
fn well_typed_41_choice_replace_false_full_perm() {
    let values = array_i64(&[10, 20, 30, 40, 50], &[5]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[5], false, None).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    let mut sorted: Vec<i64> = arr.iter().copied().collect();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![10, 20, 30, 40, 50]);
}

#[test]
fn well_typed_42_integers_seed_stream_two_runs() {
    // Two new generators with same seed → same sequence.
    let mut a = default_rng(Some(7));
    let mut b = default_rng(Some(7));
    let ra = a.integers(0, 1_000_000, &[50]).unwrap();
    let rb = b.integers(0, 1_000_000, &[50]).unwrap();
    assert_eq!(ra.to_json(), rb.to_json());
}

#[test]
fn well_typed_43_normal_seed_stream_two_runs() {
    let mut a = default_rng(Some(7));
    let mut b = default_rng(Some(7));
    let ra = a.normal(0.0, 1.0, &[50]).unwrap();
    let rb = b.normal(0.0, 1.0, &[50]).unwrap();
    assert_eq!(ra.to_json(), rb.to_json());
}

#[test]
fn well_typed_44_choice_seed_stream_two_runs() {
    let values = array_i64(&[1, 2, 3, 4, 5], &[5]).unwrap();
    let mut a = default_rng(Some(13));
    let mut b = default_rng(Some(13));
    let ra = a.choice(&values, &[20], true, None).unwrap();
    let rb = b.choice(&values, &[20], true, None).unwrap();
    assert_eq!(ra.to_json(), rb.to_json());
}

#[test]
fn well_typed_45_integers_close_range() {
    let mut g = default_rng(Some(42));
    let r = g.integers(99, 101, &[100]).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v == 99 || *v == 100);
    }
}

#[test]
fn well_typed_46_uniform_one_to_two() {
    let mut g = default_rng(Some(42));
    let r = g.uniform(1.0, 2.0, &[1000]).unwrap();
    let Array::Float64(arr) = r else { panic!() };
    for v in arr.iter() {
        assert!(*v >= 1.0 && *v < 2.0);
    }
}

#[test]
fn well_typed_47_normal_default_loc_zero() {
    let mut g = default_rng(Some(42));
    let r = g.normal(0.0, 1.0, &[1]).unwrap();
    assert_eq!(r.size(), 1);
}

#[test]
fn well_typed_48_choice_dtype_preserved_int32() {
    use coil::array_i32;
    let values = array_i32(&[10, 20, 30], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[5], true, None).unwrap();
    assert!(matches!(r, Array::Int32(_)));
}

#[test]
fn well_typed_49_choice_dtype_preserved_float32() {
    use coil::array_f32;
    let values = array_f32(&[1.5, 2.5, 3.5], &[3]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[5], true, None).unwrap();
    assert!(matches!(r, Array::Float32(_)));
}

#[test]
fn well_typed_50_choice_dtype_preserved_bool() {
    use coil::array_bool;
    let values = array_bool(&[true, false], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    let r = g.choice(&values, &[20], true, None).unwrap();
    assert!(matches!(r, Array::Bool(_)));
}

#[test]
fn well_typed_51_random_2_runs_different_seeds() {
    let mut a = default_rng(Some(1));
    let mut b = default_rng(Some(2));
    assert_ne!(
        a.random(&[10]).unwrap().to_json(),
        b.random(&[10]).unwrap().to_json()
    );
}

#[test]
fn well_typed_52_choice_p_skewed() {
    let values = array_i64(&[100, 200], &[2]).unwrap();
    let mut g = default_rng(Some(42));
    let p = vec![0.99, 0.01];
    let r = g.choice(&values, &[1000], true, Some(&p)).unwrap();
    let Array::Int64(arr) = r else { panic!() };
    let count_100 = arr.iter().filter(|&&v| v == 100).count();
    assert!(count_100 > 950, "count_100={count_100}");
}

#[test]
fn well_typed_53_seed_value_none_preserved() {
    let mut g = default_rng(None);
    let _ = g.random(&[5]).unwrap();
    assert_eq!(g.seed_value(), None);
}

#[test]
fn well_typed_54_default_rng_then_seed_some() {
    let mut g = default_rng(None);
    g.seed(42);
    assert_eq!(g.seed_value(), Some(42));
}

#[test]
fn well_typed_55_integers_4d_shape() {
    let mut g = default_rng(Some(42));
    let r = g.integers(0, 10, &[2, 2, 2, 2]).unwrap();
    assert_eq!(r.shape(), vec![2, 2, 2, 2]);
    assert_eq!(r.size(), 16);
}
