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

use crate::collections::{
    __cobrust_list_get, __cobrust_list_len, __cobrust_list_new, __cobrust_list_set,
};

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

// --- `sorted(xs)` (ADR-0108 / F95) ---------------------------------------
//
// `sorted(xs: list[T]) -> list[T]` returns a NEW ascending-sorted list;
// the SOURCE is NOT mutated (Python copy semantics — distinct from
// `list.sort()` which mutates in place; the in-place form is a deferred
// follow-up). `reverse=` / `key=` kwargs are OUT OF SCOPE (ascending
// only — ADR-0108 §"Deferred").
//
// Like the `min`/`max`/`sum` reducers above, each shim BORROWS the source
// list (reads len + each slot via `__cobrust_list_len` / `__cobrust_list_get`,
// never `Box::from_raw`) and builds a FRESH `list[T]` the `.cb` scope owns
// and drops EXACTLY ONCE:
//
//   - `_int`   : numeric ascending sort of the raw i64 slots.
//   - `_float` : numeric ascending sort, each i64 slot reinterpreted as the
//                stored `f64` bit-pattern (`from_bits`); the fresh list's
//                slots store the SAME `to_bits()` patterns (the codegen
//                float-list materialisation shape). NaN is out of scope
//                (ADR-0108 §"NaN") — a `total_cmp` keeps the sort total.
//   - `_str`   : LEXICOGRAPHIC ascending sort. Each slot is a `*mut u8`
//                Str-buffer pointer; we sort the slot pointers by
//                `__cobrust_str_cmp` (UTF-8 byte order == codepoint order ==
//                CPython, F92 / ADR-0104), then DEEP-COPY each via
//                `__cobrust_str_clone` into the fresh list. The fresh
//                `list[str]` OWNS its clones (the codegen drops it via
//                `__cobrust_list_drop_elems` + `__cobrust_str_drop`); the
//                SOURCE keeps its own slots (NOT consumed), so a subsequent
//                read of the source still works and the source drops once.
//
// An empty / null source yields a fresh empty list (`sorted([]) == []`).

/// `sorted(xs: list[int]) -> list[int]` — a NEW ascending-sorted list.
///
/// Borrow-reads `xs` (never frees it); returns a fresh `list[i64]` the
/// caller owns + drops EXACTLY ONCE via `__cobrust_list_drop`.
///
/// # Safety
///
/// `list` must be a pointer returned by `__cobrust_list_new` and not yet
/// dropped, OR null (a null list yields a fresh empty list).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_sort_int(list: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    let mut buf: Vec<i64> = Vec::with_capacity(n.max(0) as usize);
    let mut i = 0;
    while i < n {
        // SAFETY: 0 <= i < n, in-bounds.
        buf.push(unsafe { __cobrust_list_get(list, i) });
        i += 1;
    }
    buf.sort_unstable();
    // SAFETY: fresh list of length `n`; set each slot from the sorted buf.
    let out = unsafe { __cobrust_list_new(8, buf.len() as i64) };
    for (k, &v) in buf.iter().enumerate() {
        // SAFETY: k in [0, len), in-bounds for the fresh `out`.
        unsafe { __cobrust_list_set(out, k as i64, v) };
    }
    out
}

/// `sorted(xs: list[float]) -> list[float]` — a NEW ascending-sorted
/// list. Each i64 slot is the stored `f64` bit-pattern. Uses `f64::total_cmp`
/// so the sort is total (NaN out of scope — ADR-0108). See
/// [`__cobrust_list_sort_int`].
///
/// # Safety
///
/// Same as [`__cobrust_list_sort_int`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_sort_float(list: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    let mut buf: Vec<f64> = Vec::with_capacity(n.max(0) as usize);
    let mut i = 0;
    while i < n {
        // SAFETY: in-bounds. The slot is a `to_bits()` f64 pattern.
        buf.push(f64::from_bits(unsafe { __cobrust_list_get(list, i) } as u64));
        i += 1;
    }
    buf.sort_by(f64::total_cmp);
    // SAFETY: fresh list of length `n`; store each f64 as its bit-pattern.
    let out = unsafe { __cobrust_list_new(8, buf.len() as i64) };
    for (k, &v) in buf.iter().enumerate() {
        // SAFETY: k in [0, len), in-bounds for the fresh `out`.
        unsafe { __cobrust_list_set(out, k as i64, v.to_bits() as i64) };
    }
    out
}

/// `sorted(xs: list[str]) -> list[str]` — a NEW LEXICOGRAPHIC ascending-
/// sorted list. Each slot is a `*mut u8` Str-buffer pointer; the slot
/// pointers are sorted by `__cobrust_str_cmp` (UTF-8 byte order ==
/// codepoint order == CPython), then DEEP-COPIED via `__cobrust_str_clone`
/// into the fresh list — the SOURCE is NOT consumed (Python copy
/// semantics). See [`__cobrust_list_sort_int`].
///
/// # Safety
///
/// Same as [`__cobrust_list_sort_int`]. The slot values must be valid Str
/// pointers (or zero, treated as an empty-string slot).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_sort_str(list: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation; `__cobrust_list_len` tolerates null.
    let n = unsafe { __cobrust_list_len(list) };
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(n.max(0) as usize);
    let mut i = 0;
    while i < n {
        // SAFETY: in-bounds. The slot holds a `*mut u8` Str pointer (i64).
        ptrs.push(unsafe { __cobrust_list_get(list, i) } as *mut u8);
        i += 1;
    }
    // Sort the borrowed slot pointers lexicographically. `__cobrust_str_cmp`
    // tolerates null (treats it as ""); a non-null slot is a valid Str.
    ptrs.sort_by(|&a, &b| {
        // SAFETY: a, b are slot pointers read from a valid list; str_cmp
        // null-checks each.
        match unsafe { crate::io::__cobrust_str_cmp(a, b) } {
            x if x < 0 => std::cmp::Ordering::Less,
            x if x > 0 => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
    // SAFETY: fresh list of length `n`; each slot is a fresh OWNED clone so
    // the fresh list and the source own disjoint Str allocations.
    let out = unsafe { __cobrust_list_new(8, ptrs.len() as i64) };
    for (k, &p) in ptrs.iter().enumerate() {
        // SAFETY: clone deep-copies the source buffer (null → fresh empty
        // Str); the fresh list owns the clone.
        let cloned = unsafe { crate::fmt::__cobrust_str_clone(p) };
        // SAFETY: k in [0, len), in-bounds for the fresh `out`.
        unsafe { __cobrust_list_set(out, k as i64, cloned as i64) };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::{__cobrust_list_drop, __cobrust_list_drop_elems};

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

    // --- sorted(xs) (ADR-0108 / F95) -------------------------------------

    /// Allocate a fresh Str buffer holding `s` (mirrors the f-string
    /// runtime path; the caller owns + drops via `__cobrust_str_drop`).
    fn build_str(s: &str) -> *mut u8 {
        // SAFETY: fresh Str alloc + push the static bytes.
        unsafe {
            let buf = crate::fmt::__cobrust_str_new();
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
            buf
        }
    }

    /// Build a `list[str]` whose slots are fresh owned Str pointers.
    fn build_str_list(items: &[&str]) -> *mut u8 {
        // SAFETY: fresh list; each slot is a fresh Str pointer (i64).
        unsafe {
            let l = __cobrust_list_new(8, items.len() as i64);
            for (i, &s) in items.iter().enumerate() {
                __cobrust_list_set(l, i as i64, build_str(s) as i64);
            }
            l
        }
    }

    /// Read slot `i` of a `list[str]` as a Rust `String`.
    fn slot_str(list: *mut u8, i: i64) -> String {
        // SAFETY: in-bounds slot holds a valid Str pointer.
        unsafe {
            let p = __cobrust_list_get(list, i) as *mut u8;
            let len = crate::fmt::__cobrust_str_len(p) as usize;
            if len == 0 {
                return String::new();
            }
            let ptr = crate::fmt::__cobrust_str_ptr(p);
            let bytes = std::slice::from_raw_parts(ptr, len);
            String::from_utf8_lossy(bytes).into_owned()
        }
    }

    #[test]
    fn sort_int_ascending_and_source_unmutated() {
        let l = build_int_list(&[3, 1, 2]);
        // SAFETY: valid list — sort BORROWS it.
        unsafe {
            let out = __cobrust_list_sort_int(l);
            assert_eq!(__cobrust_list_len(out), 3);
            assert_eq!(__cobrust_list_get(out, 0), 1);
            assert_eq!(__cobrust_list_get(out, 1), 2);
            assert_eq!(__cobrust_list_get(out, 2), 3);
            // SOURCE UNMUTATED — original order intact (Python copy).
            assert_eq!(__cobrust_list_get(l, 0), 3);
            assert_eq!(__cobrust_list_get(l, 1), 1);
            assert_eq!(__cobrust_list_get(l, 2), 2);
            __cobrust_list_drop(out);
            __cobrust_list_drop(l);
        }
    }

    #[test]
    fn sort_int_duplicates_negatives_singleton_empty() {
        let dup = build_int_list(&[5, 5, 1, 3]);
        let neg = build_int_list(&[-1, -9, 4, 0]);
        let one = build_int_list(&[7]);
        let empty = build_int_list(&[]);
        // SAFETY: valid lists.
        unsafe {
            let so = __cobrust_list_sort_int(dup);
            assert_eq!(
                (0..4)
                    .map(|i| __cobrust_list_get(so, i))
                    .collect::<Vec<_>>(),
                vec![1, 3, 5, 5]
            );
            let sn = __cobrust_list_sort_int(neg);
            assert_eq!(
                (0..4)
                    .map(|i| __cobrust_list_get(sn, i))
                    .collect::<Vec<_>>(),
                vec![-9, -1, 0, 4]
            );
            let s1 = __cobrust_list_sort_int(one);
            assert_eq!(__cobrust_list_len(s1), 1);
            assert_eq!(__cobrust_list_get(s1, 0), 7);
            let se = __cobrust_list_sort_int(empty);
            assert_eq!(__cobrust_list_len(se), 0);
            for p in [so, sn, s1, se, dup, neg, one, empty] {
                __cobrust_list_drop(p);
            }
        }
    }

    #[test]
    fn sort_float_ascending() {
        let l = build_float_list(&[3.5, 1.5, 2.0, 1.5]);
        // SAFETY: valid float list.
        unsafe {
            let out = __cobrust_list_sort_float(l);
            let got: Vec<f64> = (0..4)
                .map(|i| f64::from_bits(__cobrust_list_get(out, i) as u64))
                .collect();
            assert_eq!(got, vec![1.5, 1.5, 2.0, 3.5]);
            // Source unmutated.
            assert!((f64::from_bits(__cobrust_list_get(l, 0) as u64) - 3.5).abs() < 1e-12);
            __cobrust_list_drop(out);
            __cobrust_list_drop(l);
        }
    }

    #[test]
    fn sort_str_lexicographic_and_source_unmutated() {
        let l = build_str_list(&["banana", "apple", "cherry"]);
        // SAFETY: valid list[str]; sort deep-copies each slot.
        unsafe {
            let out = __cobrust_list_sort_str(l);
            assert_eq!(__cobrust_list_len(out), 3);
            assert_eq!(slot_str(out, 0), "apple");
            assert_eq!(slot_str(out, 1), "banana");
            assert_eq!(slot_str(out, 2), "cherry");
            // SOURCE UNMUTATED — original order + values intact.
            assert_eq!(slot_str(l, 0), "banana");
            assert_eq!(slot_str(l, 1), "apple");
            assert_eq!(slot_str(l, 2), "cherry");
            // Both lists own DISJOINT Str allocations — drop both cleanly
            // (no double-free): the fresh list owns its clones, the source
            // owns its originals.
            __cobrust_list_drop_elems(out, crate::fmt::__cobrust_str_drop);
            __cobrust_list_drop_elems(l, crate::fmt::__cobrust_str_drop);
        }
    }

    #[test]
    fn sort_str_empty_and_singleton() {
        let empty = build_str_list(&[]);
        let one = build_str_list(&["solo"]);
        // SAFETY: valid lists.
        unsafe {
            let se = __cobrust_list_sort_str(empty);
            assert_eq!(__cobrust_list_len(se), 0);
            let s1 = __cobrust_list_sort_str(one);
            assert_eq!(__cobrust_list_len(s1), 1);
            assert_eq!(slot_str(s1, 0), "solo");
            __cobrust_list_drop_elems(se, crate::fmt::__cobrust_str_drop);
            __cobrust_list_drop_elems(s1, crate::fmt::__cobrust_str_drop);
            __cobrust_list_drop_elems(empty, crate::fmt::__cobrust_str_drop);
            __cobrust_list_drop_elems(one, crate::fmt::__cobrust_str_drop);
        }
    }

    #[test]
    fn sort_null_yields_empty() {
        // SAFETY: null is an accepted input → fresh empty list.
        unsafe {
            let si = __cobrust_list_sort_int(std::ptr::null_mut());
            assert_eq!(__cobrust_list_len(si), 0);
            let sf = __cobrust_list_sort_float(std::ptr::null_mut());
            assert_eq!(__cobrust_list_len(sf), 0);
            let ss = __cobrust_list_sort_str(std::ptr::null_mut());
            assert_eq!(__cobrust_list_len(ss), 0);
            __cobrust_list_drop(si);
            __cobrust_list_drop(sf);
            __cobrust_list_drop(ss);
        }
    }
}
