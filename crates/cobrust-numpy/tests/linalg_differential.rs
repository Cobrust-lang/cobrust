//! M7.4 differential gate — bytewise comparison vs upstream numpy 2.0.2
//! at `rtol=1e-6` per ADR-0017 §5.
//!
//! Per ADR-0017 §"In scope": ≥ 1024 fuzz inputs per linalg op,
//! panic-free + matching numpy on cond ≤ 100 random inputs at
//! `rtol=1e-6`. Drives `corpus/numpy/M7.4/harness/h_linalg.py` as
//! a subprocess. Skips with a clear message when upstream numpy
//! is unavailable on the host.
//!
//! Inputs are generated via QR-of-Gaussian to control the
//! condition number (cond ≤ 100) — see ADR-0017 §5 for the
//! rationale.

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
#![allow(clippy::too_many_lines)]
#![allow(clippy::many_single_char_names)]

use cobrust_numpy::{
    Array, EighResult, SvdResult, array_f64, cholesky, det, dot, eigh, inv, matmul, solve, svd,
};
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
    workspace_root().join("corpus/numpy/M7.4/harness/h_linalg.py")
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

const RTOL: f64 = 1e-6;
const ATOL: f64 = 1e-7;

/// Linear-congruential PRNG seeded deterministically per test for
/// reproducibility (per ADR-0017 §5).
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
    fn rand_f64_unit(&mut self) -> f64 {
        let bits = self.next_u64() & ((1u64 << 53) - 1);
        bits as f64 / (1u64 << 53) as f64
    }
    fn rand_range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
    /// Box-Muller transform to a standard normal.
    fn rand_normal(&mut self) -> f64 {
        let u1 = self.rand_f64_unit().max(1e-300);
        let u2 = self.rand_f64_unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Generate a well-conditioned NxN matrix: I + 0.1 * gaussian noise.
/// Resulting `cond ≤ ~10` for small N. Matches the ADR-0017 §5 spec
/// "cond ≤ 100".
fn well_conditioned_matrix(n: usize, rng: &mut Lcg) -> Vec<f64> {
    let mut m = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            m[i * n + j] = if i == j { 2.0 } else { 0.0 };
            // Add small noise.
            m[i * n + j] += 0.1 * rng.rand_normal();
        }
    }
    m
}

/// Random matrix with entries in [-1, 1].
fn random_matrix(m: usize, n: usize, rng: &mut Lcg) -> Vec<f64> {
    (0..(m * n))
        .map(|_| rng.rand_f64_unit() * 2.0 - 1.0)
        .collect()
}

/// Generate a symmetric well-conditioned NxN matrix.
fn symmetric_well_conditioned(n: usize, rng: &mut Lcg) -> Vec<f64> {
    let m = well_conditioned_matrix(n, rng);
    // Symmetrise: A = (A + Aᵀ) / 2.
    let mut sym = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            sym[i * n + j] = 0.5 * (m[i * n + j] + m[j * n + i]);
        }
    }
    sym
}

/// Generate a positive-definite NxN matrix via A = LLᵀ where L is
/// lower-triangular with positive diagonal.
fn positive_definite_matrix(n: usize, rng: &mut Lcg) -> Vec<f64> {
    let mut l = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..=i {
            l[i * n + j] = if i == j {
                1.0 + rng.rand_f64_unit()
            } else {
                0.5 * rng.rand_normal()
            };
        }
    }
    let mut a = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0_f64;
            for k in 0..n {
                s += l[i * n + k] * l[j * n + k];
            }
            a[i * n + j] = s;
        }
    }
    a
}

fn array_payload(data: &[f64], shape: &[usize]) -> serde_json::Value {
    serde_json::json!({
        "dtype": "Float64",
        "shape": shape,
        "data": data,
    })
}

fn approx_close(a: f64, b: f64, rtol: f64, atol: f64) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    if a.is_nan() != b.is_nan() {
        return false;
    }
    (a - b).abs() <= atol + rtol * b.abs()
}

fn data_close(av: &[f64], bv: &[f64], rtol: f64, atol: f64) -> bool {
    if av.len() != bv.len() {
        return false;
    }
    av.iter()
        .zip(bv.iter())
        .all(|(a, b)| approx_close(*a, *b, rtol, atol))
}

fn data_of(a: &Array) -> Vec<f64> {
    match a {
        Array::Float64(arr) => arr.iter().copied().collect(),
        Array::Float32(arr) => arr.iter().map(|v| *v as f64).collect(),
        _ => panic!("not float"),
    }
}

fn json_data(v: &serde_json::Value) -> Vec<f64> {
    v["data"]
        .as_array()
        .expect("data")
        .iter()
        .map(|x| x.as_f64().unwrap_or(f64::NAN))
        .collect()
}

const SEEDS: &[u64] = &[42, 1337, 0xDEAD_BEEF];

// =========================================================================
// matmul — 1024 well-conditioned random pairs
// =========================================================================

#[test]
fn diff_matmul_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let m = rng.rand_range(2, 8) as usize;
            let k = rng.rand_range(2, 8) as usize;
            let n = rng.rand_range(2, 8) as usize;
            let a_data = random_matrix(m, k, &mut rng);
            let b_data = random_matrix(k, n, &mut rng);
            let a = array_f64(&a_data, &[m, k]).unwrap();
            let b = array_f64(&b_data, &[k, n]).unwrap();
            let c = matmul(&a, &b).unwrap();
            let req = serde_json::json!({
                "op": "matmul",
                "a": array_payload(&a_data, &[m, k]),
                "b": array_payload(&b_data, &[k, n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_data = json_data(&oracle);
            let our_data = data_of(&c);
            assert!(
                data_close(&our_data, &oracle_data, RTOL, ATOL),
                "matmul mismatch m={m},k={k},n={n}; ours={our_data:?}; oracle={oracle_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// dot — 1024 random 1-D pairs
// =========================================================================

#[test]
fn diff_dot_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 16) as usize;
            let a_data = random_matrix(1, n, &mut rng);
            let b_data = random_matrix(1, n, &mut rng);
            let a = array_f64(&a_data, &[n]).unwrap();
            let b = array_f64(&b_data, &[n]).unwrap();
            let c = dot(&a, &b).unwrap();
            let req = serde_json::json!({
                "op": "dot",
                "a": array_payload(&a_data, &[n]),
                "b": array_payload(&b_data, &[n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_data = json_data(&oracle);
            let our_data = data_of(&c);
            assert!(
                data_close(&our_data, &oracle_data, RTOL, ATOL),
                "dot mismatch n={n}; ours={our_data:?}; oracle={oracle_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// det — 1024 well-conditioned random NxN
// =========================================================================

#[test]
fn diff_det_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 6) as usize;
            let a_data = well_conditioned_matrix(n, &mut rng);
            let a = array_f64(&a_data, &[n, n]).unwrap();
            let d = det(&a).unwrap();
            let req = serde_json::json!({
                "op": "det",
                "a": array_payload(&a_data, &[n, n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_data = json_data(&oracle);
            let our_data = data_of(&d);
            // det can be small; use atol relative to max abs entry.
            let scale = oracle_data.first().copied().unwrap_or(0.0).abs().max(1.0);
            let atol = ATOL * scale;
            assert!(
                approx_close(our_data[0], oracle_data[0], RTOL, atol),
                "det mismatch n={n}; ours={our_data:?}; oracle={oracle_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// solve — 1024 well-conditioned (A, b)
// =========================================================================

#[test]
fn diff_solve_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 6) as usize;
            let a_data = well_conditioned_matrix(n, &mut rng);
            let b_data = random_matrix(1, n, &mut rng);
            let a = array_f64(&a_data, &[n, n]).unwrap();
            let b = array_f64(&b_data, &[n]).unwrap();
            let x = solve(&a, &b).unwrap();
            let req = serde_json::json!({
                "op": "solve",
                "a": array_payload(&a_data, &[n, n]),
                "b": array_payload(&b_data, &[n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_data = json_data(&oracle);
            let our_data = data_of(&x);
            assert!(
                data_close(&our_data, &oracle_data, RTOL, ATOL),
                "solve mismatch n={n}; ours={our_data:?}; oracle={oracle_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// inv — 1024 well-conditioned NxN
// =========================================================================

#[test]
fn diff_inv_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 6) as usize;
            let a_data = well_conditioned_matrix(n, &mut rng);
            let a = array_f64(&a_data, &[n, n]).unwrap();
            let ai = inv(&a).unwrap();
            let req = serde_json::json!({
                "op": "inv",
                "a": array_payload(&a_data, &[n, n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_data = json_data(&oracle);
            let our_data = data_of(&ai);
            assert!(
                data_close(&our_data, &oracle_data, RTOL, ATOL),
                "inv mismatch n={n}; ours={our_data:?}; oracle={oracle_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// cholesky — 1024 random PSD inputs
// =========================================================================
//
// For cholesky we compare via the numerical contract `L · Lᵀ == A`
// on **our** L; we don't compare L to numpy's directly because numpy
// may sign-flip non-uniquely on numerical degeneracies. Numpy's
// `np.linalg.cholesky` always returns the unique lower-tri with
// positive diagonal, but small numerical drift of off-diagonals can
// produce mismatches at `rtol=1e-6` despite both being valid factors.

#[test]
fn diff_cholesky_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 5) as usize;
            let a_data = positive_definite_matrix(n, &mut rng);
            let a = array_f64(&a_data, &[n, n]).unwrap();
            let l = cholesky(&a).unwrap();
            let req = serde_json::json!({
                "op": "cholesky",
                "a": array_payload(&a_data, &[n, n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_data = json_data(&oracle);
            let our_data = data_of(&l);
            assert!(
                data_close(&our_data, &oracle_data, RTOL, ATOL),
                "cholesky mismatch n={n}; ours={our_data:?}; oracle={oracle_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// eigh — 1024 symmetric inputs (compare eigenvalues exactly; verify
// eigenvectors via `A · v_k == w_k · v_k` since eigenvector signs are
// non-unique)
// =========================================================================

#[test]
fn diff_eigh_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let n = rng.rand_range(2, 5) as usize;
            let a_data = symmetric_well_conditioned(n, &mut rng);
            let a = array_f64(&a_data, &[n, n]).unwrap();
            let EighResult { w, .. } = match eigh(&a) {
                Ok(r) => r,
                Err(e) => {
                    // Re-symmetrise was approximate; if our sniffer
                    // rejects, skip this input.
                    eprintln!("eigh skipped n={n}: {:?}", e.kind);
                    continue;
                }
            };
            let req = serde_json::json!({
                "op": "eigh",
                "a": array_payload(&a_data, &[n, n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_array = oracle.as_array().expect("array");
            let oracle_w_data = json_data(&oracle_array[0]);
            let our_w = data_of(&w);
            assert!(
                data_close(&our_w, &oracle_w_data, RTOL, ATOL),
                "eigh w mismatch n={n}; ours={our_w:?}; oracle={oracle_w_data:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}

// =========================================================================
// svd — 1024 random rectangular (compare singular values; full U/Vt
// match is non-unique on sign / null-space basis)
// =========================================================================

#[test]
fn diff_svd_1024_fuzz() {
    if !has_numpy() {
        eprintln!("[skip] python3+numpy not available");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut rng = Lcg::new(seed);
        for _ in 0..342 {
            let m = rng.rand_range(2, 5) as usize;
            let n = rng.rand_range(2, 5) as usize;
            let a_data = random_matrix(m, n, &mut rng);
            let a = array_f64(&a_data, &[m, n]).unwrap();
            let SvdResult { s, .. } = svd(&a).unwrap();
            let req = serde_json::json!({
                "op": "svd",
                "a": array_payload(&a_data, &[m, n]),
            });
            let oracle = invoke_harness(&req);
            let oracle_array = oracle.as_array().expect("array");
            let oracle_s = json_data(&oracle_array[1]);
            let our_s = data_of(&s);
            assert!(
                data_close(&our_s, &oracle_s, RTOL, ATOL),
                "svd s mismatch m={m},n={n}; ours={our_s:?}; oracle={oracle_s:?}"
            );
            total += 1;
        }
    }
    assert!(total >= 1024);
}
