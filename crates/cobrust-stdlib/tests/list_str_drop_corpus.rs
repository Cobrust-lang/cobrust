//! M-F.3.2 — Rust-side `list[str]` drop schedule + clone shim corpus.
//!
//! Closes TD-1 per ADR-0050c Option A. These tests exercise the
//! C-ABI surface that the codegen `Terminator::Drop` arm calls into:
//!
//!   - `__cobrust_str_clone(buf: *mut u8) -> *mut u8` — new shim per
//!     ADR-0050c §"Phase 3"; allocates a fresh `StringBuffer`, copies
//!     the bytes, returns the new pointer.
//!   - `__cobrust_list_drop_elems(list: *mut u8, elem_drop_fn: extern "C" fn(*mut u8))`
//!     — new shim per ADR-0050c §"Phase 3"; iterates the i64 slots,
//!     casts each to `*mut u8`, calls `elem_drop_fn(slot)`, then
//!     `__cobrust_list_drop(list)`.
//!   - `__cobrust_list_is_empty(list: *mut u8) -> i64` — new shim per
//!     ADR-0050c §"Phase 6" / F5 §2.2 uniformity addendum; returns 1
//!     for empty, 0 otherwise (matches SwitchInt codegen convention).
//!
//! These tests MUST FAIL pre-impl because the three shims above do not
//! yet exist in `cobrust-stdlib`. After DEV adds them, the tests must
//! pass without modification.
//!
//! All tests use `cobrust_stdlib::fmt` + `cobrust_stdlib::collections`
//! public re-exports.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::approx_constant)]
#![allow(clippy::stable_sort_primitive)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::len_zero)]
#![allow(clippy::no_effect_underscore_binding)]

use cobrust_stdlib::collections::{
    __cobrust_list_drop, __cobrust_list_get, __cobrust_list_len, __cobrust_list_new,
    __cobrust_list_set,
};
use cobrust_stdlib::fmt::{
    __cobrust_str_drop, __cobrust_str_len, __cobrust_str_new, __cobrust_str_ptr,
    __cobrust_str_push_static,
};

// =====================================================================
// New ADR-0050c shims that DEV MUST add. These extern declarations
// match the signatures pinned in ADR-0050c §"Phase 3" + §"Phase 6".
//
// They live as `unsafe extern "C"` symbols in `cobrust_stdlib::fmt`
// and `cobrust_stdlib::collections`. Until DEV adds them, the linker
// errors out and the entire test binary fails to build — that is the
// expected FAILING state for this corpus pre-impl.
// =====================================================================

unsafe extern "C" {
    /// Allocate a fresh `StringBuffer`, bytewise-copy `buf`'s payload,
    /// return the new pointer. ADR-0050c §"Phase 3":
    ///
    ///   `pub unsafe extern "C" fn __cobrust_str_clone(buf: *mut u8) -> *mut u8`
    ///
    /// `# Safety` clause mirrors `__cobrust_str_new`: returned pointer
    /// must be passed to `__cobrust_str_drop` exactly once.
    /// `NULL` input returns `NULL`.
    fn __cobrust_str_clone(buf: *mut u8) -> *mut u8;

    /// Iterate the i64 slots of `list`, cast each to `*mut u8`, call
    /// `elem_drop_fn(slot)`, then call `__cobrust_list_drop(list)`.
    /// ADR-0050c §"Phase 3":
    ///
    ///   `pub unsafe extern "C" fn __cobrust_list_drop_elems(list: *mut u8, elem_drop_fn: extern "C" fn(*mut u8))`
    fn __cobrust_list_drop_elems(list: *mut u8, elem_drop_fn: unsafe extern "C" fn(*mut u8));

    /// Returns 1 if list is empty (len==0), 0 otherwise. F5 §2.2
    /// uniformity / ADR-0050c §"Phase 6" / ADR-0050d Decision 5
    /// addendum.
    fn __cobrust_list_is_empty(list: *mut u8) -> i64;
}

// =====================================================================
// Helper: read a Str buffer's bytes into a Rust String for assertions.
// =====================================================================

unsafe fn str_buf_to_string(buf: *mut u8) -> String {
    // SAFETY: caller-attestation. buf must be a valid Str pointer.
    let len = unsafe { __cobrust_str_len(buf) };
    if len == 0 {
        return String::new();
    }
    let ptr = unsafe { __cobrust_str_ptr(buf) };
    assert!(!ptr.is_null(), "non-empty Str must have non-null ptr");
    // SAFETY: ptr/len describe a UTF-8 slice owned by the Str buffer.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).expect("Str payload is UTF-8").to_string()
}

unsafe fn alloc_str_buf(bytes: &[u8]) -> *mut u8 {
    // SAFETY: __cobrust_str_new returns a valid pointer; push_static
    // appends bytes from a static slice.
    let buf = unsafe { __cobrust_str_new() };
    if !bytes.is_empty() {
        unsafe { __cobrust_str_push_static(buf, bytes.as_ptr(), bytes.len() as i64) };
    }
    buf
}

// =====================================================================
// Tier D.1 — Build a list[Str*] of 10 elements; iterate; verify
// element payloads; drop each + drop list. Assert no panic.
// =====================================================================

#[test]
fn d01_list_of_10_strs_build_read_drop_no_panic() {
    // SAFETY: pure C-ABI contract calls.
    unsafe {
        let list = __cobrust_list_new(8, 10);
        assert!(!list.is_null(), "list_new must return non-null");
        assert_eq!(__cobrust_list_len(list), 10, "list len == 10");

        // Populate slots with fresh Str allocations.
        for i in 0..10i64 {
            let payload = format!("elem{}", i);
            let buf = alloc_str_buf(payload.as_bytes());
            __cobrust_list_set(list, i, buf as i64);
        }

        // Read back + verify.
        for i in 0..10i64 {
            let slot = __cobrust_list_get(list, i);
            assert_ne!(slot, 0, "slot {} pointer must be non-zero", i);
            let buf = slot as *mut u8;
            let actual = str_buf_to_string(buf);
            let expected = format!("elem{}", i);
            assert_eq!(actual, expected, "slot {} payload mismatch", i);
        }

        // Drop each Str then drop the list. No panic.
        for i in 0..10i64 {
            let slot = __cobrust_list_get(list, i);
            __cobrust_str_drop(slot as *mut u8);
        }
        __cobrust_list_drop(list);
    }
}

// =====================================================================
// Tier D.2 — `__cobrust_str_clone` on each element allocates a fresh
// pointer (different from source) with identical bytes.
// =====================================================================

#[test]
fn d02_str_clone_allocates_fresh_pointer_with_identical_bytes() {
    // SAFETY: pure C-ABI contract calls.
    unsafe {
        let src = alloc_str_buf(b"hello-clone");
        assert!(!src.is_null());

        let cloned = __cobrust_str_clone(src);
        assert!(!cloned.is_null(), "clone of non-null Str must be non-null");
        assert_ne!(
            src as usize, cloned as usize,
            "clone must allocate a fresh pointer (not aliased)"
        );

        let src_bytes = str_buf_to_string(src);
        let clone_bytes = str_buf_to_string(cloned);
        assert_eq!(
            src_bytes, clone_bytes,
            "clone payload must match source payload"
        );
        assert_eq!(src_bytes, "hello-clone");

        // Both must drop independently (no double-free).
        __cobrust_str_drop(src);
        __cobrust_str_drop(cloned);
    }
}

// =====================================================================
// Tier D.3 — 10k-element stress: build, iterate twice, drop. No leak.
// Assert no panic + program completes; RSS unbounded growth would
// manifest as OOM under aggressive memory pressure but this test is
// the unit-level analog (correctness under N=10000).
// =====================================================================

#[test]
fn d03_10k_strs_build_iter_twice_drop_no_leak() {
    // SAFETY: pure C-ABI.
    unsafe {
        const N: i64 = 10_000;
        let list = __cobrust_list_new(8, N);
        assert!(!list.is_null());
        assert_eq!(__cobrust_list_len(list), N);

        // Populate.
        for i in 0..N {
            // Vary payload length to exercise different Str sizes.
            let payload = format!("k{}", i);
            let buf = alloc_str_buf(payload.as_bytes());
            __cobrust_list_set(list, i, buf as i64);
        }

        // Iterate once.
        let mut sum_lens: i64 = 0;
        for i in 0..N {
            let slot = __cobrust_list_get(list, i);
            sum_lens += __cobrust_str_len(slot as *mut u8);
        }
        assert!(sum_lens > 0, "sum of Str lengths must be positive");

        // Iterate twice — same result.
        let mut sum_lens2: i64 = 0;
        for i in 0..N {
            let slot = __cobrust_list_get(list, i);
            sum_lens2 += __cobrust_str_len(slot as *mut u8);
        }
        assert_eq!(sum_lens, sum_lens2, "two iterations yield the same sum");

        // Drop all Strs + drop the list.
        for i in 0..N {
            let slot = __cobrust_list_get(list, i);
            __cobrust_str_drop(slot as *mut u8);
        }
        __cobrust_list_drop(list);
    }
}

// =====================================================================
// Tier D.4 — Partial drop: build list of 100 Strs, drop first 50 via
// loop + then drop the list via `__cobrust_list_drop_elems` which
// drops the remaining 50 + the list itself. Assert no double-free.
//
// NOTE: realistically the language's drop schedule never partially-
// drops a list — when the list goes out of scope, all elements drop.
// This test exercises the C-ABI contract that `__cobrust_list_drop_elems`
// drops every slot regardless of prior state. The "first 50 already
// dropped" path is a hypothetical — if codegen ever emitted a partial
// drop sequence, this would catch the double-free. The test is the
// stress invariant.
// =====================================================================

#[test]
fn d04_list_drop_elems_drops_all_remaining_no_double_free() {
    // SAFETY: pure C-ABI.
    unsafe {
        let list = __cobrust_list_new(8, 100);
        assert!(!list.is_null());

        // Populate with 100 fresh Strs.
        for i in 0..100i64 {
            let buf = alloc_str_buf(format!("p{}", i).as_bytes());
            __cobrust_list_set(list, i, buf as i64);
        }

        // Drop via list_drop_elems with __cobrust_str_drop as the elem
        // drop fn. After this call, the list and all 100 Strs are freed.
        __cobrust_list_drop_elems(list, __cobrust_str_drop);

        // No use-after-free; no assertion on `list` (it has been freed).
    }
}

// =====================================================================
// Tier D.5 — `__cobrust_str_clone(NULL)` returns NULL safely.
// =====================================================================

#[test]
fn d05_str_clone_null_returns_null() {
    // SAFETY: NULL is an explicit accepted input per ADR-0050c §"Phase 3"
    // safety doc (mirrors `__cobrust_str_new` + `_drop` null-input handling).
    unsafe {
        let cloned = __cobrust_str_clone(std::ptr::null_mut());
        assert!(cloned.is_null(), "clone of NULL must return NULL");
    }
}

// =====================================================================
// Tier D.6 — `__cobrust_str_clone(empty)` returns a valid (non-null)
// pointer to an empty Str.
// =====================================================================

#[test]
fn d06_str_clone_empty_returns_valid_empty_str() {
    // SAFETY: C-ABI contract.
    unsafe {
        let empty = __cobrust_str_new(); // length 0
        assert!(!empty.is_null());
        assert_eq!(__cobrust_str_len(empty), 0);

        let cloned = __cobrust_str_clone(empty);
        assert!(!cloned.is_null(), "clone of empty Str must be non-null");
        assert_ne!(
            empty as usize, cloned as usize,
            "clone must allocate a fresh pointer even when source is empty"
        );
        assert_eq!(__cobrust_str_len(cloned), 0, "cloned empty Str has length 0");

        __cobrust_str_drop(empty);
        __cobrust_str_drop(cloned);
    }
}

// =====================================================================
// Tier D.7 — `__cobrust_list_is_empty` F5 §2.2 uniformity invariants.
// =====================================================================

#[test]
fn d07_list_is_empty_returns_1_for_empty() {
    // SAFETY: C-ABI contract.
    unsafe {
        let list = __cobrust_list_new(8, 0);
        assert!(!list.is_null());
        assert_eq!(__cobrust_list_is_empty(list), 1, "empty list returns 1");
        __cobrust_list_drop(list);
    }
}

#[test]
fn d08_list_is_empty_returns_0_for_non_empty() {
    // SAFETY: C-ABI contract.
    unsafe {
        let list = __cobrust_list_new(8, 3);
        assert!(!list.is_null());
        __cobrust_list_set(list, 0, 100);
        __cobrust_list_set(list, 1, 200);
        __cobrust_list_set(list, 2, 300);
        assert_eq!(
            __cobrust_list_is_empty(list),
            0,
            "non-empty list returns 0"
        );
        __cobrust_list_drop(list);
    }
}

#[test]
fn d09_list_is_empty_null_returns_1_or_safe() {
    // SAFETY: NULL handling is part of the shim's documented contract
    // per ADR-0050c §"Phase 6" + the existing C-ABI null-safety pattern
    // (`__cobrust_list_len(NULL) == 0` → empty → `is_empty(NULL) == 1`).
    unsafe {
        assert_eq!(
            __cobrust_list_is_empty(std::ptr::null_mut()),
            1,
            "NULL list is empty (len==0 contract)"
        );
    }
}

// =====================================================================
// Tier D.8 — Drop schedule composition: build list of 100 Strs, use
// `__cobrust_list_drop_elems` with `__cobrust_str_drop`, then verify
// no panic on a fresh list afterwards (no global state corruption).
// =====================================================================

#[test]
fn d10_repeat_build_drop_elems_no_global_corruption() {
    // Run the build-drop cycle 5 times to confirm no static-state
    // leak between iterations.
    for round in 0..5 {
        // SAFETY: C-ABI contract.
        unsafe {
            let list = __cobrust_list_new(8, 100);
            assert!(!list.is_null(), "round {} list_new non-null", round);
            for i in 0..100i64 {
                let buf = alloc_str_buf(format!("r{}-i{}", round, i).as_bytes());
                __cobrust_list_set(list, i, buf as i64);
            }
            __cobrust_list_drop_elems(list, __cobrust_str_drop);
        }
    }
}
