//! `std.reduce` — list-reducer runtime shims for the Python builtins
//! `min(xs)` / `max(xs)` / `sum(xs)` (ADR-0090).
//!
//! # The list-consume (borrow-read) mechanism
//!
//! These shims are the first builtins that CONSUME a `list[T]`
//! *argument* (every prior list shim — `list_get` / `list_set` /
//! `list_len` — is a per-element accessor; `min` / `max` / `sum`
//! REDUCE the whole list to a scalar). A list is already passed to a
//! callee by POINTER (`cobrust-mir/src/lower.rs` `is_copy_type`
//! returns `true` for `Ty::List(_)`, so the operand is Copy-at-call and
//! the `.cb` scope retains ownership), so the reducer receives the SAME
//! `*mut u8` the `.cb` local holds.
//!
//! Critically, these shims **BORROW** (read-only) the list — exactly
//! like [`__cobrust_list_len`] / [`__cobrust_list_get`] in
//! `collections.rs`, which dereference `&*list.cast::<ListI64Layout>()`
//! via a SHARED reference and never `Box::from_raw`. The reducer:
//!
//! 1. reads the length with [`__cobrust_list_len`];
//! 2. reads each slot with [`__cobrust_list_get`] (an `i64`);
//! 3. for the float family, reinterprets each `i64` slot as the stored
//!    `f64` bit-pattern (`f64::from_bits`) — a `list[f64]`'s elements
//!    are materialised as `to_bits()` i64 slots (the codegen lowers a
//!    `Constant::Float` to `Constant::Float(v.to_bits())`), and the
//!    list index-read path bitcasts them back.
//!
//! The shim does **NOT** free the list (no `Box::from_raw`, no drop) —
//! the `.cb` scope drops it exactly once at scope exit. A double-free
//! (shim frees + scope drops) would be a use-after-free; the
//! `list_reduce_e2e` corpus includes a list-reused-after-`min` test to
//! lock the borrow discipline.
//!
//! # Empty-list policy (ADR-0090 §"Empty list")
//!
//! - `min([])` / `max([])` — CPython raises `ValueError: min() arg is
//!   an empty sequence`. Cobrust has no exceptions (Constitution §2.2);
//!   the reducer TRAPS via [`crate::panic::panic`] (a clean non-zero
//!   exit, `INTERNAL_PANIC`).
//! - `sum([])` — CPython returns `0` (int). The int reducer returns
//!   `0`; the float reducer returns `0.0`.
//!
//! All four element/op combinations of `{min, max} × {int, float}` plus
//! `sum × {int, float}` are exposed as `extern "C"` symbols the
//! intrinsic-rewrite pass (`cobrust-cli/src/build/intrinsics.rs`)
//! retargets `min` / `max` / `sum` onto, keyed on the call's element /
//! destination type.

use crate::collections::{__cobrust_list_get, __cobrust_list_len};

/// `min(xs: list[int]) -> int` — the smallest element.
///
/// Borrows `list` (reads length + each `i64` slot), never frees it.
/// Traps on an empty list (CPython `ValueError` parity, §2.2 no
/// exceptions → clean non-zero exit).
///
/// # Safety
///
/// `list` must be a pointer returned by `__cobrust_list_new` and not
/// yet dropped, OR null (a null list reads as length 0 → empty trap).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_min_int(list: *mut u8) -> i64 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    if n <= 0 {
        crate::panic::panic("min() arg is an empty sequence");
    }
    // SAFETY: 0 <= i < n, in-bounds for `__cobrust_list_get`.
    let mut acc = unsafe { __cobrust_list_get(list, 0) };
    let mut i = 1;
    while i < n {
        // SAFETY: i in [1, n), in-bounds.
        let v = unsafe { __cobrust_list_get(list, i) };
        if v < acc {
            acc = v;
        }
        i += 1;
    }
    acc
}

/// `max(xs: list[int]) -> int` — the largest element. Borrow-reads;
/// empty list traps. See [`__cobrust_min_int`].
///
/// # Safety
///
/// Same as [`__cobrust_min_int`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_max_int(list: *mut u8) -> i64 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    if n <= 0 {
        crate::panic::panic("max() arg is an empty sequence");
    }
    // SAFETY: in-bounds.
    let mut acc = unsafe { __cobrust_list_get(list, 0) };
    let mut i = 1;
    while i < n {
        // SAFETY: in-bounds.
        let v = unsafe { __cobrust_list_get(list, i) };
        if v > acc {
            acc = v;
        }
        i += 1;
    }
    acc
}

/// `sum(xs: list[int]) -> int` — the sum of the elements. Borrow-reads;
/// `sum([]) == 0` (CPython parity — NOT a trap). Wrapping addition
/// mirrors the codegen integer-overflow discipline of the existing
/// list shims (M12.x fixed-width i64 storage).
///
/// # Safety
///
/// Same as [`__cobrust_min_int`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_sum_int(list: *mut u8) -> i64 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    let mut acc: i64 = 0;
    let mut i = 0;
    while i < n {
        // SAFETY: in-bounds.
        let v = unsafe { __cobrust_list_get(list, i) };
        acc = acc.wrapping_add(v);
        i += 1;
    }
    acc
}

/// `min(xs: list[float]) -> float` — the smallest element. Each `i64`
/// slot is reinterpreted as the stored `f64` bit-pattern
/// (`f64::from_bits`). Borrow-reads; empty list traps. See
/// [`__cobrust_min_int`].
///
/// NaN handling matches a simple `<` reduction (NaN never compares
/// less, so a leading NaN can propagate); CPython's `min` likewise has
/// NaN-order-dependent behaviour. The differential corpus avoids NaN
/// inputs (out of scope for ADR-0090).
///
/// # Safety
///
/// Same as [`__cobrust_min_int`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_min_float(list: *mut u8) -> f64 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    if n <= 0 {
        crate::panic::panic("min() arg is an empty sequence");
    }
    // SAFETY: in-bounds. The slot is a `to_bits()` f64 pattern.
    let mut acc = f64::from_bits(unsafe { __cobrust_list_get(list, 0) } as u64);
    let mut i = 1;
    while i < n {
        // SAFETY: in-bounds.
        let v = f64::from_bits(unsafe { __cobrust_list_get(list, i) } as u64);
        if v < acc {
            acc = v;
        }
        i += 1;
    }
    acc
}

/// `max(xs: list[float]) -> float` — the largest element (f64 slots via
/// `from_bits`). Borrow-reads; empty list traps. See
/// [`__cobrust_min_float`].
///
/// # Safety
///
/// Same as [`__cobrust_min_int`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_max_float(list: *mut u8) -> f64 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    if n <= 0 {
        crate::panic::panic("max() arg is an empty sequence");
    }
    // SAFETY: in-bounds.
    let mut acc = f64::from_bits(unsafe { __cobrust_list_get(list, 0) } as u64);
    let mut i = 1;
    while i < n {
        // SAFETY: in-bounds.
        let v = f64::from_bits(unsafe { __cobrust_list_get(list, i) } as u64);
        if v > acc {
            acc = v;
        }
        i += 1;
    }
    acc
}

/// `sum(xs: list[float]) -> float` — the sum (f64 slots via
/// `from_bits`). Borrow-reads; `sum([]) == 0.0` (CPython parity, NOT a
/// trap). See [`__cobrust_sum_int`].
///
/// # Safety
///
/// Same as [`__cobrust_min_int`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_sum_float(list: *mut u8) -> f64 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    let mut acc: f64 = 0.0;
    let mut i = 0;
    while i < n {
        // SAFETY: in-bounds.
        let v = f64::from_bits(unsafe { __cobrust_list_get(list, i) } as u64);
        acc += v;
        i += 1;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::{__cobrust_list_new, __cobrust_list_set};

    /// Build a `list[i64]` from a slice (the storage shape codegen
    /// emits for an int-list literal).
    fn build_int_list(items: &[i64]) -> *mut u8 {
        // SAFETY: fresh list of the right length; set each slot.
        unsafe {
            let l = __cobrust_list_new(8, items.len() as i64);
            for (i, &v) in items.iter().enumerate() {
                __cobrust_list_set(l, i as i64, v);
            }
            l
        }
    }

    /// Build a `list[f64]` — each slot holds the `to_bits()` pattern,
    /// mirroring the codegen float-list materialisation.
    fn build_float_list(items: &[f64]) -> *mut u8 {
        // SAFETY: fresh list; store each f64 as its i64 bit-pattern.
        unsafe {
            let l = __cobrust_list_new(8, items.len() as i64);
            for (i, &v) in items.iter().enumerate() {
                __cobrust_list_set(l, i as i64, v.to_bits() as i64);
            }
            l
        }
    }

    #[test]
    fn min_max_sum_int_basic() {
        let l = build_int_list(&[3, 1, 2]);
        // SAFETY: valid non-empty list.
        unsafe {
            assert_eq!(__cobrust_min_int(l), 1);
            assert_eq!(__cobrust_max_int(l), 3);
            assert_eq!(__cobrust_sum_int(l), 6);
            // Reused after every reducer — the list was BORROWED, not
            // freed. The accessor still works.
            assert_eq!(__cobrust_list_len(l), 3);
            assert_eq!(__cobrust_list_get(l, 0), 3);
            crate::collections::__cobrust_list_drop(l);
        }
    }

    #[test]
    fn min_max_int_negatives_and_singleton() {
        let l = build_int_list(&[-5, -1, -9, -3]);
        // SAFETY: valid list.
        unsafe {
            assert_eq!(__cobrust_min_int(l), -9);
            assert_eq!(__cobrust_max_int(l), -1);
            crate::collections::__cobrust_list_drop(l);
        }
        let one = build_int_list(&[7]);
        // SAFETY: valid list.
        unsafe {
            assert_eq!(__cobrust_min_int(one), 7);
            assert_eq!(__cobrust_max_int(one), 7);
            assert_eq!(__cobrust_sum_int(one), 7);
            crate::collections::__cobrust_list_drop(one);
        }
    }

    #[test]
    fn sum_int_empty_is_zero() {
        let l = build_int_list(&[]);
        // SAFETY: valid empty list — sum is 0, NOT a trap.
        unsafe {
            assert_eq!(__cobrust_sum_int(l), 0);
            crate::collections::__cobrust_list_drop(l);
        }
    }

    #[test]
    fn min_max_sum_float_basic() {
        let l = build_float_list(&[1.5, 2.5, 3.0]);
        // SAFETY: valid float list (to_bits slots).
        unsafe {
            assert!((__cobrust_min_float(l) - 1.5).abs() < 1e-12);
            assert!((__cobrust_max_float(l) - 3.0).abs() < 1e-12);
            assert!((__cobrust_sum_float(l) - 7.0).abs() < 1e-12);
            crate::collections::__cobrust_list_drop(l);
        }
    }

    #[test]
    fn sum_float_empty_is_zero() {
        let l = build_float_list(&[]);
        // SAFETY: valid empty float list.
        unsafe {
            assert!(__cobrust_sum_float(l).abs() < 1e-12);
            crate::collections::__cobrust_list_drop(l);
        }
    }

    #[test]
    fn null_list_sum_is_zero() {
        // A null list reads as length 0 (mirrors __cobrust_list_len).
        // SAFETY: null is an accepted input.
        unsafe {
            assert_eq!(__cobrust_sum_int(std::ptr::null_mut()), 0);
            assert!(__cobrust_sum_float(std::ptr::null_mut()).abs() < 1e-12);
        }
    }
}
