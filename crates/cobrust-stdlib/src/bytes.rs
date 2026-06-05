//! `std.bytes` — the `__cobrust_bytes_*` C-ABI family backing the
//! first-class `bytes` runtime value.
//!
//! ADR-0093 §"Decision 1": `bytes` is **"Str without UTF-8"** — an
//! immutable, heap-allocated byte buffer behind an opaque `*mut u8`
//! handle. This module MIRRORS the `__cobrust_str_*` family
//! (`fmt.rs` + `string.rs`) shape — `(ptr handle, len, get, drop,
//! clone)` — minus the UTF-8 invariant on the stored bytes, so a
//! non-UTF-8 byte (`b"\xff"`) round-trips byte-exact (the old lossy
//! str-buffer path corrupted it).
//!
//! **Ownership convention** (ADR-0050c-mirror, the SAME discipline Str
//! runs): a `bytes` value is `.cb`-owned and freed EXACTLY ONCE via
//! [`__cobrust_bytes_drop`] at scope exit. A `bytes` value is `Move`-only
//! (`cobrust-mir/src/lower.rs::is_copy_type` excludes `Ty::Bytes`): a
//! rebind transfers ownership, and aliasing-then-reuse is a compile-time
//! `use of moved value` error today. [`__cobrust_bytes_clone`] is the
//! deep-copy shim RESERVED for the Phase-2 aliasing surface (slice /
//! concat — ADR-0093 §Phasing); it is exercised by the unit tests in this
//! module but NO `.cb`-lowering emits a call to it yet.
//! [`__cobrust_bytes_from_raw`] mints a FRESH owned buffer (the `b"..."`
//! literal is not shared with `.rodata`). [`__cobrust_bytes_len`] /
//! [`__cobrust_bytes_get`] BORROW (read-only) — they never consume the
//! handle.
//!
//! The shims live in `cobrust-stdlib` (`libcobrust_stdlib.a`) — the
//! shared runtime archive every `.cb` program links — so a future
//! `cobrust-dora` `event.data_bytes()` accessor (ADR-0093 Phase 2 /
//! ADR-0076c (D)-B-1b) gets these symbols WITHOUT a cross-crate cabi
//! feature dance.

/// The opaque heap buffer behind a `bytes` handle. The exact shape of
/// `fmt.rs`'s `StringBuffer`, minus the UTF-8 invariant: the stored
/// `Vec<u8>` may hold ANY byte sequence.
#[repr(C)]
struct BytesBuffer {
    /// Heap-allocated raw byte buffer (NOT necessarily valid UTF-8).
    bytes: Vec<u8>,
}

/// Mint a heap `bytes` from a static / raw `(*const u8, len)` slice.
///
/// The runtime target of the `b"..."` byte-string literal: codegen
/// materialises the byte array into `.rodata` then calls this to mint a
/// FRESH owned buffer carrying a COPY of those bytes. NULL ptr or
/// `len <= 0` yields a valid EMPTY buffer (never null), so downstream
/// `len` / `get` / `drop` are always well-defined.
///
/// Mirrors `__cobrust_str_new` + `__cobrust_str_push_static` collapsed
/// into one mint (a `bytes` literal is immutable, so there is no
/// incremental-push phase).
///
/// # Safety
///
/// `ptr`/`len` must describe a valid byte slice (codegen always emits
/// this from `.rodata`), OR `ptr` may be NULL. The returned pointer must
/// be passed to [`__cobrust_bytes_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_from_raw(ptr: *const u8, len: i64) -> *mut u8 {
    let bytes = if ptr.is_null() || len <= 0 {
        Vec::new()
    } else {
        // SAFETY: caller-attestation per `# Safety` — `ptr` points to
        // `len` readable bytes maintained by the `.rodata` global.
        let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        slice.to_vec()
    };
    let buf = Box::new(BytesBuffer { bytes });
    Box::into_raw(buf).cast::<u8>()
}

/// Read the buffer's byte length without consuming it. NULL → 0.
///
/// The runtime target of `len(b)` (ADR-0093 §3 — `bytes` joins the
/// ADR-0088 sized set). Sibling of `__cobrust_str_len`.
///
/// # Safety
///
/// `b` must be a pointer returned by [`__cobrust_bytes_from_raw`] (or
/// [`__cobrust_bytes_clone`]) and not yet dropped, OR NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_len(b: *mut u8) -> i64 {
    if b.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let buf = unsafe { &*b.cast::<BytesBuffer>() };
    buf.bytes.len() as i64
}

/// Read the `i`-th byte as a `0..255` int. The runtime target of `b[i]`
/// (ADR-0093 §"§2.5 surface decision": `b"abc"[0] == 97`, an `int`, NOT
/// a 1-byte `bytes` — matches CPython 3 `bytes.__getitem__`).
///
/// An out-of-range index (`i < 0` or `i >= len`) or a NULL handle
/// returns `-1` (the bounds sentinel; a real byte is always `0..255`,
/// so `-1` is unambiguous). Sibling of `__cobrust_str_find`'s `-1`
/// convention. An explicit bounds-PANIC is an ADR-0093 Phase 2
/// deferral.
///
/// # Safety
///
/// `b` must be a pointer returned by [`__cobrust_bytes_from_raw`] (or
/// [`__cobrust_bytes_clone`]) and not yet dropped, OR NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_get(b: *mut u8, i: i64) -> i64 {
    if b.is_null() || i < 0 {
        return -1;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let buf = unsafe { &*b.cast::<BytesBuffer>() };
    match buf.bytes.get(i as usize) {
        Some(&byte) => i64::from(byte),
        None => -1,
    }
}

/// Free a `bytes` buffer. The runtime target of the scope-exit drop
/// schedule (`emit_drop_for_ty` `Ty::Bytes` arm). Idempotent on NULL.
/// Must be called EXACTLY ONCE per owned handle. Sibling of
/// `__cobrust_str_drop`.
///
/// # Safety
///
/// `b` must be a pointer returned by [`__cobrust_bytes_from_raw`] (or
/// [`__cobrust_bytes_clone`]) and not yet dropped, OR NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_drop(b: *mut u8) {
    if b.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety` — reclaims the Box and
    // drops the inner `Vec<u8>` exactly once.
    let _ = unsafe { Box::from_raw(b.cast::<BytesBuffer>()) };
}

/// Deep-copy a `bytes` buffer. Allocates a fresh `BytesBuffer`, copies
/// the source bytes, returns the new pointer. NULL → NULL.
///
/// ADR-0093 §"Ownership convention" — the clone-on-read escape hatch:
/// MIR operand-lowering emits this when a `bytes`-typed local would
/// otherwise be read twice (`let a = b; let b2 = b` source-level
/// pattern). Sibling of `__cobrust_str_clone`.
///
/// # Safety
///
/// `b` must be a pointer returned by [`__cobrust_bytes_from_raw`] and
/// not yet dropped, OR NULL. The returned pointer must be passed to
/// [`__cobrust_bytes_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_clone(b: *mut u8) -> *mut u8 {
    if b.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { &*b.cast::<BytesBuffer>() };
    let copy = Box::new(BytesBuffer {
        bytes: src.bytes.clone(),
    });
    Box::into_raw(copy).cast::<u8>()
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_then_len() {
        // SAFETY: documented contract.
        unsafe {
            let raw = b"abc";
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            assert!(!b.is_null());
            assert_eq!(__cobrust_bytes_len(b), 3);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn get_byte_values() {
        // SAFETY: contract. `b"abc"` → 97, 98, 99 (ASCII a/b/c).
        unsafe {
            let raw = b"abc";
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            assert_eq!(__cobrust_bytes_get(b, 0), 97);
            assert_eq!(__cobrust_bytes_get(b, 1), 98);
            assert_eq!(__cobrust_bytes_get(b, 2), 99);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn non_utf8_byte_roundtrips() {
        // The whole POINT of a dedicated family: `\xff` is NOT valid
        // UTF-8; the old str-buffer literal path corrupted it. The
        // bytes family stores + reads it byte-exact (255).
        // SAFETY: contract.
        unsafe {
            let raw: [u8; 3] = [0xff, 0x00, 0xfe];
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            assert_eq!(__cobrust_bytes_len(b), 3);
            assert_eq!(__cobrust_bytes_get(b, 0), 255);
            assert_eq!(__cobrust_bytes_get(b, 1), 0);
            assert_eq!(__cobrust_bytes_get(b, 2), 254);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn get_out_of_range_sentinel() {
        // SAFETY: contract. Out-of-range / negative → -1 (unambiguous;
        // a real byte is 0..255).
        unsafe {
            let raw = b"ab";
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            assert_eq!(__cobrust_bytes_get(b, 2), -1);
            assert_eq!(__cobrust_bytes_get(b, 100), -1);
            assert_eq!(__cobrust_bytes_get(b, -1), -1);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn empty_from_null_or_zero_len() {
        // SAFETY: contract. NULL ptr or len<=0 → valid EMPTY buffer.
        unsafe {
            let b = __cobrust_bytes_from_raw(std::ptr::null(), 0);
            assert!(!b.is_null());
            assert_eq!(__cobrust_bytes_len(b), 0);
            assert_eq!(__cobrust_bytes_get(b, 0), -1);
            __cobrust_bytes_drop(b);

            let raw = b"x";
            let b2 = __cobrust_bytes_from_raw(raw.as_ptr(), 0);
            assert_eq!(__cobrust_bytes_len(b2), 0);
            __cobrust_bytes_drop(b2);
        }
    }

    #[test]
    fn clone_is_independent() {
        // SAFETY: contract. The clone is a deep copy — dropping one does
        // NOT free the other (no double-free / use-after-free).
        unsafe {
            let raw = b"hello";
            let a = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            let c = __cobrust_bytes_clone(a);
            assert!(!c.is_null());
            assert_eq!(__cobrust_bytes_len(c), 5);
            assert_eq!(__cobrust_bytes_get(c, 0), 104); // 'h'
            // Drop the original; the clone must still be readable.
            __cobrust_bytes_drop(a);
            assert_eq!(__cobrust_bytes_len(c), 5);
            assert_eq!(__cobrust_bytes_get(c, 4), 111); // 'o'
            __cobrust_bytes_drop(c);
        }
    }

    #[test]
    fn null_safety_on_every_shim() {
        // SAFETY: documented NULL path on every helper.
        unsafe {
            __cobrust_bytes_drop(std::ptr::null_mut());
            assert_eq!(__cobrust_bytes_len(std::ptr::null_mut()), 0);
            assert_eq!(__cobrust_bytes_get(std::ptr::null_mut(), 0), -1);
            assert!(__cobrust_bytes_clone(std::ptr::null_mut()).is_null());
        }
    }

    #[test]
    fn hammer_no_leak_or_crash() {
        // The DROP/UB hammer: 1000 mint→read→drop cycles. A double-free
        // or use-after-free would crash here; a leak shows under a
        // sanitizer / valgrind run of the test binary.
        // SAFETY: contract.
        unsafe {
            for i in 0..1000u32 {
                let raw = b"payload";
                let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
                assert_eq!(__cobrust_bytes_len(b), 7);
                assert_eq!(
                    __cobrust_bytes_get(b, (i % 7) as i64),
                    i64::from(raw[(i % 7) as usize])
                );
                let c = __cobrust_bytes_clone(b);
                __cobrust_bytes_drop(b);
                __cobrust_bytes_drop(c);
            }
        }
    }
}
