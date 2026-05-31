//! 3-tier matrix-multiply (`a @ b`, f64) performance benchmark for
//! cobrust-coil.
//!
//! The SECOND increment of the Cobrust performance-benchmark suite (after
//! `elementwise_add`), measuring the `.cb` `@` operator — numpy MATRIX
//! multiplication on `coil.Buffer` (ADR-0077 §"@-operator"). Same
//! methodology + honesty rules as the elementwise bench; methodology source
//! of truth: `docs/agent/benchmarks/README.md`.
//!
//! ## The three tiers (same op, same sizes, same f64 dtype, RUNTIME-ONLY)
//!
//! - **T1 — Python numpy** (the ergonomics baseline). A subprocess
//!   `python3 -c` times `a @ b` on two `N x N` f64 matrices over N iters with
//!   `time.perf_counter_ns()`, after a warm-up. numpy's `@` is **BLAS**-backed
//!   (OpenBLAS / Accelerate), so this is "what a Python user already gets" —
//!   and for matmul that is a TUNED, multi-threaded, blocked GEMM. Self-SKIPS
//!   when no `python3` with numpy is found (CI); T2/T3 still run.
//! - **T2 — raw Rust `ndarray`** (the performance CEILING *for this backend*).
//!   Times `a.dot(&b)` on `ndarray::Array2<f64>` — the EXACT kernel coil's
//!   `matmul` calls internally (`crates/cobrust-coil/src/linalg.rs` 2-D·2-D
//!   arm → `Array2::dot`). With no `linalg-backend`/BLAS feature, ndarray's
//!   `.dot` is its OWN pure-Rust GEMM (NOT BLAS). So T2 is the ceiling coil
//!   can reach, and the T2-vs-T1 gap is the **ndarray-GEMM-vs-BLAS** gap — NOT
//!   anything about Cobrust.
//! - **T3 — Cobrust `coil`** (the `.cb`-WRAPPING cost). Times the C-ABI shim
//!   `__cobrust_coil_buffer_matmul(a, b)` — the exact symbol a compiled `.cb`
//!   program binds onto for `a @ b` — plus the per-op result
//!   `__cobrust_coil_buffer_drop`. coil's `matmul` adds, around the SAME
//!   `Array2::dot` T2 calls, two O(N²) marshalling copies (`from_shape_vec`
//!   in, `iter().collect()` out) — which amortize against the O(N³) GEMM as N
//!   grows.
//!
//! ## The diagnostic axis
//!
//! - **T3 / T2** (coil vs raw ndarray) is the MOST diagnostic ratio: does the
//!   `.cb` wrapping (FFI cross + the O(N²) in/out marshalling copies + per-op
//!   result alloc) PRESERVE the ndarray-GEMM ceiling or erode it? > 1.0 means
//!   coil is slower than its own backend's ceiling by that factor; it should
//!   trend toward 1.0 as N grows (O(N²) tax / O(N³) work → 0).
//! - **T3 / T1** (coil vs numpy) is the headline "Cobrust vs Python" number.
//!   HONEST EXPECTATION: **> 1.0 (numpy WINS)** at non-trivial N, because numpy
//!   `@` is BLAS and coil's default backend (ndarray `.dot`) is not. This is
//!   the ndarray-vs-BLAS gap, and it MOTIVATES #157 (a pure-Rust BLAS-class
//!   linalg, e.g. `faer`) — it is NOT a cost of coil's `@`-operator wiring.
//!   We report it honestly and do NOT claim a win coil does not have.
//!
//! ## Honesty rules (§5.3 — enforced; the report restates them)
//!
//! (a) RUNTIME-ONLY. No compile time anywhere. For every tier the INPUT
//!     matrices are allocated ONCE, OUTSIDE the timed region. The op's RESULT
//!     allocation + free IS timed and IS documented as included (every `.cb`
//!     `a @ b` allocates a fresh `Buffer` the scope later drops); T1/T2 are
//!     made symmetric (numpy `a @ b` and ndarray `a.dot(&b)` each allocate +
//!     free one result per iter too).
//! (b) WARM-UP then MEDIAN (never mean) over N per-iter samples. Mean + min
//!     are reported alongside for transparency; the headline is the median.
//! (c) SAME WORK. Same `N x N` shape, same f64 dtype, same `a @ b` matmul
//!     semantics across all three tiers; inputs are a deterministic ramp so no
//!     constant-folding and identical values cross-tier.
//! (d) HARDWARE-TAGGED. The wrapper script captures CPU / cores / OS / rustc
//!     into the report. Dev-laptop numbers (no fixed governor / thermal
//!     control) — indicative, not a controlled rig.
//! (e) REPRODUCIBLE. One entrypoint re-runs everything:
//!     `cargo bench -p cobrust-coil --bench matmul`
//!     (or `scripts/bench/coil_matmul.sh` for the hw-tagged report). Sizes +
//!     iters are overridable via env (see below) but default to the committed
//!     sweep.
//!
//! ## Output
//!
//! `KEY=value` lines on stdout so CI / scripts can grep specific numbers
//! (`T3_OVER_T2_N256=`, `T3_MEDIAN_NS_N64=`, ...). A human-readable table is
//! printed alongside.
//!
//! ## Tuning (optional; defaults are the committed sweep)
//!
//! - `COIL_MATMUL_SIZES` — comma-separated SQUARE matrix dimensions (the `N`
//!   of an `N x N @ N x N`; default `16,64,256`).
//! - `COIL_MATMUL_ITERS` — measured iterations per size (default `51`; odd so
//!   the median is a single sample. Lower than the elementwise bench's 201
//!   because matmul is O(N³) — n=256 is ~33.5M FLOPs/iter).
//! - `COIL_MATMUL_WARMUP` — warm-up iterations per size (default `50`; matmul
//!   needs a longer ramp than elementwise — a cold capture can read `T3 < T2`).

// This is a `harness = false` bench binary (a plain `fn main`), so it is
// allowed to print and to use unwrap/expect on its own controlled inputs —
// mirrors `elementwise_add.rs`'s allow-set.
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
// allow + the elementwise bench's sibling allow.
#![allow(clippy::cast_ptr_alignment)]
// #157 faer tier: the `tf_over_t2` / `tf_over_t1` ratio bindings read as "too
// similar" to the existing `t3_over_t2` / `t3_over_t1`. The TF (faer) vs T3
// (coil) naming is the deliberate, load-bearing distinction of the spike, so
// the similarity is intended. These bindings exist in BOTH feature configs
// (`tf: Option<Stats>` is `None` without the feature), so the allow is
// unconditional to keep the default build's lint set passing too.
#![allow(clippy::similar_names)]

use std::hint::black_box;
use std::process::{Command, Stdio};
use std::time::Instant;

use coil::Array;
use coil::array_f64;
use coil::cabi::{__cobrust_coil_buffer_drop, __cobrust_coil_buffer_matmul};

use ndarray::Array2;

// =====================================================================
// Stdlib ABI stubs. coil's `cabi` shims declare three cross-crate stdlib
// externs (`__cobrust_panic` / `__cobrust_list_new` / `__cobrust_list_set`)
// that are normally link-resolved from `libcobrust_stdlib.a` only at
// `.cb`-link time, NOT into this bench binary. We provide minimal stubs so
// the binary links — identical to `elementwise_add.rs` + the coil corpora.
// The benchmark only ever calls `__cobrust_coil_buffer_matmul` on
// CONFORMABLE square inputs, which never reaches the `coil_panic` path, so
// these stubs are never invoked during the timed region; they exist purely
// to satisfy the linker.
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
// Python (numpy) discovery — mirrors `elementwise_add.rs`.
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
// elementwise bench's `summarize`.)
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
// Inputs. Two deterministic `N x N` ramps (NOT all-zeros / all-ones) so the
// matmul does real work, cannot be constant-folded, and is bit-identical
// across T2 and T3 (and re-derivable in numpy via the same formula). Built
// ONCE per size, OUTSIDE every timed region (honesty rule a).
//
// `a[r][c] = (r*N + c) * 0.5 + 1.0`, `b[r][c] = (r*N + c) * 0.25 - 3.0`,
// row-major flattened. Used identically by T2 (Vec → Array2), T3 (Vec → coil
// Array, shape [N,N]), and T1 (numpy `np.arange(N*N).reshape(N,N)` arithmetic).
// =====================================================================

fn ramp_a(n: usize) -> Vec<f64> {
    (0..n * n).map(|i| i as f64 * 0.5 + 1.0).collect()
}
fn ramp_b(n: usize) -> Vec<f64> {
    (0..n * n).map(|i| i as f64 * 0.25 - 3.0).collect()
}

// =====================================================================
// T2 — raw Rust ndarray (the performance ceiling for the ndarray backend).
//
// `a.dot(&b)` on `Array2<f64>` is the EXACT kernel coil's `matmul` 2-D·2-D
// arm calls (linalg.rs). Inputs allocated once before the loop. The owned
// result is `black_box`'d (so the matmul is not DCE'd) and dropped at the end
// of each loop body — symmetric with T3's result alloc+free and T1's `a @ b`.
// =====================================================================

fn bench_t2_ndarray(n: usize, iters: usize, warmup: usize) -> Stats {
    let a: Array2<f64> = Array2::from_shape_vec((n, n), ramp_a(n)).unwrap();
    let b: Array2<f64> = Array2::from_shape_vec((n, n), ramp_b(n)).unwrap();

    for _ in 0..warmup {
        let c = black_box(&a).dot(black_box(&b));
        black_box(&c);
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let c = black_box(&a).dot(black_box(&b));
        black_box(&c);
        // Free the result INSIDE the timed region — symmetric with T3 (which
        // times `__cobrust_coil_buffer_drop`) and T1 (numpy frees its result).
        drop(c);
        samples.push(t0.elapsed().as_nanos() as f64);
    }
    summarize(samples)
}

// =====================================================================
// TF — faer GEMM (#157 SPIKE, `--features coil-faer` only).
//
// Times the EXACT faer path coil's `matmul_f64_2d` runs under the feature:
// build two column-major `Mat<f64>` from the row-major ramps via
// `Mat::from_fn` (logical (i,j) indexing — layout-agnostic), `&a * &b` (the
// GEMM), and marshal the result back to a row-major `Vec<f64>` (the same
// O(N²) in/out copies the production kernel pays). This is the faer analogue
// of T2 (`a.dot(&b)`): it isolates the faer BACKEND so TF/T2 = the faer-vs-
// ndarray-GEMM speedup and TF/T1 = the faer-vs-numpy(Accelerate) residual —
// the number the survey (§3.3/§6.3 RISK #1) could not retrieve.
//
// Built ONCE per size outside the timed region; the result Mat is dropped
// inside the loop (symmetric with T2's `drop(c)` and T1's numpy result free).
// =====================================================================

#[cfg(feature = "coil-faer")]
fn bench_faer(n: usize, iters: usize, warmup: usize) -> Stats {
    use faer::Mat;

    let a = ramp_a(n);
    let b = ramp_b(n);
    // Row-major ramp -> faer Mat (column-major storage; from_fn is logical).
    let a_mat: Mat<f64> = Mat::from_fn(n, n, |i, j| a[i * n + j]);
    let b_mat: Mat<f64> = Mat::from_fn(n, n, |i, j| b[i * n + j]);

    // Marshal the (n,n) faer result back to a row-major Vec<f64> — the same
    // O(N²) copy-out the production kernel does, so TF is honest about the
    // marshalling the faer path actually pays (NOT just the bare GEMM).
    let marshal_out = |c: &Mat<f64>| -> Vec<f64> {
        let mut out = vec![0.0_f64; n * n];
        for i in 0..n {
            for j in 0..n {
                out[i * n + j] = *c.get(i, j);
            }
        }
        out
    };

    for _ in 0..warmup {
        let c = black_box(&a_mat) * black_box(&b_mat);
        black_box(marshal_out(&c));
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let c = black_box(&a_mat) * black_box(&b_mat);
        let out = marshal_out(&c);
        black_box(&out);
        drop(c);
        drop(out);
        samples.push(t0.elapsed().as_nanos() as f64);
    }
    summarize(samples)
}

// =====================================================================
// T3 — Cobrust coil C-ABI (the .cb-wrapping cost).
// =====================================================================

/// Box an `Array` as an opaque `Buffer` handle — exactly what coil's
/// constructors and the corpus tests do.
fn into_handle(arr: Array) -> *mut u8 {
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// Time the coil C-ABI `a @ b` over `iters` samples after `warmup`. The two
/// input `Buffer` handles (each an `N x N` f64 `Array`) are built ONCE before
/// the loop. The TIMED region is `__cobrust_coil_buffer_matmul(ha, hb)`
/// followed by `__cobrust_coil_buffer_drop(result)` — the full, honest per-op
/// cost a `.cb` program pays: cross-in, the matmul kernel (the SAME
/// `Array2::dot` T2 calls, plus coil's two O(N²) in/out marshalling copies),
/// cross-out, and the result free. The input handles are dropped once, after
/// the loop, OUTSIDE timing.
fn bench_t3_coil(n: usize, iters: usize, warmup: usize) -> Stats {
    let ha = into_handle(array_f64(&ramp_a(n), &[n, n]).unwrap());
    let hb = into_handle(array_f64(&ramp_b(n), &[n, n]).unwrap());

    for _ in 0..warmup {
        // SAFETY: ha/hb are live, conformable (N x N) f64 Buffer handles; the
        // result is freed immediately via the drop shim.
        let hc = unsafe { __cobrust_coil_buffer_matmul(black_box(ha), black_box(hb)) };
        black_box(hc);
        unsafe { __cobrust_coil_buffer_drop(hc) };
    }

    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        // SAFETY: as above.
        let hc = unsafe { __cobrust_coil_buffer_matmul(black_box(ha), black_box(hb)) };
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

/// Time `a @ b` in CPython over `iters` samples after `warmup`. The script
/// allocates the SAME ramp matrices ONCE (outside the loop), collects N
/// per-iteration `perf_counter_ns` samples, and prints them one-per-line; the
/// Rust side parses + medians them with the SAME `summarize` used for T2/T3.
/// `a @ b` allocates a fresh result array each call (symmetric with T2/T3's
/// result alloc+free). numpy `@` dispatches to BLAS GEMM. Returns `None` on
/// any failure (T1 self-skips).
fn bench_t1_numpy(python: &str, n: usize, iters: usize, warmup: usize) -> Option<Stats> {
    // The ramp formula MUST match `ramp_a`/`ramp_b` exactly (row-major).
    let script = format!(
        "import numpy as np, time, sys\n\
         n = {n}\n\
         iters = {iters}\n\
         warmup = {warmup}\n\
         idx = np.arange(n * n, dtype=np.float64)\n\
         a = (idx * 0.5 + 1.0).reshape(n, n)\n\
         b = (idx * 0.25 - 3.0).reshape(n, n)\n\
         assert a.dtype == np.float64 and b.dtype == np.float64\n\
         assert a.shape == (n, n) and b.shape == (n, n)\n\
         for _ in range(warmup):\n\
         \x20   c = a @ b\n\
         out = []\n\
         for _ in range(iters):\n\
         \x20   t0 = time.perf_counter_ns()\n\
         \x20   c = a @ b\n\
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
    std::env::var("COIL_MATMUL_SIZES")
        .ok()
        .map(|s| {
            s.split(',')
                .filter_map(|x| x.trim().parse::<usize>().ok())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec![16, 64, 256])
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
    let iters = env_usize("COIL_MATMUL_ITERS", 51);
    // Warm-up = 50 (not 10): matmul drives multi-threaded BLAS (T1) +
    // ndarray-GEMM (T2) that contend on an unpinned laptop and need the CPU at
    // steady frequency. A cold warm-up=10 capture once produced an IMPOSSIBLE
    // `T3 < T2` (coil "faster" than the bare `Array2::dot` it wraps) — a
    // mid-frequency-ramp artifact. 50 stabilizes it (see coil-matmul.md §5).
    let warmup = env_usize("COIL_MATMUL_WARMUP", 50);

    let python = pick_python();

    println!("# coil matrix-multiply 3-tier benchmark");
    println!("# methodology: docs/agent/benchmarks/README.md");
    println!("BENCH_OP=matmul_f64_NxN");
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
        "{:>8} | {:>14} | {:>14} | {:>14} | {:>10} | {:>10}",
        "N (NxN)", "T1 numpy ns", "T2 ndarray ns", "T3 coil ns", "T3/T2", "T3/T1"
    );
    println!("{}", "-".repeat(86));

    for &n in &sizes {
        let t2 = bench_t2_ndarray(n, iters, warmup);
        let t3 = bench_t3_coil(n, iters, warmup);
        let t1 = python
            .as_ref()
            .and_then(|p| bench_t1_numpy(p, n, iters, warmup));

        // TF — faer tier (#157). Only present under `--features coil-faer`.
        #[cfg(feature = "coil-faer")]
        let tf = Some(bench_faer(n, iters, warmup));
        #[cfg(not(feature = "coil-faer"))]
        let tf: Option<Stats> = None;

        let t3_over_t2 = t3.median_ns / t2.median_ns;
        let t3_over_t1 = t1.as_ref().map(|t| t3.median_ns / t.median_ns);
        // faer ratios — the spike's headline numbers (gap-closure check):
        // TF/T2 = faer vs ndarray-GEMM backend; TF/T1 = faer vs numpy-BLAS.
        let tf_over_t2 = tf.as_ref().map(|t| t.median_ns / t2.median_ns);
        let tf_over_t1 = tf
            .as_ref()
            .zip(t1.as_ref())
            .map(|(tf, t1)| tf.median_ns / t1.median_ns);

        // Number of f64 multiply-adds per matmul = N^3 (NxN @ NxN).
        let flops = (n as f64).powi(3);

        // Human row.
        let t1_disp = t1
            .as_ref()
            .map_or_else(|| "SKIP".to_string(), |t| format!("{:.1}", t.median_ns));
        let t3_over_t1_disp = t3_over_t1.map_or_else(|| "—".to_string(), |r| format!("{r:.3}"));
        println!(
            "{n:>8} | {t1_disp:>14} | {:>14.1} | {:>14.1} | {t3_over_t2:>10.3} | {t3_over_t1_disp:>10}",
            t2.median_ns, t3.median_ns
        );

        // Machine-readable KEY=value lines (grep-able per size + tier).
        println!("T2_MEDIAN_NS_N{n}={:.1}", t2.median_ns);
        println!("T2_MEAN_NS_N{n}={:.1}", t2.mean_ns);
        println!("T2_MIN_NS_N{n}={:.1}", t2.min_ns);
        println!("T2_NS_PER_FLOP_N{n}={:.6}", t2.median_ns / flops);
        println!("T3_MEDIAN_NS_N{n}={:.1}", t3.median_ns);
        println!("T3_MEAN_NS_N{n}={:.1}", t3.mean_ns);
        println!("T3_MIN_NS_N{n}={:.1}", t3.min_ns);
        println!("T3_NS_PER_FLOP_N{n}={:.6}", t3.median_ns / flops);
        println!("T3_OVER_T2_N{n}={t3_over_t2:.4}");
        if let Some(t1) = &t1 {
            println!("T1_MEDIAN_NS_N{n}={:.1}", t1.median_ns);
            println!("T1_MEAN_NS_N{n}={:.1}", t1.mean_ns);
            println!("T1_MIN_NS_N{n}={:.1}", t1.min_ns);
            println!("T1_NS_PER_FLOP_N{n}={:.6}", t1.median_ns / flops);
            if let Some(r) = t3_over_t1 {
                println!("T3_OVER_T1_N{n}={r:.4}");
            }
        } else {
            println!("T1_MEDIAN_NS_N{n}=SKIPPED");
        }
        // TF — faer tier (#157 spike). Only emitted under `--features
        // coil-faer`; the default build prints nothing here, so existing
        // grep keys (T1_/T2_/T3_) are byte-stable.
        if let Some(tf) = &tf {
            println!("TF_MEDIAN_NS_N{n}={:.1}", tf.median_ns);
            println!("TF_MEAN_NS_N{n}={:.1}", tf.mean_ns);
            println!("TF_MIN_NS_N{n}={:.1}", tf.min_ns);
            println!("TF_NS_PER_FLOP_N{n}={:.6}", tf.median_ns / flops);
            if let Some(r) = tf_over_t2 {
                // < 1.0 = faer FASTER than ndarray-GEMM (closes the T2/T1 gap).
                println!("TF_OVER_T2_N{n}={r:.4}");
            }
            if let Some(r) = tf_over_t1 {
                // The headline: faer vs numpy-Accelerate. ~1.0 = parity (gap
                // CLOSED); > 1.0 = numpy still ahead by that factor.
                println!("TF_OVER_T1_N{n}={r:.4}");
            }
        }
        // Sample count sanity (honesty rule b: report N).
        println!("SAMPLES_N{n}={}", t3.n);
        println!();
    }

    println!("# done. T3/T2 is the diagnostic axis (.cb wrapping vs ndarray-GEMM ceiling);");
    println!("# T3/T1 is the headline (coil vs numpy). HONEST: T3/T1 > 1 is EXPECTED at");
    println!("# non-trivial N — numpy `@` is BLAS, coil's default backend (ndarray .dot)");
    println!("# is NOT. The gap is ndarray-vs-BLAS, not coil's wrapping; it motivates #157.");
}
