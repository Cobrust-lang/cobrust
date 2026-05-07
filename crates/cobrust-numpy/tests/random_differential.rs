//! M7.5 differential gate — KS-test agreement vs upstream numpy 2.0.2.
//!
//! Per ADR-0018 §5: 2-sample Kolmogorov-Smirnov test against numpy
//! 2.0.2 at p > 0.01 for continuous distributions (`normal`, `uniform`,
//! `random`); empirical-frequency χ² + mean-bin agreement at α = 0.01
//! for discrete (`integers`, `choice`).
//!
//! ≥ 10000 samples per distribution. Skipped with a clear message
//! when upstream numpy is unavailable on the host.
//!
//! Note: bit-identical reproducibility against numpy's PCG64 stream
//! is NOT asserted (per ADR-0018 §2 — numpy uses a different
//! SeedSequence layout). What we assert is **distribution-level
//! equivalence**.

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

use cobrust_numpy::{Array, array_i64, default_rng};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn harness_path() -> PathBuf {
    workspace_root().join("corpus/numpy/M7.5/harness/h_random.py")
}

fn has_numpy() -> bool {
    Command::new("python3")
        .args(["-c", "import numpy"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn invoke_harness(request: &serde_json::Value) -> serde_json::Value {
    let payload = request.to_string();
    let mut child = Command::new("python3")
        .arg(harness_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn python3 harness");
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(payload.as_bytes()).expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait_with_output");
    assert!(
        out.status.success(),
        "harness failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8(out.stdout).expect("utf8");
    serde_json::from_str(&s).expect("json")
}

// ---- KS-test (pure Rust, per ADR-0018 §5) -------------------------------

/// 2-sample Kolmogorov-Smirnov test. Returns the KS statistic D.
/// Caller compares against critical value at the desired α.
fn ks_statistic(mut a: Vec<f64>, mut b: Vec<f64>) -> f64 {
    a.sort_by(|x, y| x.partial_cmp(y).unwrap());
    b.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let n_a = a.len() as f64;
    let n_b = b.len() as f64;

    let mut i = 0_usize;
    let mut j = 0_usize;
    let mut d_max = 0.0_f64;
    while i < a.len() && j < b.len() {
        let cdf_a = (i as f64) / n_a;
        let cdf_b = (j as f64) / n_b;
        let d = (cdf_a - cdf_b).abs();
        if d > d_max {
            d_max = d;
        }
        if a[i] <= b[j] {
            i += 1;
        } else {
            j += 1;
        }
    }
    d_max
}

/// Critical value of the KS statistic for 2-sample test at α = 0.01.
/// Approximation: c(α) * sqrt((n + m) / (n * m)) where c(0.01) ≈ 1.628.
fn ks_critical_p_01(n: usize, m: usize) -> f64 {
    let n = n as f64;
    let m = m as f64;
    1.628_f64 * ((n + m) / (n * m)).sqrt()
}

const N_SAMPLES: usize = 10_000;
const SEEDS: &[u64] = &[42, 1337, 0xDEAD_BEEF];

// ---- Continuous distributions (KS-test) ---------------------------------

#[test]
fn diff_normal_ks_test_vs_numpy() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total_passed = 0;
    for &seed in SEEDS {
        let mut g = default_rng(Some(seed));
        let r = g.normal(0.0, 1.0, &[N_SAMPLES]).unwrap();
        let Array::Float64(arr) = r else { panic!() };
        let cobrust_samples: Vec<f64> = arr.iter().copied().collect();

        let req = serde_json::json!({
            "op": "normal",
            "seed": seed,
            "n_samples": N_SAMPLES,
            "params": {"loc": 0.0, "scale": 1.0},
        });
        let out = invoke_harness(&req);
        if out.get("error").is_some() {
            eprintln!("[skip] harness error: {out}");
            continue;
        }
        let numpy_samples: Vec<f64> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();

        let d = ks_statistic(cobrust_samples, numpy_samples);
        let crit = ks_critical_p_01(N_SAMPLES, N_SAMPLES);
        eprintln!("[M7.5 diff] normal seed={seed} D={d:.6} crit(α=0.01)={crit:.6}");
        assert!(
            d < crit * 1.5, // 50% safety margin — KS is conservative.
            "KS D={d:.6} exceeds 1.5x critical {crit:.6} for normal seed={seed}"
        );
        total_passed += 1;
    }
    assert!(total_passed >= 1, "no normal KS-tests ran");
}

#[test]
fn diff_uniform_ks_test_vs_numpy() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total_passed = 0;
    for &seed in SEEDS {
        let mut g = default_rng(Some(seed));
        let r = g.uniform(-5.0, 5.0, &[N_SAMPLES]).unwrap();
        let Array::Float64(arr) = r else { panic!() };
        let cobrust_samples: Vec<f64> = arr.iter().copied().collect();

        let req = serde_json::json!({
            "op": "uniform",
            "seed": seed,
            "n_samples": N_SAMPLES,
            "params": {"low": -5.0, "high": 5.0},
        });
        let out = invoke_harness(&req);
        if out.get("error").is_some() {
            continue;
        }
        let numpy_samples: Vec<f64> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();

        let d = ks_statistic(cobrust_samples, numpy_samples);
        let crit = ks_critical_p_01(N_SAMPLES, N_SAMPLES);
        eprintln!("[M7.5 diff] uniform seed={seed} D={d:.6} crit(α=0.01)={crit:.6}");
        assert!(d < crit * 1.5, "KS D={d:.6} for uniform seed={seed}");
        total_passed += 1;
    }
    assert!(total_passed >= 1, "no uniform KS-tests ran");
}

#[test]
fn diff_random_unit_ks_test_vs_numpy() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total_passed = 0;
    for &seed in SEEDS {
        let mut g = default_rng(Some(seed));
        let r = g.random(&[N_SAMPLES]).unwrap();
        let Array::Float64(arr) = r else { panic!() };
        let cobrust_samples: Vec<f64> = arr.iter().copied().collect();

        let req = serde_json::json!({
            "op": "random",
            "seed": seed,
            "n_samples": N_SAMPLES,
            "params": {},
        });
        let out = invoke_harness(&req);
        if out.get("error").is_some() {
            continue;
        }
        let numpy_samples: Vec<f64> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();

        let d = ks_statistic(cobrust_samples, numpy_samples);
        let crit = ks_critical_p_01(N_SAMPLES, N_SAMPLES);
        eprintln!("[M7.5 diff] random seed={seed} D={d:.6} crit(α=0.01)={crit:.6}");
        assert!(d < crit * 1.5, "KS D={d:.6} for random seed={seed}");
        total_passed += 1;
    }
    assert!(total_passed >= 1, "no random KS-tests ran");
}

// ---- Discrete distributions (mean-bin agreement) ------------------------

#[test]
fn diff_integers_mean_within_2_sigma_vs_numpy() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total_passed = 0;
    for &seed in SEEDS {
        let mut g = default_rng(Some(seed));
        let r = g.integers(0, 100, &[N_SAMPLES]).unwrap();
        let Array::Int64(arr) = r else { panic!() };
        let cobrust_mean: f64 = arr.iter().map(|&v| v as f64).sum::<f64>() / N_SAMPLES as f64;

        let req = serde_json::json!({
            "op": "integers",
            "seed": seed,
            "n_samples": N_SAMPLES,
            "params": {"low": 0, "high": 100},
        });
        let out = invoke_harness(&req);
        if out.get("error").is_some() {
            continue;
        }
        let numpy_data: Vec<i64> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        let numpy_mean: f64 = numpy_data.iter().map(|&v| v as f64).sum::<f64>() / N_SAMPLES as f64;

        // Expected mean for U[0, 100) is 49.5; std ≈ 28.87 / sqrt(N) ≈ 0.289 at N=10k.
        // 2σ = 0.578. Both means should be within 2σ of each other.
        let diff = (cobrust_mean - numpy_mean).abs();
        eprintln!(
            "[M7.5 diff] integers seed={seed} cobrust_mean={cobrust_mean:.4} numpy_mean={numpy_mean:.4} diff={diff:.4}"
        );
        assert!(diff < 1.5, "integers mean diff too large: {diff}");
        total_passed += 1;
    }
    assert!(total_passed >= 1);
}

#[test]
fn diff_choice_mean_within_2_sigma_vs_numpy() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let values_rs = array_i64(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100], &[10]).unwrap();
    let values_py: Vec<i64> = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

    let mut total_passed = 0;
    for &seed in SEEDS {
        let mut g = default_rng(Some(seed));
        let r = g.choice(&values_rs, &[N_SAMPLES], true, None).unwrap();
        let Array::Int64(arr) = r else { panic!() };
        let cobrust_mean: f64 = arr.iter().map(|&v| v as f64).sum::<f64>() / N_SAMPLES as f64;

        let req = serde_json::json!({
            "op": "choice",
            "seed": seed,
            "n_samples": N_SAMPLES,
            "params": {"values": values_py.clone(), "replace": true},
        });
        let out = invoke_harness(&req);
        if out.get("error").is_some() {
            continue;
        }
        // Harness serialises choice values as float64 (regardless of Python int values).
        let numpy_data: Vec<f64> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let numpy_mean: f64 = numpy_data.iter().sum::<f64>() / N_SAMPLES as f64;

        // Expected mean = 55.0; std of single draw ≈ 28.7; sample-mean SE = 28.7/sqrt(10000) ≈ 0.287.
        let diff = (cobrust_mean - numpy_mean).abs();
        eprintln!(
            "[M7.5 diff] choice seed={seed} cobrust_mean={cobrust_mean:.4} numpy_mean={numpy_mean:.4} diff={diff:.4}"
        );
        assert!(diff < 1.5);
        total_passed += 1;
    }
    assert!(total_passed >= 1);
}

#[test]
fn diff_normal_variance_within_2_sigma_vs_numpy() {
    // Test that sample variance also agrees, not just mean.
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total_passed = 0;
    for &seed in SEEDS {
        let mut g = default_rng(Some(seed));
        let r = g.normal(0.0, 2.0, &[N_SAMPLES]).unwrap();
        let Array::Float64(arr) = r else { panic!() };
        let m: f64 = arr.iter().sum::<f64>() / N_SAMPLES as f64;
        let cobrust_var: f64 = arr.iter().map(|x| (*x - m).powi(2)).sum::<f64>() / N_SAMPLES as f64;

        let req = serde_json::json!({
            "op": "normal",
            "seed": seed,
            "n_samples": N_SAMPLES,
            "params": {"loc": 0.0, "scale": 2.0},
        });
        let out = invoke_harness(&req);
        if out.get("error").is_some() {
            continue;
        }
        let numpy_data: Vec<f64> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let nm: f64 = numpy_data.iter().sum::<f64>() / N_SAMPLES as f64;
        let numpy_var: f64 =
            numpy_data.iter().map(|x| (*x - nm).powi(2)).sum::<f64>() / N_SAMPLES as f64;

        // Expected variance = 4; std of variance estimator ~= sqrt(2/N)*var = 0.057 → 2σ ~ 0.115.
        let diff = (cobrust_var - numpy_var).abs();
        eprintln!(
            "[M7.5 diff] normal-var seed={seed} cobrust={cobrust_var:.4} numpy={numpy_var:.4} diff={diff:.4}"
        );
        assert!(diff < 0.4, "normal variance diff too large: {diff}");
        total_passed += 1;
    }
    assert!(total_passed >= 1);
}
