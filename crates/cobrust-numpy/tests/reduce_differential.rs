//! M7.3 differential gate — bytewise comparison vs upstream numpy 2.0.2.
//!
//! Per ADR-0016 §"M7.3 scope window": ≥ 1000 fuzz inputs per
//! reduction, panic-free + matching numpy via the differential
//! harness (bit-identical for int/bool, `rtol=1e-7` for float;
//! argmin/argmax exact match).
//!
//! Drives `corpus/numpy/M7.3/harness/h_reduction.py` as a subprocess
//! per fuzz input and compares the upstream numpy result against
//! `cobrust_numpy::<reduce>(...).to_json()`. When upstream numpy is
//! unavailable on the host the gate skips with a clear message (same
//! pattern as M7.0/M7.1/M7.2).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::if_not_else)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_range_contains)]

use cobrust_numpy::{Array, array_bool, array_f64, array_i64};
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
    workspace_root().join("corpus/numpy/M7.3/harness/h_reduction.py")
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

/// Linear-congruential PRNG seeded deterministically per test for
/// reproducibility (per ADR-0016 §"Verification" + ADR-0007).
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn rand_range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
    fn rand_f64_unit(&mut self) -> f64 {
        // 53-bit unit float in [0, 1).
        let bits = self.next_u64() & ((1u64 << 53) - 1);
        bits as f64 / (1u64 << 53) as f64
    }
}

fn array_match_int(a: &Array, b: &serde_json::Value) -> bool {
    a.to_json() == *b
}

fn array_match_float(a: &Array, b: &serde_json::Value, rtol: f64) -> bool {
    let aj = a.to_json();
    let a_data = aj["data"].as_array().expect("data array");
    let b_data = b["data"].as_array().expect("data array");
    if a_data.len() != b_data.len() {
        return false;
    }
    if aj["dtype"] != b["dtype"] || aj["shape"] != b["shape"] {
        return false;
    }
    for (av, bv) in a_data.iter().zip(b_data) {
        let af = av.as_f64().unwrap_or(f64::NAN);
        let bf = bv.as_f64().unwrap_or(f64::NAN);
        if af.is_nan() && bf.is_nan() {
            continue;
        }
        if af.is_nan() != bf.is_nan() {
            return false;
        }
        let diff = (af - bf).abs();
        let ok = if bf.abs() > 1.0 {
            diff < rtol * bf.abs()
        } else {
            diff < rtol
        };
        if !ok {
            return false;
        }
    }
    true
}

const RTOL: f64 = 1e-7;
const SEEDS: &[u64] = &[42, 1337, 0xDEAD_BEEF];

#[test]
fn diff_sum_int_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<i64> = (0..n).map(|_| rng.rand_range(-1000, 1000)).collect();
            let a = array_i64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "sum",
                "a": {"dtype": "Int64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.sum(None).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_sum_int mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} differential matches verified");
}

#[test]
fn diff_sum_float_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<f64> = (0..n).map(|_| rng.rand_f64_unit() * 100.0 - 50.0).collect();
            let a = array_f64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "sum",
                "a": {"dtype": "Float64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.sum(None).unwrap();
            assert!(
                array_match_float(&r, &out, RTOL),
                "diff_sum_float mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} float-sum diffs verified");
}

#[test]
fn diff_prod_float_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 8) as usize; // shorter so prod doesn't overflow to inf
            let v: Vec<f64> = (0..n).map(|_| 0.5 + rng.rand_f64_unit() * 1.5).collect();
            let a = array_f64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "prod",
                "a": {"dtype": "Float64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.prod(None).unwrap();
            assert!(
                array_match_float(&r, &out, RTOL),
                "diff_prod_float mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} prod diffs verified");
}

#[test]
fn diff_mean_float_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<f64> = (0..n).map(|_| rng.rand_f64_unit() * 100.0 - 50.0).collect();
            let a = array_f64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "mean",
                "a": {"dtype": "Float64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.mean(None).unwrap();
            assert!(
                array_match_float(&r, &out, RTOL),
                "diff_mean_float mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} mean diffs verified");
}

#[test]
fn diff_var_float_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 64) as usize;
            let v: Vec<f64> = (0..n).map(|_| rng.rand_f64_unit() * 100.0 - 50.0).collect();
            let a = array_f64(&v, &[n]).unwrap();
            let ddof = (rng.rand_range(0, 1) as u32).min(n as u32 - 1);
            let req = serde_json::json!({
                "op": "var",
                "a": {"dtype": "Float64", "shape": [n], "data": v},
                "axis": null,
                "ddof": ddof,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.var(None, ddof).unwrap();
            assert!(
                array_match_float(&r, &out, RTOL),
                "diff_var mismatch (ddof={ddof}): cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} var diffs verified");
}

#[test]
fn diff_std_float_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 64) as usize;
            let v: Vec<f64> = (0..n).map(|_| rng.rand_f64_unit() * 100.0 - 50.0).collect();
            let a = array_f64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "std",
                "a": {"dtype": "Float64", "shape": [n], "data": v},
                "axis": null,
                "ddof": 0,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.std(None, 0).unwrap();
            assert!(
                array_match_float(&r, &out, RTOL),
                "diff_std mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} std diffs verified");
}

#[test]
fn diff_min_int_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<i64> = (0..n).map(|_| rng.rand_range(-1000, 1000)).collect();
            let a = array_i64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "min",
                "a": {"dtype": "Int64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.min(None).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_min mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} min diffs verified");
}

#[test]
fn diff_max_int_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<i64> = (0..n).map(|_| rng.rand_range(-1000, 1000)).collect();
            let a = array_i64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "max",
                "a": {"dtype": "Int64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.max(None).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_max mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} max diffs verified");
}

#[test]
fn diff_argmin_int_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<i64> = (0..n).map(|_| rng.rand_range(-1000, 1000)).collect();
            let a = array_i64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "argmin",
                "a": {"dtype": "Int64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.argmin(None).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_argmin mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} argmin diffs verified");
}

#[test]
fn diff_argmax_int_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(1, 64) as usize;
            let v: Vec<i64> = (0..n).map(|_| rng.rand_range(-1000, 1000)).collect();
            let a = array_i64(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "argmax",
                "a": {"dtype": "Int64", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.argmax(None).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_argmax mismatch: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} argmax diffs verified");
}

#[test]
fn diff_sum_axis_2d_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let r0 = rng.rand_range(1, 8) as usize;
            let r1 = rng.rand_range(1, 8) as usize;
            let n = r0 * r1;
            let v: Vec<i64> = (0..n).map(|_| rng.rand_range(-100, 100)).collect();
            let a = array_i64(&v, &[r0, r1]).unwrap();
            let axis = rng.rand_range(0, 1) % 2;
            let req = serde_json::json!({
                "op": "sum",
                "a": {"dtype": "Int64", "shape": [r0, r1], "data": v},
                "axis": axis,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.sum(Some(axis)).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_sum_axis_2d mismatch (axis={axis}): cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 1024, "only {total} axis-2d diffs verified");
}

#[test]
fn diff_bool_count_via_sum() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..50 {
            let n = rng.rand_range(1, 32) as usize;
            let v: Vec<bool> = (0..n).map(|_| rng.next_u64() % 2 == 0).collect();
            let a = array_bool(&v, &[n]).unwrap();
            let req = serde_json::json!({
                "op": "sum",
                "a": {"dtype": "Bool", "shape": [n], "data": v},
                "axis": null,
            });
            let out = invoke_harness(&req);
            if out.get("error").is_some() {
                continue;
            }
            let r = a.sum(None).unwrap();
            assert!(
                array_match_int(&r, &out),
                "diff_bool sum: cobrust={} numpy={}",
                r.to_json(),
                out
            );
            total += 1;
        }
    }
    assert!(total >= 100, "only {total} bool-sum diffs verified");
}
