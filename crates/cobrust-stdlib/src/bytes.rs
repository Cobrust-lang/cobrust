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
/// **Negative index** (ADR-0095 Option B): `i` is Python-normalized
/// against the byte length `n` — a negative `i` reads `n + i`, so
/// `b"\x01\x02\xff"[-1] == 255` (the last byte). **Out-of-range**
/// (`i >= n` after normalization, OR `i < -n`): this TRAPS (panic → the
/// cobrust runtime maps it to exit 3, INTERNAL_PANIC) with a readable
/// `bytes index out of range: i=.. len=..` diagnostic (§2.5-B),
/// mirroring Rust's slice-OOB panic. There is NO silent `-1` sentinel
/// (the ADR-0093-interim §2.2 hole this Option-B maturation closes — a
/// sentinel `-1` is an in-band wrong value §2.2 forbids). A NULL handle
/// is treated as empty (`n == 0`), so ANY index traps.
///
/// # Panics
///
/// Panics (the unrecoverable index-out-of-range TRAP) when the normalized
/// index falls outside `[0, len)`.
///
/// # Safety
///
/// `b` must be a pointer returned by [`__cobrust_bytes_from_raw`] (or
/// [`__cobrust_bytes_clone`]) and not yet dropped, OR NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_get(b: *mut u8, i: i64) -> i64 {
    // A NULL handle is empty — `len == 0`, so any index traps.
    let bytes: &[u8] = if b.is_null() {
        &[]
    } else {
        // SAFETY: caller-attestation per `# Safety`.
        let buf = unsafe { &*b.cast::<BytesBuffer>() };
        &buf.bytes
    };
    let len = bytes.len() as i64;
    // Python negative-index normalization: a negative `i` counts from the
    // end (`len + i`); a non-negative `i` is itself.
    let idx = if i < 0 { len + i } else { i };
    // §2.2: TRAP on a true OOB — never an in-band sentinel `-1`.
    // §2.5-B: the diagnostic names the bad index AND the length. Route
    // through the project trap convention (`crate::panic::panic`, the same
    // path `__cobrust_bytes_decode` uses) — NOT a raw `assert!`: across the
    // `extern "C"` boundary a raw panic aborts (SIGABRT / exit 134) with a
    // path-leaking Rust backtrace, whereas `panic()` cleanly maps to exit 3
    // + a one-line `cobrust panic: ...` message (the INTERNAL_PANIC contract).
    if idx < 0 || idx >= len {
        crate::panic::panic(&format!("bytes index out of range: i={i} len={len}"));
    }
    i64::from(bytes[idx as usize])
}

/// Read the buffer's raw byte pointer without consuming it. Returns
/// `null` for an empty / NULL buffer. Sibling of `__cobrust_str_ptr`.
///
/// The escape hatch for an O(1) `&[u8]` read of a `bytes` value across
/// the C ABI (the `cobrust-dora` `send_output_bytes` shim reads the
/// payload via `from_raw_parts(ptr, len)` rather than looping
/// [`__cobrust_bytes_get`] O(n) times). The handle is BORROWED — the
/// returned pointer is valid only while the handle lives, and the caller
/// must NOT free through it (the `.cb` scope's `__cobrust_bytes_drop`
/// owns the allocation).
///
/// # Safety
///
/// `b` must be a pointer returned by [`__cobrust_bytes_from_raw`] (or
/// [`__cobrust_bytes_clone`]) and not yet dropped, OR NULL. The returned
/// pointer aliases the buffer's heap allocation and is invalidated by a
/// subsequent [`__cobrust_bytes_drop`] of `b`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_ptr(b: *mut u8) -> *const u8 {
    if b.is_null() {
        return std::ptr::null();
    }
    // SAFETY: caller-attestation per `# Safety`.
    let buf = unsafe { &*b.cast::<BytesBuffer>() };
    if buf.bytes.is_empty() {
        std::ptr::null()
    } else {
        buf.bytes.as_ptr()
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

// =====================================================================
// ADR-0093 Phase 2 — the byte-buffer surface (slice / concat / encode /
// decode / hex). EVERY function below MINTS a fresh heap value (a fresh
// `bytes` or a fresh `str`) that the `.cb` scope owns + drops EXACTLY
// ONCE; the input handle(s) are BORROWED (read-only) — NEVER freed here.
// This is the SAME mint-fresh / borrow-inputs discipline `__cobrust_str_
// concat` / `__cobrust_str_lower` already run (ADR-0050c).
// =====================================================================

/// Borrow a `bytes` handle as a read-only byte slice. NULL / empty → `&[]`.
/// The handle is NOT consumed.
///
/// # Safety
///
/// `b` must be NULL or a pointer returned by [`__cobrust_bytes_from_raw`]
/// (or [`__cobrust_bytes_clone`]) and not yet dropped.
unsafe fn bytes_buf_as_slice<'a>(b: *mut u8) -> &'a [u8] {
    if b.is_null() {
        return &[];
    }
    // SAFETY: caller-attestation per `# Safety`.
    let buf = unsafe { &*b.cast::<BytesBuffer>() };
    buf.bytes.as_slice()
}

/// Mint a fresh owned `bytes` from a byte slice (the shared tail of
/// `slice` / `concat` / `from_str`). Always a fresh `Box`, so the result
/// is `.cb`-owned + dropped once.
fn mint_bytes(bytes: Vec<u8>) -> *mut u8 {
    let buf = Box::new(BytesBuffer { bytes });
    Box::into_raw(buf).cast::<u8>()
}

/// Slice `b[lo:hi]` into a FRESH owned `bytes` carrying a COPY of the
/// `[lo, hi)` byte range. The runtime target of the `.cb` `b[lo:hi]`
/// slice expression (ADR-0093 Phase 2, the `__cobrust_coil_buffer_slice`
/// mirror — but with **Python clamp** semantics, NOT the buffer's
/// abort-on-OOB: CPython `b"abcd"[1:99] == b"bcd"` and `b"abcd"[3:1] ==
/// b""`, never an exception). Bounds are clamped to `[0, len]` and
/// `hi < lo` yields an empty buffer. NULL handle → empty buffer.
///
/// The input `b` is BORROWED (read-only); the caller's drop schedule
/// still frees it. The returned pointer is a fresh buffer the caller owns
/// and must free EXACTLY ONCE via [`__cobrust_bytes_drop`].
///
/// # Safety
///
/// `b` must be NULL or a pointer returned by [`__cobrust_bytes_from_raw`]
/// (or [`__cobrust_bytes_clone`]) and not yet dropped. The returned
/// pointer must be passed to [`__cobrust_bytes_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_slice(b: *mut u8, lo: i64, hi: i64) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { bytes_buf_as_slice(b) };
    let len = src.len() as i64;
    // CPython slice clamp: negative is NOT handled here (a `b[-1:]` form
    // is an ADR-0093 §Phasing deferral, like the buffer slice); a bare
    // `lo:hi` with non-negative bounds clamps to `[0, len]`.
    let lo_c = lo.clamp(0, len) as usize;
    let hi_c = hi.clamp(0, len) as usize;
    let slice = if hi_c > lo_c {
        src[lo_c..hi_c].to_vec()
    } else {
        Vec::new()
    };
    mint_bytes(slice)
}

/// Concatenate `a + b` into a FRESH owned `bytes` carrying `a`'s bytes
/// followed by `b`'s bytes. The runtime target of the `.cb` `b1 + b2`
/// operator (ADR-0093 Phase 2, the `__cobrust_str_concat` mirror). NULL
/// operands are treated as empty.
///
/// Both inputs are BORROWED (read-only); the caller's drop schedule still
/// frees them. The returned pointer is a fresh buffer the caller owns and
/// must free EXACTLY ONCE via [`__cobrust_bytes_drop`].
///
/// # Safety
///
/// `a` and `b` must each be NULL or a pointer returned by
/// [`__cobrust_bytes_from_raw`] (or [`__cobrust_bytes_clone`]) and not
/// yet dropped. The returned pointer must be passed to
/// [`__cobrust_bytes_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_concat(a: *mut u8, b: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let sa = unsafe { bytes_buf_as_slice(a) };
    // SAFETY: caller-attestation per `# Safety`.
    let sb = unsafe { bytes_buf_as_slice(b) };
    let mut bytes = Vec::with_capacity(sa.len() + sb.len());
    bytes.extend_from_slice(sa);
    bytes.extend_from_slice(sb);
    mint_bytes(bytes)
}

/// Mint a FRESH owned `bytes` from a `str` handle (UTF-8 encode — the
/// runtime target of `.cb` `s.encode()`, ADR-0093 Phase 2). The `str`'s
/// stored bytes are ALWAYS valid UTF-8 (the Str buffer invariant), so
/// encode is total — there is no error path. NULL → empty buffer.
///
/// The input `s` is BORROWED (read-only) via the `__cobrust_str_*`
/// accessors; the caller's Str drop schedule still frees it. The returned
/// `bytes` pointer is a fresh buffer the caller owns + drops EXACTLY ONCE
/// via [`__cobrust_bytes_drop`].
///
/// # Safety
///
/// `s` must be NULL or a pointer returned by the `__cobrust_str_*` family
/// (a `StringBuffer`) and not yet dropped. The returned pointer must be
/// passed to [`__cobrust_bytes_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_from_str(s: *mut u8) -> *mut u8 {
    if s.is_null() {
        return mint_bytes(Vec::new());
    }
    // Borrow the Str buffer's UTF-8 bytes via the fmt-crate C-ABI (no
    // need to name the private `StringBuffer` — same `(ptr, len)` read
    // `io.rs::str_buf_as_str_phase3` uses).
    // SAFETY: `s` is a valid Str pointer per `# Safety`.
    let len = unsafe { crate::fmt::__cobrust_str_len(s) } as usize;
    if len == 0 {
        return mint_bytes(Vec::new());
    }
    // SAFETY: `s` is a valid Str pointer; `ptr` is its UTF-8 buffer.
    let ptr = unsafe { crate::fmt::__cobrust_str_ptr(s) };
    if ptr.is_null() {
        return mint_bytes(Vec::new());
    }
    // SAFETY: `ptr` points to `len` bytes maintained by the Str write
    // paths (UTF-8 by the StringBuffer invariant).
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    mint_bytes(slice.to_vec())
}

/// Decode `bytes` → `str` (UTF-8). The runtime target of `.cb`
/// `b.decode()` (ADR-0093 Phase 2).
///
/// **The §2.2 no-silent-coercion design point.** INVALID UTF-8 is NOT
/// lossily replaced (no U+FFFD substitution) and NOT silently truncated —
/// CLAUDE.md §2.2 forbids silent coercion. An invalid byte sequence
/// **TRAPS**: it writes a structured `bytes.decode: invalid utf-8 at byte
/// N` diagnostic to stderr and exits the process (the same `std.panic`
/// trap every other Cobrust domain error surfaces through — exit code 3,
/// `INTERNAL_PANIC` per ADR-0024 / `runtime.rs`). The byte offset `N` is the first invalid byte (the
/// LLM/user consumes stderr to locate the bad input). A `Result[str,
/// DecodeError]` ergonomic surface is the named ADR-0093 §Phasing
/// follow-up once stdlib-fallible-Result returns are wired (today NO
/// stdlib op returns a surface `Result`; the trap is the sound v1 — a
/// decode failure is a precondition violation, "truly unrecoverable" per
/// §2.2).
///
/// On VALID UTF-8 it mints a FRESH owned `str` (a `StringBuffer` copy)
/// the caller owns + drops EXACTLY ONCE via `__cobrust_str_drop`. The
/// input `b` is BORROWED (read-only). NULL → empty string.
///
/// # Safety
///
/// `b` must be NULL or a pointer returned by [`__cobrust_bytes_from_raw`]
/// (or [`__cobrust_bytes_clone`]) and not yet dropped. The returned
/// pointer must be passed to `__cobrust_str_drop` exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_decode(b: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { bytes_buf_as_slice(b) };
    match std::str::from_utf8(src) {
        Ok(_) => {
            // Valid UTF-8 — mint a fresh Str buffer carrying a COPY.
            // SAFETY: `__cobrust_str_new` returns a fresh StringBuffer.
            let out = unsafe { crate::fmt::__cobrust_str_new() };
            if !src.is_empty() {
                // SAFETY: `out` is a fresh Str buffer; `src` is a valid
                // UTF-8 slice (just verified).
                unsafe {
                    crate::fmt::__cobrust_str_push_static(out, src.as_ptr(), src.len() as i64);
                }
            }
            out
        }
        Err(e) => {
            // INVALID UTF-8 — TRAP (NEVER lossy / replacement-char per
            // §2.2). `valid_up_to()` is the byte offset of the first
            // invalid byte. Sibling of every `std.panic` domain trap.
            let msg = format!("bytes.decode: invalid utf-8 at byte {}", e.valid_up_to());
            crate::panic::panic(&msg)
        }
    }
}

/// Lowercase hex-encode `bytes` → `str` (the runtime target of `.cb`
/// `b.hex()`, ADR-0093 Phase 2). CPython `bytes.hex()`: `b"\xff\x00".hex()
/// == "ff00"` (lowercase, two chars per byte, no separator). NULL / empty
/// → empty string.
///
/// Mints a FRESH owned `str` the caller owns + drops EXACTLY ONCE via
/// `__cobrust_str_drop`. The input `b` is BORROWED (read-only).
///
/// # Safety
///
/// `b` must be NULL or a pointer returned by [`__cobrust_bytes_from_raw`]
/// (or [`__cobrust_bytes_clone`]) and not yet dropped. The returned
/// pointer must be passed to `__cobrust_str_drop` exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_bytes_hex(b: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { bytes_buf_as_slice(b) };
    // SAFETY: `__cobrust_str_new` returns a fresh StringBuffer.
    let out = unsafe { crate::fmt::__cobrust_str_new() };
    if src.is_empty() {
        return out;
    }
    let mut hex = String::with_capacity(src.len() * 2);
    for &byte in src {
        use std::fmt::Write as _;
        // `write!` to a String never fails; lowercase two-digit hex.
        let _ = write!(hex, "{byte:02x}");
    }
    // SAFETY: `out` is a fresh Str buffer; `hex` is ASCII (valid UTF-8).
    unsafe {
        crate::fmt::__cobrust_str_push_static(out, hex.as_ptr(), hex.len() as i64);
    }
    out
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
    fn ptr_reads_raw_slice_byte_exact() {
        // SAFETY: contract. `__cobrust_bytes_ptr` + len yields an O(1)
        // `&[u8]` that round-trips a non-UTF-8 payload byte-exact (the
        // `send_output_bytes` dora shim's read path). Empty / NULL → null
        // pointer.
        unsafe {
            let raw: [u8; 4] = [0x00, 0xff, 0x01, 0xfe];
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            let ptr = __cobrust_bytes_ptr(b);
            let len = __cobrust_bytes_len(b) as usize;
            assert!(!ptr.is_null());
            assert_eq!(len, 4);
            let slice = std::slice::from_raw_parts(ptr, len);
            assert_eq!(slice, &raw, "raw slice must be byte-exact (non-UTF-8 safe)");
            __cobrust_bytes_drop(b);
            // Empty + NULL → null pointer.
            let e = __cobrust_bytes_from_raw(std::ptr::null(), 0);
            assert!(__cobrust_bytes_ptr(e).is_null());
            __cobrust_bytes_drop(e);
            assert!(__cobrust_bytes_ptr(std::ptr::null_mut()).is_null());
        }
    }

    #[test]
    fn get_negative_index_from_end() {
        // ADR-0095 Option B: `b[-1]` is the LAST byte (CPython
        // b"\x01\x02\xff"[-1] == 255), `b[-2]` the second-to-last, etc.
        // SAFETY: contract.
        unsafe {
            let raw: [u8; 3] = [0x01, 0x02, 0xff];
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            assert_eq!(__cobrust_bytes_get(b, -1), 255);
            assert_eq!(__cobrust_bytes_get(b, -2), 2);
            assert_eq!(__cobrust_bytes_get(b, -3), 1);
            __cobrust_bytes_drop(b);
        }
    }

    // NOTE on OOB-trap coverage: `__cobrust_bytes_get` is `extern "C"`, so a
    // panic crossing that ABI boundary is a NON-unwinding abort (SIGABRT),
    // NOT a catchable unwind — `#[should_panic]` cannot observe it here
    // (it aborts the whole test process). The OOB TRAP (§2.2, both index
    // directions + NULL) is verified END-TO-END in
    // `cobrust-cli/tests/bytes_ops_e2e.rs` (build a `.cb`, run the exe,
    // assert the non-zero `std.panic` exit + the `bytes index out of range`
    // diagnostic on stderr), which is the production trap path.

    #[test]
    fn empty_from_null_or_zero_len() {
        // SAFETY: contract. NULL ptr or len<=0 → valid EMPTY buffer.
        unsafe {
            let b = __cobrust_bytes_from_raw(std::ptr::null(), 0);
            assert!(!b.is_null());
            assert_eq!(__cobrust_bytes_len(b), 0);
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
            assert!(__cobrust_bytes_clone(std::ptr::null_mut()).is_null());
            // `__cobrust_bytes_get` on NULL is len-0, so ANY index is OOB and
            // traps via `crate::panic::panic` (exit 3). That trap can't be
            // asserted here — it terminates the process, not a catchable
            // `#[should_panic]` — so it is verified END-TO-END in the cli
            // `bytes_ops_e2e` OOB-trap suite (asserting exit == 3), not as a
            // unit test.
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

    // ----- ADR-0093 Phase 2 — slice / concat / from_str / hex -----
    // (decode of INVALID UTF-8 traps + exits the process, so it is
    // verified end-to-end via the `.cb` corpus, not a unit test — a unit
    // test cannot assert process exit without forking; the VALID-UTF-8
    // decode path IS unit-tested below.)

    /// Read a fresh Str buffer (minted by from_str / decode / hex) back as
    /// a `String`, then drop it. SAFETY: `s` is a fresh Str pointer.
    unsafe fn read_str_and_drop(s: *mut u8) -> String {
        let len = unsafe { crate::fmt::__cobrust_str_len(s) } as usize;
        let out = if len == 0 {
            String::new()
        } else {
            let ptr = unsafe { crate::fmt::__cobrust_str_ptr(s) };
            let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
            String::from_utf8(bytes.to_vec()).unwrap()
        };
        unsafe { crate::fmt::__cobrust_str_drop(s) };
        out
    }

    #[test]
    fn slice_basic_and_clamp() {
        // SAFETY: contract. `b"abcde"[1:4] == b"bcd"`; Python clamp on OOB.
        unsafe {
            let raw = b"abcde";
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            // [1:4] -> "bcd"
            let s = __cobrust_bytes_slice(b, 1, 4);
            assert_eq!(__cobrust_bytes_len(s), 3);
            assert_eq!(__cobrust_bytes_get(s, 0), 98); // 'b'
            assert_eq!(__cobrust_bytes_get(s, 2), 100); // 'd'
            __cobrust_bytes_drop(s);
            // [1:99] clamps to [1:5] -> "bcde" (CPython, NOT an abort)
            let s2 = __cobrust_bytes_slice(b, 1, 99);
            assert_eq!(__cobrust_bytes_len(s2), 4);
            __cobrust_bytes_drop(s2);
            // [3:1] (hi < lo) -> empty
            let s3 = __cobrust_bytes_slice(b, 3, 1);
            assert_eq!(__cobrust_bytes_len(s3), 0);
            __cobrust_bytes_drop(s3);
            // The SOURCE survives (borrowed, not consumed) — drops once.
            assert_eq!(__cobrust_bytes_len(b), 5);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn slice_preserves_non_utf8() {
        // SAFETY: contract. A slice of a non-UTF-8 buffer is byte-exact.
        unsafe {
            let raw: [u8; 4] = [0xff, 0x41, 0x00, 0xfe];
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            let s = __cobrust_bytes_slice(b, 0, 2);
            assert_eq!(__cobrust_bytes_len(s), 2);
            assert_eq!(__cobrust_bytes_get(s, 0), 255);
            assert_eq!(__cobrust_bytes_get(s, 1), 65); // 'A'
            __cobrust_bytes_drop(s);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn concat_basic() {
        // SAFETY: contract. `b"ab" + b"cd" == b"abcd"`; both inputs
        // borrowed (survive + drop once).
        unsafe {
            let ra = b"ab";
            let rb = b"cd";
            let a = __cobrust_bytes_from_raw(ra.as_ptr(), ra.len() as i64);
            let b = __cobrust_bytes_from_raw(rb.as_ptr(), rb.len() as i64);
            let c = __cobrust_bytes_concat(a, b);
            assert_eq!(__cobrust_bytes_len(c), 4);
            assert_eq!(__cobrust_bytes_get(c, 0), 97); // 'a'
            assert_eq!(__cobrust_bytes_get(c, 3), 100); // 'd'
            __cobrust_bytes_drop(c);
            // Both sources survive (borrowed).
            assert_eq!(__cobrust_bytes_len(a), 2);
            assert_eq!(__cobrust_bytes_len(b), 2);
            __cobrust_bytes_drop(a);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn concat_null_operand_is_empty() {
        // SAFETY: contract. NULL operand treated as empty.
        unsafe {
            let ra = b"xy";
            let a = __cobrust_bytes_from_raw(ra.as_ptr(), ra.len() as i64);
            let c = __cobrust_bytes_concat(a, std::ptr::null_mut());
            assert_eq!(__cobrust_bytes_len(c), 2);
            __cobrust_bytes_drop(c);
            __cobrust_bytes_drop(a);
        }
    }

    #[test]
    fn encode_then_decode_roundtrip() {
        // SAFETY: contract. str.encode -> bytes -> .decode == str (the
        // load-bearing round-trip, on VALID UTF-8).
        unsafe {
            let src = "héllo"; // multi-byte UTF-8 (é = 2 bytes)
            let s = crate::fmt::__cobrust_str_new();
            crate::fmt::__cobrust_str_push_static(s, src.as_ptr(), src.len() as i64);
            // encode
            let b = __cobrust_bytes_from_str(s);
            assert_eq!(__cobrust_bytes_len(b), src.len() as i64);
            // decode (valid UTF-8 → fresh str)
            let back = __cobrust_bytes_decode(b);
            let back_s = read_str_and_drop(back);
            assert_eq!(back_s, src);
            __cobrust_bytes_drop(b);
            crate::fmt::__cobrust_str_drop(s);
        }
    }

    #[test]
    fn decode_valid_ascii() {
        // SAFETY: contract. decode of a valid ASCII bytes → str.
        unsafe {
            let raw = b"hello";
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            let s = __cobrust_bytes_decode(b);
            assert_eq!(read_str_and_drop(s), "hello");
            // input survives (borrowed).
            assert_eq!(__cobrust_bytes_len(b), 5);
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn hex_lowercase() {
        // SAFETY: contract. `b"\xff\x00\x10".hex() == "ff0010"` (CPython).
        unsafe {
            let raw: [u8; 3] = [0xff, 0x00, 0x10];
            let b = __cobrust_bytes_from_raw(raw.as_ptr(), raw.len() as i64);
            let s = __cobrust_bytes_hex(b);
            assert_eq!(read_str_and_drop(s), "ff0010");
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn hex_empty() {
        // SAFETY: contract. Empty bytes → empty hex string.
        unsafe {
            let b = __cobrust_bytes_from_raw(std::ptr::null(), 0);
            let s = __cobrust_bytes_hex(b);
            assert_eq!(read_str_and_drop(s), "");
            __cobrust_bytes_drop(b);
        }
    }

    #[test]
    fn phase2_hammer_no_leak_or_crash() {
        // DROP/UB hammer for the Phase-2 minting ops: 1000 cycles each
        // minting a fresh bytes (slice/concat) + a fresh str (decode/hex)
        // and dropping all. A double-free / leak crashes or diverges here.
        // SAFETY: contract.
        unsafe {
            for _ in 0..1000u32 {
                let ra = b"abc";
                let rb = b"def";
                let a = __cobrust_bytes_from_raw(ra.as_ptr(), ra.len() as i64);
                let b = __cobrust_bytes_from_raw(rb.as_ptr(), rb.len() as i64);
                // slice mints fresh bytes
                let sl = __cobrust_bytes_slice(a, 0, 2);
                __cobrust_bytes_drop(sl);
                // concat mints fresh bytes
                let cat = __cobrust_bytes_concat(a, b);
                assert_eq!(__cobrust_bytes_len(cat), 6);
                // decode mints fresh str
                let dec = __cobrust_bytes_decode(cat);
                crate::fmt::__cobrust_str_drop(dec);
                // hex mints fresh str
                let hx = __cobrust_bytes_hex(cat);
                crate::fmt::__cobrust_str_drop(hx);
                __cobrust_bytes_drop(cat);
                // inputs (a, b) borrowed throughout — drop once each.
                __cobrust_bytes_drop(a);
                __cobrust_bytes_drop(b);
            }
        }
    }
}
