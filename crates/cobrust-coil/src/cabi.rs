//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import coil` and calls `coil.zeros(n)` /
//! `coil.ones(n)` / `coil.eye(n)` / `coil.print_buffer(b)` (ADR-0072
//! 8/8 first proof — the EIGHTH and final cobra-batch ecosystem
//! module; completes the workspace-vendored ecosystem chain).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libcoil.a` after `libcobrust_stdlib.a`.
//!
//! # ABI
//!
//! - **Handles** (`Buffer`) cross as opaque `*mut u8` pointers,
//!   `Box::into_raw`'d on construction and `Box::from_raw`'d exactly
//!   once at the `.cb` scope-exit drop. Identical to den/molt/strike's
//!   value-handle pattern. The boxed payload is a `coil::Array` (the
//!   existing tagged-union over `ndarray::ArrayD<T>`); the `Buffer`
//!   surface name is the `.cb`-side handle alias.
//! - **Scalars** (`i64`) cross by value.
//! - **No strings on this surface**: the print side prints to stdout
//!   directly via `println!` on the Rust side (the user's intent is
//!   the printed bytes, not a `.cb`-owned `Str` buffer for the first
//!   proof). A future `Buffer.tolist() -> str` shape would lift the
//!   den-style `__cobrust_str_*` extern wiring per ADR-0072 Q5; the
//!   `build.rs` deferral flag is already in place for that extension.
//! - **No callbacks on this surface**: pure value-handle chain (mirrors
//!   den/molt/strike — NOT pit/hood's callback chain).
//!
//! # Ownership (ADR-0072 §5 prime risk)
//!
//! - `zeros`/`ones`/`eye` **return** freshly-Boxed handles the `.cb`
//!   caller owns; the caller's MIR drop schedule frees them once at
//!   scope exit via `__cobrust_coil_buffer_drop`.
//! - `print_buffer` **borrows** its handle arg (`&*(ptr as *const T)`)
//!   — it never reboxes or frees it.
//! - A `DROP_COUNT` instrument lets the test suite assert each handle
//!   is dropped exactly once (no leak, no double-free).
//!
//! # First-proof scope (ADR-0072 §"coil deep operator/index")
//!
//! Three constructors + one read method. Operator dispatch (`a + b`)
//! and index dispatch (`a[i]`) are EXPLICITLY DEFERRED to a sub-ADR
//! per ADR-0072 — those want their own design pass (the `EcoParam`
//! manifest shape doesn't yet model binary operators, and the
//! .cb-side BinOp dispatch needs a method-form lowering). Same scope
//! discipline as nest's first proof (str→str only; no structured
//! TOML value surface).

// C-ABI-boundary cast allows — mirror the den/hood/pit cabi pattern.
// The casts are intrinsic to the opaque-pointer / length ABI and are
// correct here:
// - `*mut u8 -> *mut Array`: the pointer was produced by `Box::into_raw`
//   (correctly aligned) and only ever cast back to its original type,
//   so the alignment-narrowing lint is a false positive.
// - `i64 <-> usize` length round-trips: shape lengths the `.cb` source
//   passes are non-negative and well under `usize::MAX` on the
//   64-bit targets the AOT backend supports. We clamp to `0` on a
//   negative `n` rather than panic.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_ptr_alignment)]
// ADR-0077 Q2 getitem: int/bool-dtype elements promote to f64 (the
// f64-only Phase-1 return contract). Same intrinsically-correct numpy
// i64→f64 promotion as `aggregates::scalar_to_f64`, whose file shares
// this allow.
#![allow(clippy::cast_precision_loss)]

use std::sync::atomic::{AtomicU64, Ordering};

use crate::aggregates::{
    all_scalar, any_scalar, argmax_scalar, argmin_scalar, max_scalar, mean_scalar, median_scalar,
    min_scalar, nanmean_scalar, nanstd_scalar, nansum_scalar, percentile_scalar, prod_scalar,
    ptp_scalar, split_first_chunk, std_scalar, var_scalar,
};
use crate::array::Array;
use crate::broadcast::broadcast_shape;
use crate::broadcast_extra::broadcast_to_1d;
use crate::constructors::{
    array_f64, eye as coil_eye, full as coil_full, linspace as coil_linspace,
    logspace as coil_logspace, ones as coil_ones, zeros as coil_zeros,
};
use crate::dtype::Dtype;
use crate::elementwise::{
    abs as coil_abs, cbrt as coil_cbrt, ceil as coil_ceil, clip as coil_clip, cos as coil_cos,
    cosh as coil_cosh, exp as coil_exp, exp2 as coil_exp2, floor as coil_floor, fmax as coil_fmax,
    fmin as coil_fmin, isfinite as coil_isfinite, isinf as coil_isinf, isnan as coil_isnan,
    log as coil_log, log2 as coil_log2, log10 as coil_log10, maximum as coil_maximum,
    minimum as coil_minimum, power as coil_power, round as coil_round, sign as coil_sign,
    sin as coil_sin, sinh as coil_sinh, sqrt as coil_sqrt, square as coil_square, tan as coil_tan,
    tanh as coil_tanh, trunc as coil_trunc,
};
use crate::grid::{mgrid_1d, ogrid_1d};
use crate::linalg::{det as linalg_det, inv as linalg_inv, solve as linalg_solve};
use crate::manipulate::{
    argsort as coil_argsort, concatenate as coil_concatenate, diff as coil_diff,
    flatnonzero as coil_flatnonzero, flatten as coil_flatten, flip as coil_flip,
    hstack as coil_hstack, ravel as coil_ravel, repeat as coil_repeat, roll as coil_roll,
    sort as coil_sort, tile as coil_tile, transpose as coil_transpose, unique as coil_unique,
    vstack as coil_vstack, where_select as coil_where,
};
use crate::print::array_repr;
use crate::reduce::{cumprod as coil_cumprod, cumsum as coil_cumsum};

// =====================================================================
// Cobrust stdlib ABI — declared here, resolved from libcobrust_stdlib.a
// at link time (ADR-0072 Q5 cross-crate binding pattern; no Rust dep —
// mirrors den's `__cobrust_str_*` extern block). ADR-0077 Q3 `a.shape`
// is coil's FIRST use of the stdlib `list[i64]` ABI: the shim allocates
// an owned `List<i64>` the `.cb` scope drops once.
// =====================================================================

unsafe extern "C" {
    /// Allocate a `List<i64>` with `len` zeroed slots (`len == cap`).
    /// `elem_size` is reserved (M12.x fixes the elem width at i64).
    fn __cobrust_list_new(elem_size: i64, len: i64) -> *mut u8;
    /// Write `list[i] = v` (out-of-bounds writes are silently dropped).
    fn __cobrust_list_set(list: *mut u8, i: i64, v: i64);
    /// Abort the process with a UTF-8 diagnostic (ADR-0077 Q4 panic-on-
    /// shape-mismatch — the same `__cobrust_panic` shim the codegen
    /// abort path uses; diverges, never returns).
    fn __cobrust_panic(ptr: *const u8, len: usize) -> !;
}

/// Abort the process via the stdlib `__cobrust_panic` shim with `msg`
/// (ADR-0077 Q4). Used by the Buffer operator shims on a non-broadcastable
/// shape pair (ADR-0077 Phase 3) — the operators return a bare `Buffer`
/// and an incompatible pair panics-and-aborts (matching numpy's raise, the
/// §2.5 closest honest behavior; a fallible `a.checked_add(b) -> Result`
/// escape is a later surface).
fn coil_panic(msg: &str) -> ! {
    // SAFETY: `msg` is a valid UTF-8 `&str`; `__cobrust_panic` reads
    // exactly `msg.len()` bytes at `msg.as_ptr()` and diverges.
    unsafe { __cobrust_panic(msg.as_ptr(), msg.len()) }
}

// =====================================================================
// Drop instrumentation (ADR-0072 §4 done-means 5 — drop-once evidence).
// =====================================================================

/// Total `Buffer` handle drops performed by the `_drop` shim this
/// process. Read by the test suite to assert no-leak / no-double-free.
pub static DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current `DROP_COUNT`. Test-only accessor.
#[must_use]
pub fn drop_count() -> u64 {
    DROP_COUNT.load(Ordering::SeqCst)
}

// =====================================================================
// coil C-ABI surface — Buffer constructors (handle-returning).
// =====================================================================

/// Clamp `n: i64` to a non-negative `usize`. The `.cb` source's
/// `coil.zeros(n)` signature is `i64 -> Buffer`; a negative `n` would
/// represent a programming error on the source side. The first proof
/// tolerates it by yielding an empty buffer rather than aborting (the
/// type signature already enforces `i64`; a `usize` constraint at the
/// type level is a tracked follow-up).
fn clamp_to_usize(n: i64) -> usize {
    if n < 0 { 0 } else { n as usize }
}

/// `coil.zeros(n) -> Buffer`. Allocate an `n`-element f64-zero 1-D
/// buffer.
///
/// Returns a freshly-Boxed `Array` handle the `.cb` caller owns; the
/// caller's scope-exit drop frees it via `__cobrust_coil_buffer_drop`.
///
/// # Safety
///
/// The returned pointer is an owned `Buffer` handle (boxed
/// `coil::Array`), freed once via `__cobrust_coil_buffer_drop`. Safe
/// to call concurrently — the underlying `ndarray::ArrayD<f64>::zeros`
/// is allocation-only with no shared state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_zeros(n: i64) -> *mut u8 {
    let n = clamp_to_usize(n);
    // unwrap is sound: the only error branch is the complex-dtype arm,
    // unreachable for Float64. Mirrors the den `connect` pattern of
    // returning a Boxed handle (we use unwrap-or-fall-back rather than
    // a sentinel null since the value path here is infallible by
    // construction).
    let arr = coil_zeros(&[n], Dtype::Float64)
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[n]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.ones(n) -> Buffer`. Allocate an `n`-element f64-one 1-D
/// buffer.
///
/// # Safety
///
/// As `__cobrust_coil_zeros`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_ones(n: i64) -> *mut u8 {
    let n = clamp_to_usize(n);
    let arr = coil_ones(&[n], Dtype::Float64).unwrap_or_else(|_| {
        Array::Float64(ndarray::ArrayD::<f64>::from_elem(
            ndarray::IxDyn(&[n]),
            1.0_f64,
        ))
    });
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.eye(n) -> Buffer`. Allocate the `n x n` f64 identity matrix
/// (`k=0` main-diagonal; the first-proof shape proves the chain
/// handles a non-1-D buffer too — drop discipline is shape-agnostic).
///
/// # Safety
///
/// As `__cobrust_coil_zeros`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_eye(n: i64) -> *mut u8 {
    let n = clamp_to_usize(n);
    let arr = coil_eye(n, None, 0, Dtype::Float64).unwrap_or_else(|_| {
        // Safe fallback: an empty 2-D buffer with the same dtype.
        Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0, 0])))
    });
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

// =====================================================================
// #145 gap-closure BATCH 11 (2026-06-02) — the spacing / value
// CONSTRUCTORS `linspace` / `logspace` / `full`. ALL-SCALAR-ARG
// producers (NO Buffer input) that allocate a fresh `Float64` 1-D
// `Buffer` the `.cb` caller owns + scope-exit drops. The mirror of the
// `coil.zeros(n)` / `coil.array1d2(a, b)` all-scalar-arg ctor shape:
// no borrow (nothing to free), fresh `Box::into_raw`, TOTAL (no trap —
// a `num`/`n <= 0` yields an EMPTY buffer, like `coil.zeros(-1)`).
// =====================================================================

/// `coil.linspace(start, stop, num) -> Buffer`. `num` evenly-spaced f64
/// samples over `[start, stop]` INCLUSIVE of `stop` (numpy's
/// `endpoint=True` default). `np.linspace(0, 1, 5) == [0, .25, .5, .75,
/// 1]`. `num == 1 -> [start]`; `num <= 0 -> []` (empty). The last sample
/// is pinned to `stop` bit-exactly (see `constructors::linspace`).
///
/// Returns a freshly-Boxed `Array` handle the `.cb` caller owns; freed
/// once via `__cobrust_coil_buffer_drop`.
///
/// # Safety
///
/// As `__cobrust_coil_zeros`. No borrow — an all-scalar-arg producer.
/// Safe to call concurrently (allocation-only, no shared state).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_linspace(start: f64, stop: f64, num: i64) -> *mut u8 {
    let num = clamp_to_usize(num);
    // `endpoint = true` (numpy default). map-or-fallback: the only error
    // branch in `linspace` is `array(...)`'s shape check, which cannot
    // fail for a freshly-sized `[num]` Float64 buffer.
    let arr = coil_linspace(start, stop, num, true, Dtype::Float64).map_or_else(
        |_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[num]))),
        |r| r.array,
    );
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.logspace(start, stop, num) -> Buffer`. `10 ** linspace(start,
/// stop, num)` — `num` samples spaced evenly on a base-10 log scale,
/// INCLUSIVE of the `stop` exponent. `np.logspace(0, 2, 3) == [1, 10,
/// 100]`. `num <= 0 -> []`.
///
/// # Safety
///
/// As `__cobrust_coil_linspace`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_logspace(start: f64, stop: f64, num: i64) -> *mut u8 {
    let num = clamp_to_usize(num);
    // base = 10.0, endpoint = true (numpy defaults).
    let arr = coil_logspace(start, stop, num, true, 10.0, Dtype::Float64)
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[num]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.full(n, value) -> Buffer`. A 1-D `Float64` buffer of `n` copies
/// of `value`. `np.full(3, 5.0) == [5, 5, 5]`. `n <= 0 -> []` (empty,
/// clamping a negative `n` to `0` like `coil.zeros`).
///
/// # Safety
///
/// As `__cobrust_coil_linspace`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_full(n: i64, value: f64) -> *mut u8 {
    let n = clamp_to_usize(n);
    // unwrap-or-fallback: the only error branch is the complex-dtype arm,
    // unreachable for Float64.
    let arr = coil_full(&[n], value, Dtype::Float64).unwrap_or_else(|_| {
        Array::Float64(ndarray::ArrayD::<f64>::from_elem(
            ndarray::IxDyn(&[n]),
            value,
        ))
    });
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

// =====================================================================
// coil C-ABI surface — Buffer read method (print_buffer).
// =====================================================================

/// `coil.print_buffer(b) -> i64`. Print the buffer's `array_repr` to
/// stdout (`array([0, 0, 0], dtype=float64)`-style — coil's existing
/// numpy-compatible repr per ADR-0013 §4). BORROWS the handle arg
/// (never frees it).
///
/// Returns `0` on success — a sentinel matching pit's
/// `app.route -> Ty::None` discipline (the call's intent is the
/// side-effect, not the return). `-1` is returned on a null receiver
/// (defensive; the typechecker guarantees non-null).
///
/// # Safety
///
/// `b` must be a live `Buffer` handle from one of `coil.zeros` /
/// `coil.ones` / `coil.eye` (not yet dropped).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_print_buffer(b: *mut u8) -> i64 {
    if b.is_null() {
        return -1;
    }
    // SAFETY: caller attests `b` is a live Buffer handle. We only
    // BORROW it — no rebox / free.
    let arr_ref: &Array = unsafe { &*b.cast::<Array>() };
    println!("{}", array_repr(arr_ref));
    0
}

// =====================================================================
// coil C-ABI surface — Buffer handle drop (mirror den/hood _drop).
// =====================================================================

/// Drop a `Buffer` handle. `Box::from_raw` + drop, exactly once, at
/// the `.cb` scope exit. Idempotent on null.
///
/// # Safety
///
/// `b` must be null or a `Buffer` handle from one of `coil.zeros` /
/// `coil.ones` / `coil.eye` / `coil.linspace` / `coil.logspace` /
/// `coil.full` / `coil.mgrid` / `coil.ogrid` / `coil.broadcast_to` /
/// `coil.split` (or any other coil Buffer-returning op) that has not
/// already been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_drop(b: *mut u8) {
    if b.is_null() {
        return;
    }
    // SAFETY: caller attests single, not-yet-dropped ownership. The
    // Boxed `Array` owns its inner `ndarray::ArrayD<T>` which in turn
    // owns its `Vec<T>`; dropping the Box reclaims the whole chain.
    drop(unsafe { Box::from_raw(b.cast::<Array>()) });
    DROP_COUNT.fetch_add(1, Ordering::SeqCst);
}

// =====================================================================
// Stream W P0 增量 (2026-05-29) — handle-returning grid + broadcast +
// split constructors.
// =====================================================================

/// `coil.mgrid(start, stop) -> Buffer` 1-D form. See `grid::mgrid_1d`.
///
/// # Safety
///
/// As `__cobrust_coil_zeros`. Returns a freshly-Boxed handle the
/// `.cb` caller owns; freed once via `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_mgrid(start: i64, stop: i64) -> *mut u8 {
    let arr = mgrid_1d(start, stop)
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.ogrid(start, stop) -> Buffer` 1-D form. See `grid::ogrid_1d`.
///
/// # Safety
///
/// As `__cobrust_coil_mgrid`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_ogrid(start: i64, stop: i64) -> *mut u8 {
    let arr = ogrid_1d(start, stop)
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.broadcast_to(a, n) -> Buffer` 1-D tile to `n`. See
/// `broadcast_extra::broadcast_to_1d`.
///
/// BORROWS its input handle (never frees it) — the caller's scope-
/// exit drop schedule still owns `a`. Returns a fresh handle.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped). The returned
/// pointer is a freshly-Boxed handle the `.cb` caller owns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_broadcast_to(a: *mut u8, n: i64) -> *mut u8 {
    if a.is_null() {
        let empty = Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0])));
        return Box::into_raw(Box::new(empty)).cast::<u8>();
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    let out = broadcast_to_1d(arr_ref, n)
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0]))));
    Box::into_raw(Box::new(out)).cast::<u8>()
}

/// `coil.split(a, n) -> Buffer` first-proof — first chunk of an n-way
/// `array_split`. See `aggregates::split_first_chunk`.
///
/// BORROWS its input handle. Returns a fresh handle.
///
/// # Safety
///
/// As `__cobrust_coil_broadcast_to`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_split(a: *mut u8, n: i64) -> *mut u8 {
    if a.is_null() {
        let empty = Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0])));
        return Box::into_raw(Box::new(empty)).cast::<u8>();
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    let out = split_first_chunk(arr_ref, n)
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[0]))));
    Box::into_raw(Box::new(out)).cast::<u8>()
}

// =====================================================================
// Stream W P0 增量 (2026-05-29) — scalar-returning aggregate reductions.
// =====================================================================

/// `coil.mean(a) -> f64`. BORROWS the handle arg. NaN on empty input.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_mean(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    mean_scalar(arr_ref).unwrap_or(f64::NAN)
}

/// `coil.median(a) -> f64`. BORROWS the handle arg. NaN on empty input.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_median(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    median_scalar(arr_ref).unwrap_or(f64::NAN)
}

/// `coil.std(a) -> f64`. Population standard deviation (ddof=0).
/// BORROWS the handle arg.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_std(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    std_scalar(arr_ref).unwrap_or(f64::NAN)
}

/// `coil.var(a) -> f64`. Population variance (ddof=0). BORROWS the
/// handle arg.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_var(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    var_scalar(arr_ref).unwrap_or(f64::NAN)
}

// =====================================================================
// #145 statistics gap-closure (2026-06-01) — NaN-aware + spread scalar
// aggregates (`ptp` / `nansum` / `nanmean` / `nanstd`, single-Buffer →
// f64) plus `percentile` (Buffer + f64 → f64, the FIRST coil aggregate
// taking a scalar arg beside the handle). All BORROW the handle arg.
// =====================================================================

/// `coil.ptp(a) -> f64`. Peak-to-peak (`max - min`). BORROWS the handle.
/// NaN-propagating; `NaN` on empty input.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_ptp(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    ptp_scalar(arr_ref).unwrap_or(f64::NAN)
}

/// `coil.nansum(a) -> f64`. Sum treating NaN as zero. BORROWS the
/// handle. `0.0` on all-NaN / empty input (matches numpy `np.nansum`).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_nansum(a: *mut u8) -> f64 {
    if a.is_null() {
        return 0.0;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    nansum_scalar(arr_ref).unwrap_or(0.0)
}

/// `coil.nanmean(a) -> f64`. Arithmetic mean ignoring NaN. BORROWS the
/// handle. `NaN` on all-NaN / empty input.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_nanmean(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    nanmean_scalar(arr_ref).unwrap_or(f64::NAN)
}

/// `coil.nanstd(a) -> f64`. Population std (ddof=0) ignoring NaN.
/// BORROWS the handle. `NaN` on all-NaN / empty input.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_nanstd(a: *mut u8) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    nanstd_scalar(arr_ref).unwrap_or(f64::NAN)
}

/// `coil.percentile(a, q) -> f64`. The `q`-th percentile (`q` in
/// `[0, 100]`, `linear` interpolation). BORROWS the handle; `q` crosses
/// by value. NaN-propagating; `NaN` on empty input; `q` clamped to
/// `[0, 100]`.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_percentile(a: *mut u8, q: f64) -> f64 {
    if a.is_null() {
        return f64::NAN;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    percentile_scalar(arr_ref, q).unwrap_or(f64::NAN)
}

// =====================================================================
// #145 gap-closure BATCH 5 (2026-06-01) — SCALAR-returning reductions.
// `argmin` / `argmax` return an `i64` (the flat C-order index); `any` /
// `all` return a `bool` (the C-ABI `bool`, i.e. an i8 that lands in the
// `.cb` `Ty::Bool` local via the i1 extern — the SAME shape as
// `fang.verify_password`). All BORROW the handle (no rebox / free).
//
// EMPTY-input contract (the load-bearing edge): numpy `argmin`/`argmax`
// RAISE `ValueError` on an empty array — coil CANNOT raise across the
// C-ABI, so the shim `coil_panic`s (a clean process abort via the stdlib
// `__cobrust_panic`, which diverges — NEVER a Rust `panic!` unwind across
// the FFI boundary, which would be UB). `any`/`all` are total: an empty
// (or null) input yields `False` / `True` respectively (vacuous truth),
// matching numpy + the f64 aggregates' graceful-null posture.
// =====================================================================

/// `coil.argmin(a) -> i64`. The FLAT (C-order) index of the first
/// occurrence of the minimum. NaN propagates (its index is returned).
/// BORROWS the handle. An EMPTY (or null) input `coil_panic`s — a clean
/// abort (numpy raises `ValueError`), NEVER a Rust unwind across the FFI.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_argmin(a: *mut u8) -> i64 {
    if a.is_null() {
        coil_panic("coil.argmin: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    // Empty input → numpy ValueError → clean coil_panic (NOT an unwind).
    argmin_scalar(arr_ref).unwrap_or_else(|e| coil_panic(&format!("coil.argmin: {}", e.message)))
}

/// `coil.argmax(a) -> i64`. The FLAT (C-order) index of the first
/// occurrence of the maximum. NaN propagates. BORROWS the handle. EMPTY /
/// null → clean `coil_panic` (NEVER a Rust unwind).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_argmax(a: *mut u8) -> i64 {
    if a.is_null() {
        coil_panic("coil.argmax: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    argmax_scalar(arr_ref).unwrap_or_else(|e| coil_panic(&format!("coil.argmax: {}", e.message)))
}

// =====================================================================
// #145 gap-closure BATCH 7 (2026-06-01) — the VALUE reductions
// `min` / `max` / `prod`. Each returns an `f64` SCALAR — the SAME
// `(ptr) -> f64` extern shape as `coil.mean` (coil's established
// scalar-reduction convention; every `.cb` Buffer is Float64 so
// `min`/`max`/`prod -> f64` is numpy-EXACT for every `.cb` buffer). All
// BORROW the handle (no rebox / free).
//
// EMPTY-input contract: `min`/`max` of an empty array RAISE `ValueError`
// in numpy — coil cannot raise across the C-ABI, so the shim maps the
// kernel's `Err` to a clean `coil_panic` (mirror `argmin`/`argmax`):
// the stdlib `__cobrust_panic` diverges — NEVER a Rust `panic!` unwind
// across the FFI (which would be UB). `prod` is TOTAL: `prod([]) == 1.0`
// (the multiplicative identity, numpy parity — NOT a trap), so a null
// handle yields the identity `1.0`; f64 overflow saturates to `+inf`.
// =====================================================================

/// `coil.min(a) -> f64`. The smallest element. NaN PROPAGATES (any NaN in
/// a lane → `NaN`, like numpy `np.min`). BORROWS the handle. An EMPTY (or
/// null) input `coil_panic`s — a clean abort (numpy raises `ValueError`),
/// NEVER a Rust unwind across the FFI.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_min(a: *mut u8) -> f64 {
    if a.is_null() {
        coil_panic("coil.min: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    // Empty input → numpy ValueError → clean coil_panic (NOT an unwind).
    min_scalar(arr_ref).unwrap_or_else(|e| coil_panic(&format!("coil.min: {}", e.message)))
}

/// `coil.max(a) -> f64`. The largest element. NaN PROPAGATES. BORROWS the
/// handle. An EMPTY (or null) input `coil_panic`s (numpy `ValueError`),
/// NEVER a Rust unwind across the FFI.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_max(a: *mut u8) -> f64 {
    if a.is_null() {
        coil_panic("coil.max: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    max_scalar(arr_ref).unwrap_or_else(|e| coil_panic(&format!("coil.max: {}", e.message)))
}

/// `coil.prod(a) -> f64`. The product of all elements. NaN PROPAGATES.
/// EMPTY → `1.0` (the multiplicative identity — numpy `np.prod([]) ==
/// 1.0`, NOT a trap). f64 overflow → `+inf` (numpy parity). BORROWS the
/// handle. TOTAL — no `coil_panic` path; a null handle yields the identity
/// `1.0`.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_prod(a: *mut u8) -> f64 {
    if a.is_null() {
        return 1.0;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    // prod is infallible (empty → 1.0); the `Result` is for ABI uniformity.
    prod_scalar(arr_ref).unwrap_or(1.0)
}

/// `coil.any(a) -> bool`. `True` iff ANY element is truthy. `any([]) ==
/// False`; `NaN` is truthy (numpy). BORROWS the handle. A null handle is
/// treated as empty → `False`. Total — never traps.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_any(a: *mut u8) -> bool {
    if a.is_null() {
        return false;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    any_scalar(arr_ref).unwrap_or(false)
}

/// `coil.all(a) -> bool`. `True` iff ALL elements are truthy. `all([]) ==
/// True` (vacuous truth); `NaN` is truthy (numpy). BORROWS the handle. A
/// null handle is treated as empty → `True`. Total — never traps.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_all(a: *mut u8) -> bool {
    if a.is_null() {
        return true;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr_ref: &Array = unsafe { &*a.cast::<Array>() };
    all_scalar(arr_ref).unwrap_or(true)
}

// =====================================================================
// ADR-0077 Phase 1 (+ Phase 3 broadcasting) — Buffer operator / index /
// attribute C-ABI surface. The FIRST ecosystem-handle operator. The
// `.cb`-side `a + b` / `a[i]` / `a.shape` retarget (at MIR) onto these
// symbols; codegen only declares them (no `lower_binop` type-switch —
// ADR-0077 §1.1). Phase 3 makes the elementwise binops (`+` / `-` / `*`)
// broadcast numpy-compatible shapes (the guard consults `broadcast_shape`
// instead of demanding equal shapes); see `buffer_binop`.
// =====================================================================

/// Shared elementwise-binop body for `+` / `-` / `*` (ADR-0077 Q1;
/// **broadcasting relaxation** ADR-0077 Phase 3). Borrows both handles,
/// enforces a **numpy-broadcast-compatibility** runtime contract (the
/// guard aborts via `coil_panic` ONLY when the two shapes are not
/// broadcastable per numpy rules — `broadcast_shape(..).is_err()`),
/// applies `f` (one of `Array::add` / `sub` / `mul` — whose kernel
/// already broadcasts compatible shapes per `ufunc::binary_dispatch`),
/// and returns a freshly-Boxed result handle the `.cb` caller owns.
///
/// ## Broadcasting (Phase 3)
///
/// Cobrust's static types carry no shape, so the shape relationship is
/// only knowable at runtime — this is the ONLY place an incompatible
/// pair is catchable. The guard delegates the decision to
/// [`broadcast_shape`] (the exact predicate `Array::add` already
/// consults internally): broadcast-compatible pairs — equal shapes, a
/// size-1 axis expanding (`(3,1)+(1,4) -> (3,4)`), a missing leading dim
/// counting as 1 (`(2,3)+(3,) -> (2,3)`), the 1-D `(3,)+(1,) -> (3,)`
/// scalar-stand-in — fall through to the broadcasting kernel; only a
/// genuinely incompatible pair (a trailing axis that is neither equal
/// nor 1, e.g. `(3,)+(4,)`) aborts. The diagnostic on the abort path is
/// the numpy-style `"operands could not be broadcast together with
/// shapes ..."` message carried by `broadcast_shape`'s `Err`. The
/// operator returns a bare `Buffer` (not a `Result`), so an incompatible
/// pair aborts — matching numpy's raise (the §2.5 closest honest
/// behavior; a fallible `a.checked_add(b) -> Result` escape is a later
/// surface).
///
/// # Safety
///
/// `a` and `b` must be live `Buffer` handles (not yet dropped).
unsafe fn buffer_binop(
    a: *mut u8,
    b: *mut u8,
    op_name: &str,
    f: fn(&Array, &Array) -> Result<Array, crate::error::NumpyError>,
) -> *mut u8 {
    if a.is_null() || b.is_null() {
        coil_panic("coil.Buffer operator: null operand handle");
    }
    // SAFETY: caller attests both are live Buffer handles. Borrow only —
    // neither is reboxed / freed; the `.cb` scope still owns + drops them.
    let lhs: &Array = unsafe { &*a.cast::<Array>() };
    let rhs: &Array = unsafe { &*b.cast::<Array>() };
    // ADR-0077 Phase 3 — broadcast-compatibility runtime check. Abort ONLY
    // when the shapes are not numpy-broadcastable; broadcast-compatible
    // pairs fall through to `f`, whose kernel broadcasts them. The abort
    // path reuses `broadcast_shape`'s numpy-exact "operands could not be
    // broadcast together with shapes ..." diagnostic.
    if let Err(e) = broadcast_shape(&lhs.shape(), &rhs.shape()) {
        coil_panic(&format!("coil.Buffer {op_name}: {}", e.message));
    }
    let out = match f(lhs, rhs) {
        Ok(arr) => arr,
        Err(e) => coil_panic(&format!("coil.Buffer {op_name}: {}", e.message)),
    };
    Box::into_raw(Box::new(out)).cast::<u8>()
}

/// `a + b` → fresh `Buffer`. Elementwise add (ADR-0077 Q1).
///
/// # Safety
///
/// `a`, `b` must be live `Buffer` handles. Returns an owned handle the
/// `.cb` caller drops once via `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_add(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "add", Array::add) }
}

/// `a - b` → fresh `Buffer`. Elementwise subtract (ADR-0077 Q1).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_sub(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "sub", Array::sub) }
}

/// `a * b` → fresh `Buffer`. Elementwise multiply (ADR-0077 Q1).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_mul(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "mul", Array::mul) }
}

/// `a / b` → fresh `Buffer`. Elementwise NumPy **true division**
/// (`true_divide`, ADR-0077 Phase-1 completion). `/` ALWAYS yields a
/// FLOAT result: int operands promote to `Float64` first, so
/// `int / int → float64` (`[1,2,3]/[2] → [0.5,1,1.5]`, NOT integer
/// `[0,1,1]`) and `int / 0 → IEEE inf` (a NumPy RuntimeWarning, NEVER a
/// `coil_panic`). Routes through the shared broadcast-aware
/// [`buffer_binop`] body onto [`Array::true_div`] (the IEEE float-arm
/// kernel), so it broadcasts free like `+`/`-`/`*`. Float div-by-zero is
/// IEEE (`±inf` / `NaN`), so the only abort path is a non-broadcastable
/// shape pair (matching numpy's raise).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_div(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "div", Array::true_div) }
}

/// `a @ b` → fresh `Buffer`. Matrix multiplication (numpy `matmul`,
/// ADR-0077 §"@-operator"): `(m,k)@(k,n) -> (m,n)`, `(m,k)@(k,) -> (m,)`,
/// `(k,)@(k,n) -> (n,)`, and the 1-D·1-D `(k,)@(k,) -> ` 0-d scalar buffer.
/// Wraps the EXISTING runtime kernel [`Array::matmul`] (→ `coil::linalg::
/// matmul`, which promotes int operands to `Float64`, uses `ndarray`'s
/// `Array2::dot` for the 2-D·2-D case, and is NOT BLAS by default — see the
/// `coil-matmul` benchmark report).
///
/// **Why NOT the shared [`buffer_binop`] body**: that helper runs a
/// `broadcast_shape` pre-check, but matmul conformability is the
/// inner-dim-alignment rule (`a.shape[-1] == b.shape[-2]`), NOT numpy
/// broadcasting — a valid `(2,3)@(3,4)` is NON-broadcastable and would be
/// wrongly aborted. This shim therefore forwards STRAIGHT to `Array::matmul`
/// and lets it own the shape check.
///
/// **Trap discipline (ADR-0077 Q4)**: a non-conformable pair (or an
/// `LinalgDtypeUnsupported` — unreachable here since `matmul` coerces ints
/// to float) makes `Array::matmul` return `Err`; we convert it to a
/// `coil_panic` (the `__cobrust_panic` abort path) — NEVER letting a Rust
/// `Err`/panic unwind across the C-ABI. Matches `buffer_binop`'s
/// abort-on-incompatible-shape behavior (numpy raises) and the §2.5 closest
/// honest semantics.
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_matmul(a: *mut u8, b: *mut u8) -> *mut u8 {
    if a.is_null() || b.is_null() {
        coil_panic("coil.Buffer @ (matmul): null operand handle");
    }
    // SAFETY: caller attests both are live Buffer handles. Borrow only —
    // neither is reboxed / freed; the `.cb` scope still owns + drops them.
    let lhs: &Array = unsafe { &*a.cast::<Array>() };
    let rhs: &Array = unsafe { &*b.cast::<Array>() };
    let out = match lhs.matmul(rhs) {
        Ok(arr) => arr,
        // Shape-mismatch (`shapes ... not aligned`) or dtype — abort with
        // the kernel's numpy-style diagnostic; diverges, never unwinds.
        Err(e) => coil_panic(&format!("coil.Buffer @ (matmul): {}", e.message)),
    };
    Box::into_raw(Box::new(out)).cast::<u8>()
}

// =====================================================================
// #145 array-MANIPULATION gap-closure (2026-06-01) — Buffer-RETURNING
// combine + reshape ops (`transpose` / `flatten` / `ravel` 1-arg;
// `concatenate` / `vstack` / `hstack` 2-arg). Each BORROWS its handle
// arg(s) (the `.cb` scope still owns + drops them) and returns a FRESH
// Boxed `Buffer` handle the scope drops via `__cobrust_coil_buffer_drop`
// — the EXACT ownership shape of `__cobrust_coil_buffer_matmul` /
// `__cobrust_coil_linalg_solve` (borrow-Buffer-args → fresh-Buffer-ret).
// The 1-arg ops are infallible (dtype-generic reshape); the 2-arg
// combine ops `coil_panic` on a non-conformable / dtype-mismatch pair
// (numpy raises `ValueError`) — NEVER unwinding a Rust `Err` across the
// C-ABI, matching `buffer_binop`'s abort-on-incompatible-shape.
// =====================================================================

/// `coil.transpose(a) -> Buffer`. Reverse all axes (`a.T`); a 1-D array
/// is returned unchanged, a 2-D `(m, n)` becomes `(n, m)`. BORROWS `a`;
/// returns a fresh owned handle. Infallible (dtype + size preserved).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped). The returned
/// pointer is a freshly-Boxed handle the `.cb` caller owns; freed once
/// via `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_transpose(a: *mut u8) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.transpose: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only —
    // not reboxed / freed; the `.cb` scope still owns + drops it.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(coil_transpose(arr))).cast::<u8>()
}

/// `coil.flatten(a) -> Buffer`. Collapse to a 1-D C-order (row-major)
/// copy. BORROWS `a`; returns a fresh owned handle. Infallible.
///
/// # Safety
///
/// As `__cobrust_coil_transpose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_flatten(a: *mut u8) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.flatten: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(coil_flatten(arr))).cast::<u8>()
}

/// `coil.ravel(a) -> Buffer`. Collapse to a 1-D C-order copy (numpy's
/// `ravel` returns a view; the handle ABI has no view-into-parent
/// surface, so this is an owned copy with identical VALUES). BORROWS
/// `a`; returns a fresh owned handle. Infallible.
///
/// # Safety
///
/// As `__cobrust_coil_transpose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_ravel(a: *mut u8) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.ravel: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(coil_ravel(arr))).cast::<u8>()
}

/// Shared body for the 2-array combine shims (`concatenate` / `vstack` /
/// `hstack`). BORROWS both handles, applies the `Result`-returning kernel
/// `f`, and `coil_panic`s on a non-conformable / dtype-mismatch pair
/// (numpy raises `ValueError`) — NEVER unwinding across the C-ABI.
/// Returns a freshly-Boxed result handle the `.cb` caller owns.
///
/// # Safety
///
/// `a` and `b` must be live `Buffer` handles (not yet dropped).
unsafe fn buffer_combine(
    a: *mut u8,
    b: *mut u8,
    op_name: &str,
    f: fn(&Array, &Array) -> Result<Array, crate::error::NumpyError>,
) -> *mut u8 {
    if a.is_null() || b.is_null() {
        coil_panic(&format!("coil.{op_name}: null operand handle"));
    }
    // SAFETY: caller attests both are live Buffer handles. Borrow only —
    // neither is reboxed / freed; the `.cb` scope still owns + drops them.
    let lhs: &Array = unsafe { &*a.cast::<Array>() };
    let rhs: &Array = unsafe { &*b.cast::<Array>() };
    let out = match f(lhs, rhs) {
        Ok(arr) => arr,
        Err(e) => coil_panic(&format!("coil.{op_name}: {}", e.message)),
    };
    Box::into_raw(Box::new(out)).cast::<u8>()
}

/// `coil.concatenate(a, b) -> Buffer`. Join the two arrays along axis 0
/// (the default `np.concatenate` axis). BORROWS both handles; returns a
/// fresh owned handle. A non-conformable pair (rank / non-axis-dim /
/// dtype mismatch) `coil_panic`s (numpy raises `ValueError`).
///
/// # Safety
///
/// `a` and `b` must be live `Buffer` handles (not yet dropped). The
/// returned pointer is a freshly-Boxed handle the `.cb` caller owns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_concatenate(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "concatenate", coil_concatenate) }
}

/// `coil.vstack(a, b) -> Buffer`. Stack row-wise (1-D `(n,)` operands are
/// promoted to `(1, n)`, then concatenated along axis 0). BORROWS both
/// handles. A column-count / dtype mismatch `coil_panic`s.
///
/// # Safety
///
/// As `__cobrust_coil_concatenate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_vstack(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "vstack", coil_vstack) }
}

/// `coil.hstack(a, b) -> Buffer`. Stack column-wise (1-D operands concat
/// along axis 0; ≥2-D along axis 1). BORROWS both handles. A
/// non-conformable (e.g. differing row counts) / dtype mismatch
/// `coil_panic`s.
///
/// # Safety
///
/// As `__cobrust_coil_concatenate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_hstack(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "hstack", coil_hstack) }
}

// =====================================================================
// #163 gap-closure BATCH 13 (2026-06-02) — the elementwise BINARY
// min/max ufuncs `maximum` / `minimum` / `fmax` / `fmin`. Each is a
// 2-Buffer `(ptr, ptr) -> ptr` shim riding the IDENTICAL `buffer_combine`
// shared body as `concatenate` / `vstack` / `hstack`: BORROWS both
// handles (the `.cb` scope still owns + drops them), applies the
// `Result`-returning kernel, and `coil_panic`s on a non-conformable /
// dtype-mismatch pair (numpy raises `ValueError`) — NEVER unwinding a
// Rust `Err` across the C-ABI. The ONLY behavioural difference between
// these four and the combine ops lives in the Rust kernel
// (`elementwise.rs`): the elementwise min/max pick + the NaN split
// (`maximum`/`minimum` PROPAGATE NaN; `fmax`/`fmin` IGNORE NaN). The ABI
// is byte-identical, so codegen reuses the `coil_binop_ty` extern shape.
// =====================================================================

/// `coil.maximum(a, b) -> Buffer`. Elementwise maximum, **PROPAGATES
/// NaN** (`maximum(1, nan) = nan`). BORROWS both handles; returns a fresh
/// owned handle. A non-conformable (unequal-shape) / dtype-mismatch pair
/// `coil_panic`s (numpy raises `ValueError`).
///
/// # Safety
///
/// `a` and `b` must be live `Buffer` handles (not yet dropped). The
/// returned pointer is a freshly-Boxed handle the `.cb` caller owns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_maximum(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "maximum", coil_maximum) }
}

/// `coil.minimum(a, b) -> Buffer`. Elementwise minimum, **PROPAGATES
/// NaN** (`minimum(1, nan) = nan`). BORROWS both handles. A
/// non-conformable / dtype-mismatch pair `coil_panic`s.
///
/// # Safety
///
/// As `__cobrust_coil_maximum`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_minimum(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "minimum", coil_minimum) }
}

/// `coil.fmax(a, b) -> Buffer`. Elementwise maximum, **IGNORES NaN**
/// (picks the non-NaN operand; `fmax(1, nan) = 1`, `fmax(nan, nan) =
/// nan`). BORROWS both handles. A non-conformable / dtype-mismatch pair
/// `coil_panic`s.
///
/// # Safety
///
/// As `__cobrust_coil_maximum`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_fmax(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "fmax", coil_fmax) }
}

/// `coil.fmin(a, b) -> Buffer`. Elementwise minimum, **IGNORES NaN**
/// (`fmin(1, nan) = 1`, `fmin(nan, nan) = nan`). BORROWS both handles. A
/// non-conformable / dtype-mismatch pair `coil_panic`s.
///
/// # Safety
///
/// As `__cobrust_coil_maximum`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_fmin(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_combine(a, b, "fmin", coil_fmin) }
}

// =====================================================================
// #145 gap-closure BATCH 8 (2026-06-01) — `coil.where(cond, a, b)`, the
// THREE-Buffer elementwise conditional select (`result[i] = cond[i]
// truthy ? a[i] : b[i]`). The FIRST coil shim borrowing THREE handles —
// it EXTENDS the 2-Buffer `buffer_combine` (`concatenate` / `vstack` /
// `hstack`) and the 2-Buffer `coil.linalg.solve` to a third borrowed
// arg. Each of the three handles is BORROWED (none consumed / freed; the
// `.cb` scope still owns + drops all three) and a FRESH Boxed `Buffer`
// the scope drops is returned — the EXACT ownership shape of the 2-arg
// combine ops, plus one more borrow. A non-conformable triple (a/b/cond
// shape mismatch) or an `a`/`b` dtype mismatch `coil_panic`s (numpy
// raises `ValueError`) — NEVER unwinding a Rust `Err` across the C-ABI.
// `cond` is typically a Bool-dtype Buffer from a `a < b` comparison
// (ADR-0077); a numeric cond is truthy on any nonzero element.
// =====================================================================

/// `coil.where(cond, a, b) -> Buffer`. Elementwise conditional select:
/// `result[i] = cond[i] truthy ? a[i] : b[i]`. BORROWS all THREE handles
/// (none consumed / freed); returns a fresh owned handle the `.cb` caller
/// drops once via `__cobrust_coil_buffer_drop`. The result dtype is `a`'s
/// dtype (`a` and `b` must match). A non-conformable triple (shapes not
/// all equal) or an `a`/`b` dtype mismatch `coil_panic`s (numpy raises
/// `ValueError`) — NEVER a Rust unwind across the C-ABI. `cond` is read
/// as a truthiness mask (a `Bool`-dtype cond uses its value; a numeric
/// cond is truthy on any nonzero element). A `NaN` in `a`/`b` FLOWS
/// THROUGH as a selected value.
///
/// # Safety
///
/// `cond`, `a`, and `b` must each be a live `Buffer` handle (not yet
/// dropped). The returned pointer is a freshly-Boxed handle the `.cb`
/// caller owns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_where(cond: *mut u8, a: *mut u8, b: *mut u8) -> *mut u8 {
    if cond.is_null() || a.is_null() || b.is_null() {
        coil_panic("coil.where: null operand handle");
    }
    // SAFETY: caller attests all three are live Buffer handles. Borrow
    // only — none is reboxed / freed; the `.cb` scope still owns + drops
    // all three.
    let cond_ref: &Array = unsafe { &*cond.cast::<Array>() };
    let a_ref: &Array = unsafe { &*a.cast::<Array>() };
    let b_ref: &Array = unsafe { &*b.cast::<Array>() };
    let out = match coil_where(cond_ref, a_ref, b_ref) {
        Ok(arr) => arr,
        // Non-conformable shapes / dtype mismatch — abort with the
        // kernel's numpy-style diagnostic; diverges, never unwinds.
        Err(e) => coil_panic(&format!("coil.where: {}", e.message)),
    };
    Box::into_raw(Box::new(out)).cast::<u8>()
}

// =====================================================================
// #145 unary TRANSCENDENTAL gap-closure (2026-06-01) — the FLOAT-
// returning 1-arg elementwise ufunc family (`exp` / `log` (natural ln) /
// `log10` / `sqrt` / `sin` / `cos` / `tan`, plus the trivial same-dtype-
// rule `exp2` / `log2` / `cbrt` / `sinh` / `cosh` / `tanh`). Each BORROWS
// its single handle arg (the `.cb` scope still owns + drops it) and
// returns a FRESH Boxed `Buffer` handle the scope drops via
// `__cobrust_coil_buffer_drop` — the EXACT ownership shape of the BATCH-2
// `__cobrust_coil_transpose` / `_flatten` / `_ravel` 1-arg reshape shims
// (borrow-Buffer-arg → fresh-Buffer-return). Unlike the 2-array combine
// ops, these are TOTAL: there is NO conformability concept for a unary
// op (a domain-error input — `log(-1)`, `sqrt(-1)`, `exp(710)` — yields
// an IEEE-754 special VALUE, NOT an error), so there is NO `coil_panic`
// path; the shim ALWAYS returns a fresh `Buffer`. Dtype promotion is
// numpy-exact: int / bool inputs -> `Float64`, `Float32` stays `Float32`,
// `Float64` stays `Float64` (see `elementwise.rs`).
// =====================================================================

/// Shared body for the 1-arg unary-transcendental shims. BORROWS the
/// single handle, applies the infallible `Array -> Array` kernel `f`, and
/// returns a freshly-Boxed result handle the `.cb` caller owns. Total —
/// no `coil_panic` path (the kernels never fail; a null handle is the
/// only abort, mirroring the BATCH-2 `__cobrust_coil_transpose` guard).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped).
unsafe fn buffer_unary(a: *mut u8, op_name: &str, f: fn(&Array) -> Array) -> *mut u8 {
    if a.is_null() {
        coil_panic(&format!("coil.{op_name}: null operand handle"));
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only —
    // not reboxed / freed; the `.cb` scope still owns + drops it.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(f(arr))).cast::<u8>()
}

/// `coil.exp(a) -> Buffer`. `e**x` elementwise. `exp(710) -> +inf`
/// (IEEE-754 overflow, a VALUE not an error). Int / bool -> Float64,
/// Float32 stays Float32. BORROWS `a`; returns a fresh owned handle.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped). The returned
/// pointer is a freshly-Boxed handle the `.cb` caller owns; freed once
/// via `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_exp(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "exp", coil_exp) }
}

/// `coil.log(a) -> Buffer`. NATURAL log (base e). `log(0) -> -inf`,
/// `log(-1) -> NaN` (IEEE-754 domain VALUES, not errors). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_log(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "log", coil_log) }
}

/// `coil.log10(a) -> Buffer`. Base-10 log. `log10(0) -> -inf`,
/// `log10(-1) -> NaN`. BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_log10(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "log10", coil_log10) }
}

/// `coil.sqrt(a) -> Buffer`. Square root. `sqrt(-1) -> NaN` (IEEE-754
/// domain VALUE, not an error). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_sqrt(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "sqrt", coil_sqrt) }
}

/// `coil.sin(a) -> Buffer`. Sine (radians). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_sin(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "sin", coil_sin) }
}

/// `coil.cos(a) -> Buffer`. Cosine (radians). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_cos(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "cos", coil_cos) }
}

/// `coil.tan(a) -> Buffer`. Tangent (radians). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_tan(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "tan", coil_tan) }
}

/// `coil.exp2(a) -> Buffer`. `2**x` elementwise (same dtype rule as
/// `exp`). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_exp2(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "exp2", coil_exp2) }
}

/// `coil.log2(a) -> Buffer`. Base-2 log (same dtype rule as `log`).
/// `log2(0) -> -inf`, `log2(-1) -> NaN`. BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_log2(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "log2", coil_log2) }
}

/// `coil.cbrt(a) -> Buffer`. Cube root (same dtype rule as `sqrt`; unlike
/// `sqrt`, defined for negatives — `cbrt(-8) -> -2`). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_cbrt(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "cbrt", coil_cbrt) }
}

/// `coil.sinh(a) -> Buffer`. Hyperbolic sine (same dtype rule as `sin`).
/// BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_sinh(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "sinh", coil_sinh) }
}

/// `coil.cosh(a) -> Buffer`. Hyperbolic cosine (same dtype rule as
/// `cos`). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_cosh(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "cosh", coil_cosh) }
}

/// `coil.tanh(a) -> Buffer`. Hyperbolic tangent (same dtype rule as
/// `tan`). BORROWS `a`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_tanh(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "tanh", coil_tanh) }
}

// =====================================================================
// #145 unary ROUNDING / SIGN gap-closure BATCH 4 (2026-06-01) — the
// DTYPE-PRESERVING 1-arg elementwise ufunc family (`abs` / `floor` /
// `ceil` / `round` / `trunc` / `square` / `sign`). SAME 1-arg
// borrow-Buffer-arg → fresh-Buffer-return value-handle ABI as the
// BATCH-3 transcendentals above, riding the SAME shared `buffer_unary`
// body (TOTAL — no `coil_panic` path; a null handle is the only abort).
// The ONLY difference from BATCH 3 is the kernel's dtype contract:
// these PRESERVE the dtype (int->int, f32->f32, f64->f64; floor/ceil/
// round/trunc are int no-ops) rather than promoting int -> Float64. The
// ABI shape is byte-identical, so codegen rides the SAME `coil_shape_ty`
// `(ptr) -> ptr` extern + the flat `__cobrust_coil_` recognizer; the
// dtype rule is entirely inside the Rust kernel (`elementwise.rs`).
// =====================================================================

/// `coil.abs(a) -> Buffer`. Absolute value, **dtype-preserving**
/// (`abs(int)->int`, `abs(float)->float`). BORROWS `a`; returns a fresh
/// owned handle. Total (`abs(NaN)=NaN`, `i64::MIN` wraps to itself per
/// numpy two's-complement). NOTE this is the coil MODULE fn
/// `coil.abs(buf)` — distinct from any scalar `abs`.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_abs(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "abs", coil_abs) }
}

/// `coil.floor(a) -> Buffer`. Largest integer `<= x`, **dtype-
/// preserving**; a NO-OP on integer / bool input (numpy 2.x). BORROWS
/// `a`. Total (`floor(NaN)=NaN`, `floor(±inf)=±inf`).
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_floor(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "floor", coil_floor) }
}

/// `coil.ceil(a) -> Buffer`. Smallest integer `>= x`, **dtype-
/// preserving**; a NO-OP on integer / bool input. BORROWS `a`. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_ceil(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "ceil", coil_ceil) }
}

/// `coil.round(a) -> Buffer`. Round to nearest with **round-half-to-EVEN**
/// (banker's rounding — `0.5 -> 0`, `1.5 -> 2`, `2.5 -> 2`), **dtype-
/// preserving**; a NO-OP on integer / bool input. BORROWS `a`. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_round(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "round", coil_round) }
}

/// `coil.trunc(a) -> Buffer`. Truncate toward zero, **dtype-preserving**;
/// a NO-OP on integer / bool input. BORROWS `a`. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_trunc(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "trunc", coil_trunc) }
}

/// `coil.square(a) -> Buffer`. `x * x` elementwise, **dtype-preserving**
/// (`square(int)->int`, `square(float)->float`). BORROWS `a`. Total
/// (integer wrapping on overflow per numpy two's-complement).
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_square(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "square", coil_square) }
}

/// `coil.sign(a) -> Buffer`. `-1` / `0` / `1`, **dtype-preserving**.
/// numpy-exact `sign(0.0)=0.0`, `sign(-0.0)=0.0`, `sign(NaN)=NaN` (NOT
/// Rust `f64::signum`). BORROWS `a`. Total (`sign(bool)` returns the
/// bool array unchanged — a documented coil Semantic divergence; numpy
/// raises).
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_sign(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "sign", coil_sign) }
}

// =====================================================================
// #163 PREDICATE gap-closure BATCH 12 (2026-06-02) — the per-element
// predicate ufuncs `isnan` / `isinf` / `isfinite`. Each is a 1-arg
// Buffer -> Buffer op riding the SAME shared `buffer_unary` body +
// `coil_shape_ty` `(ptr) -> ptr` extern as every other unary ufunc
// above. The ONLY difference is the kernel's RESULT DTYPE: these ALWAYS
// produce a `Dtype::Bool` Buffer (the per-element MASK), REGARDLESS of
// the input dtype (`np.isnan(x).dtype == bool`) — like the `a < b`
// comparison, but unary. That bool-dtype result rides the IDENTICAL
// opaque-`Buffer`-handle return as `transpose` / `abs`, so the ABI is
// byte-identical; the bool rule lives entirely in the Rust kernel
// (`elementwise.rs`). TOTAL — a predicate NEVER fails (no NaN / inf
// "domain error"; it simply ANSWERS for every IEEE value), so there is
// NO `coil_panic` path (a null handle is the only abort).
// =====================================================================

/// `coil.isnan(a) -> Buffer`. Per-element "is this NaN?". Returns a fresh
/// `Dtype::Bool` Buffer (the MASK) of `a`'s shape, REGARDLESS of `a`'s
/// dtype. Integer / bool input is ALL-`false` (integers are always finite
/// — `np.isnan([1,2]) = [False, False]`). `isnan(nan)=True`,
/// `isnan(inf)=False`. BORROWS `a`. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_isnan(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "isnan", coil_isnan) }
}

/// `coil.isinf(a) -> Buffer`. Per-element "is this +inf or -inf?".
/// Returns a fresh `Dtype::Bool` Buffer (the MASK) of `a`'s shape,
/// REGARDLESS of `a`'s dtype. Integer / bool input is ALL-`false`
/// (integers are always finite). BOTH `+inf` and `-inf` are `True`;
/// `isinf(nan)=False`. BORROWS `a`. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_isinf(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "isinf", coil_isinf) }
}

/// `coil.isfinite(a) -> Buffer`. Per-element "is this finite (NOT NaN,
/// NOT inf)?". Returns a fresh `Dtype::Bool` Buffer (the MASK) of `a`'s
/// shape, REGARDLESS of `a`'s dtype. Integer / bool input is ALL-`true`
/// (integers are ALWAYS finite — `np.isfinite([1,2]) = [True, True]`).
/// `isfinite(1.0)=True`, `isfinite(nan)=False`, `isfinite(inf)=False`.
/// BORROWS `a`. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_isfinite(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "isfinite", coil_isfinite) }
}

// =====================================================================
// #145 gap-closure BATCH 5 (2026-06-01) — the REDUCTIONS family in three
// return shapes. `cumsum` / `cumprod` are Buffer-RETURNING (the no-axis
// FLATTEN-to-1-D cumulative scan), riding the SAME `buffer_unary` body +
// `coil_shape_ty` `(ptr) -> ptr` extern as the transcendental / rounding
// ufuncs. They are TOTAL (no `coil_panic` path — a cumulative scan never
// fails; an empty input yields an empty 1-D Buffer). `argmin` / `argmax`
// (→ i64) + `any` / `all` (→ bool) are SCALAR-returning and live further
// down beside the existing scalar-aggregate shims (`mean` / `ptp`).
// =====================================================================

/// `coil.cumsum(a) -> Buffer`. Cumulative sum over the C-order FLATTENED
/// array → a fresh 1-D Buffer of length `a.size`. DTYPE (numpy): int32 /
/// int64 / bool → Int64, float32 stays Float32, float64 stays Float64.
/// BORROWS `a`; returns a fresh owned handle. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_cumsum(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "cumsum", coil_cumsum) }
}

/// `coil.cumprod(a) -> Buffer`. Cumulative product over the C-order
/// FLATTENED array. Same DTYPE rule + 1-D shape as `cumsum`. BORROWS `a`.
/// Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_cumprod(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "cumprod", coil_cumprod) }
}

// =====================================================================
// #145 gap-closure BATCH 9 (2026-06-01) — the FLAT search / order
// surface (`sort` / `argsort` / `unique` / `flatnonzero`). Each is a
// 1-arg Buffer -> Buffer op riding the SAME shared `buffer_unary` body +
// `coil_shape_ty` `(ptr) -> ptr` extern as the transcendental / rounding
// ufuncs + the BATCH-2 reshape ops (borrow-Buffer-arg → fresh-Buffer-
// return). All are TOTAL (no `coil_panic` domain trap — a sort / dedupe /
// nonzero scan never fails on a valid Buffer; a null handle is the only
// abort, via `buffer_unary`'s guard). The RETURN-DTYPE split lives
// entirely in the Rust kernel (`manipulate.rs`): `sort` / `unique`
// PRESERVE the input dtype; `argsort` / `flatnonzero` ALWAYS produce an
// `Int64` Buffer (the indices). The ABI shape is byte-identical for all
// four, so codegen rides the SAME `coil_shape_ty` extern + the flat
// `__cobrust_coil_*` recognizer prefix (no batch-specific arm).
// =====================================================================

/// `coil.sort(a) -> Buffer`. Fresh ASCENDING-sorted 1-D copy (numpy's
/// no-axis default flattens C-order first), **dtype-preserving**. For
/// floats ALL `NaN` sort LAST (sign-agnostic). BORROWS `a`; returns a
/// fresh owned handle. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_sort(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "sort", coil_sort) }
}

/// `coil.argsort(a) -> Buffer`. The `Int64` indices that would sort `a`
/// ascending (STABLE; over the C-order flattened array). For floats the
/// `NaN`-bearing indices go LAST in input order. The result Buffer is
/// ALWAYS `Int64`-dtype regardless of input dtype. BORROWS `a`; returns
/// a fresh owned handle. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_argsort(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "argsort", coil_argsort) }
}

/// `coil.unique(a) -> Buffer`. The SORTED unique values as a fresh 1-D
/// copy, **dtype-preserving**. For floats, MULTIPLE `NaN` collapse to ONE
/// trailing `NaN` (numpy 1.21+). BORROWS `a`; returns a fresh owned
/// handle. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_unique(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "unique", coil_unique) }
}

/// `coil.flatnonzero(a) -> Buffer`. The `Int64` flat C-order indices
/// where `a != 0`. For floats the predicate is `a != 0.0`, so `NaN` (`!=
/// 0.0`) IS included. The result Buffer is ALWAYS `Int64`-dtype. BORROWS
/// `a`; returns a fresh owned handle. Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_flatnonzero(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "flatnonzero", coil_flatnonzero) }
}

// =====================================================================
// #145 REARRANGE / REPEAT gap-closure BATCH 10 (2026-06-02) — `diff` /
// `flip` / `roll` / `repeat` / `tile`, each Buffer-RETURNING over the
// C-order FLATTENED array. `diff` / `flip` are 1-arg, riding the SHARED
// `buffer_unary` body (`(ptr) -> ptr`) like the BATCH-2 reshape ops +
// the unary ufuncs. `roll` / `repeat` / `tile` take a trailing i64 SCALAR
// (`(ptr, i64) -> ptr`) — the SAME scalar-besides-handle shape as the
// BATCH-6 `clip` / `power` f64 scalar, but i64 (`shift` / `count`). They
// BORROW the handle (the `.cb` scope still owns + drops it) and return a
// freshly-Boxed result handle the scope drops via
// `__cobrust_coil_buffer_drop`. ALL FIVE are TOTAL — a rearrange / repeat
// never fails (an empty input or `n <= 0` yields an empty Buffer; numpy
// `np.repeat(a, 0) == []` / `np.tile(a, 0) == []`), so there is NO
// `coil_panic` domain trap; a null handle is the only abort.
// =====================================================================

/// Shared body for the 1-arg-Buffer + trailing-i64-SCALAR shims (`roll` /
/// `repeat` / `tile`). BORROWS the handle, applies the infallible
/// `(&Array, i64) -> Array` kernel `f`, and returns a freshly-Boxed result
/// handle the `.cb` caller owns. The i64 SCALAR mirror of the BATCH-6
/// `clip` / `power` f64-scalar borrow pattern (`__cobrust_coil_clip` /
/// `_power`); the `.cb`-side int literal crosses by value as an i64 (the
/// MIR `EcoSig` param `Ty::Int` lowers it directly — no f64 cast, unlike
/// `percentile`'s `q`). TOTAL — a null handle is the only abort.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped).
unsafe fn buffer_unary_scalar_i64(
    a: *mut u8,
    k: i64,
    op_name: &str,
    f: fn(&Array, i64) -> Array,
) -> *mut u8 {
    if a.is_null() {
        coil_panic(&format!("coil.{op_name}: null operand handle"));
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only —
    // not reboxed / freed; the `.cb` scope still owns + drops it.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(f(arr, k))).cast::<u8>()
}

/// `coil.diff(a) -> Buffer`. Discrete first difference `a[1:] - a[:-1]`
/// over the C-order FLATTENED array → a fresh 1-D Buffer of length
/// `max(size - 1, 0)`. **Dtype-preserving** (`diff(int) -> int`). A len-≤1
/// / empty input yields an EMPTY Buffer. BORROWS `a`; returns a fresh owned
/// handle. Total (a null handle is the only abort).
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_diff(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "diff", coil_diff) }
}

/// `coil.flip(a) -> Buffer`. Reverse the C-order FLATTENED array → a fresh
/// 1-D Buffer of the SAME length, reversed. **Dtype-preserving**. An empty
/// input yields an empty Buffer. BORROWS `a`; returns a fresh owned handle.
/// Total.
///
/// # Safety
///
/// As `__cobrust_coil_exp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_flip(a: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary(a, "flip", coil_flip) }
}

/// `coil.roll(a, k) -> Buffer`. Cyclically shift the C-order flattened
/// array by `k`, then reshape BACK to the ORIGINAL shape (numpy's no-axis
/// `np.roll` keeps the shape). **Dtype-preserving**. A NEGATIVE `k` rolls
/// LEFT; `k` is normalised mod size (`k = 0` / `k % size == 0` →
/// unchanged). An empty input is returned unchanged. BORROWS `a`; returns a
/// fresh owned handle. TOTAL — no domain trap (a cyclic shift never fails);
/// a null handle is the only abort.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped). The returned
/// pointer is a freshly-Boxed handle the `.cb` caller owns; freed once via
/// `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_roll(a: *mut u8, k: i64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary_scalar_i64(a, k, "roll", coil_roll) }
}

/// `coil.repeat(a, n) -> Buffer`. Repeat EACH element `n` times over the
/// C-order flattened array → a fresh 1-D Buffer of length `n * size`.
/// **Dtype-preserving**. `n <= 0` yields an EMPTY Buffer (numpy
/// `np.repeat(a, 0) == []`); `n == 1` is a flat copy. BORROWS `a`; returns
/// a fresh owned handle. TOTAL — a null handle is the only abort.
///
/// # Safety
///
/// As `__cobrust_coil_roll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_repeat(a: *mut u8, n: i64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary_scalar_i64(a, n, "repeat", coil_repeat) }
}

/// `coil.tile(a, n) -> Buffer`. Tile the WHOLE C-order flattened array `n`
/// times → a fresh 1-D Buffer of length `n * size`. **Dtype-preserving**.
/// `n <= 0` yields an EMPTY Buffer (numpy `np.tile(a, 0) == []`); `n == 1`
/// is a flat copy. BORROWS `a`; returns a fresh owned handle. TOTAL — a
/// null handle is the only abort.
///
/// # Safety
///
/// As `__cobrust_coil_roll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_tile(a: *mut u8, n: i64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_unary_scalar_i64(a, n, "tile", coil_tile) }
}

// =====================================================================
// #145 SCALAR-ARG ufunc gap-closure BATCH 6 (2026-06-01) — `clip` /
// `power`, the FIRST Buffer-RETURNING ops to take EXTRA f64 SCALAR args
// beside the handle. `clip(a, lo, hi)` is `(ptr, f64, f64) -> ptr`;
// `power(a, p)` is `(ptr, f64) -> ptr` (the SAME shape as the scalar-
// RETURNING `__cobrust_coil_percentile(a, q)` — a Buffer + f64 — except
// these RETURN a fresh Buffer). They BORROW the handle (the `.cb` scope
// still owns + drops it) and return a freshly-Boxed result handle the
// scope drops via `__cobrust_coil_buffer_drop` — the EXACT ownership shape
// of the BATCH-3/4 unary ufunc shims, plus the trailing f64 scalar(s) that
// cross by value (the MIR retarget lowers the `.cb` int / float literal to
// f64 via `lower_eco_arg`, exactly as `percentile`'s `q` does). TOTAL —
// `clip` / `power` cannot fail (NaN / inf are VALUES, not errors; `power`'s
// f64 exponent sidesteps numpy's int**int<0 ValueError), so there is NO
// `coil_panic` domain trap; a null handle is the only abort.
// =====================================================================

/// `coil.clip(a, lo, hi) -> Buffer`. Clamp each element to `[lo, hi]`,
/// **dtype-preserving** (`np.clip(int_array, lo, hi).dtype == int64`). For
/// an int / bool `Array` the `f64` bounds are ROUNDED to the integer dtype;
/// for a float `Array` they clamp in that float type. PRESERVES NaN
/// (`clip(nan, 0, 1) = nan`); the UPPER bound wins when `lo > hi`
/// (numpy `minimum(maximum(a, lo), hi)`). BORROWS `a`; returns a fresh owned
/// handle. TOTAL — no domain trap (NaN / inf are VALUES); a null handle is
/// the only abort.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped). The returned
/// pointer is a freshly-Boxed handle the `.cb` caller owns; freed once via
/// `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_clip(a: *mut u8, lo: f64, hi: f64) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.clip: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only —
    // not reboxed / freed; the `.cb` scope still owns + drops it.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(coil_clip(arr, lo, hi))).cast::<u8>()
}

/// `coil.power(a, p) -> Buffer`. Raise each element to the `p`-th power,
/// **float-promoting** with an f64 exponent (int / bool -> `Float64`,
/// `Float32` stays `Float32`, `Float64` stays `Float64`). `power(x, 0.5) =
/// sqrt(x)`, `power(x, 0) = 1` (even `0**0 = 1`), `power(neg, 0.5) = NaN`
/// (the real branch). BORROWS `a`; returns a fresh owned handle. TOTAL — the
/// f64 exponent sidesteps numpy's int**int<0 ValueError, so there is NO
/// domain trap (a domain-error element yields an IEEE-754 special VALUE); a
/// null handle is the only abort.
///
/// # Safety
///
/// As `__cobrust_coil_clip`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_power(a: *mut u8, p: f64) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.power: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    Box::into_raw(Box::new(coil_power(arr, p))).cast::<u8>()
}

/// Shared body for the `a ⊕ k` SCALAR-broadcast shims (ADR-0077 Phase-1
/// completion). NumPy's `array ⊕ scalar` is exactly a length-1 broadcast
/// (`a ⊕ array([k])`): we materialise the python scalar `k` as a
/// 1-element `Float64` `Buffer`, then forward to the SAME broadcast-aware
/// kernel `f` the array-array operators use, so `+`/`-`/`*`/`/` all get
/// scalar support through one path (and `/` correctly true-divides). The
/// (1,)-vs-(N,) broadcast is always compatible, so the only abort the
/// kernel can take is `Array::true_div`-internal (never — IEEE is total).
///
/// `k` is the scalar as `f64` (the `.cb`-side int / float literal is cast
/// to `f64` at MIR-retarget time, mirroring `a[i]`'s f64 scalar contract).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped).
unsafe fn buffer_binop_scalar(
    a: *mut u8,
    k: f64,
    op_name: &str,
    f: fn(&Array, &Array) -> Result<Array, crate::error::NumpyError>,
) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.Buffer scalar operator: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only —
    // not reboxed / freed; the `.cb` scope still owns + drops it.
    let lhs: &Array = unsafe { &*a.cast::<Array>() };
    // The scalar as a 1-element f64 array — numpy's `a ⊕ k` IS `a ⊕ [k]`.
    let rhs = array_f64(&[k], &[1]).unwrap_or_else(|e| {
        coil_panic(&format!("coil.Buffer {op_name} scalar: {}", e.message));
    });
    let out = match f(lhs, &rhs) {
        Ok(arr) => arr,
        Err(e) => coil_panic(&format!("coil.Buffer {op_name} scalar: {}", e.message)),
    };
    Box::into_raw(Box::new(out)).cast::<u8>()
}

/// `a + k` (Buffer + python scalar) → fresh `Buffer`. Adds `k` to every
/// element via a length-1 broadcast (ADR-0077 Phase-1 completion).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_add_scalar(a: *mut u8, k: f64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop_scalar(a, k, "add", Array::add) }
}

/// `a - k` (Buffer - python scalar) → fresh `Buffer`. Subtracts `k` from
/// every element via a length-1 broadcast (ADR-0077 Phase-1 completion).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add_scalar`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_sub_scalar(a: *mut u8, k: f64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop_scalar(a, k, "sub", Array::sub) }
}

/// `a * k` (Buffer * python scalar) → fresh `Buffer`. Scales every
/// element by `k` via a length-1 broadcast (ADR-0077 Phase-1 completion).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add_scalar`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_mul_scalar(a: *mut u8, k: f64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop_scalar(a, k, "mul", Array::mul) }
}

/// `a / k` (Buffer / python scalar) → fresh `Buffer`. NumPy **true
/// division** of every element by `k` via a length-1 broadcast (ADR-0077
/// Phase-1 completion). `/ 0` is IEEE `±inf` / `NaN`, never a trap.
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add_scalar`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_div_scalar(a: *mut u8, k: f64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop_scalar(a, k, "div", Array::true_div) }
}

/// Shared body for the REVERSED `k ⊕ a` left-scalar shims (ADR-0077
/// Phase-2/3). NumPy's `scalar ⊕ array` with a NON-commutative `⊕`
/// (`-` / `/`) is `array([k]) ⊕ a` — the scalar is the LEFT operand, so
/// `2 - a` is `2 - a[i]` (NOT `a[i] - 2`) and `6 / a` is `6 / a[i]`. The
/// twin [`buffer_binop_scalar`] is the RIGHT-scalar form (`a ⊕ k` =
/// `a ⊕ array([k])`); the ONLY difference here is operand ORDER: we
/// materialise `k` as a length-1 `Float64` buffer and call `f(&k_buf, a)`
/// (LHS = the scalar), reusing the SAME broadcast-aware array-array kernel
/// `f`. Commutative ops (`+` / `*`) do NOT route here — they reuse the
/// right-scalar `*_scalar` shims directly (the MIR retarget maps `k + a`
/// onto `add_scalar`, ADR-0077 §"left-scalar"). The (1,)-vs-(N,) broadcast
/// is always compatible, so the only abort the kernel can take is
/// `Array::true_div`-internal (never — IEEE is total).
///
/// `k` is the scalar as `f64` (the `.cb`-side int / float literal is cast
/// to `f64` at MIR-retarget time, mirroring the right-scalar contract).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (not yet dropped).
unsafe fn buffer_binop_scalar_rev(
    a: *mut u8,
    k: f64,
    op_name: &str,
    f: fn(&Array, &Array) -> Result<Array, crate::error::NumpyError>,
) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.Buffer left-scalar operator: null operand handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only —
    // not reboxed / freed; the `.cb` scope still owns + drops it.
    let rhs: &Array = unsafe { &*a.cast::<Array>() };
    // The scalar as a 1-element f64 array — numpy's `k ⊕ a` IS `[k] ⊕ a`.
    let lhs = array_f64(&[k], &[1]).unwrap_or_else(|e| {
        coil_panic(&format!("coil.Buffer left-scalar {op_name}: {}", e.message));
    });
    // Operand ORDER is the whole point: the scalar buffer is the LEFT arg.
    let out = match f(&lhs, rhs) {
        Ok(arr) => arr,
        Err(e) => coil_panic(&format!("coil.Buffer left-scalar {op_name}: {}", e.message)),
    };
    Box::into_raw(Box::new(out)).cast::<u8>()
}

/// `k - a` (python scalar - Buffer) → fresh `Buffer`. REVERSED subtract:
/// every element becomes `k - a[i]` (NOT `a[i] - k` — that is the
/// right-scalar `_sub_scalar`). ADR-0077 Phase-2/3 left-scalar surface.
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add_scalar`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_rsub_scalar(a: *mut u8, k: f64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop_scalar_rev(a, k, "rsub", Array::sub) }
}

/// `k / a` (python scalar / Buffer) → fresh `Buffer`. REVERSED numpy
/// **true division**: every element becomes `k / a[i]` (NOT `a[i] / k`).
/// `/ 0` is IEEE `±inf` / `NaN`, never a trap. ADR-0077 Phase-2/3.
///
/// # Safety
///
/// As `__cobrust_coil_buffer_add_scalar`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_rdiv_scalar(a: *mut u8, k: f64) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop_scalar_rev(a, k, "rdiv", Array::true_div) }
}

// ---- ADR-0077 Phase-2/3 — buffer-buffer COMPARISON ops -------------
// `a cmp b` (cmp ∈ <, <=, >, >=, ==, !=) → a fresh `Buffer` of dtype
// Bool (numpy semantics — an element-wise mask, NOT a Cobrust bool
// scalar; ADR-0077 §"comparison-returns-Bool-Buffer"). Each forwards
// through the SAME broadcast-aware shared `buffer_binop` body the
// arithmetic ops use, onto the runtime `Array::{lt,le,gt,ge,eq_,ne_}`
// kernels (array.rs:210-259), which ALWAYS return a `Dtype::Bool`
// array. The owned handle is dropped once by the `.cb` scope. Note the
// runtime method names: `eq_` / `ne_` carry a trailing underscore (the
// `eq`/`ne` idents collide with the `PartialEq` trait); `lt`/`le`/`gt`/
// `ge` do not.

/// `a < b` → fresh Bool-dtype `Buffer` (element-wise less-than mask).
///
/// # Safety
///
/// `a`, `b` must be live `Buffer` handles. Returns an owned handle the
/// `.cb` caller drops once via `__cobrust_coil_buffer_drop`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_lt(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "lt", Array::lt) }
}

/// `a <= b` → fresh Bool-dtype `Buffer` (less-than-or-equal mask).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_lt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_le(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "le", Array::le) }
}

/// `a > b` → fresh Bool-dtype `Buffer` (greater-than mask).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_lt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_gt(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "gt", Array::gt) }
}

/// `a >= b` → fresh Bool-dtype `Buffer` (greater-than-or-equal mask).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_lt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_ge(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation.
    unsafe { buffer_binop(a, b, "ge", Array::ge) }
}

/// `a == b` → fresh Bool-dtype `Buffer` (element-wise equality mask).
/// NumPy semantics: `==` on two arrays is an ELEMENT-WISE mask, NOT a
/// single bool (`np.array([1,2]) == np.array([1,3]) → [True, False]`).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_lt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_eq(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation. `Array::eq_` (trailing `_`
    // avoids the `PartialEq::eq` ident clash) returns a Bool array.
    unsafe { buffer_binop(a, b, "eq", Array::eq_) }
}

/// `a != b` → fresh Bool-dtype `Buffer` (element-wise inequality mask).
///
/// # Safety
///
/// As `__cobrust_coil_buffer_lt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_ne(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: forwarded caller attestation. `Array::ne_` returns a Bool
    // array.
    unsafe { buffer_binop(a, b, "ne", Array::ne_) }
}

/// `a[i]` scalar read → `f64` (ADR-0077 Q2). BORROWS the handle.
/// Bounds-checked on the first axis (numpy-style negative indices
/// allowed via `index_single`); an out-of-bounds index aborts via
/// `coil_panic`. Returns a plain `f64` (numpy's 0-d scalar is not a
/// Cobrust type — ADR-0077 §4 known divergence).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_getitem(a: *mut u8, i: i64) -> f64 {
    if a.is_null() {
        coil_panic("coil.Buffer[i]: null handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    let view = match arr.index_single(i) {
        Ok(v) => v,
        Err(e) => coil_panic(&format!("coil.Buffer[{i}]: {}", e.message)),
    };
    // `index_single` returns a 0-d (one fewer axis) view of the element.
    // Materialise + extract the single f64 (mirrors `aggregates::
    // scalar_to_f64` — int/bool dtypes promote, matching the f64-only
    // Phase-1 return contract).
    match view.to_owned() {
        Array::Float64(x) => x.iter().next().copied().unwrap_or(f64::NAN),
        Array::Float32(x) => x.iter().next().copied().map_or(f64::NAN, f64::from),
        Array::Int64(x) => x.iter().next().copied().map_or(f64::NAN, |v| v as f64),
        Array::Int32(x) => x.iter().next().copied().map_or(f64::NAN, f64::from),
        Array::Bool(x) => x
            .iter()
            .next()
            .copied()
            .map_or(f64::NAN, |v| if v { 1.0 } else { 0.0 }),
    }
}

/// `a.shape` → owned `list[i64]` (ADR-0077 Q3). BORROWS the handle;
/// allocates a fresh `List<i64>` via the stdlib `__cobrust_list_*`
/// externs (coil's first use of the cross-crate list ABI, ADR-0072 Q5).
/// The `.cb` scope drops the list once via `__cobrust_list_drop`. numpy
/// returns a tuple; the `list[i64]` divergence is recorded in the coil
/// PROVENANCE manifest.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle. The returned pointer is an owned
/// `List<i64>` the `.cb` caller drops once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_shape(a: *mut u8) -> *mut u8 {
    let shape: Vec<usize> = if a.is_null() {
        Vec::new()
    } else {
        // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
        let arr: &Array = unsafe { &*a.cast::<Array>() };
        arr.shape()
    };
    let len = shape.len() as i64;
    // SAFETY: the stdlib list externs are link-resolved from
    // libcobrust_stdlib.a; `__cobrust_list_new` returns a list with `len`
    // zeroed slots, `__cobrust_list_set` writes the in-bounds dims.
    unsafe {
        let list = __cobrust_list_new(8, len);
        for (i, &dim) in shape.iter().enumerate() {
            __cobrust_list_set(list, i as i64, dim as i64);
        }
        list
    }
}

/// `a.ndim` → `i64` (number of axes; ADR-0077 Q3). BORROWS the handle.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_ndim(a: *mut u8) -> i64 {
    if a.is_null() {
        return 0;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    arr.ndim() as i64
}

/// `a.size` → `i64` (total element count; ADR-0077 Q3). BORROWS the
/// handle.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_size(a: *mut u8) -> i64 {
    if a.is_null() {
        return 0;
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    arr.size() as i64
}

// =====================================================================
// ADR-0077 Phase 2a — Buffer method-op / index-write / slice-read.
// `a.dot(b)` / `a[i] = v` / `a[lo:hi]` retarget (at MIR) onto these
// symbols; codegen only declares them. Runtime shape / bounds
// violations abort via `coil_panic` (ADR-0077 Q4 panic-on-violation) —
// a bare scalar/Buffer is returned, never a `Result`, matching numpy's
// raise + the §2.5 "looks like numpy" surface.
// =====================================================================

/// Extract the single `f64` from a 0-d (or 1-element) `Array`,
/// promoting int / bool dtypes (the f64-only Phase-2a `dot` return
/// contract — same promotion as `__cobrust_coil_buffer_getitem`).
fn scalar_array_to_f64(arr: &Array) -> f64 {
    match arr {
        Array::Float64(x) => x.iter().next().copied().unwrap_or(f64::NAN),
        Array::Float32(x) => x.iter().next().copied().map_or(f64::NAN, f64::from),
        Array::Int64(x) => x.iter().next().copied().map_or(f64::NAN, |v| v as f64),
        Array::Int32(x) => x.iter().next().copied().map_or(f64::NAN, f64::from),
        Array::Bool(x) => x
            .iter()
            .next()
            .copied()
            .map_or(f64::NAN, |v| if v { 1.0 } else { 0.0 }),
    }
}

/// `a.dot(b)` → `f64` (ADR-0077 Q5 / Phase 2a). BORROWS both handles.
/// Phase 2a ships the 1-D dot product → scalar (`Array::dot` defers to
/// `linalg::dot`, which for 1-D × 1-D returns a 0-d `Array`; this shim
/// extracts the scalar). A length mismatch is NOT in the static type —
/// `linalg::dot` raises `LinalgShapeError`, forwarded to `coil_panic`
/// (ADR-0077 Q4). The 2-D matmul → `Buffer` rank case is a Phase-3
/// follow-up (the manifest carries the f64 scalar return — recorded as
/// the per-rank divergence, ADR-0077 §7).
///
/// # Safety
///
/// `a`, `b` must be live `Buffer` handles (borrowed, never freed here).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_dot(a: *mut u8, b: *mut u8) -> f64 {
    if a.is_null() || b.is_null() {
        coil_panic("coil.Buffer.dot: null operand handle");
    }
    // SAFETY: caller attests both are live Buffer handles. Borrow only.
    let lhs: &Array = unsafe { &*a.cast::<Array>() };
    let rhs: &Array = unsafe { &*b.cast::<Array>() };
    match lhs.dot(rhs) {
        Ok(scalar) => scalar_array_to_f64(&scalar),
        Err(e) => coil_panic(&format!("coil.Buffer.dot: {}", e.message)),
    }
}

/// `a[i] = v` scalar WRITE (ADR-0077 Q2 write-path, Phase 2a). BORROWS
/// `a` mutably and writes `v` into slot `i` in place (sound — the `.cb`
/// scope owns the only handle to the box, ADR-0077 §4 / ADR-0072 Q4).
/// Negative indices are numpy-normalised; an out-of-bounds index aborts
/// via `coil_panic` (ADR-0077 Q4 — NOT a silent no-op; the HEAD legacy
/// `Place::Index` path dropped the write + segfaulted on read-back).
/// `v` is an `f64`; non-f64-dtype buffers cast the written value to the
/// element dtype (the f64-only Phase-2a write contract — int/bool
/// buffers truncate, matching numpy's dtype-preserving assignment).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle. The mutable borrow is exclusive
/// for the duration of the write (no other live alias — scope-local).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_setitem(a: *mut u8, i: i64, v: f64) {
    if a.is_null() {
        coil_panic("coil.Buffer[i] = v: null handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Exclusive
    // borrow — the write site is the sole live reference (scope-local).
    let arr: &mut Array = unsafe { &mut *a.cast::<Array>() };
    let len = arr.shape().first().copied().unwrap_or(0) as i64;
    let idx = if i < 0 { i + len } else { i };
    if idx < 0 || idx >= len {
        coil_panic(&format!(
            "coil.Buffer[{i}] = v: index out of bounds for axis with length {len}"
        ));
    }
    let ix = ndarray::IxDyn(&[idx as usize]);
    match arr {
        Array::Float64(x) => {
            if let Some(slot) = x.get_mut(ix) {
                *slot = v;
            }
        }
        Array::Float32(x) => {
            if let Some(slot) = x.get_mut(ix) {
                *slot = v as f32;
            }
        }
        Array::Int64(x) => {
            if let Some(slot) = x.get_mut(ix) {
                *slot = v as i64;
            }
        }
        Array::Int32(x) => {
            if let Some(slot) = x.get_mut(ix) {
                *slot = v as i32;
            }
        }
        Array::Bool(x) => {
            if let Some(slot) = x.get_mut(ix) {
                *slot = v != 0.0;
            }
        }
    }
}

/// `a[lo:hi]` contiguous slice READ → fresh owned `Buffer` (ADR-0077 Q2
/// slice-path, Phase 2a). BORROWS `a`, returns a COPY of `a[lo..hi]` the
/// `.cb` scope drops once via `__cobrust_coil_buffer_drop`. Phase 2a is
/// the simple `lo:hi` form (default step, both bounds present).
///
/// Bounds discipline (ADR-0077 Q4 panic-on-violation): `lo`/`hi` are
/// numpy-normalised for negatives, but an out-of-bounds `hi > len` (or
/// `lo > len`, or `lo > hi` after normalisation) ABORTS via `coil_panic`
/// — the Cobrust-honest "out-of-bounds slice traps" contract, NOT
/// numpy's silent clamp (numpy clamps an over-long stop; `coil::index::
/// resolve_slice` would also clamp, so this shim pre-checks BEFORE
/// delegating, to trap instead — the explicit choice ADR-0077 Q4
/// records). The result is materialised to an owned `Array` (slicing
/// returns a borrowing view; `to_owned` lifts it off `a`'s storage so
/// the fresh handle is independently droppable).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle. The returned pointer is an owned
/// `Buffer` the `.cb` caller drops once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_buffer_slice(a: *mut u8, lo: i64, hi: i64) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.Buffer[lo:hi]: null handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let arr: &Array = unsafe { &*a.cast::<Array>() };
    let len = arr.shape().first().copied().unwrap_or(0) as i64;
    let start = if lo < 0 { lo + len } else { lo };
    let stop = if hi < 0 { hi + len } else { hi };
    // ADR-0077 Q4 — trap on out-of-bounds rather than clamp. `start` may
    // equal `len` (an empty slice `a[len:len]` is valid); `stop` may NOT
    // exceed `len` (that is the out-of-bounds case the negative test
    // pins). `start > stop` after normalisation is also a violation.
    if start < 0 || start > len || stop < 0 || stop > len || start > stop {
        coil_panic(&format!(
            "coil.Buffer[{lo}:{hi}]: slice out of bounds for axis with length {len}"
        ));
    }
    let view = match arr.slice(crate::index::SliceSpec::range(start, stop)) {
        Ok(v) => v,
        Err(e) => coil_panic(&format!("coil.Buffer[{lo}:{hi}]: {}", e.message)),
    };
    Box::into_raw(Box::new(view.to_owned())).cast::<u8>()
}

// =====================================================================
// ADR-0079 Phase 1 — minimal 2-D / explicit-data constructors.
//
// The `coil.linalg.*` sub-namespace operates on 2-D matrices, but the
// pre-ADR-0079 `.cb` constructor surface was almost entirely 1-D (the
// sole 2-D ctor was `coil.eye(n)`, the identity — degenerate for
// det/solve/inv proofs). These three all-scalar-arg shims build the
// minimal NON-identity matrices the linalg proofs need, each delegating
// to the EXISTING `coil::array_f64(values, shape)` Rust ctor (the
// cheapest path — no `list[f64]`→coil marshalling). Each returns a
// freshly-Boxed `Buffer` handle the `.cb` caller owns + drops once. Kept
// deliberately minimal (fixed small shapes, no `np.matrix` legacy
// footgun, §5 elegance ledger); a general nested-list `coil.array` is a
// follow-up once `list[f64]`→coil marshalling lands.
// =====================================================================

/// `coil.array2x2(a, b, c, d) -> Buffer`. Row-major `2 x 2` f64 matrix
/// `[[a, b], [c, d]]`.
///
/// # Safety
///
/// Returns an owned `Buffer` handle (boxed `coil::Array`), freed once via
/// `__cobrust_coil_buffer_drop`. Safe to call concurrently — allocation-only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_array2x2(a: f64, b: f64, c: f64, d: f64) -> *mut u8 {
    let arr = array_f64(&[a, b, c, d], &[2, 2])
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[2, 2]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.array2x3(a, b, c, d, e, f) -> Buffer`. Row-major `2 x 3` f64
/// matrix `[[a, b, c], [d, e, f]]` — a NON-square matrix, used by the
/// non-square `det` runtime-shape-error test (ADR-0079 §7 / ADR-0017).
///
/// # Safety
///
/// As `__cobrust_coil_array2x2`.
// The six scalar params `a..f` ARE the natural row-major matrix-element
// labels (the `.cb` call is `coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0,
// 6.0)`); renaming them to descriptive words would obscure, not clarify.
#[allow(clippy::many_single_char_names)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_array2x3(
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) -> *mut u8 {
    let arr = array_f64(&[a, b, c, d, e, f], &[2, 3])
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[2, 3]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// `coil.array1d2(a, b) -> Buffer`. A 2-element 1-D f64 vector `[a, b]`
/// with explicit data — an arbitrary RHS (e.g. `[5, 11]` / `[1, 1]`) the
/// `coil.ones` / `coil.mgrid` ctors cannot produce.
///
/// # Safety
///
/// As `__cobrust_coil_array2x2`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_array1d2(a: f64, b: f64) -> *mut u8 {
    let arr = array_f64(&[a, b], &[2])
        .unwrap_or_else(|_| Array::Float64(ndarray::ArrayD::<f64>::zeros(ndarray::IxDyn(&[2]))));
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

// =====================================================================
// ADR-0079 Phase 1 — coil.linalg.* sub-namespace C-ABI surface (the
// FIRST dotted sub-namespace under an ecosystem module, mirroring numpy's
// `np.linalg.*`). The `.cb`-side `coil.linalg.{solve,det,inv}(...)`
// retarget (at MIR) onto these flat `__cobrust_coil_linalg_*` symbols;
// codegen only declares the externs. ZERO new numerical code — each shim
// borrows its handle arg(s) and forwards to the EXISTING pure-Rust kernel
// `coil::linalg::{solve,det,inv}` (which pass the ADR-0017 rtol=1e-6
// gate). Runtime shape / singularity violations (invisible to the static
// type — a `coil.Buffer` carries no rank / conditioning) abort via
// `coil_panic` (ADR-0079 Q4 / ADR-0017 `LinalgShapeError` /
// `SingularMatrix`), matching numpy's raise + the §2.5 "looks like numpy"
// surface (a bare scalar / Buffer is returned, never a `Result`).
// =====================================================================

/// `coil.linalg.solve(a, b) -> Buffer`. Solve `A · x = b` (LU partial
/// pivot, numpy's `np.linalg.solve` / LAPACK `*gesv` analogue). BORROWS
/// both handle args (never frees them); returns a freshly-Boxed solution
/// `Buffer` the `.cb` caller owns. A non-square `A`, incompatible `b`
/// shape, or singular `A` is a RUNTIME `coil_panic` (ADR-0079 Q4 — NOT a
/// silent garbage result).
///
/// # Safety
///
/// `a`, `b` must be live `Buffer` handles (borrowed, never freed here).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_linalg_solve(a: *mut u8, b: *mut u8) -> *mut u8 {
    if a.is_null() || b.is_null() {
        coil_panic("coil.linalg.solve: null operand handle");
    }
    // SAFETY: caller attests both are live Buffer handles. Borrow only.
    let a_ref: &Array = unsafe { &*a.cast::<Array>() };
    let b_ref: &Array = unsafe { &*b.cast::<Array>() };
    match linalg_solve(a_ref, b_ref) {
        Ok(x) => Box::into_raw(Box::new(x)).cast::<u8>(),
        Err(e) => coil_panic(&format!("coil.linalg.solve: {}", e.message)),
    }
}

/// `coil.linalg.det(a) -> f64`. Determinant via LU partial pivot (numpy's
/// `np.linalg.det` / LAPACK `*getrf` ∏-diag analogue). BORROWS the handle
/// arg. Returns a plain `f64` — numpy's 0-d scalar is not a Cobrust type
/// (ADR-0077 Q2 / ADR-0079 §9 honesty), extracted from the kernel's 0-d
/// `Array` via `scalar_array_to_f64`. A NON-square input is a RUNTIME
/// `coil_panic` (`LinalgShapeError`); a *singular* (but square) input is
/// NOT a panic — `det` returns `0.0`, matching numpy + the kernel.
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (borrowed, never freed here).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_linalg_det(a: *mut u8) -> f64 {
    if a.is_null() {
        coil_panic("coil.linalg.det: null handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let a_ref: &Array = unsafe { &*a.cast::<Array>() };
    match linalg_det(a_ref) {
        Ok(scalar) => scalar_array_to_f64(&scalar),
        Err(e) => coil_panic(&format!("coil.linalg.det: {}", e.message)),
    }
}

/// `coil.linalg.inv(a) -> Buffer`. Matrix inverse via `solve(a, I)`
/// (numpy's `np.linalg.inv` / LAPACK `*getrf`+`*getri` analogue). BORROWS
/// the handle arg; returns a freshly-Boxed inverse `Buffer` the `.cb`
/// caller owns. A non-square or singular `A` is a RUNTIME `coil_panic`
/// (`LinalgShapeError` / `SingularMatrix` — ADR-0079 Q4).
///
/// # Safety
///
/// `a` must be a live `Buffer` handle (borrowed, never freed here).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_coil_linalg_inv(a: *mut u8) -> *mut u8 {
    if a.is_null() {
        coil_panic("coil.linalg.inv: null handle");
    }
    // SAFETY: caller attests `a` is a live Buffer handle. Borrow only.
    let a_ref: &Array = unsafe { &*a.cast::<Array>() };
    match linalg_inv(a_ref) {
        Ok(out) => Box::into_raw(Box::new(out)).cast::<u8>(),
        Err(e) => coil_panic(&format!("coil.linalg.inv: {}", e.message)),
    }
}

#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
// Shim round-trip tests assert EXACT buffer contents (e.g. the non-NaN
// lane of a min/max result is exactly `3.0`); a strict float compare is
// correct here — same as the `elementwise` test module.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    // ADR-0079 Phase 1 — test-only definition of the stdlib `__cobrust_panic`
    // ABI symbol. The real impl lives in `cobrust-stdlib` (linked as a static
    // `.a` only at `.cb`-link time, NOT into this crate's lib-test binary);
    // the coil cabi shims declare it `extern` (line ~92). Any unit test that
    // exercises a `coil_panic`-referencing shim (the `coil.linalg.*` family
    // forwards `LinalgShapeError` / `SingularMatrix` to it) would otherwise
    // fail to LINK with an undefined `__cobrust_panic`. This stub aborts —
    // honouring the `-> !` contract — so the panic-path is observable in-
    // process via `#[should_panic]` if ever needed; the happy-path tests
    // below never reach it. (The pre-ADR-0079 cabi panic-shims —
    // `buffer_dot` / `buffer_add` etc. — had NO lib unit tests for exactly
    // this reason; the stub lets the linalg shims gain in-process numeric
    // coverage beyond the CLI E2E corpus.)
    #[unsafe(no_mangle)]
    extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> ! {
        // SAFETY: callers (the coil_panic helper) pass a valid UTF-8
        // `&str`'s `(ptr, len)`. Reconstruct it for the abort message.
        let msg = unsafe { std::slice::from_raw_parts(ptr, len) };
        let msg = String::from_utf8_lossy(msg);
        panic!("__cobrust_panic (test stub): {msg}");
    }

    /// Serialize the count-asserting tests to keep `DROP_COUNT`
    /// deltas deterministic under cargo's default-parallel runner.
    static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `coil.zeros(3)` + `__cobrust_coil_buffer_drop` drop exactly once.
    #[test]
    fn zeros_then_drop_increments_counter_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_zeros(3);
            assert!(!buf.is_null(), "zeros(3) returned null");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// `coil.ones(3)` round trip.
    #[test]
    fn ones_then_drop_increments_counter_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_ones(3);
            assert!(!buf.is_null(), "ones(3) returned null");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// `coil.eye(2)` round trip — 2-D shape proves the chain handles
    /// non-1-D buffers (drop is shape-agnostic).
    #[test]
    fn eye_then_drop_increments_counter_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_eye(2);
            assert!(!buf.is_null(), "eye(2) returned null");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// `print_buffer` is a side-effecting borrow + 0 sentinel.
    #[test]
    fn print_buffer_borrows_handle_returns_zero() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_zeros(3);
            let ret = __cobrust_coil_print_buffer(buf);
            assert_eq!(ret, 0, "print_buffer must return Ty::Int sentinel 0");
            __cobrust_coil_buffer_drop(buf);
        }
        // print_buffer borrows; the only drop is the explicit one at scope exit.
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// Null tolerance — `_drop` is a no-op on null and never touches
    /// the counter; `print_buffer` returns -1 on null.
    #[test]
    fn null_handles_are_tolerated() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            assert_eq!(
                __cobrust_coil_print_buffer(std::ptr::null_mut()),
                -1,
                "print_buffer on null must yield -1 sentinel"
            );
            __cobrust_coil_buffer_drop(std::ptr::null_mut());
        }
        assert_eq!(drop_count(), before, "null drop must be no-op");
    }

    /// Negative `n` clamps to zero (defensive — typechecker passes i64
    /// without a `usize` constraint today; future widening could lift
    /// this).
    #[test]
    fn negative_n_clamps_to_empty() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_zeros(-1);
            assert!(!buf.is_null(), "zeros(-1) must clamp to empty, not null");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    // =====================================================================
    // #145 BATCH 11 — spacing/value constructor shim tests. ALL-scalar-arg
    // producers (no Buffer input); values asserted via the `mean` reducer
    // (proves the fresh constructor handle feeds another op) + drop-once.
    // =====================================================================

    /// `coil.linspace(0, 10, 5)` -> `[0, 2.5, 5, 7.5, 10]`; `mean == 5.0`.
    /// Drops exactly once at scope exit (all-scalar-arg producer, no
    /// borrow to balance).
    #[test]
    fn linspace_round_trip_mean() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_linspace(0.0, 10.0, 5);
            assert!(!buf.is_null(), "linspace returned null");
            let m = __cobrust_coil_mean(buf);
            assert!((m - 5.0).abs() < 1e-12, "mean(linspace(0,10,5)) got {m}");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// `coil.linspace(0, 1, 0)` -> empty buffer (num <= 0 clamps), `mean`
    /// of an empty buffer is `NaN` (matches `coil.mean([])`).
    #[test]
    fn linspace_num_zero_is_empty() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let buf = __cobrust_coil_linspace(0.0, 1.0, 0);
            assert!(!buf.is_null(), "linspace(0,1,0) must clamp to empty buffer");
            assert!(
                __cobrust_coil_mean(buf).is_nan(),
                "mean of empty linspace must be NaN"
            );
            __cobrust_coil_buffer_drop(buf);
        }
    }

    /// `coil.logspace(0, 2, 3)` -> `[1, 10, 100]`; `mean == 37.0`.
    #[test]
    fn logspace_round_trip_mean() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_logspace(0.0, 2.0, 3);
            assert!(!buf.is_null(), "logspace returned null");
            let m = __cobrust_coil_mean(buf);
            assert!((m - 37.0).abs() < 1e-12, "mean(logspace(0,2,3)) got {m}");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// `coil.full(4, 7.0)` -> `[7, 7, 7, 7]`; `mean == 7.0`.
    #[test]
    fn full_round_trip_mean() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_full(4, 7.0);
            assert!(!buf.is_null(), "full returned null");
            let m = __cobrust_coil_mean(buf);
            assert!((m - 7.0).abs() < 1e-12, "mean(full(4,7)) got {m}");
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1, "Buffer must drop exactly once");
    }

    /// `coil.full(0, 5.0)` and `coil.full(-1, 5.0)` both clamp to an empty
    /// buffer (mirror `coil.zeros` negative-n clamp); `mean` is `NaN`.
    #[test]
    fn full_zero_and_negative_n_are_empty() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let z = __cobrust_coil_full(0, 5.0);
            assert!(!z.is_null(), "full(0,5) must clamp to empty buffer");
            assert!(
                __cobrust_coil_mean(z).is_nan(),
                "mean(full(0,5)) must be NaN"
            );
            __cobrust_coil_buffer_drop(z);
            let neg = __cobrust_coil_full(-1, 5.0);
            assert!(!neg.is_null(), "full(-1,5) must clamp to empty buffer");
            assert!(
                __cobrust_coil_mean(neg).is_nan(),
                "mean(full(-1,5)) must be NaN"
            );
            __cobrust_coil_buffer_drop(neg);
        }
    }

    // =====================================================================
    // Stream W P0 增量 shim tests.
    // =====================================================================

    /// `coil.mgrid(0, 5)` returns a 5-elem buffer; drops once at scope.
    #[test]
    fn mgrid_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_mgrid(0, 5);
            assert!(!buf.is_null());
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.ogrid(0, 5)` returns a 5-elem buffer; drops once.
    #[test]
    fn ogrid_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let buf = __cobrust_coil_ogrid(0, 5);
            assert!(!buf.is_null());
            __cobrust_coil_buffer_drop(buf);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.broadcast_to(a, 4)` borrows `a` and yields a fresh
    /// handle; both drop exactly once.
    #[test]
    fn broadcast_to_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_ones(2);
            let b = __cobrust_coil_broadcast_to(a, 4);
            assert!(!a.is_null());
            assert!(!b.is_null());
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.split(a, 3)` first chunk + drop discipline.
    #[test]
    fn split_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_mgrid(0, 6);
            let c = __cobrust_coil_split(a, 3);
            assert!(!c.is_null());
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(c);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.mean(mgrid(0,5))` borrows the handle (counter unchanged
    /// until the explicit drop) and yields `(0+1+2+3+4)/5 = 2.0`.
    #[test]
    fn mean_of_mgrid_0_5_is_two() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_mgrid(0, 5);
            let m = __cobrust_coil_mean(a);
            assert!((m - 2.0).abs() < 1e-12, "mean got {m}");
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.median(mgrid(0,5))` = 2.0 (middle of [0,1,2,3,4]).
    #[test]
    fn median_of_mgrid_0_5_is_two() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_mgrid(0, 5);
            let m = __cobrust_coil_median(a);
            assert!((m - 2.0).abs() < 1e-12, "median got {m}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.std(mgrid(0,5))` = sqrt(2) ≈ 1.41421.
    #[test]
    fn std_of_mgrid_0_5_is_sqrt_two() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_mgrid(0, 5);
            let s = __cobrust_coil_std(a);
            assert!((s - 2.0_f64.sqrt()).abs() < 1e-12, "std got {s}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.var(mgrid(0,5))` = 2.0.
    #[test]
    fn var_of_mgrid_0_5_is_two() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_mgrid(0, 5);
            let v = __cobrust_coil_var(a);
            assert!((v - 2.0).abs() < 1e-12, "var got {v}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// Aggregates on null handle yield NaN sentinel rather than
    /// panic. Drop on null is a no-op.
    #[test]
    fn aggregates_on_null_yield_nan() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            assert!(__cobrust_coil_mean(std::ptr::null_mut()).is_nan());
            assert!(__cobrust_coil_median(std::ptr::null_mut()).is_nan());
            assert!(__cobrust_coil_std(std::ptr::null_mut()).is_nan());
            assert!(__cobrust_coil_var(std::ptr::null_mut()).is_nan());
        }
    }

    // -- #145 statistics gap-closure cabi shims ------------------------

    /// `coil.ptp(mgrid(0,5))` = 4.0 (max 4 - min 0); borrows + drops once.
    #[test]
    fn ptp_of_mgrid_0_5_is_four() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_mgrid(0, 5);
            let p = __cobrust_coil_ptp(a);
            assert!((p - 4.0).abs() < 1e-12, "ptp got {p}");
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.nansum([1.0, NaN])` = 1.0 (NaN treated as zero).
    #[test]
    fn nansum_skips_nan_via_cabi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, f64::NAN);
            let s = __cobrust_coil_nansum(a);
            assert!((s - 1.0).abs() < 1e-12, "nansum got {s}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.nanmean([2.0, NaN])` = 2.0 (mean over the single non-NaN).
    #[test]
    fn nanmean_skips_nan_via_cabi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(2.0, f64::NAN);
            let m = __cobrust_coil_nanmean(a);
            assert!((m - 2.0).abs() < 1e-12, "nanmean got {m}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.nanstd([1.0, 3.0])` = 1.0 (population std, no NaN).
    #[test]
    fn nanstd_population_via_cabi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 3.0);
            let s = __cobrust_coil_nanstd(a);
            assert!((s - 1.0).abs() < 1e-12, "nanstd got {s}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.percentile(mgrid(0,5), 50)` = 2.0 (median of [0,1,2,3,4]).
    /// The 2-arg `(Buffer, f64) -> f64` shim path.
    #[test]
    fn percentile_p50_of_mgrid_0_5_is_two() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_mgrid(0, 5);
            let p = __cobrust_coil_percentile(a, 50.0);
            assert!((p - 2.0).abs() < 1e-12, "percentile got {p}");
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// The new aggregates handle a null pointer: `nansum` → 0.0
    /// sentinel; `ptp` / `nanmean` / `nanstd` / `percentile` → NaN.
    #[test]
    fn new_aggregates_on_null() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            assert!(__cobrust_coil_ptp(std::ptr::null_mut()).is_nan());
            assert!((__cobrust_coil_nansum(std::ptr::null_mut())).abs() < 1e-12);
            assert!(__cobrust_coil_nanmean(std::ptr::null_mut()).is_nan());
            assert!(__cobrust_coil_nanstd(std::ptr::null_mut()).is_nan());
            assert!(__cobrust_coil_percentile(std::ptr::null_mut(), 50.0).is_nan());
        }
    }

    // -- #145 BATCH 7: min / max / prod VALUE-reduction cabi shims ------
    //
    // (min/max of an EMPTY or NULL handle `coil_panic`s — a process abort
    // that cannot be asserted from a unit test; the e2e suite's
    // empty-min/max clean-trap tests cover that edge. Here we exercise the
    // borrow + drop-once path for the value + NaN-propagate cases, plus
    // prod's TOTAL null → 1.0 identity.)

    /// `coil.min([2.0, 5.0]) = 2.0`, `coil.max(...) = 5.0`; each borrows the
    /// handle (no rebox) and the buffer drops exactly once.
    #[test]
    fn min_max_via_cabi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(2.0, 5.0);
            let lo = __cobrust_coil_min(a);
            let hi = __cobrust_coil_max(a);
            assert!((lo - 2.0).abs() < 1e-12, "min got {lo}");
            assert!((hi - 5.0).abs() < 1e-12, "max got {hi}");
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.prod([2.0, 3.0]) = 6.0`; borrows + drops once.
    #[test]
    fn prod_via_cabi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(2.0, 3.0);
            let p = __cobrust_coil_prod(a);
            assert!((p - 6.0).abs() < 1e-12, "prod got {p}");
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `min`/`max`/`prod` PROPAGATE NaN (any NaN lane → NaN), like `mean`.
    #[test]
    fn min_max_prod_propagate_nan_via_cabi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, f64::NAN);
            assert!(__cobrust_coil_min(a).is_nan(), "min([1,nan]) must be NaN");
            assert!(__cobrust_coil_max(a).is_nan(), "max([1,nan]) must be NaN");
            assert!(__cobrust_coil_prod(a).is_nan(), "prod([1,nan]) must be NaN");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.prod` is TOTAL: a NULL handle yields the multiplicative
    /// identity `1.0` (numpy `np.prod([]) == 1.0`), NEVER a trap. (min/max
    /// of null trap — not unit-testable; see the e2e empty-trap tests.)
    #[test]
    fn prod_on_null_is_one() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let p = __cobrust_coil_prod(std::ptr::null_mut());
            assert!((p - 1.0).abs() < 1e-12, "prod(null) must be 1.0, got {p}");
        }
    }

    // -- ADR-0079 Phase 1: 2-D ctors + coil.linalg.* shims ------------

    /// `coil.array2x2(1,2,3,4)` builds a `2 x 2`; drops once.
    #[test]
    fn array2x2_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x2(1.0, 2.0, 3.0, 4.0);
            assert!(!a.is_null());
            let arr: &Array = &*a.cast::<Array>();
            assert_eq!(arr.shape(), &[2, 2]);
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.linalg.det(array2x2(1,2,3,4))` == `1*4 - 2*3` == `-2.0`.
    /// BORROWS the handle (drop is explicit + once).
    #[test]
    fn linalg_det_known_2x2_is_minus_two() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x2(1.0, 2.0, 3.0, 4.0);
            let d = __cobrust_coil_linalg_det(a);
            assert!((d - (-2.0)).abs() < 1e-9, "det got {d}");
            __cobrust_coil_buffer_drop(a);
        }
        assert_eq!(drop_count() - before, 1);
    }

    /// `coil.linalg.det(eye(3))` == `1.0` (the identity-tier positive).
    #[test]
    fn linalg_det_eye3_is_one() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_eye(3);
            let d = __cobrust_coil_linalg_det(a);
            assert!((d - 1.0).abs() < 1e-9, "det(eye3) got {d}");
            __cobrust_coil_buffer_drop(a);
        }
    }

    /// `coil.linalg.solve(array2x2(1,2,3,4), array1d2(5,11))` == `[1, 2]`.
    /// Borrows both inputs; the fresh solution drops once (3 total drops).
    #[test]
    fn linalg_solve_known_2x2() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x2(1.0, 2.0, 3.0, 4.0);
            let b = __cobrust_coil_array1d2(5.0, 11.0);
            let x = __cobrust_coil_linalg_solve(a, b);
            assert!(!x.is_null());
            let xr: &Array = &*x.cast::<Array>();
            assert_eq!(xr.shape(), &[2]);
            assert!((__cobrust_coil_buffer_getitem(x, 0) - 1.0).abs() < 1e-9);
            assert!((__cobrust_coil_buffer_getitem(x, 1) - 2.0).abs() < 1e-9);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(x);
        }
        assert_eq!(drop_count() - before, 3);
    }

    /// `coil.linalg.inv(array2x2(2,0,0,4))` == `[[0.5,0],[0,0.25]]`.
    /// Borrows the input; the fresh inverse drops once (2 total drops).
    #[test]
    fn linalg_inv_diag_2x2() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x2(2.0, 0.0, 0.0, 4.0);
            let i = __cobrust_coil_linalg_inv(a);
            assert!(!i.is_null());
            assert_eq!(
                array_repr(&*i.cast::<Array>()),
                "array([[0.5, 0], [0, 0.25]], dtype=float64)"
            );
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(i);
        }
        assert_eq!(drop_count() - before, 2);
    }

    // -- #145 array-manipulation shims (transpose / flatten / ravel /
    //    concatenate / vstack / hstack) — round-trip + drop-once -------

    /// `coil.transpose(array2x3(...))` → a `(3, 2)` Buffer; borrows the
    /// input, the fresh result drops once (2 total drops).
    #[test]
    fn transpose_shim_2x3_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
            let t = __cobrust_coil_transpose(a);
            assert!(!t.is_null());
            let arr: &Array = &*t.cast::<Array>();
            assert_eq!(arr.shape(), &[3, 2]);
            assert_eq!(
                array_repr(arr),
                "array([[1, 4], [2, 5], [3, 6]], dtype=float64)"
            );
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(t);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.flatten(array2x3(...))` → a `(6,)` C-order Buffer.
    #[test]
    fn flatten_shim_2x3_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
            let f = __cobrust_coil_flatten(a);
            let arr: &Array = &*f.cast::<Array>();
            assert_eq!(arr.shape(), &[6]);
            assert_eq!(array_repr(arr), "array([1, 2, 3, 4, 5, 6], dtype=float64)");
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(f);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.ravel(array2x2(...))` matches `coil.flatten` values.
    #[test]
    fn ravel_shim_matches_flatten() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array2x2(7.0, 8.0, 9.0, 10.0);
            let r = __cobrust_coil_ravel(a);
            let arr: &Array = &*r.cast::<Array>();
            assert_eq!(array_repr(arr), "array([7, 8, 9, 10], dtype=float64)");
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
    }

    /// `coil.concatenate(array2x3, array2x3)` → a `(4, 3)` Buffer; borrows
    /// both inputs, the fresh result drops once (3 total drops).
    #[test]
    fn concatenate_shim_axis0_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
            let b = __cobrust_coil_array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0);
            let c = __cobrust_coil_concatenate(a, b);
            let arr: &Array = &*c.cast::<Array>();
            assert_eq!(arr.shape(), &[4, 3]);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(c);
        }
        assert_eq!(drop_count() - before, 3);
    }

    /// `coil.vstack(array2x3, array2x3)` → a `(4, 3)` Buffer.
    #[test]
    fn vstack_shim_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
            let b = __cobrust_coil_array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0);
            let v = __cobrust_coil_vstack(a, b);
            assert_eq!((*v.cast::<Array>()).shape(), &[4, 3]);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(v);
        }
    }

    /// `coil.hstack(array2x3, array2x3)` → a `(2, 6)` Buffer (axis-1 join).
    #[test]
    fn hstack_shim_2d_axis1_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
            let b = __cobrust_coil_array2x3(7.0, 8.0, 9.0, 10.0, 11.0, 12.0);
            let h = __cobrust_coil_hstack(a, b);
            let arr: &Array = &*h.cast::<Array>();
            assert_eq!(arr.shape(), &[2, 6]);
            assert_eq!(
                array_repr(arr),
                "array([[1, 2, 3, 7, 8, 9], [4, 5, 6, 10, 11, 12]], dtype=float64)"
            );
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(h);
        }
    }

    /// #163 BATCH 13 — `coil.maximum(a, b)` over two `(2,)` buffers,
    /// borrows both, fresh result drops once (3 total drops). A NaN in `b`
    /// PROPAGATES (`maximum([1,2],[3,nan]) = [3, nan]`).
    #[test]
    fn maximum_shim_round_trip_nan_propagates() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let b = __cobrust_coil_array1d2(3.0, f64::NAN);
            let m = __cobrust_coil_maximum(a, b);
            let arr: &Array = &*m.cast::<Array>();
            assert_eq!(arr.shape(), &[2]);
            match arr {
                Array::Float64(v) => {
                    assert_eq!(v[0], 3.0);
                    assert!(v[1].is_nan(), "maximum PROPAGATES NaN");
                }
                _ => panic!("expected Float64"),
            }
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(m);
        }
        assert_eq!(drop_count() - before, 3);
    }

    /// #163 BATCH 13 — `coil.fmax(a, b)` IGNORES NaN: `fmax([1,2],[3,nan])
    /// = [3, 2]` (the non-NaN operand 2 wins at idx 1). The discriminating
    /// shim test vs `maximum` above (SAME inputs, different idx-1 result).
    #[test]
    fn fmax_shim_round_trip_nan_ignored() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let b = __cobrust_coil_array1d2(3.0, f64::NAN);
            let m = __cobrust_coil_fmax(a, b);
            match &*m.cast::<Array>() {
                Array::Float64(v) => {
                    assert_eq!(v[0], 3.0);
                    assert_eq!(v[1], 2.0, "fmax IGNORES NaN — picks 2.0");
                }
                _ => panic!("expected Float64"),
            }
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(m);
        }
    }

    /// #163 BATCH 13 — `coil.minimum` / `coil.fmin` round-trip (no NaN):
    /// `minimum([2,5],[4,1]) = [2,1]`, `fmin = [2,1]` (agree).
    #[test]
    fn minimum_fmin_shim_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(2.0, 5.0);
            let b = __cobrust_coil_array1d2(4.0, 1.0);
            let mn = __cobrust_coil_minimum(a, b);
            let fm = __cobrust_coil_fmin(a, b);
            assert_eq!(
                array_repr(&*mn.cast::<Array>()),
                "array([2, 1], dtype=float64)"
            );
            assert_eq!(
                array_repr(&*fm.cast::<Array>()),
                "array([2, 1], dtype=float64)"
            );
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(mn);
            __cobrust_coil_buffer_drop(fm);
        }
    }

    /// Null-handle defense: the 1-arg shims abort (proven indirectly by
    /// the `coil_panic` path); here we only assert the non-null inputs
    /// produce non-null fresh handles (the abort path diverges and cannot
    /// be unit-tested without a sub-process — covered by the `.cb` e2e
    /// `_traps` cases instead).
    #[test]
    fn manip_shims_return_nonnull() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array2x2(1.0, 2.0, 3.0, 4.0);
            let t = __cobrust_coil_transpose(a);
            let f = __cobrust_coil_flatten(a);
            let r = __cobrust_coil_ravel(a);
            assert!(!t.is_null() && !f.is_null() && !r.is_null());
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(t);
            __cobrust_coil_buffer_drop(f);
            __cobrust_coil_buffer_drop(r);
        }
    }

    // -- #145 BATCH 8: `coil.where(cond, a, b)` 3-Buffer select shim ----

    /// `coil.where(cond, a, b)` with a bool-dtype cond from `a < b`: the
    /// THREE-Buffer round-trip. cond=[1,5]<[3,2]=[True,False];
    /// where(cond, [10,20], [30,40]) -> [10, 40]. Borrows all THREE inputs
    /// (none freed); the fresh result drops once. Total drops: 5
    /// (cond, x, y, the `a`/`b` comparison operands, the result) — we drop
    /// each handle we create exactly once.
    #[test]
    #[allow(clippy::many_single_char_names)]
    fn where_shim_three_buffer_round_trip() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            // Build cond from a REAL comparison: a<b yields a Bool Buffer.
            let a = __cobrust_coil_array1d2(1.0, 5.0);
            let b = __cobrust_coil_array1d2(3.0, 2.0);
            let cond = __cobrust_coil_buffer_lt(a, b); // [True, False]
            let x = __cobrust_coil_array1d2(10.0, 20.0);
            let y = __cobrust_coil_array1d2(30.0, 40.0);
            let r = __cobrust_coil_where(cond, x, y);
            let arr: &Array = &*r.cast::<Array>();
            // [10, 40]: lane 0 True -> x[0]=10, lane 1 False -> y[1]=40.
            assert_eq!(array_repr(arr), "array([10, 40], dtype=float64)");
            // cond inputs survive (borrow-only); drop every created handle.
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(cond);
            __cobrust_coil_buffer_drop(x);
            __cobrust_coil_buffer_drop(y);
            __cobrust_coil_buffer_drop(r);
        }
        assert_eq!(drop_count() - before, 6);
    }

    /// `coil.where` borrows its inputs (does NOT free them): the inputs are
    /// still readable AFTER the call, and the fresh result is independent.
    #[test]
    fn where_shim_borrows_inputs() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let cond = __cobrust_coil_array1d2(1.0, 0.0); // numeric truthy mask
            let a = __cobrust_coil_array1d2(7.0, 8.0);
            let b = __cobrust_coil_array1d2(9.0, 10.0);
            let r = __cobrust_coil_where(cond, a, b);
            // Inputs are NOT freed — still readable post-call.
            let a_ref: &Array = &*a.cast::<Array>();
            assert_eq!(array_repr(a_ref), "array([7, 8], dtype=float64)");
            // result: cond=[1,0] -> [a[0]=7, b[1]=10].
            let arr: &Array = &*r.cast::<Array>();
            assert_eq!(array_repr(arr), "array([7, 10], dtype=float64)");
            __cobrust_coil_buffer_drop(cond);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(r);
        }
    }

    /// Null-handle defense (indirect): non-null inputs produce a non-null
    /// fresh handle. The `coil_panic` abort on a null / non-conformable
    /// triple diverges and is covered by the `.cb` e2e `_traps` case.
    #[test]
    fn where_shim_returns_nonnull() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let cond = __cobrust_coil_array1d2(1.0, 1.0);
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let b = __cobrust_coil_array1d2(3.0, 4.0);
            let r = __cobrust_coil_where(cond, a, b);
            assert!(!r.is_null());
            __cobrust_coil_buffer_drop(cond);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(b);
            __cobrust_coil_buffer_drop(r);
        }
    }

    // -- #145 SCALAR-ARG ufunc BATCH 6: clip / power shims --------------
    // Use the 1-D `array1d2(a, b)` ctor: `__cobrust_coil_buffer_getitem`
    // reads a FLAT element only on a 1-D array (on a 2-D it would index a
    // ROW view), so a 1-D buffer is the right per-element read surface.

    /// `coil.clip(array1d2(1, 9), 2, 7)` clamps `[1, 9]` → `[2, 7]` (a 1-D
    /// buffer). BORROWS the input; the fresh result drops once (2 total
    /// drops). Proves the `(ptr, f64, f64) -> ptr` shim shape — the FIRST
    /// coil shim with TWO trailing f64 scalars.
    #[test]
    fn clip_shim_clamps_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 9.0);
            let r = __cobrust_coil_clip(a, 2.0, 7.0);
            assert!(!r.is_null());
            // [1, 9] -> clip[2, 7] -> [2, 7].
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 7.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
        assert_eq!(
            drop_count() - before,
            2,
            "input + fresh result drop once each"
        );
    }

    /// `coil.clip(_, 7, 2)` with `lo > hi`: the UPPER bound wins (numpy
    /// `minimum(maximum(a, lo), hi)`). `clip([1, 9], 7, 2) = [2, 2]`.
    #[test]
    fn clip_shim_lo_gt_hi_clamps_to_hi() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 9.0);
            let r = __cobrust_coil_clip(a, 7.0, 2.0);
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 2.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
    }

    /// `coil.power(array1d2(2, 3), 2.0)` squares `[2, 3]` → `[4, 9]`.
    /// BORROWS the input; the fresh result drops once (2 total drops).
    /// Proves the `(ptr, f64) -> ptr` shim shape (percentile's shape, but
    /// Buffer-returning).
    #[test]
    fn power_shim_squares_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(2.0, 3.0);
            let r = __cobrust_coil_power(a, 2.0);
            assert!(!r.is_null());
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 4.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 9.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.power(_, 0.5)` is `sqrt`: `power([4, 9], 0.5) = [2, 3]`.
    #[test]
    fn power_shim_half_is_sqrt() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(4.0, 9.0);
            let r = __cobrust_coil_power(a, 0.5);
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 3.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
    }

    // -- #145 REARRANGE / REPEAT BATCH 10: diff / flip / roll / repeat /
    // tile shims. `diff` / `flip` ride `buffer_unary` (`(ptr) -> ptr`);
    // `roll` / `repeat` / `tile` are `(ptr, i64) -> ptr` (the i64-scalar
    // mirror of the BATCH-6 clip / power f64-scalar shims). Use the 1-D
    // `array1d2(a, b)` ctor so `__cobrust_coil_buffer_getitem` reads a FLAT
    // element. BORROW the input; the fresh result drops once (2 total
    // drops on the drop-counted cases).

    /// `coil.flip(array1d2(1, 2))` reverses `[1, 2]` → `[2, 1]`. BORROWS the
    /// input; the fresh result drops once (2 total drops). Proves the 1-arg
    /// `(ptr) -> ptr` shim shape (transpose's shape).
    #[test]
    fn flip_shim_reverses_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let r = __cobrust_coil_flip(a);
            assert!(!r.is_null());
            // [1, 2] -> flip -> [2, 1].
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 1.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
        assert_eq!(
            drop_count() - before,
            2,
            "input + fresh result drop once each"
        );
    }

    /// `coil.diff(array1d2(1, 4))` → `[3]` (one element, `4 - 1`). Proves
    /// the `diff` shim returns a fresh non-null handle of length size-1.
    #[test]
    fn diff_shim_adjacent_difference() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 4.0);
            let r = __cobrust_coil_diff(a);
            assert!(!r.is_null());
            // diff([1, 4]) = [4 - 1] = [3] (a single element at flat idx 0).
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 3.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
    }

    /// `coil.roll(array1d2(1, 2), 1)` cyclically shifts `[1, 2]` → `[2, 1]`
    /// (SAME shape). BORROWS the input; the fresh result drops once (2 total
    /// drops). Proves the `(ptr, i64) -> ptr` shim shape — the FIRST coil
    /// shim with a trailing i64 scalar.
    #[test]
    fn roll_shim_cyclic_shift_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let r = __cobrust_coil_roll(a, 1);
            assert!(!r.is_null());
            // roll([1, 2], 1) = [2, 1] (last element wraps to the front).
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 1.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.roll(_, -1)` rolls LEFT: `roll([1, 2], -1) = [2, 1]`. Pins the
    /// negative-k (left-roll) path of the i64-scalar shim.
    #[test]
    fn roll_shim_negative_k_rolls_left() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let r = __cobrust_coil_roll(a, -1);
            // roll([1, 2], -1) shifts left: [2, 1].
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 1.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
    }

    /// `coil.repeat(array1d2(1, 2), 2)` repeats each element → `[1, 1, 2, 2]`
    /// (len n*size = 4). BORROWS the input; the fresh result drops once (2
    /// total drops).
    #[test]
    fn repeat_shim_each_element_and_drops_once() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = drop_count();
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let r = __cobrust_coil_repeat(a, 2);
            assert!(!r.is_null());
            // repeat([1, 2], 2) = [1, 1, 2, 2].
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 1.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 1.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 2) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 3) - 2.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
        assert_eq!(drop_count() - before, 2);
    }

    /// `coil.tile(array1d2(1, 2), 2)` tiles the whole array → `[1, 2, 1, 2]`
    /// (len n*size = 4). Pins the whole-repeat (vs. `repeat`'s per-element)
    /// interleave of the i64-scalar shim.
    #[test]
    fn tile_shim_whole_array() {
        let _guard = DROP_COUNTER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            let a = __cobrust_coil_array1d2(1.0, 2.0);
            let r = __cobrust_coil_tile(a, 2);
            assert!(!r.is_null());
            // tile([1, 2], 2) = [1, 2, 1, 2].
            assert!((__cobrust_coil_buffer_getitem(r, 0) - 1.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 1) - 2.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 2) - 1.0).abs() < 1e-12);
            assert!((__cobrust_coil_buffer_getitem(r, 3) - 2.0).abs() < 1e-12);
            __cobrust_coil_buffer_drop(a);
            __cobrust_coil_buffer_drop(r);
        }
    }
}
