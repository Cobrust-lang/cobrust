//! L2.perf benchmark for cobrust-tomli vs CPython tomllib.
//!
//! 0.1.0-beta T1.1 perf gate: must reach ≥ 0.8× CPython tomllib on
//! representative parse workloads. The full T1.1 finding has the
//! at-promotion-time numbers (1KB / 100KB / 10MB doc sizes); this
//! standalone benchmark binary lets developers re-measure on demand.
//!
//! ## Run
//!
//! ```bash
//! cargo bench -p cobrust-tomli --bench vs_cpython
//! ```
//!
//! ## What it measures
//!
//! - Parse time on a synthesized 1 KB / 100 KB / 10 MB TOML document
//!   (same shape as the T1.1 harness for direct cross-reference).
//! - CPython `tomllib.loads()` parse time on the same input via
//!   subprocess (`/opt/homebrew/bin/python3.11 -c ...`).
//! - Ratio CPython ns / Cobrust ns. Higher is better; ≥ 0.8 PASSes
//!   the 0.1.0-beta perf gate.
//!
//! Output goes to stdout in `KEY=value` shape so CI can grep for
//! `RATIO_10MB=` and gate on the number.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_panics_doc,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::uninlined_format_args,
    clippy::format_push_string,
    clippy::cast_lossless
)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

use cobrust_tomli::loads;

const PYTHON: &str = "/opt/homebrew/bin/python3.11";

fn synth_doc(target_bytes: usize) -> String {
    let mut s = String::new();
    let mut idx = 0u64;
    while s.len() < target_bytes {
        s.push_str(&format!("[section_{idx}]\n"));
        for k in 0..50 {
            s.push_str(&format!("k{k} = {}\n", (idx as i64) * 31 - (k as i64) * 7));
            if s.len() >= target_bytes {
                break;
            }
            s.push_str(&format!("s{k} = \"abcdefghij\"\n"));
            if s.len() >= target_bytes {
                break;
            }
            s.push_str(&format!(
                "b{k} = {}\n",
                if k % 2 == 0 { "true" } else { "false" }
            ));
            if s.len() >= target_bytes {
                break;
            }
        }
        idx += 1;
    }
    s
}

fn time_cobrust(doc: &str, iters: u32) -> u128 {
    // Warmup
    for _ in 0..5 {
        let _ = loads(doc);
    }
    let start = Instant::now();
    for _ in 0..iters {
        let _ = loads(doc);
    }
    let total_ns = start.elapsed().as_nanos();
    total_ns / u128::from(iters.max(1))
}

fn time_cpython(doc: &str, iters: u32) -> u128 {
    let script = format!(
        "import sys, tomllib, time\nsrc=sys.stdin.read()\nn={iters}\n# warmup\nfor _ in range(5): tomllib.loads(src)\nt0=time.perf_counter_ns()\nfor _ in range(n): tomllib.loads(src)\nt1=time.perf_counter_ns()\nprint(t1-t0)\n"
    );
    let mut py = Command::new(PYTHON)
        .arg("-c")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn python perf");
    py.stdin
        .take()
        .expect("stdin")
        .write_all(doc.as_bytes())
        .expect("write stdin");
    let out = py.wait_with_output().expect("wait python perf");
    if !out.status.success() {
        return 0;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let total_ns: u128 = s.trim().parse().unwrap_or(0);
    total_ns / u128::from(iters.max(1))
}

fn run_for(target_bytes: usize, label: &str, iters: u32) -> (u128, u128, f64) {
    let doc = synth_doc(target_bytes);
    let cobrust_ns = time_cobrust(&doc, iters);
    let cpython_ns = time_cpython(&doc, iters);
    let ratio = if cobrust_ns > 0 {
        cpython_ns as f64 / cobrust_ns as f64
    } else {
        0.0
    };
    println!(
        "BENCH label={label} bytes={} cobrust_ns_per_iter={cobrust_ns} cpython_ns_per_iter={cpython_ns} ratio={ratio:.3}",
        doc.len()
    );
    (cobrust_ns, cpython_ns, ratio)
}

fn main() {
    println!("vs_cpython bench — Cobrust 0.1.0-beta T1.1 perf gate");
    println!("=====================================================");
    let (_c1, _p1, r1) = run_for(1_000, "1KB", 1000);
    let (_c2, _p2, r2) = run_for(100_000, "100KB", 50);
    let (_c3, _p3, r3) = run_for(10_000_000, "10MB", 2);

    println!("\n--- Summary ---");
    println!("RATIO_1KB={r1:.3}");
    println!("RATIO_100KB={r2:.3}");
    println!("RATIO_10MB={r3:.3}");

    // 0.8x perf gate. We expect ≥ 8× from T1.1 measurements; flag if
    // anything is below 1.0× (i.e. CPython is faster).
    let min_ratio = r1.min(r2).min(r3);
    if min_ratio < 0.8 {
        eprintln!("\nPERF GATE FAIL: minimum ratio {min_ratio:.3} < 0.8");
        std::process::exit(1);
    }
    println!("\nPERF GATE PASS: minimum ratio {min_ratio:.3} >= 0.8");
}
