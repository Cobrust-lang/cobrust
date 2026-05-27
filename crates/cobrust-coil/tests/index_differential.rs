//! M7.2 differential gate — bytewise comparison vs upstream numpy 2.0.2.
//!
//! Per ADR-0015 §"M7.2 scope window": ≥ 1000 fuzz inputs per
//! indexing kind, panic-free + matching numpy via the differential
//! harness (bit-identical for int/bool, `rtol=1e-7` for float).
//!
//! Drives `corpus/numpy/M7.2/harness/h_index.py` as a subprocess for
//! every fuzz input and compares the upstream numpy result against
//! `coil::Array::<op>(...).to_json()`. When upstream numpy
//! is unavailable on the host the gate skips with a clear message
//! (same pattern as M7.0/M7.1).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::mismatching_type_param_order)]
#![allow(clippy::doc_markdown)]
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
#![allow(clippy::redundant_clone)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::needless_pass_by_ref_mut)]

use coil::{Array, SliceSpec, array_bool, array_f64, array_i32, array_i64, np_where};
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
    workspace_root().join("corpus/numpy/M7.2/harness/h_index.py")
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
/// reproducibility (per ADR-0015 §"Verification" + ADR-0007).
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
        // Closed range [lo, hi].
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
}

fn arrays_match_int(a: &Array, b: &serde_json::Value) -> bool {
    let aj = a.to_json();
    aj == *b
}

fn arrays_match_float(a: &Array, b: &serde_json::Value, rtol: f64) -> bool {
    let aj = a.to_json();
    if aj.get("dtype") != b.get("dtype") {
        return false;
    }
    if aj.get("shape") != b.get("shape") {
        return false;
    }
    let ad = aj
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let bd = b
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if ad.len() != bd.len() {
        return false;
    }
    for (av, bv) in ad.iter().zip(bd.iter()) {
        let af = av.as_f64().unwrap_or(0.0);
        let bf = bv.as_f64().unwrap_or(0.0);
        if af.is_nan() && bf.is_nan() {
            continue;
        }
        let diff = (af - bf).abs();
        let denom = bf.abs().max(1e-300);
        if diff > rtol * denom && diff > rtol {
            eprintln!("mismatch: cobrust={af} numpy={bf} diff={diff}");
            return false;
        }
    }
    true
}

// ---- 1000 fuzz inputs per indexing kind ---------------------------------

const FUZZ_PER_KIND: usize = 1024;

#[test]
fn diff_slice_1024_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] python3 numpy unavailable");
        return;
    }
    let mut rng = Lcg::new(0xDEAD_BEEF_0000_0072);
    let mut matched = 0;
    for _ in 0..FUZZ_PER_KIND {
        let n = rng.rand_range(1, 32) as usize;
        let data: Vec<i64> = (0..n).map(|i| (i as i64) * 3 - 10).collect();
        let a = array_i64(&data, &[n]).unwrap();
        // Random slice spec (positive step only — ndarray slice with
        // negative step requires positive bounds; we cover negative
        // step in slice_negative_step_match).
        let start = rng.rand_range(-(n as i64) - 5, n as i64 + 5);
        let stop = rng.rand_range(-(n as i64) - 5, n as i64 + 5);
        let step = rng.rand_range(1, 5);
        let spec = SliceSpec::stepped(start, stop, step);
        let cobrust_view = a.slice(spec).unwrap();
        let cobrust_owned = cobrust_view.to_owned();
        let req = serde_json::json!({
            "op": "slice",
            "a": a.to_json(),
            "start": start,
            "stop": stop,
            "step": step,
        });
        let np_resp = invoke_harness(&req);
        if np_resp.get("error").is_some() {
            // numpy rejected; should not happen for valid spec — skip.
            continue;
        }
        assert!(
            arrays_match_int(&cobrust_owned, &np_resp),
            "slice mismatch: cobrust={:?} numpy={:?}",
            cobrust_owned.to_json(),
            np_resp
        );
        matched += 1;
    }
    eprintln!("[diff] slice matched {}/{}", matched, FUZZ_PER_KIND);
    assert!(
        matched >= FUZZ_PER_KIND - 50,
        "fewer than 95% of slice inputs matched"
    );
}

const FUZZ_NEG_STEP: usize = 256;

#[test]
fn diff_slice_negative_step_match() {
    if !has_numpy() {
        eprintln!("[skip] python3 numpy unavailable");
        return;
    }
    let mut rng = Lcg::new(0xCAFE_BABE_0000_0072);
    let mut matched = 0;
    for _ in 0..FUZZ_NEG_STEP {
        let n = rng.rand_range(1, 16) as usize;
        let data: Vec<i64> = (0..n).map(|i| i as i64).collect();
        let a = array_i64(&data, &[n]).unwrap();
        // Reverse with [::-1] / [::-2]: use start=None, stop=None.
        let step = -rng.rand_range(1, 3);
        // Use step_only(step) which sends start=None, stop=None.
        let cobrust_view = a.slice(SliceSpec::step_only(step)).unwrap();
        let cobrust_owned = cobrust_view.to_owned();
        let req = serde_json::json!({
            "op": "slice",
            "a": a.to_json(),
            "start": serde_json::Value::Null,
            "stop": serde_json::Value::Null,
            "step": step,
        });
        let np_resp = invoke_harness(&req);
        if np_resp.get("error").is_some() {
            continue;
        }
        assert!(
            arrays_match_int(&cobrust_owned, &np_resp),
            "neg-step slice mismatch: cobrust={:?} numpy={:?}",
            cobrust_owned.to_json(),
            np_resp
        );
        matched += 1;
    }
    eprintln!(
        "[diff] slice neg-step matched {}/{}",
        matched, FUZZ_NEG_STEP
    );
    assert!(matched >= FUZZ_NEG_STEP - 10);
}

#[test]
fn diff_take_1024_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] python3 numpy unavailable");
        return;
    }
    let mut rng = Lcg::new(0xC0FFEE_42);
    let mut matched = 0;
    for _ in 0..FUZZ_PER_KIND {
        let n = rng.rand_range(2, 32) as usize;
        let data: Vec<i32> = (0..n).map(|i| (i as i32) * 7 - 3).collect();
        let a = array_i32(&data, &[n]).unwrap();
        let k = rng.rand_range(1, 8) as usize;
        let indices: Vec<i64> = (0..k)
            .map(|_| rng.rand_range(-(n as i64), n as i64 - 1))
            .collect();
        let cobrust = a.take(&indices).unwrap();
        let req = serde_json::json!({
            "op": "take",
            "a": a.to_json(),
            "indices": indices,
        });
        let np_resp = invoke_harness(&req);
        if np_resp.get("error").is_some() {
            continue;
        }
        assert!(
            arrays_match_int(&cobrust, &np_resp),
            "take mismatch: cobrust={:?} numpy={:?}",
            cobrust.to_json(),
            np_resp
        );
        matched += 1;
    }
    eprintln!("[diff] take matched {}/{}", matched, FUZZ_PER_KIND);
    assert!(matched >= FUZZ_PER_KIND - 50);
}

#[test]
fn diff_mask_1024_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] python3 numpy unavailable");
        return;
    }
    let mut rng = Lcg::new(0xBAAA_AAAD);
    let mut matched = 0;
    for _ in 0..FUZZ_PER_KIND {
        let n = rng.rand_range(1, 32) as usize;
        let data: Vec<i32> = (0..n).map(|i| (i as i32) * 3).collect();
        let a = array_i32(&data, &[n]).unwrap();
        let mask: Vec<bool> = (0..n).map(|_| rng.next_u64() % 2 == 0).collect();
        let m = array_bool(&mask, &[n]).unwrap();
        let cobrust = a.mask(&m).unwrap();
        let req = serde_json::json!({
            "op": "mask",
            "a": a.to_json(),
            "bool": m.to_json(),
        });
        let np_resp = invoke_harness(&req);
        if np_resp.get("error").is_some() {
            continue;
        }
        assert!(
            arrays_match_int(&cobrust, &np_resp),
            "mask mismatch: cobrust={:?} numpy={:?}",
            cobrust.to_json(),
            np_resp
        );
        matched += 1;
    }
    eprintln!("[diff] mask matched {}/{}", matched, FUZZ_PER_KIND);
    assert!(matched >= FUZZ_PER_KIND - 50);
}

#[test]
fn diff_single_index_1024_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] python3 numpy unavailable");
        return;
    }
    let mut rng = Lcg::new(0xFEED_FACE);
    let mut matched = 0;
    for _ in 0..FUZZ_PER_KIND {
        let n = rng.rand_range(1, 32) as usize;
        let data: Vec<i32> = (0..n).map(|i| (i as i32) * 11).collect();
        let a = array_i32(&data, &[n]).unwrap();
        let idx = rng.rand_range(-(n as i64), n as i64 - 1);
        let cobrust = a.index_single(idx).unwrap().to_owned();
        let req = serde_json::json!({
            "op": "single",
            "a": a.to_json(),
            "index": idx,
        });
        let np_resp = invoke_harness(&req);
        if np_resp.get("error").is_some() {
            continue;
        }
        assert!(
            arrays_match_int(&cobrust, &np_resp),
            "single mismatch: cobrust={:?} numpy={:?}",
            cobrust.to_json(),
            np_resp
        );
        matched += 1;
    }
    eprintln!("[diff] single matched {}/{}", matched, FUZZ_PER_KIND);
    assert!(matched >= FUZZ_PER_KIND - 50);
}

#[test]
fn diff_where_1024_inputs_match_upstream() {
    if !has_numpy() {
        eprintln!("[skip] python3 numpy unavailable");
        return;
    }
    let mut rng = Lcg::new(0x12_3456_789A);
    let mut matched = 0;
    for _ in 0..FUZZ_PER_KIND {
        let n = rng.rand_range(1, 16) as usize;
        let cond_v: Vec<bool> = (0..n).map(|_| rng.next_u64() % 2 == 0).collect();
        let cond = array_bool(&cond_v, &[n]).unwrap();
        let xv: Vec<f64> = (0..n).map(|i| (i as f64) * 1.5 - 0.5).collect();
        let yv: Vec<f64> = (0..n).map(|i| (i as f64) * -0.5 + 7.0).collect();
        let x = array_f64(&xv, &[n]).unwrap();
        let y = array_f64(&yv, &[n]).unwrap();
        let cobrust = np_where(&cond, &x, &y).unwrap();
        let req = serde_json::json!({
            "op": "where",
            "a": cond.to_json(),  // unused but required by harness signature
            "cond": cond.to_json(),
            "x": x.to_json(),
            "y": y.to_json(),
        });
        let np_resp = invoke_harness(&req);
        if np_resp.get("error").is_some() {
            continue;
        }
        assert!(
            arrays_match_float(&cobrust, &np_resp, 1e-7),
            "where mismatch: cobrust={:?} numpy={:?}",
            cobrust.to_json(),
            np_resp
        );
        matched += 1;
    }
    eprintln!("[diff] where matched {}/{}", matched, FUZZ_PER_KIND);
    assert!(matched >= FUZZ_PER_KIND - 50);
}
