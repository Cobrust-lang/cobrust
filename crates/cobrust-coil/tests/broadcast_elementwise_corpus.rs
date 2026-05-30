//! ADR-0077 **Phase 3** (broadcasting) — failing TEST-FIRST corpus for
//! the `coil.Buffer` elementwise-operator C-ABI shims (`a + b` / `a - b`
//! / `a * b`) under **numpy broadcasting**.
//!
//! ## What this corpus pins
//!
//! Phase 1 (commit `73c2747`) wired the `.cb`-side `a + b` end-to-end onto
//! `__cobrust_coil_buffer_add` (+ `_sub` / `_mul`), but the shim's shared
//! body `buffer_binop` (`crates/cobrust-coil/src/cabi.rs:415-445`) enforces
//! a **same-shape** contract: line 432 does
//!
//! ```ignore
//! if lhs.shape() != rhs.shape() { coil_panic("... shape mismatch ...") }
//! ```
//!
//! BEFORE ever calling `Array::add`. So today ANY shape difference — even a
//! numpy-broadcastable one like `(3,1) + (1,4)` or `(3,) + (1,)` — aborts
//! the process via `__cobrust_panic`, identically to a genuinely
//! incompatible `(3,) + (4,)`. The Phase-1 module doc says so verbatim
//! (`cabi.rs:434-435`): "Phase 1 requires same-shape operands; broadcasting
//! is deferred to Phase 2".
//!
//! The underlying Rust kernel **already broadcasts**: `Array::add`
//! (`array.rs:156` → `ufunc::add`, `ufunc.rs:353`) calls `broadcast_shape`
//! (`broadcast.rs:35`) + `broadcast_owned` (`ufunc.rs:136`) and produces the
//! numpy-exact result shape + values. (Verified empirically: a `(3,1)+(1,4)`
//! `Array::add` yields `(3,4)` with the broadcasted values.) So Phase 3 is a
//! ONE-SITE change: relax the `cabi.rs:432` guard so it only aborts on a
//! genuinely non-broadcastable pair (`broadcast_shape(..).is_err()`), letting
//! `Array::add` broadcast the rest.
//!
//! ## Why a Rust corpus (not only a `.cb` E2E)
//!
//! The `.cb`-side coil constructors are 1-D-or-identity only (`coil.zeros(n)`
//! / `coil.ones(n)` / `coil.mgrid` / `coil.array1d2` are 1-D; `coil.eye(n)`
//! is `n×n`). There is NO `.cb` constructor that builds a `(3,1)` column or a
//! `(1,4)` row, and no `.cb` `reshape`. So the canonical numpy-doc broadcast
//! cases — `(3,1)+(1,4)`, `(1,3)+(3,1)` — can ONLY be constructed at the Rust
//! `Array` level. This corpus drives the C-ABI shims directly with arbitrary
//! shapes (mirroring the `cabi.rs` `#[cfg(test)]` idiom: provide the stdlib
//! stub externs, `Box::into_raw` an `Array` as a `Buffer` handle, call the
//! `__cobrust_coil_buffer_*` shim, inspect the result via `array_repr` +
//! element extraction). The sibling `.cb` E2E (`coil_broadcast_e2e.rs`)
//! covers the broadcast shapes that ARE `.cb`-buildable (`(3,)+(1,)`).
//!
//! ## Oracle
//!
//! NumPy 2.0.2 broadcasting rules
//! (<https://numpy.org/doc/stable/user/basics.broadcasting.html>): compare
//! shapes from the TRAILING dimension; two dims are compatible iff equal OR
//! one is 1; a missing leading dim counts as 1; the result dim is the max;
//! incompatible (neither equal nor 1) → error. Every expected shape + value
//! below is the EXACT array numpy produces (computed by hand and annotated).
//!
//! ## RED at HEAD `3aa32ae`
//!
//! Every `*_broadcasts_*` positive case calls `__cobrust_coil_buffer_add` (or
//! `_mul`) on differently-shaped handles → `buffer_binop` hits the
//! `cabi.rs:432` same-shape guard → `coil_panic` → the test-local
//! `__cobrust_panic` stub `panic!`s (which, because the shim's `coil_panic`
//! is `-> !`, surfaces as a process ABORT — `signal: 6, SIGABRT` — that takes
//! down the test binary). So these tests FAIL at HEAD; **run them one at a
//! time** (`cargo test --test broadcast_elementwise_corpus <name> -- --exact`)
//! so one abort doesn't mask the others. Once Phase 3 relaxes the guard they
//! pass (the broadcast result returns instead of aborting).
//!
//! GREEN at HEAD (the boundary cases): `same_shape_add_2x2_still_works` (the
//! no-regression equal-shape baseline), `kernel_array_add_already_broadcasts`
//! (the evidence the kernel already broadcasts — so Phase 3 is a guard-only
//! change), and `broadcast_shape_discriminator_matches_numpy` (the exact
//! predicate the relaxed guard must use). The incompatible-shape process-trap
//! itself is pinned at the `.cb` E2E level (`coil_broadcast_e2e.rs`) as a
//! non-zero exit — the abort is uncatchable by `#[should_panic]`. NONE are
//! `#[ignore]`d — they are the contract the Phase-3 DEV turns green (corpus +
//! impl land atomically).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::undocumented_unsafe_blocks)]
#![allow(clippy::missing_panics_doc)]
// Opaque-handle ABI round-trip casts (`*mut u8 <-> *mut Array` /
// `*mut Vec<i64>`). The pointers all originate from `Box::into_raw` of the
// SAME target type (the `into_handle` helper + the `__cobrust_list_*`
// stubs), so the alignment-narrowing lint is a false positive here — this
// mirrors the production `crates/cobrust-coil/src/cabi.rs` allow (cabi.rs
// §53-66), whose shims this corpus drives directly.
#![allow(clippy::cast_ptr_alignment)]
// The `__cobrust_list_*` stubs round-trip the list `len` / index `i` across
// the C-ABI as `i64`, then index a `Vec` with `as usize` — non-negative by
// construction (guarded `if len < 0` / `if i >= 0`). Same intrinsically
// correct `i64 -> usize` ABI cast `cabi.rs` allows (cabi.rs §63-66).
#![allow(clippy::cast_sign_loss)]

use coil::Array;
use coil::array_f64;
use coil::cabi::{
    __cobrust_coil_buffer_add, __cobrust_coil_buffer_drop, __cobrust_coil_buffer_mul,
    __cobrust_coil_buffer_shape, __cobrust_coil_buffer_size,
};

// =====================================================================
// Stdlib ABI stubs.
//
// The cabi shims reference three cross-crate stdlib externs
// (`__cobrust_panic` / `__cobrust_list_new` / `__cobrust_list_set`) that
// are normally link-resolved from `libcobrust_stdlib.a` only at `.cb`-link
// time, NOT into this integration-test binary. We provide minimal stubs so
// the binary links. The `__cobrust_panic` stub `panic!`s with the shim's
// diagnostic — at HEAD the broadcast positives hit it (RED); once Phase 3
// lands they no longer reach it.
// =====================================================================

#[unsafe(no_mangle)]
extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> ! {
    // SAFETY: the coil_panic helper passes a valid UTF-8 `&str`'s (ptr,len).
    let msg = unsafe { std::slice::from_raw_parts(ptr, len) };
    panic!(
        "__cobrust_panic (test stub): {}",
        String::from_utf8_lossy(msg)
    );
}

/// Minimal `list[i64]` stub backed by a `Vec<i64>`. `_shape` uses these to
/// marshal the result-shape across the C-ABI; the test reads the dims back
/// via `read_shape_list`.
#[unsafe(no_mangle)]
extern "C" fn __cobrust_list_new(_elem_size: i64, len: i64) -> *mut u8 {
    let v: Vec<i64> = vec![0; if len < 0 { 0 } else { len as usize }];
    Box::into_raw(Box::new(v)).cast::<u8>()
}

#[unsafe(no_mangle)]
extern "C" fn __cobrust_list_set(list: *mut u8, i: i64, val: i64) {
    // SAFETY: `list` is a `Box<Vec<i64>>` from `__cobrust_list_new`.
    let v = unsafe { &mut *list.cast::<Vec<i64>>() };
    // Single flat `if let`: `usize::try_from` folds the `i >= 0`
    // non-negativity guard into the conversion (a negative or OOB index is
    // silently dropped, matching the real `__cobrust_list_set` contract) and
    // `and_then(get_mut)` keeps it un-nested (no clippy::collapsible_if).
    if let Some(slot) = usize::try_from(i).ok().and_then(|idx| v.get_mut(idx)) {
        *slot = val;
    }
}

// =====================================================================
// Helpers — construct Buffer handles of ARBITRARY shape (the `.cb`
// surface cannot), call a shim, inspect the result.
// =====================================================================

/// Box an `Array` as an opaque `Buffer` handle (mirrors the cabi
/// constructors' `Box::into_raw(Box::new(arr)).cast::<u8>()`).
fn into_handle(arr: Array) -> *mut u8 {
    Box::into_raw(Box::new(arr)).cast::<u8>()
}

/// Borrow a `Buffer` handle back as an `&Array` (read-only inspection).
///
/// # Safety
/// `h` must be a live handle from `into_handle` / a cabi constructor.
unsafe fn borrow_handle<'a>(h: *mut u8) -> &'a Array {
    unsafe { &*h.cast::<Array>() }
}

/// Read a `Buffer`'s `.shape` back as a `Vec<i64>` via the C-ABI
/// `__cobrust_coil_buffer_shape` shim (exercises the real shape-marshal
/// path; the result is the dims the shim wrote into the stub list).
///
/// # Safety
/// `h` must be a live `Buffer` handle.
unsafe fn read_shape_list(h: *mut u8) -> Vec<i64> {
    let list_ptr = unsafe { __cobrust_coil_buffer_shape(h) };
    // SAFETY: `_shape` returns a `__cobrust_list_new` allocation (our stub:
    // a `Box<Vec<i64>>`). Reclaim + read it.
    let boxed = unsafe { Box::from_raw(list_ptr.cast::<Vec<i64>>()) };
    *boxed
}

/// Materialise a `Buffer` result's elements as an f64 row-major `Vec`
/// (the natural-iteration order of the underlying `ndarray::ArrayD`).
///
/// # Safety
/// `h` must be a live `Buffer` handle holding an f64 array.
unsafe fn read_values_f64(h: *mut u8) -> Vec<f64> {
    let arr = unsafe { borrow_handle(h) };
    match arr {
        Array::Float64(a) => a.iter().copied().collect(),
        other => panic!("expected Float64 result, got {:?}", other.dtype()),
    }
}

// =====================================================================
// POSITIVE — canonical numpy-doc broadcast cases (RED at HEAD).
// =====================================================================

/// `(3,1) + (1,4) -> (3,4)` — the textbook outer-sum broadcast.
///
/// Oracle (numpy 2.0.2):
/// ```python
/// a = np.array([[10.],[20.],[30.]])   # shape (3,1)
/// b = np.array([[1.,2.,3.,4.]])       # shape (1,4)
/// a + b  # shape (3,4):
/// # [[11,12,13,14],
/// #  [21,22,23,24],
/// #  [31,32,33,34]]
/// ```
///
/// PROOF OBLIGATION: at HEAD `buffer_binop` (`cabi.rs:432`) sees
/// `[3,1] != [1,4]` and aborts via `coil_panic` before `Array::add` runs.
/// Phase 3 relaxes the guard to `broadcast_shape(a,b).is_err()` so this
/// broadcasts. (The Rust `Array::add` already produces this exact result.)
#[test]
fn col3_plus_row4_broadcasts_to_3x4() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3, 1]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: ha/hb are live handles; the add shim borrows both, returns a
    // fresh handle. At HEAD this call ABORTS (RED); once Phase 3 lands it
    // returns the broadcast result.
    let hc = unsafe { __cobrust_coil_buffer_add(ha, hb) };

    let shape = unsafe { read_shape_list(hc) };
    assert_eq!(
        shape,
        vec![3, 4],
        "(3,1)+(1,4) must broadcast to shape (3,4)"
    );

    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![
            11.0, 12.0, 13.0, 14.0, // row 0: 10 + [1,2,3,4]
            21.0, 22.0, 23.0, 24.0, // row 1: 20 + [1,2,3,4]
            31.0, 32.0, 33.0, 34.0, // row 2: 30 + [1,2,3,4]
        ],
        "(3,1)+(1,4) broadcast values must equal numpy's outer sum",
    );

    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}

/// `(1,3) + (3,1) -> (3,3)` — row-vector ⊕ column-vector outer sum
/// (the swapped-rank companion of the case above).
///
/// Oracle (numpy 2.0.2):
/// ```python
/// row = np.array([[1.,2.,3.]])   # shape (1,3)
/// col = np.array([[10.],[20.],[30.]])  # shape (3,1)
/// row + col  # shape (3,3):
/// # [[11,12,13],
/// #  [21,22,23],
/// #  [31,32,33]]
/// ```
#[test]
fn row3_plus_col3_broadcasts_to_3x3() {
    let row = array_f64(&[1.0, 2.0, 3.0], &[1, 3]).unwrap();
    let col = array_f64(&[10.0, 20.0, 30.0], &[3, 1]).unwrap();
    let hr = into_handle(row);
    let hc = into_handle(col);
    // SAFETY: live handles; add shim borrows both, returns fresh handle.
    let hout = unsafe { __cobrust_coil_buffer_add(hr, hc) };

    let shape = unsafe { read_shape_list(hout) };
    assert_eq!(
        shape,
        vec![3, 3],
        "(1,3)+(3,1) must broadcast to shape (3,3)"
    );

    let values = unsafe { read_values_f64(hout) };
    assert_eq!(
        values,
        vec![
            11.0, 12.0, 13.0, // row 0: [1,2,3] + 10
            21.0, 22.0, 23.0, // row 1: [1,2,3] + 20
            31.0, 32.0, 33.0, // row 2: [1,2,3] + 30
        ],
        "(1,3)+(3,1) broadcast values must equal numpy's outer sum",
    );

    unsafe {
        __cobrust_coil_buffer_drop(hr);
        __cobrust_coil_buffer_drop(hc);
        __cobrust_coil_buffer_drop(hout);
    }
}

/// `(3,) + (1,) -> (3,)` — the 1-D "broadcast a length-1 against length-3"
/// case (the numpy stand-in for `array + scalar`, since coil's `Array`
/// holds no rank-0 scalar; a `(1,)` buffer is the closest honest analogue,
/// and is the SAME shape pair the sibling `.cb` E2E builds via
/// `coil.ones(3) + coil.ones(1)`).
///
/// Oracle (numpy 2.0.2):
/// ```python
/// a = np.array([1.,2.,3.])  # (3,)
/// b = np.array([10.])       # (1,)
/// a + b  # (3,) -> [11, 12, 13]
/// ```
#[test]
fn vec3_plus_len1_broadcasts_to_3() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let b = array_f64(&[10.0], &[1]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: live handles; add shim borrows both, returns fresh handle.
    let hc = unsafe { __cobrust_coil_buffer_add(ha, hb) };

    let shape = unsafe { read_shape_list(hc) };
    assert_eq!(shape, vec![3], "(3,)+(1,) must broadcast to shape (3,)");

    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![11.0, 12.0, 13.0],
        "(3,)+(1,) broadcast values must equal [1,2,3] + 10",
    );

    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}

/// `(2,3) + (3,) -> (2,3)` — the canonical "matrix + row" trailing-dim
/// broadcast (the missing leading dim of `(3,)` counts as 1).
///
/// Oracle (numpy 2.0.2):
/// ```python
/// m = np.array([[1.,2.,3.],[4.,5.,6.]])  # (2,3)
/// r = np.array([10.,20.,30.])            # (3,)
/// m + r  # (2,3):
/// # [[11,22,33],
/// #  [14,25,36]]
/// ```
#[test]
fn mat2x3_plus_row3_broadcasts_to_2x3() {
    let m = array_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let r = array_f64(&[10.0, 20.0, 30.0], &[3]).unwrap();
    let hm = into_handle(m);
    let hr = into_handle(r);
    // SAFETY: live handles; add shim borrows both, returns fresh handle.
    let hout = unsafe { __cobrust_coil_buffer_add(hm, hr) };

    let shape = unsafe { read_shape_list(hout) };
    assert_eq!(
        shape,
        vec![2, 3],
        "(2,3)+(3,) must broadcast to shape (2,3)"
    );

    let values = unsafe { read_values_f64(hout) };
    assert_eq!(
        values,
        vec![
            11.0, 22.0, 33.0, // row 0: [1,2,3] + [10,20,30]
            14.0, 25.0, 36.0, // row 1: [4,5,6] + [10,20,30]
        ],
        "(2,3)+(3,) broadcast values must equal numpy's per-row sum",
    );

    unsafe {
        __cobrust_coil_buffer_drop(hm);
        __cobrust_coil_buffer_drop(hr);
        __cobrust_coil_buffer_drop(hout);
    }
}

/// Broadcasting works for `*` too (not just `+`) — proves the relaxed
/// guard lives in the shared `buffer_binop` body, not bolted onto `add`
/// alone. `(3,1) * (1,4) -> (3,4)` outer product.
///
/// Oracle (numpy 2.0.2):
/// ```python
/// a = np.array([[1.],[2.],[3.]])     # (3,1)
/// b = np.array([[10.,20.,30.,40.]])  # (1,4)
/// a * b  # (3,4):
/// # [[10,20,30,40],
/// #  [20,40,60,80],
/// #  [30,60,90,120]]
/// ```
#[test]
fn col3_times_row4_broadcasts_outer_product() {
    let a = array_f64(&[1.0, 2.0, 3.0], &[3, 1]).unwrap();
    let b = array_f64(&[10.0, 20.0, 30.0, 40.0], &[1, 4]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: live handles; mul shim borrows both, returns fresh handle.
    let hc = unsafe { __cobrust_coil_buffer_mul(ha, hb) };

    let shape = unsafe { read_shape_list(hc) };
    assert_eq!(
        shape,
        vec![3, 4],
        "(3,1)*(1,4) must broadcast to shape (3,4)"
    );

    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![
            10.0, 20.0, 30.0, 40.0, // 1 * [10,20,30,40]
            20.0, 40.0, 60.0, 80.0, // 2 * [10,20,30,40]
            30.0, 60.0, 90.0, 120.0, // 3 * [10,20,30,40]
        ],
        "(3,1)*(1,4) broadcast values must equal numpy's outer product",
    );

    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}

// =====================================================================
// NO-REGRESSION — same-shape add must STILL work (PASSES at HEAD).
// =====================================================================

/// `(2,2) + (2,2) -> (2,2)` — equal shapes, the path Phase 1 already
/// supports. MUST stay green after the Phase-3 guard relaxation (relaxing
/// the same-shape gate must not break the equal-shape fast path).
///
/// Oracle (numpy 2.0.2):
/// ```python
/// a = np.array([[1.,2.],[3.,4.]])
/// b = np.array([[10.,20.],[30.,40.]])
/// a + b  # [[11,22],[33,44]]
/// ```
#[test]
fn same_shape_add_2x2_still_works() {
    let a = array_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = array_f64(&[10.0, 20.0, 30.0, 40.0], &[2, 2]).unwrap();
    let ha = into_handle(a);
    let hb = into_handle(b);
    // SAFETY: live handles; equal-shape add — no guard fires today or after.
    let hc = unsafe { __cobrust_coil_buffer_add(ha, hb) };

    let shape = unsafe { read_shape_list(hc) };
    assert_eq!(shape, vec![2, 2], "(2,2)+(2,2) result shape must be (2,2)");

    let values = unsafe { read_values_f64(hc) };
    assert_eq!(
        values,
        vec![11.0, 22.0, 33.0, 44.0],
        "(2,2)+(2,2) elementwise sum must equal numpy's",
    );

    // Also confirm the cheap scalar `.size` observer agrees (4 elements).
    let sz = unsafe { __cobrust_coil_buffer_size(hc) };
    assert_eq!(sz, 4, "(2,2) result has 4 elements");

    unsafe {
        __cobrust_coil_buffer_drop(ha);
        __cobrust_coil_buffer_drop(hb);
        __cobrust_coil_buffer_drop(hc);
    }
}

// =====================================================================
// NEGATIVE — genuinely INCOMPATIBLE shapes must STILL trap. The shim's
// trap is a process ABORT (`coil_panic` → `__cobrust_panic` is `-> !`,
// non-unwinding), which `#[should_panic]` CANNOT catch — so the
// incompatible-shape TRAP is pinned at the `.cb` E2E level
// (`coil_broadcast_e2e.rs::test_runtime_incompatible_*_traps`) as a
// non-zero process exit. HERE we pin the exact DISCRIMINATOR the relaxed
// Phase-3 guard must use: `broadcast_shape` (the function the guard must
// consult instead of `shape() != shape()`). Broadcastable pairs → `Ok`;
// incompatible → `Err(BroadcastShapeMismatch)`. PASSES at HEAD (pure
// kernel) — it documents the boundary the DEV's relaxed guard must honor:
// broadcast B's shapes, abort C's.
// =====================================================================

/// The Phase-3 guard discriminator contract. The current shim aborts when
/// `lhs.shape() != rhs.shape()` (`cabi.rs:432`) — too strict: it abuts
/// broadcastable pairs into the abort path. Phase 3 must instead abort iff
/// `broadcast_shape(lhs, rhs).is_err()`. This test pins both directions of
/// that predicate against numpy's oracle:
///   - broadcastable pairs (`(3,1)&(1,4)`, `(1,3)&(3,1)`, `(3,)&(1,)`,
///     `(2,3)&(3,)`, equal `(2,2)&(2,2)`) → `Ok(expected result shape)`;
///   - incompatible pairs (`(2,3)&(2,)`, `(3,)&(4,)`, `(5,)&(2,)`) →
///     `Err(BroadcastShapeMismatch)`.
/// (`broadcast_shape` is what `Array::add` already consults; the gap is
/// purely that the shim short-circuits BEFORE reaching it.)
#[test]
fn broadcast_shape_discriminator_matches_numpy() {
    use coil::{NumpyErrorKind, broadcast_shape};

    // Broadcastable → Ok(result shape) — the cases the relaxed guard must
    // let through to `Array::add`.
    let ok_cases: &[(&[usize], &[usize], &[usize])] = &[
        (&[3, 1], &[1, 4], &[3, 4]),
        (&[1, 3], &[3, 1], &[3, 3]),
        (&[3], &[1], &[3]),
        (&[2, 3], &[3], &[2, 3]),
        (&[2, 2], &[2, 2], &[2, 2]), // equal-shape fast path
    ];
    for (a, b, want) in ok_cases {
        let got = broadcast_shape(a, b);
        assert_eq!(
            got.as_deref(),
            Ok(*want),
            "broadcast_shape({a:?}, {b:?}) must broadcast to {want:?} (numpy oracle)",
        );
    }

    // Incompatible → Err — the cases the relaxed guard must STILL abort.
    let err_cases: &[(&[usize], &[usize])] = &[
        (&[2, 3], &[2]), // trailing 3 vs 2
        (&[3], &[4]),    // 3 vs 4
        (&[5], &[2]),    // 5 vs 2
    ];
    for (a, b) in err_cases {
        let got = broadcast_shape(a, b);
        assert!(
            matches!(
                got,
                Err(coil::NumpyError {
                    kind: NumpyErrorKind::BroadcastShapeMismatch,
                    ..
                })
            ),
            "broadcast_shape({a:?}, {b:?}) must be BroadcastShapeMismatch (numpy rejects it); \
             got {got:?}",
        );
    }
}

// =====================================================================
// Kernel cross-check — documents that the Rust layer ALREADY broadcasts,
// so Phase 3 is purely a C-ABI guard relaxation (no new numerical code).
// This one PASSES at HEAD; it is the evidence the gap is the shim guard.
// =====================================================================

/// `Array::add` (the kernel the shim forwards to) already broadcasts
/// `(3,1)+(1,4)` to the numpy-exact `(3,4)` result — verified directly,
/// bypassing the C-ABI guard. PASSES at HEAD. Phase 3 only needs to stop
/// the shim from short-circuiting BEFORE this kernel runs.
#[test]
fn kernel_array_add_already_broadcasts() {
    let a = array_f64(&[10.0, 20.0, 30.0], &[3, 1]).unwrap();
    let b = array_f64(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
    let c = a.add(&b).unwrap();
    assert_eq!(c.shape(), vec![3, 4]);
    if let Array::Float64(arr) = &c {
        let v: Vec<f64> = arr.iter().copied().collect();
        assert_eq!(
            v,
            vec![
                11.0, 12.0, 13.0, 14.0, 21.0, 22.0, 23.0, 24.0, 31.0, 32.0, 33.0, 34.0,
            ],
        );
    } else {
        panic!("expected Float64 result");
    }
}
