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

use std::sync::atomic::{AtomicU64, Ordering};

use crate::array::Array;
use crate::constructors::{eye as coil_eye, ones as coil_ones, zeros as coil_zeros};
use crate::dtype::Dtype;
use crate::print::array_repr;

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
/// `coil.ones` / `coil.eye` that has not already been dropped.
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

#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
mod tests {
    use super::*;

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
}
