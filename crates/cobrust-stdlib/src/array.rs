//! `std.array` — fixed-size array runtime helpers for dynamic-index access.
//!
//! ADR-0060b finding-closure: dynamic-index `xs[i]` on `[T; N]` cannot use
//! `build_extract_value` (requires compile-time u32) and cannot use
//! `build_in_bounds_gep` (#![forbid(unsafe_code)] blocks inkwell's unsafe
//! GEP). The safe path routes through these C-ABI helpers, mirroring the
//! List runtime-helper strategy.
//!
//! Per-type helpers (i64, i32, i8, bool, ptr/str) because `[T; N]` is
//! non-polymorphic at the runtime layer. Bounds-check panics via
//! `__cobrust_panic` on OOB (constitution §2.2 — truly unrecoverable).
//!
//! C ABI surface (declared in cobrust-codegen `declare_runtime_helpers`):
//!
//!   __cobrust_array_get_i64(arr_ptr: *const i64, len: usize, idx: usize) -> i64
//!   __cobrust_array_get_i32(arr_ptr: *const i32, len: usize, idx: usize) -> i32
//!   __cobrust_array_get_i8 (arr_ptr: *const i8,  len: usize, idx: usize) -> i8
//!   __cobrust_array_get_bool(arr_ptr: *const u8, len: usize, idx: usize) -> i64
//!
//! `bool` is stored as u8 (0/1), returned as i64 (0/1) matching codegen
//! i1→i64 convention.

use std::slice;

// =====================================================================
// i64 variant
// =====================================================================

/// Read `arr[idx]` from a `[i64; N]` array passed by pointer.
///
/// # Safety
///
/// `arr_ptr` must point to `len` consecutive `i64` values (the array
/// on the stack). Codegen always passes `alloca` base + static `N`.
/// The bounds check prevents OOB reads; null check guards against
/// a codegen invariant violation.
///
/// # Panics (via `__cobrust_panic`)
///
/// Panics if `arr_ptr` is null or `idx >= len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_array_get_i64(
    arr_ptr: *const i64,
    len: usize,
    idx: usize,
) -> i64 {
    if arr_ptr.is_null() {
        let msg = b"__cobrust_array_get_i64: null array pointer";
        // SAFETY: msg is a valid UTF-8 static slice.
        unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
    }
    // SAFETY: arr_ptr non-null; len from static N (codegen invariant).
    let s = unsafe { slice::from_raw_parts(arr_ptr, len) };
    match s.get(idx) {
        Some(&v) => v,
        None => {
            let msg = b"array index out of bounds";
            // SAFETY: static UTF-8 byte slice.
            unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
        }
    }
}

// =====================================================================
// i32 variant
// =====================================================================

/// Read `arr[idx]` from a `[i32; N]` array passed by pointer.
///
/// # Safety
///
/// Same invariants as [`__cobrust_array_get_i64`] but for `i32` elements.
///
/// # Panics (via `__cobrust_panic`)
///
/// Panics if `arr_ptr` is null or `idx >= len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_array_get_i32(
    arr_ptr: *const i32,
    len: usize,
    idx: usize,
) -> i32 {
    if arr_ptr.is_null() {
        let msg = b"__cobrust_array_get_i32: null array pointer";
        // SAFETY: static UTF-8 byte slice.
        unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
    }
    // SAFETY: arr_ptr non-null; len from static N (codegen invariant).
    let s = unsafe { slice::from_raw_parts(arr_ptr, len) };
    match s.get(idx) {
        Some(&v) => v,
        None => {
            let msg = b"array index out of bounds";
            // SAFETY: static UTF-8 byte slice.
            unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
        }
    }
}

// =====================================================================
// i8 variant
// =====================================================================

/// Read `arr[idx]` from a `[i8; N]` array passed by pointer.
///
/// # Safety
///
/// Same invariants as [`__cobrust_array_get_i64`] but for `i8` elements.
///
/// # Panics (via `__cobrust_panic`)
///
/// Panics if `arr_ptr` is null or `idx >= len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_array_get_i8(arr_ptr: *const i8, len: usize, idx: usize) -> i8 {
    if arr_ptr.is_null() {
        let msg = b"__cobrust_array_get_i8: null array pointer";
        // SAFETY: static UTF-8 byte slice.
        unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
    }
    // SAFETY: arr_ptr non-null; len from static N (codegen invariant).
    let s = unsafe { slice::from_raw_parts(arr_ptr, len) };
    match s.get(idx) {
        Some(&v) => v,
        None => {
            let msg = b"array index out of bounds";
            // SAFETY: static UTF-8 byte slice.
            unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
        }
    }
}

// =====================================================================
// bool variant (stored as u8 0/1, returned as i64 matching codegen)
// =====================================================================

/// Read `arr[idx]` from a `[bool; N]` array passed by pointer.
///
/// `bool` is stored as `u8` (0 = false, 1 = true). Returns `i64`
/// (0 or 1) matching codegen's i1→i64 zero-extension convention.
///
/// # Safety
///
/// `arr_ptr` must point to `len` consecutive `u8` values (bool stored
/// as u8). Codegen always passes `alloca` base + static `N`.
///
/// # Panics (via `__cobrust_panic`)
///
/// Panics if `arr_ptr` is null or `idx >= len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_array_get_bool(
    arr_ptr: *const u8,
    len: usize,
    idx: usize,
) -> i64 {
    if arr_ptr.is_null() {
        let msg = b"__cobrust_array_get_bool: null array pointer";
        // SAFETY: static UTF-8 byte slice.
        unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
    }
    // SAFETY: arr_ptr non-null; len from static N (codegen invariant).
    let s = unsafe { slice::from_raw_parts(arr_ptr, len) };
    match s.get(idx) {
        Some(&v) => i64::from(v != 0),
        None => {
            let msg = b"array index out of bounds";
            // SAFETY: static UTF-8 byte slice.
            unsafe { crate::panic::__cobrust_panic(msg.as_ptr(), msg.len()) }
        }
    }
}

// =====================================================================
// Unit tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_get_i64_in_bounds() {
        let arr: [i64; 4] = [10, 20, 30, 40];
        // SAFETY: arr is a local array of length 4; idx in [0,3].
        unsafe {
            assert_eq!(__cobrust_array_get_i64(arr.as_ptr(), 4, 0), 10);
            assert_eq!(__cobrust_array_get_i64(arr.as_ptr(), 4, 3), 40);
        }
    }

    #[test]
    fn test_array_get_i32_in_bounds() {
        let arr: [i32; 3] = [1, 2, 3];
        // SAFETY: arr is a local array of length 3; idx in [0,2].
        unsafe {
            assert_eq!(__cobrust_array_get_i32(arr.as_ptr(), 3, 1), 2);
        }
    }

    #[test]
    fn test_array_get_i8_in_bounds() {
        let arr: [i8; 2] = [-1, 127];
        // SAFETY: arr is a local array of length 2; idx 0.
        unsafe {
            assert_eq!(__cobrust_array_get_i8(arr.as_ptr(), 2, 0), -1_i8);
            assert_eq!(__cobrust_array_get_i8(arr.as_ptr(), 2, 1), 127_i8);
        }
    }

    #[test]
    fn test_array_get_bool_in_bounds() {
        let arr: [u8; 2] = [0, 1];
        // SAFETY: arr is a local array of length 2.
        unsafe {
            assert_eq!(__cobrust_array_get_bool(arr.as_ptr(), 2, 0), 0_i64);
            assert_eq!(__cobrust_array_get_bool(arr.as_ptr(), 2, 1), 1_i64);
        }
    }
}
