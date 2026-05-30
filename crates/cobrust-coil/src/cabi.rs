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

use crate::aggregates::{mean_scalar, median_scalar, split_first_chunk, std_scalar, var_scalar};
use crate::array::Array;
use crate::broadcast::broadcast_shape;
use crate::broadcast_extra::broadcast_to_1d;
use crate::constructors::{array_f64, eye as coil_eye, ones as coil_ones, zeros as coil_zeros};
use crate::dtype::Dtype;
use crate::grid::{mgrid_1d, ogrid_1d};
use crate::linalg::{det as linalg_det, inv as linalg_inv, solve as linalg_solve};
use crate::print::array_repr;

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
/// `coil.ones` / `coil.eye` / `coil.mgrid` / `coil.ogrid` /
/// `coil.broadcast_to` / `coil.split` that has not already been
/// dropped.
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
}
