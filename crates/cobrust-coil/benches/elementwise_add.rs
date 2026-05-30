//! 3-tier element-wise-add performance benchmark for cobrust-coil.
//!
//! The FIRST increment of the Cobrust performance-benchmark suite
//! (CLAUDE.md §5.2 "every 'faster' claim cites a reproducible experiment",
//! §5.3 "benchmarks: scripted, seeded, hardware-tagged"). It replaces
//! guesswork about the `.cb`-wrapping cost of an ecosystem module with a
//! measured number, and establishes the methodology every future library
//! benchmark follows. Methodology source of truth:
//! `docs/agent/benchmarks/README.md`.
//!
//! ## The three tiers (same op, same sizes, same f64 dtype, RUNTIME-ONLY)
//!
//! - **T1 — Python numpy** (the ergonomics baseline). A subprocess
//!   `python3.11 -c` times `np.add(a, b)` over N iterations with
//!   `time.perf_counter_ns()`, after a warm-up. numpy is C/SIMD-backed,
//!   so this is "what a Python user already gets". Self-SKIPS when no
//!   `python3` with numpy is found (e.g. on CI) — the T2/T3 Rust tiers
//!   still run and the report records the skip.
//! - **T2 — raw Rust `ndarray`** (the performance CEILING). Times
//!   `&a + &b` on `ndarray::ArrayD<f64>` — the crate `coil` wraps, called
//!   with no Cobrust layer. This is the best a Rust program can do for
//!   this op; coil cannot beat it, only approach it.
//! - **T3 — Cobrust `coil`** (the `.cb`-WRAPPING cost). Times the C-ABI
//!   shim `__cobrust_coil_buffer_add(a, b)` — the exact symbol a compiled
//!   `.cb` program binds onto for `a + b` — plus the per-op result
//!   `__cobrust_coil_buffer_drop`. Measures coil's runtime kernel + the
//!   cabi alloc/boundary cost: the real overhead a `.cb` user pays over
//!   the raw-ndarray ceiling.
//!
//! ## The diagnostic axis
//!
//! - **T3 / T2** (coil vs raw ndarray) is the MOST diagnostic ratio: does
//!   the `.cb` wrapping (FFI cross + per-op result alloc) PRESERVE Rust
//!   performance or erode it? > 1.0 means coil is slower than the ceiling
//!   by that factor.
//! - **T3 / T1** (coil vs numpy) is the headline "Cobrust vs Python"
//!   number. < 1.0 means coil is faster than numpy; > 1.0 means slower.
//!
//! ## Honesty rules (§5.3 — enforced; the report restates them)
//!
//! (a) RUNTIME-ONLY. No compile time anywhere. For every tier the INPUT
//!     arrays are allocated ONCE, OUTSIDE the timed region. The op's
//!     RESULT allocation + free IS timed and IS documented as included —
//!     it is a genuine, unavoidable part of coil's per-op cost (every
//!     `.cb` `a + b` allocates a fresh `Buffer` the scope later drops),
//!     and T1/T2 are made symmetric (numpy `np.add` and ndarray `&a + &b`
//!     each allocate + free one result per iter too).
//! (b) WARM-UP then MEDIAN (never mean) over N per-iter samples. We
//!     collect N individual `perf_counter`/`Instant` samples per tier and
//!     report the true median ns/op + ns/element. (Mean is reported too,
//!     for transparency, but the headline metric is the median.)
//! (c) SAME WORK. Same array length, same f64 dtype, same `a + b`
//!     semantics across all three tiers; inputs are a deterministic ramp
//!     so no constant-folding and identical values cross-tier.
//! (d) HARDWARE-TAGGED. The wrapper script captures CPU / cores / OS /
//!     rustc into the report. These are dev-laptop numbers (no fixed CPU
//!     governor / thermal control) — indicative, not a controlled rig.
//! (e) REPRODUCIBLE. One entrypoint re-runs everything:
//!     `cargo bench -p cobrust-coil --bench elementwise_add`
//!     (or `scripts/bench/coil_elementwise_add.sh` for the hw-tagged
//!     report). Sizes + iters are overridable via env (see below) but
//!     default to the committed sweep.
//!
//! ## Output
//!
//! `KEY=value` lines on stdout so CI / scripts can grep specific numbers
//! (`T3_OVER_T2_N1000000=`, `T3_MEDIAN_NS_N100=`, ...). A human-readable
//! table is printed alongside.
//!
//! ## Tuning (optional; defaults are the committed sweep)
//!
//! - `COIL_BENCH_SIZES` — comma-separated array sizes (default
//!   `100,10000,1000000`).
//! - `COIL_BENCH_ITERS` — measured iterations per size (default `201`;
//!   odd so the median is a single sample, not a 2-sample average).
//! - `COIL_BENCH_WARMUP` — warm-up iterations per size (default `50`).

// This is a `harness = false` bench binary (a plain `fn main`), so it is
// allowed to print and to use unwrap/expect on its own controlled inputs —
// mirrors the `cobrust-nest` `vs_cpython` bench's allow-set.
#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Opaque-handle ABI round-trip casts (`*mut u8 <-> *mut Array`). The pointers
// all originate from `Box::into_raw` of the SAME target type, so the
// alignment-narrowing lint is a false positive — mirrors the production cabi
// allow (`cabi.rs` §53-66) + the coil corpus tests' sibling allow.
#![allow(clippy::cast_ptr_alignment)]

use std::hint::black_box;
use std::process::{Command, Stdio};
use std::time::Instant;

use coil::Array;
use coil::array_f64;
use coil::cabi::{__cobrust_coil_buffer_add, __cobrust_coil_buffer_drop};

use ndarray::ArrayD;

// =====================================================================
// Stdlib ABI stubs.
//
// `coil`'s `cabi` shims declare three cross-crate stdlib externs
// (`__cobrust_panic` / `__cobrust_list_new` / `__cobrust_list_set`) that
// are normally link-resolved from `libcobrust_stdlib.a` only at `.cb`-link
// time, NOT into this bench binary. We provide minimal stubs so the binary
// links — identical to the coil integration-test corpora
// (`broadcast_elementwise_corpus.rs`). The benchmark only ever calls
// `__cobrust_coil_buffer_add` on EQUAL-shape inputs, which never reaches
// the `coil_panic` / shape-marshal paths, so these stubs are never invoked
// during the timed region; they exist purely to satisfy the linker.
// =====================================================================

#[unsafe(no_mangle)]
extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> ! {
    // SAFETY: the coil_panic helper passes a valid UTF-8 `&str`'s (ptr, len).
    let msg = unsafe { std::slice::from_raw_parts(ptr, len) };
    panic!(
        "__cobrust_panic (bench stub, should be unreachable): {}",
        String::from_utf8_lossy(msg)
    );
}

#[unsafe(no_mangle)]
extern "C" fn __cobrust_list_new(_elem_size: i64, len: i64) -> *mut u8 {
    let v: Vec<i64> = vec![0; if len < 0 { 0 } else { len as usize }];
    Box::into_raw(Box::new(v)).cast::<u8>()
}

#[unsafe(no_mangle)]
extern "C" fn __cobrust_list_set(list: *mut u8, i: i64, val: i64) {
    // SAFETY: `list` is a `Box<Vec<i64>>` from `__cobrust_list_new`.
    let v = unsafe { &mut *list.cast::<Vec<i64>>() };
    if let Some(slot) = usize::try_from(i).ok().and_then(|idx| v.get_mut(idx)) {
        *slot = val;
    }
}

// =====================================================================
// Python (numpy) discovery — mirrors `numpy_differential.rs`.
// =====================================================================

const PYTHON_CANDIDATES: &[&str] = &[
    "/opt/homebrew/bin/python3.11",
    "/opt/homebrew/bin/python3",
    "/usr/local/bin/python3.11",
    "/usr/local/bin/python3",
    "/usr/bin/python3",
    "python3",
];

/// First `python3` on the box that can `import numpy`, or `None` (T1
/// self-skips). Same self-skip discipline as the coil differential gate.
fn pick_python() -> Option<String> {
    for candidate in PYTHON_CANDIDATES {
        let ok = Command::new(candidate)
            .arg("-c")
            .arg("import numpy")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some((*candidate).to_string());
        }
    }
    None
}

// =====================================================================
// Shared statistics. The MEDIAN is the headline metric (honesty rule b);
// mean is reported alongside for transparency.
// =====================================================================

struct Stats {
    median_ns: f64,
    mean_ns: f64,
    min_ns: f64,
    n: usize,
}

fn summarize(mut samples_ns: Vec<f64>) -> Stats {
    assert!(!samples_ns.is_empty(), "need >=1 sample");
    let n = samples_ns.len();
    let sum: f64 = samples_ns.iter().sum();
    let mean_ns = sum / n as f64;
    samples_ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min_ns = samples_ns[0];
    // Odd N (the default 201) → a single middle sample; even N → mean of
    // the two middle samples. The default iters is odd precisely so the
    // median is one observed sample, not an average of two.
    let median_ns = if n % 2 == 1 {
        samples_ns[n / 2]
    } else {
        f64::midpoint(samples_ns[n / 2 - 1], samples_ns[n / 2])
    };
    Stats {
        median_ns,
        mean_ns,
        min_ns,
        n,
    }
}

// =====================================================================
// Inputs. A deterministic ramp (NOT all-zeros / all-ones) so the f64 add
// does real work, cannot be constant-folded, and is bit-identical across
// T2 and T3 (and re-derivable in numpy via the same formula). Built ONCE
// per size, OUTSIDE every timed region (honesty rule a).
// =====================================================================

/// The shared input formula: `a[i] = i * 0.5 + 1.0`, `b[i] = i * 0.25 - 3.0`.
/// Used identically by T2 (Vec → ndarray), T3 (Vec → coil Array), and T1
/// (numpy `np.arange` arithmetic) so all three tiers add the SAME values.
fn ramp_a(n: usize) -> Vec<f64> {
    (0..n).map(|i| i as f64 * 0.5 + 1.0).collect()
}
fn ramp_b(n: usize) -> Vec<f64> {
    (0..n).map(|i| i as f64 * 0.25 - 3.0).collect()
}

// =====================================================================
// T2 — raw Rust ndarray (the performance ceiling).
// =====================================================================

/// Time `&a + &b` on `ndarray::ArrayD<f64>` over `iters` samples after
/// `warmup`. Inputs allocated once before the loop. The owned result is
/// `black_box`'d (so the add is not dead-code-eliminated) and dropped at
/// the end of each loop body — symmetric with T3's result alloc+free and
/// T1's `np.add` result.
fn bench_t2_ndarray(n: usize, iters: usize, warmup: usize) -> Stats {
    let va = ramp_a(n);
    let vb = ramp_b(n);
    let a: ArrayD<f64> = ArrayD::from_shape_vec(ndarray::IxDyn(&[n]), va).unwrap();
    let b: ArrayD<f64> = ArrayD::from_shape_vec(ndarray::IxDyn(&[n]), vb).unwrap();

    for _ in 0..warmup {
        let c = black_box(&a) + black_box(&b);
        black_box(&c);
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let c = black_box(&a) + black_box(&b);
        black_box(&c);
        // Free the result INSIDE the timed region — symmetric with T3
        // (which times `__cobrust_coil_buffer_drop`). Without this, T2
        // excluded its ~40 ns ArrayD<f64> dealloc while T3 included it,
        // inflating T3/T2 (audit 2026-05-31: n=100 5.00 -> 3.77 once
        // symmetric). The result alloc+free is a genuine per-op cost both
        // tiers pay, so both must time it.
        drop(c);
        samples.push(t0.elapsed().as_nanos() as f64);
    }
    summarize(samples)
}

// =====================================================================
// T3 — Cobrust coil C-ABI (the .cb-wrapping cost).
// =====================================================================

/// Box an `Array` as an opaque `Buffer` handle — exactly what coil's
/// constructors (`__cobrust_coil_zeros` etc.) and the corpus tests do.
fn into_handle(arr: Array) -> *mut u8 {
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// Time the coil C-ABI `a + b` over `iters` samples after `warmup`. The
/// two input `Buffer` handles are built ONCE before the loop. The TIMED
/// region is `__cobrust_coil_buffer_add(ha, hb)` followed by
/// `__cobrust_coil_buffer_drop(result)` — the full, honest per-op cost a
/// `.cb` program pays: cross-in, broadcast-compat check, the kernel
/// (which today clones both operands + zero-allocs the output, see the
/// report's "why" section), cross-out, and the result free. The input
/// handles are dropped once, after the loop, OUTSIDE timing.
fn bench_t3_coil(n: usize, iters: usize, warmup: usize) -> Stats {
    let ha = into_handle(array_f64(&ramp_a(n), &[n]).unwrap());
    let hb = into_handle(array_f64(&ramp_b(n), &[n]).unwrap());

    for _ in 0..warmup {
        // SAFETY: ha/hb are live, equal-shape f64 Buffer handles; the
        // result is freed immediately via the drop shim.
        let hc = unsafe { __cobrust_coil_buffer_add(black_box(ha), black_box(hb)) };
        black_box(hc);
        unsafe { __cobrust_coil_buffer_drop(hc) };
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        // SAFETY: as above.
        let hc = unsafe { __cobrust_coil_buffer_add(black_box(ha), black_box(hb)) };
        black_box(hc);
        unsafe { __cobrust_coil_buffer_drop(hc) };
        samples.push(t0.elapsed().as_nanos() as f64);
    }

    // Free the two input handles exactly once (outside the timed region).
    // SAFETY: each was Box::into_raw'd once via `into_handle`; freed once here.
    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
    }
    summarize(samples)
}

// =====================================================================
// T1 — Python numpy (the ergonomics baseline) via subprocess.
// =====================================================================

/// Time `np.add(a, b)` in CPython over `iters` samples after `warmup`.
/// The script allocates the SAME ramp inputs ONCE (outside the loop),
/// collects N per-iteration `perf_counter_ns` samples, and prints them
/// one-per-line; the Rust side parses + medians them with the SAME
/// `summarize` used for T2/T3 (identical median definition cross-tier).
/// `np.add(a, b)` allocates a fresh result array each call (symmetric
/// with T2/T3's result alloc+free). Returns `None` on any failure (T1
/// self-skips).
fn bench_t1_numpy(python: &str, n: usize, iters: usize, warmup: usize) -> Option<Stats> {
    // The ramp formula MUST match `ramp_a`/`ramp_b` exactly.
    let script = format!(
        "import numpy as np, time, sys\n\
         n = {n}\n\
         iters = {iters}\n\
         warmup = {warmup}\n\
         idx = np.arange(n, dtype=np.float64)\n\
         a = idx * 0.5 + 1.0\n\
         b = idx * 0.25 - 3.0\n\
         assert a.dtype == np.float64 and b.dtype == np.float64\n\
         for _ in range(warmup):\n\
         \x20   c = np.add(a, b)\n\
         out = []\n\
         for _ in range(iters):\n\
         \x20   t0 = time.perf_counter_ns()\n\
         \x20   c = np.add(a, b)\n\
         \x20   t1 = time.perf_counter_ns()\n\
         \x20   out.append(t1 - t0)\n\
         sys.stdout.write('\\n'.join(str(x) for x in out))\n"
    );
    let out = Command::new(python)
        .arg("-c")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    if !out.status.success() {
        eprintln!(
            "[T1 numpy] subprocess failed for n={n}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let samples: Vec<f64> = text
        .lines()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .collect();
    if samples.len() != iters {
        eprintln!(
            "[T1 numpy] expected {iters} samples for n={n}, parsed {}",
            samples.len()
        );
        return None;
    }
    Some(summarize(samples))
}

// =====================================================================
// Driver.
// =====================================================================

fn env_sizes() -> Vec<usize> {
    std::env::var("COIL_BENCH_SIZES")
        .ok()
        .map(|s| {
            s.split(',')
                .filter_map(|x| x.trim().parse::<usize>().ok())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec![100, 10_000, 1_000_000])
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(default)
}

fn main() {
    let sizes = env_sizes();
    let iters = env_usize("COIL_BENCH_ITERS", 201);
    let warmup = env_usize("COIL_BENCH_WARMUP", 50);

    let python = pick_python();

    println!("# coil element-wise-add 3-tier benchmark");
    println!("# methodology: docs/agent/benchmarks/README.md");
    println!("BENCH_OP=elementwise_add_f64");
    println!("BENCH_ITERS={iters}");
    println!("BENCH_WARMUP={warmup}");
    println!("BENCH_METRIC=median_ns_per_op");
    match &python {
        Some(p) => println!("T1_PYTHON={p}"),
        None => println!("T1_PYTHON=SKIPPED_no_numpy"),
    }
    println!();

    // Human-readable header.
    println!(
        "{:>10} | {:>14} | {:>14} | {:>14} | {:>10} | {:>10}",
        "size", "T1 numpy ns", "T2 ndarray ns", "T3 coil ns", "T3/T2", "T3/T1"
    );
    println!("{}", "-".repeat(86));

    for &n in &sizes {
        let t2 = bench_t2_ndarray(n, iters, warmup);
        let t3 = bench_t3_coil(n, iters, warmup);
        let t1 = python
            .as_ref()
            .and_then(|p| bench_t1_numpy(p, n, iters, warmup));

        let t3_over_t2 = t3.median_ns / t2.median_ns;
        let t3_over_t1 = t1.as_ref().map(|t| t3.median_ns / t.median_ns);

        // Human row.
        let t1_disp = t1
            .as_ref()
            .map_or_else(|| "SKIP".to_string(), |t| format!("{:.1}", t.median_ns));
        let t3_over_t1_disp = t3_over_t1.map_or_else(|| "—".to_string(), |r| format!("{r:.3}"));
        println!(
            "{n:>10} | {t1_disp:>14} | {:>14.1} | {:>14.1} | {t3_over_t2:>10.3} | {t3_over_t1_disp:>10}",
            t2.median_ns, t3.median_ns
        );

        // Machine-readable KEY=value lines (grep-able per size + tier).
        println!("T2_MEDIAN_NS_N{n}={:.1}", t2.median_ns);
        println!("T2_MEAN_NS_N{n}={:.1}", t2.mean_ns);
        println!("T2_MIN_NS_N{n}={:.1}", t2.min_ns);
        println!("T2_NS_PER_ELEM_N{n}={:.4}", t2.median_ns / n as f64);
        println!("T3_MEDIAN_NS_N{n}={:.1}", t3.median_ns);
        println!("T3_MEAN_NS_N{n}={:.1}", t3.mean_ns);
        println!("T3_MIN_NS_N{n}={:.1}", t3.min_ns);
        println!("T3_NS_PER_ELEM_N{n}={:.4}", t3.median_ns / n as f64);
        println!("T3_OVER_T2_N{n}={t3_over_t2:.4}");
        if let Some(t1) = &t1 {
            println!("T1_MEDIAN_NS_N{n}={:.1}", t1.median_ns);
            println!("T1_MEAN_NS_N{n}={:.1}", t1.mean_ns);
            println!("T1_MIN_NS_N{n}={:.1}", t1.min_ns);
            println!("T1_NS_PER_ELEM_N{n}={:.4}", t1.median_ns / n as f64);
            if let Some(r) = t3_over_t1 {
                println!("T3_OVER_T1_N{n}={r:.4}");
            }
        } else {
            println!("T1_MEDIAN_NS_N{n}=SKIPPED");
        }
        // Sample count sanity (honesty rule b: report N).
        println!("SAMPLES_N{n}={}", t3.n);
        println!();
    }

    println!("# done. T3/T2 is the diagnostic axis (.cb wrapping vs raw-ndarray ceiling);");
    println!("# T3/T1 is the headline (Cobrust coil vs Python numpy).");
}
