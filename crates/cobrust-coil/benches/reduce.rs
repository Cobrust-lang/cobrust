//! 3-tier full-array reduction (`mean`, f64) performance benchmark for
//! cobrust-coil.
//!
//! The THIRD increment of the Cobrust performance-benchmark suite (after
//! `elementwise_add` and `matmul`), measuring `np.mean(a)` — a full-array
//! scalar reduction over `coil.Buffer` (the `coil.mean(a) -> f64` aggregate,
//! Stream W P0, ADR-0072 §"coil deep operator/index"). Same methodology +
//! honesty rules as the elementwise / matmul benches; methodology source of
//! truth: `docs/agent/benchmarks/README.md`.
//!
//! ## Why this op is scientifically interesting (the matmul contrast)
//!
//! A reduction is **O(N)-compute → O(1)-output**: it reads every element but
//! returns a single `f64` scalar. Unlike `a + b` (which allocates an O(N)
//! result `Buffer` the scope drops) and `a @ b` (an O(N²) result the scope
//! drops), `coil.mean(a)` has **NO output array to marshal across the FFI
//! boundary** — `__cobrust_coil_mean` returns the `f64` by value in a
//! register. HYPOTHESIS: therefore `T3/T2` should be ~1.0 (coil sitting AT the
//! raw-`ndarray` reduction ceiling), in CONTRAST to matmul's `T3/T2 > 1` gap,
//! which was entirely output-array marshalling (`coil-matmul.md` §4.3). This
//! contrast — that the wrapping tax was the OUTPUT copy, not the input cross or
//! the kernel — is the headline insight of this report. (T3 still pays the FFI
//! call-in + the null-check; the test is whether *that alone* is ~free.)
//!
//! ## The three tiers (same op, same sizes, same f64 dtype, RUNTIME-ONLY)
//!
//! - **T1 — Python numpy** (the ergonomics baseline). A subprocess
//!   `python3 -c` times `np.mean(a)` over N iterations with
//!   `time.perf_counter_ns()`, after a warm-up. numpy's `mean` is a
//!   C/SIMD-backed pairwise sum, so this is "what a Python user already gets".
//!   Self-SKIPS when no `python3` with numpy is found (e.g. on CI) — the T2/T3
//!   Rust tiers still run and the report records the skip.
//! - **T2 — raw Rust `coil::mean_scalar`** (the performance CEILING). Times
//!   `coil::mean_scalar(&a)` on a `coil::Array` (an `ndarray::ArrayD<f64>`
//!   under the hood) — the EXACT kernel the C-ABI shim calls internally
//!   (`__cobrust_coil_mean` → `mean_scalar` → `reduce::mean` → ndarray's
//!   `.mean()`), with NO C-ABI / Cobrust-handle layer. This is the best a Rust
//!   program can do for this reduction on coil's backend; T3 cannot beat it,
//!   only approach it. (`mean_scalar` is a free function, NOT a method — it
//!   borrows `&Array` and returns `f64`; importable at the coil crate root.)
//! - **T3 — Cobrust `coil`** (the `.cb`-WRAPPING cost). Times the C-ABI shim
//!   `__cobrust_coil_mean(a) -> f64` — the exact symbol a compiled `.cb`
//!   program binds onto for `coil.mean(a)`. CRUCIALLY: a reduction returns a
//!   scalar, so — unlike the add/matmul benches — there is **NO result
//!   `__cobrust_coil_buffer_drop`** in the timed region. The timed region is
//!   the single FFI call (cross-in + null-check + borrow + `mean_scalar` +
//!   scalar-return). This is the whole point of the bench (see module header).
//!
//! ## The diagnostic axis
//!
//! - **T3 / T2** (coil C-ABI vs raw `coil::mean_scalar`) is the MOST diagnostic
//!   ratio: does the `.cb` wrapping (the FFI cross + null-check) PRESERVE the
//!   raw-Rust ceiling or erode it? With NO output marshalling, the HYPOTHESIS
//!   is `≈ 1.0` — the FFI cross alone is near-free for a by-value-scalar return.
//! - **T3 / T1** (coil vs numpy) is the headline "Cobrust vs Python" number.
//!   < 1.0 means coil is faster than numpy; > 1.0 means slower.
//!
//! ## Honesty rules (§5.3 — enforced; the report restates them)
//!
//! (a) RUNTIME-ONLY. No compile time anywhere. For every tier the INPUT array
//!     is allocated ONCE, OUTSIDE the timed region. A reduction has NO result
//!     array, so — unlike add/matmul — there is no per-op result alloc/free in
//!     the timed region for ANY tier (numpy `np.mean` returns a 0-d scalar;
//!     `mean_scalar` returns an `f64`; `__cobrust_coil_mean` returns an `f64`).
//!     This keeps the three tiers symmetric: all three time pure O(N)
//!     reduce-to-scalar work, with no output marshalling on either side.
//! (b) WARM-UP then MEDIAN (never mean) over N per-iter samples. We collect N
//!     individual `perf_counter`/`Instant` samples per tier and report the true
//!     median ns/op + ns/element. (Mean is reported too, for transparency, but
//!     the headline metric is the median.)
//! (c) SAME WORK. Same array length, same f64 dtype, same `mean(a)` reduction
//!     semantics across all three tiers; the input is a deterministic ramp so
//!     no constant-folding and identical values cross-tier (numpy re-derives
//!     the SAME ramp via `np.arange`). A CORRECTNESS GUARD asserts all three
//!     tiers compute the SAME mean (within f64 eps) BEFORE the timed loop —
//!     if they did not, the ratios would be meaningless.
//! (d) HARDWARE-TAGGED. The wrapper script captures CPU / cores / OS / rustc
//!     into the report. These are dev-laptop numbers (no fixed CPU governor /
//!     thermal control) — indicative, not a controlled rig.
//! (e) REPRODUCIBLE. One entrypoint re-runs everything:
//!     `cargo bench -p cobrust-coil --bench reduce`
//!     (or `scripts/bench/coil_mean.sh` for the hw-tagged report). Sizes +
//!     iters are overridable via env (see below) but default to the committed
//!     sweep.
//!
//! ## Output
//!
//! `KEY=value` lines on stdout so CI / scripts can grep specific numbers
//! (`T3_OVER_T2_N1000000=`, `T3_MEDIAN_NS_N100=`, ...). A human-readable table
//! is printed alongside.
//!
//! ## Tuning (optional; defaults are the committed sweep)
//!
//! - `COIL_REDUCE_SIZES` — comma-separated array sizes (default
//!   `100,10000,1000000`).
//! - `COIL_REDUCE_ITERS` — measured iterations per size (default `201`; odd so
//!   the median is a single sample, not a 2-sample average). Same as the
//!   elementwise bench (a reduction is O(N), like an element-wise pass).
//! - `COIL_REDUCE_WARMUP` — warm-up iterations per size (default `50`; matches
//!   the matmul bench's stabilised default — a cold capture on an unpinned
//!   laptop can read a spurious `T3 < T2`).

// This is a `harness = false` bench binary (a plain `fn main`), so it is
// allowed to print and to use unwrap/expect on its own controlled inputs —
// mirrors `elementwise_add.rs` / `matmul.rs`'s allow-set.
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
// allow + the elementwise / matmul benches' sibling allow.
#![allow(clippy::cast_ptr_alignment)]

use std::hint::black_box;
use std::process::{Command, Stdio};
use std::time::Instant;

use coil::Array;
use coil::array_f64;
use coil::cabi::{__cobrust_coil_buffer_drop, __cobrust_coil_mean};
use coil::mean_scalar;

// =====================================================================
// Stdlib ABI stubs.
//
// `coil`'s `cabi` shims declare three cross-crate stdlib externs
// (`__cobrust_panic` / `__cobrust_list_new` / `__cobrust_list_set`) that
// are normally link-resolved from `libcobrust_stdlib.a` only at `.cb`-link
// time, NOT into this bench binary. We provide minimal stubs so the binary
// links — identical to `elementwise_add.rs` / `matmul.rs` + the coil
// integration-test corpora. The benchmark only ever calls
// `__cobrust_coil_mean` on a live, non-null f64 Buffer handle, which never
// reaches the `coil_panic` / list-marshal paths, so these stubs are never
// invoked during the timed region; they exist purely to satisfy the linker.
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
// Python (numpy) discovery — mirrors `elementwise_add.rs` / `matmul.rs`.
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
// mean is reported alongside for transparency. (Identical to the
// elementwise / matmul benches' `summarize`.)
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
// Inputs. A deterministic ramp (NOT all-zeros / all-ones) so the reduction
// does real work, cannot be constant-folded, and is bit-identical across
// T2 and T3 (and re-derivable in numpy via the same formula). Built ONCE
// per size, OUTSIDE every timed region (honesty rule a).
// =====================================================================

/// The shared input formula: `a[i] = i * 0.5 + 1.0`. Used identically by T2
/// (Vec → coil Array), T3 (Vec → coil Array → handle), and T1 (numpy
/// `np.arange` arithmetic) so all three tiers reduce the SAME values.
fn ramp_a(n: usize) -> Vec<f64> {
    (0..n).map(|i| i as f64 * 0.5 + 1.0).collect()
}

/// The closed-form expected mean of `ramp_a(n)`: the mean of
/// `{i*0.5 + 1.0 : i in 0..n}` is `0.5 * (n-1)/2 + 1.0`. Used only by the
/// pre-timing correctness guard, as an independent third opinion on top of
/// the cross-tier same-value assertion (so the guard cannot be satisfied by
/// two tiers sharing the SAME bug).
fn expected_mean(n: usize) -> f64 {
    if n == 0 {
        return f64::NAN;
    }
    0.5 * ((n - 1) as f64) / 2.0 + 1.0
}

// =====================================================================
// T2 — raw Rust `coil::mean_scalar` (the performance ceiling).
//
// `mean_scalar(&a)` is the EXACT kernel `__cobrust_coil_mean` calls (cabi.rs
// §344 → `mean_scalar` → `reduce::mean`), with NO C-ABI / handle layer. The
// input `Array` is allocated once before the loop. The returned `f64` is
// `black_box`'d so the reduction is not dead-code-eliminated. There is NO
// result array to drop — a reduction is O(N)→O(1) — so the timed region is
// the pure reduce, symmetric with T1 (numpy 0-d scalar) and T3 (FFI scalar).
// =====================================================================

fn bench_t2_raw(n: usize, iters: usize, warmup: usize) -> Stats {
    let a: Array = array_f64(&ramp_a(n), &[n]).unwrap();

    for _ in 0..warmup {
        let m = mean_scalar(black_box(&a)).unwrap();
        black_box(m);
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let m = mean_scalar(black_box(&a)).unwrap();
        black_box(m);
        samples.push(t0.elapsed().as_nanos() as f64);
    }
    summarize(samples)
}

// =====================================================================
// T3 — Cobrust coil C-ABI (the .cb-wrapping cost).
// =====================================================================

/// Box an `Array` as an opaque `Buffer` handle — exactly what coil's
/// constructors (`__cobrust_coil_zeros` etc.) and the cabi tests
/// (`mean_of_mgrid_0_5_is_two`) do.
fn into_handle(arr: Array) -> *mut u8 {
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// Time the coil C-ABI `mean(a)` over `iters` samples after `warmup`. The one
/// input `Buffer` handle is built ONCE before the loop. The TIMED region is a
/// SINGLE `__cobrust_coil_mean(ha)` call — cross-in, null-check, borrow, the
/// `mean_scalar` kernel, and the by-value `f64` scalar return. CRUCIALLY there
/// is NO `__cobrust_coil_buffer_drop` in the timed region: a reduction returns
/// a scalar, not a fresh `Buffer`, so there is no per-op output to marshal or
/// free — the scientifically important contrast with the add/matmul benches
/// (see module header). The input handle is dropped once, after the loop,
/// OUTSIDE timing.
fn bench_t3_coil(n: usize, iters: usize, warmup: usize) -> Stats {
    let ha = into_handle(array_f64(&ramp_a(n), &[n]).unwrap());

    for _ in 0..warmup {
        // SAFETY: ha is a live, non-null f64 Buffer handle; mean only borrows
        // it and returns an f64 (no handle is produced, so nothing to free).
        let m = unsafe { __cobrust_coil_mean(black_box(ha)) };
        black_box(m);
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        // SAFETY: as above.
        let m = unsafe { __cobrust_coil_mean(black_box(ha)) };
        black_box(m);
        samples.push(t0.elapsed().as_nanos() as f64);
    }

    // Free the input handle exactly once (outside the timed region).
    // SAFETY: `ha` was Box::into_raw'd once via `into_handle`; freed once here.
    unsafe {
        __cobrust_coil_buffer_drop(ha);
    }
    summarize(samples)
}

// =====================================================================
// T1 — Python numpy (the ergonomics baseline) via subprocess.
// =====================================================================

/// Time `np.mean(a)` in CPython over `iters` samples after `warmup`. The
/// script allocates the SAME ramp input ONCE (outside the loop), collects N
/// per-iteration `perf_counter_ns` samples, and prints them one-per-line; the
/// Rust side parses + medians them with the SAME `summarize` used for T2/T3
/// (identical median definition cross-tier). `np.mean(a)` returns a 0-d scalar
/// (no result array to free), symmetric with T2/T3's scalar returns. Returns
/// `None` on any failure (T1 self-skips).
fn bench_t1_numpy(python: &str, n: usize, iters: usize, warmup: usize) -> Option<Stats> {
    // The ramp formula MUST match `ramp_a` exactly.
    let script = format!(
        "import numpy as np, time, sys\n\
         n = {n}\n\
         iters = {iters}\n\
         warmup = {warmup}\n\
         idx = np.arange(n, dtype=np.float64)\n\
         a = idx * 0.5 + 1.0\n\
         assert a.dtype == np.float64\n\
         for _ in range(warmup):\n\
         \x20   m = np.mean(a)\n\
         out = []\n\
         for _ in range(iters):\n\
         \x20   t0 = time.perf_counter_ns()\n\
         \x20   m = np.mean(a)\n\
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
// Correctness guard (honesty rule c). BEFORE any timing, assert all three
// tiers compute the SAME mean on the ramp — and that it matches the
// independent closed-form `expected_mean`. If the tiers disagreed, the
// ratios would be comparing different work and the report would be a lie.
// This is the assertion the audit mutation-proves, so it is real (it reads
// the actual T2 + T3 values and the numpy value, not constants).
// =====================================================================

/// Returns the numpy mean of `ramp_a(n)`, or `None` (numpy absent / failure).
fn numpy_mean_value(python: &str, n: usize) -> Option<f64> {
    let script = format!(
        "import numpy as np\n\
         idx = np.arange({n}, dtype=np.float64)\n\
         a = idx * 0.5 + 1.0\n\
         print(repr(float(np.mean(a))))\n"
    );
    let out = Command::new(python)
        .arg("-c")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .ok()
}

/// Assert T2 (`mean_scalar`) and T3 (`__cobrust_coil_mean`) agree with each
/// other, with the closed-form `expected_mean`, and — when numpy is present —
/// with numpy, all within a relative f64 tolerance. Panics on disagreement so
/// the bench cannot silently report ratios over non-identical work.
fn assert_same_value(python: Option<&str>, n: usize) {
    let a: Array = array_f64(&ramp_a(n), &[n]).unwrap();
    let t2 = mean_scalar(&a).unwrap();

    let ha = into_handle(array_f64(&ramp_a(n), &[n]).unwrap());
    // SAFETY: `ha` is a live, non-null f64 Buffer handle just constructed.
    let t3 = unsafe { __cobrust_coil_mean(ha) };
    // SAFETY: freed exactly once; constructed exactly once above.
    unsafe { __cobrust_coil_buffer_drop(ha) };

    let want = expected_mean(n);

    // Relative tolerance: pairwise vs naive summation can differ in the last
    // few ULPs at n=1e6, so compare relative to the magnitude (eps scaled by
    // value), not an absolute 1e-12 that a 250_000.75-magnitude mean blows.
    let rel = |x: f64, y: f64| (x - y).abs() <= 1e-9 * y.abs().max(1.0);

    assert!(
        rel(t2, want),
        "T2 mean_scalar disagrees with closed-form at n={n}: got {t2}, want {want}"
    );
    assert!(
        rel(t3, want),
        "T3 __cobrust_coil_mean disagrees with closed-form at n={n}: got {t3}, want {want}"
    );
    assert!(
        rel(t2, t3),
        "T2 and T3 compute DIFFERENT means at n={n}: T2={t2}, T3={t3} \
         (the tiers are not doing the same work — ratios would be meaningless)"
    );
    if let Some(py) = python
        && let Some(t1) = numpy_mean_value(py, n)
    {
        assert!(
            rel(t1, t3),
            "T1 numpy and T3 coil compute DIFFERENT means at n={n}: \
             numpy={t1}, coil={t3} (ramp formula drifted between tiers)"
        );
    }
}

// =====================================================================
// Driver.
// =====================================================================

fn env_sizes() -> Vec<usize> {
    std::env::var("COIL_REDUCE_SIZES")
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
    let iters = env_usize("COIL_REDUCE_ITERS", 201);
    // Warm-up = 50 (matches the matmul bench's stabilised default, raised from
    // the elementwise origin's 10). A reduction over 1e6 f64 streams enough
    // memory that an unpinned laptop mid-frequency-ramp can read a spurious
    // `T3 < T2`; 50 reaches steady state. See coil-mean.md §5.
    let warmup = env_usize("COIL_REDUCE_WARMUP", 50);

    let python = pick_python();

    println!("# coil full-array reduction (mean) 3-tier benchmark");
    println!("# methodology: docs/agent/benchmarks/README.md");
    println!("BENCH_OP=reduce_mean_f64");
    println!("BENCH_ITERS={iters}");
    println!("BENCH_WARMUP={warmup}");
    println!("BENCH_METRIC=median_ns_per_op");
    match &python {
        Some(p) => println!("T1_PYTHON={p}"),
        None => println!("T1_PYTHON=SKIPPED_no_numpy"),
    }
    println!();

    // CORRECTNESS GUARD (honesty rule c) — runs BEFORE any timing, on every
    // size, so a tier mismatch aborts the bench rather than producing a
    // meaningless ratio. Asserts T2 == T3 == closed-form (== numpy if present).
    for &n in &sizes {
        assert_same_value(python.as_deref(), n);
    }
    println!("SAME_VALUE_GUARD=passed_all_sizes");
    println!();

    // Human-readable header.
    println!(
        "{:>10} | {:>14} | {:>14} | {:>14} | {:>10} | {:>10}",
        "size", "T1 numpy ns", "T2 raw ns", "T3 coil ns", "T3/T2", "T3/T1"
    );
    println!("{}", "-".repeat(86));

    for &n in &sizes {
        let t2 = bench_t2_raw(n, iters, warmup);
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

    println!("# done. T3/T2 is the diagnostic axis (.cb wrapping vs raw-mean_scalar ceiling);");
    println!("# T3/T1 is the headline (Cobrust coil vs Python numpy). HYPOTHESIS: because a");
    println!("# reduction returns an f64 scalar (NO output array to marshal across the FFI");
    println!("# boundary, unlike add/matmul), T3/T2 should be ~1.0 — coil at the ndarray");
    println!("# reduction ceiling. The matmul T3/T2 gap was output marshalling; here there is");
    println!("# none. Confirm/refute from the T3_OVER_T2_N* lines above.");
}
