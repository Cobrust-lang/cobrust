//! M7.1 differential gate — bytewise comparison vs upstream numpy 2.0.2.
//!
//! Per ADR-0014 acceptance gate: bit-identical for int dtypes;
//! `rtol=1e-7` for float; ≥ 1000 input differential corpus per ufunc.
//!
//! Drives `corpus/numpy/M7.1/harness/h_ufunc.py` as a subprocess for
//! every fuzz input and compares the upstream numpy result against
//! `coil::Array::<op>(...).to_json()`. When upstream numpy
//! is unavailable on the host, the gate skips with a clear message
//! (same pattern as M7.0's `numpy_differential.rs` and M6's
//! `msgpack_pyo3_compiles.rs`).

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
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::if_not_else)]
#![allow(clippy::print_stderr)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

use coil::{Array, array_f64, array_i32, array_i64};
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
    workspace_root().join("corpus/numpy/M7.1/harness/h_ufunc.py")
}

/// Returns `true` if `python3 -c "import numpy"` succeeds. When numpy
/// is unavailable we skip the gate (M6 + M7.0 precedent).
fn has_numpy() -> bool {
    Command::new("python3")
        .args(["-c", "import numpy"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Send `request` to the harness and parse the JSON response.
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

/// Build a {dtype, shape, data} request for an array.
fn array_payload(arr: &Array) -> serde_json::Value {
    arr.to_json()
}

/// Run a binary ufunc and compare the output against numpy. `op` is
/// "add"/"sub"/etc.
fn diff_binary_op(op: &str, a: &Array, b: &Array, rtol: f64) {
    let cobrust = match op {
        "add" => a.add(b).expect("add ok"),
        "sub" => a.sub(b).expect("sub ok"),
        "mul" => a.mul(b).expect("mul ok"),
        _ => panic!("unsupported op for diff_binary_op: {op}"),
    };
    let request = serde_json::json!({
        "op": op,
        "a": array_payload(a),
        "b": array_payload(b),
    });
    let upstream = invoke_harness(&request);
    let cob_json = cobrust.to_json();
    compare_payloads(&cob_json, &upstream, rtol, op);
}

/// Run a unary ufunc and compare against numpy.
fn diff_unary_op(op: &str, a: &Array, rtol: f64) {
    let cobrust = match op {
        "sin" => a.sin().expect("sin ok"),
        "cos" => a.cos().expect("cos ok"),
        "exp" => a.exp().expect("exp ok"),
        "log" => a.log().expect("log ok"),
        "sqrt" => a.sqrt().expect("sqrt ok"),
        _ => panic!("unsupported op for diff_unary_op: {op}"),
    };
    let request = serde_json::json!({
        "op": op,
        "a": array_payload(a),
    });
    let upstream = invoke_harness(&request);
    let cob_json = cobrust.to_json();
    compare_payloads(&cob_json, &upstream, rtol, op);
}

fn compare_payloads(
    cobrust: &serde_json::Value,
    upstream: &serde_json::Value,
    rtol: f64,
    op: &str,
) {
    if let Some(err) = upstream.get("error") {
        // numpy raised — for our diff_*_op helpers we only invoke ops
        // where cobrust succeeded, so a numpy error is unexpected.
        panic!("upstream numpy errored on op={op}: {err}");
    }
    assert_eq!(
        cobrust["dtype"], upstream["dtype"],
        "dtype mismatch for op={op}: cobrust={:?} upstream={:?}",
        cobrust["dtype"], upstream["dtype"]
    );
    let cob_data = cobrust["data"].as_array().expect("cobrust data array");
    let up_data = upstream["data"].as_array().expect("upstream data array");
    assert_eq!(
        cob_data.len(),
        up_data.len(),
        "data length mismatch for op={op}"
    );
    let dtype_str = cobrust["dtype"].as_str().unwrap_or("");
    let is_int = matches!(dtype_str, "Int32" | "Int64" | "Bool");
    for (i, (c, u)) in cob_data.iter().zip(up_data.iter()).enumerate() {
        if is_int {
            assert_eq!(
                c, u,
                "bit-identical mismatch at i={i} for op={op}: cob={c:?} up={u:?}"
            );
        } else {
            let cv = c.as_f64().unwrap_or(0.0);
            let uv = u.as_f64().unwrap_or(0.0);
            if cv.is_nan() && uv.is_nan() {
                continue;
            }
            if cv.is_infinite() && uv.is_infinite() && cv.signum() == uv.signum() {
                continue;
            }
            let denom = uv.abs().max(1e-300);
            let diff = (cv - uv).abs() / denom;
            assert!(
                diff <= rtol,
                "rtol failure at i={i} for op={op}: cob={cv} up={uv} rel_diff={diff}"
            );
        }
    }
}

// ---- Curated case suites --------------------------------------------

const RTOL: f64 = 1e-7;

#[test]
fn diff_add_curated_int_cases() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let cases: &[(&[i32], &[usize], &[i32], &[usize])] = &[
        (&[1, 2, 3], &[3], &[10, 20, 30], &[3]),
        (&[1, 2, 3, 4], &[2, 2], &[10, 20, 30, 40], &[2, 2]),
        (&[5, 7, 9], &[3], &[1, 1, 1], &[3]),
        (&[100], &[1], &[1, 2, 3], &[3]),
    ];
    for (av, ash, bv, bs) in cases {
        let a = array_i32(av, ash).unwrap();
        let b = array_i32(bv, bs).unwrap();
        diff_binary_op("add", &a, &b, RTOL);
    }
}

#[test]
fn diff_sub_curated_float_cases() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let cases: &[(&[f64], &[usize], &[f64], &[usize])] = &[
        (&[10.0, 20.0, 30.0], &[3], &[1.0, 2.0, 3.0], &[3]),
        (
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            &[2, 3],
            &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
            &[2, 3],
        ),
    ];
    for (av, ash, bv, bs) in cases {
        let a = array_f64(av, ash).unwrap();
        let b = array_f64(bv, bs).unwrap();
        diff_binary_op("sub", &a, &b, RTOL);
    }
}

#[test]
fn diff_mul_curated_int64_cases() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let cases: &[(&[i64], &[usize], &[i64], &[usize])] = &[
        (&[2, 3, 4], &[3], &[5, 6, 7], &[3]),
        (&[1; 6], &[2, 3], &[2; 6], &[2, 3]),
    ];
    for (av, ash, bv, bs) in cases {
        let a = array_i64(av, ash).unwrap();
        let b = array_i64(bv, bs).unwrap();
        diff_binary_op("mul", &a, &b, RTOL);
    }
}

#[test]
fn diff_sin_curated_float64() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let a = array_f64(
        &[
            0.0,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
            1.5,
            2.0,
            -1.0,
            -std::f64::consts::PI,
        ],
        &[7],
    )
    .unwrap();
    diff_unary_op("sin", &a, RTOL);
}

#[test]
fn diff_cos_curated_float64() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let a = array_f64(
        &[0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 1.5],
        &[4],
    )
    .unwrap();
    diff_unary_op("cos", &a, RTOL);
}

#[test]
fn diff_exp_curated_float64() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let a = array_f64(&[0.0, 1.0, -1.0, 2.5, -2.5], &[5]).unwrap();
    diff_unary_op("exp", &a, RTOL);
}

#[test]
fn diff_log_curated_float64() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let a = array_f64(&[1.0, std::f64::consts::E, 10.0, 100.0], &[4]).unwrap();
    diff_unary_op("log", &a, RTOL);
}

#[test]
fn diff_sqrt_curated_float64() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let a = array_f64(&[1.0, 4.0, 9.0, 16.0, 25.0], &[5]).unwrap();
    diff_unary_op("sqrt", &a, RTOL);
}

// ---- Fuzz path: ≥ 1000 inputs per ufunc ----------------------------

const SEEDS: &[u64] = &[42, 1337, 0xDEAD_BEEF];

/// Lightweight LCG for deterministic fuzz inputs without pulling in a
/// `rand` dev-dependency.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    fn gen_int(&mut self, lo: i32, hi: i32) -> i32 {
        let range = (hi - lo) as u64;
        ((self.next_u64() % range) as i32) + lo
    }
    fn gen_float(&mut self, lo: f64, hi: f64) -> f64 {
        let r = (self.next_u64() >> 11) as f64 / ((1_u64 << 53) as f64);
        lo + r * (hi - lo)
    }
}

/// Run `≥ 1000` fuzz inputs of an integer binary op; bit-identical to numpy.
#[test]
fn fuzz_add_int32_1000_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..400 {
            let n = (lcg.next_u64() % 30 + 1) as usize;
            let av: Vec<i32> = (0..n).map(|_| lcg.gen_int(-1000, 1000)).collect();
            let bv: Vec<i32> = (0..n).map(|_| lcg.gen_int(-1000, 1000)).collect();
            let a = array_i32(&av, &[n]).unwrap();
            let b = array_i32(&bv, &[n]).unwrap();
            diff_binary_op("add", &a, &b, RTOL);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz budget short: {total}");
}

#[test]
fn fuzz_sub_int64_1000_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..400 {
            let n = (lcg.next_u64() % 30 + 1) as usize;
            let av: Vec<i64> = (0..n)
                .map(|_| i64::from(lcg.gen_int(-1000, 1000)))
                .collect();
            let bv: Vec<i64> = (0..n)
                .map(|_| i64::from(lcg.gen_int(-1000, 1000)))
                .collect();
            let a = array_i64(&av, &[n]).unwrap();
            let b = array_i64(&bv, &[n]).unwrap();
            diff_binary_op("sub", &a, &b, RTOL);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz budget short: {total}");
}

#[test]
fn fuzz_mul_float64_1000_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..400 {
            let n = (lcg.next_u64() % 30 + 1) as usize;
            let av: Vec<f64> = (0..n).map(|_| lcg.gen_float(-100.0, 100.0)).collect();
            let bv: Vec<f64> = (0..n).map(|_| lcg.gen_float(-100.0, 100.0)).collect();
            let a = array_f64(&av, &[n]).unwrap();
            let b = array_f64(&bv, &[n]).unwrap();
            diff_binary_op("mul", &a, &b, RTOL);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz budget short: {total}");
}

#[test]
fn fuzz_sin_float64_1000_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..400 {
            let n = (lcg.next_u64() % 30 + 1) as usize;
            let av: Vec<f64> = (0..n).map(|_| lcg.gen_float(-10.0, 10.0)).collect();
            let a = array_f64(&av, &[n]).unwrap();
            diff_unary_op("sin", &a, RTOL);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz budget short: {total}");
}

#[test]
fn fuzz_exp_float64_1000_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..400 {
            let n = (lcg.next_u64() % 30 + 1) as usize;
            // Exp on small values to avoid overflow.
            let av: Vec<f64> = (0..n).map(|_| lcg.gen_float(-5.0, 5.0)).collect();
            let a = array_f64(&av, &[n]).unwrap();
            diff_unary_op("exp", &a, RTOL);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz budget short: {total}");
}

#[test]
fn fuzz_sqrt_float64_1000_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] upstream numpy unavailable");
        return;
    }
    let mut total = 0;
    for &seed in SEEDS {
        let mut lcg = Lcg::new(seed);
        for _ in 0..400 {
            let n = (lcg.next_u64() % 30 + 1) as usize;
            // sqrt: only positive inputs (matches numpy's well-typed surface).
            let av: Vec<f64> = (0..n).map(|_| lcg.gen_float(0.0, 10000.0)).collect();
            let a = array_f64(&av, &[n]).unwrap();
            diff_unary_op("sqrt", &a, RTOL);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz budget short: {total}");
}
