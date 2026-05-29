//! C-ABI shims — the runtime surface a compiled `.cb` program binds onto
//! when it does `import fang` and calls `fang.hash_password(pw)` /
//! `fang.verify_password(pw, hash)` (ADR-0078 backend Phase 2; the
//! tenth ecosystem module, FIRST backend Phase-2 crate).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libfang.a` after `libcobrust_stdlib.a` (Linux wraps both
//! in `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI — value pattern (str→str and (str,str)→bool), no handles
//!
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//! - **Bool** crosses as the C-ABI `bool` (the FIRST `-> bool` value-fn
//!   return on the chain). The codegen extern declares an `i1` return
//!   that lands in the `_ecoret` bool local; the
//!   `if ok:` branch consumes it directly.
//!
//! # Security choices (elegance law — no auth footguns)
//!
//! - `hash_password` uses [`argon2::Argon2::default`] = **argon2id**
//!   with OWASP-recommended parameters. NO algorithm / cost knob is
//!   exposed in Phase 1, so a weak algo / weak params cannot be picked
//!   by accident.
//! - The returned hash is the **full PHC string** (`$argon2id$…`): the
//!   random salt + parameters travel WITH the hash. No separate-salt
//!   management.
//! - `verify_password` is **constant-time**
//!   ([`argon2::Argon2::verify_password`]).
//! - A WRONG password is a normal `false` return — NOT a panic, NOT an
//!   error across the boundary (CLAUDE.md §2.2: exceptions are not the
//!   default error path). No plaintext password is ever logged.
//!
//! # Ownership
//!
//! `hash_password` is pure value-in-value-out (`Str → Str`): the input
//! Str is BORROWED (read into an owned Rust `String` then released —
//! the `.cb` caller's drop schedule frees the input buffer at scope
//! exit). The returned `*mut u8` Str buffer is owned by the caller and
//! freed exactly once by the existing `__cobrust_str_drop` at scope
//! exit. `verify_password` borrows both input Strs and returns a scalar
//! `bool` (nothing to free). NO handles, NO callbacks.

// C-ABI-boundary cast allows — mirror `cobrust-scale/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]

use argon2::Argon2;
use password_hash::rand_core::OsRng;
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

// =====================================================================
// Cobrust Str-buffer ABI — declared here, resolved from
// libcobrust_stdlib.a at link time (ADR-0072 Q5; no Rust dep).
// =====================================================================

unsafe extern "C" {
    /// Allocate a fresh empty Cobrust `Str` buffer.
    fn __cobrust_str_new() -> *mut u8;
    /// Append `len` UTF-8 bytes at `ptr` to the buffer.
    fn __cobrust_str_push_static(buf: *mut u8, ptr: *const u8, len: i64);
    /// Borrow the buffer's byte pointer (valid until the next mutation).
    fn __cobrust_str_ptr(buf: *mut u8) -> *const u8;
    /// The buffer's byte length.
    fn __cobrust_str_len(buf: *mut u8) -> i64;
}

/// Read a Cobrust `Str` buffer pointer into an owned `String`. Tolerates
/// null / empty. Mirrors `cobrust-scale/src/cabi.rs::read_str_buf`.
///
/// # Safety
///
/// `buf` must be null or a valid Cobrust `Str` buffer produced by
/// `__cobrust_str_new`.
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    // SAFETY: caller attests `buf` is a valid Cobrust Str buffer.
    unsafe {
        let ptr = __cobrust_str_ptr(buf);
        let len = __cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

/// Allocate a fresh Cobrust `Str` buffer carrying `s`'s bytes. The `.cb`
/// caller's drop schedule frees it via `__cobrust_str_drop`.
fn alloc_str_buffer(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` returns a valid buffer;
    // `__cobrust_str_push_static` copies `s` into it.
    unsafe {
        let buf = __cobrust_str_new();
        if !s.is_empty() {
            __cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

// =====================================================================
// fang C-ABI surface.
// =====================================================================

/// `fang.hash_password(pw) -> str`. Computes an **argon2id** PHC hash of
/// `pw` with a fresh random salt (drawn from the OS CSPRNG) and returns
/// the full self-describing PHC string (`$argon2id$v=…$m=…,t=…,p=…$
/// <salt>$<hash>`) as a freshly-allocated Cobrust `Str` buffer.
///
/// The salt is embedded in the returned string — there is no separate
/// salt to manage. Two calls with the same password return DIFFERENT
/// strings (a fresh salt each time), and both verify TRUE.
///
/// On the (effectively impossible) internal hashing error the returned
/// buffer carries the empty-string sentinel — matching the std.json /
/// F59 fail-clean convention. NO panic, NO null across the boundary, NO
/// plaintext logging.
///
/// # Safety
///
/// `pw` must be null or a valid Cobrust `Str` buffer. The returned
/// pointer is an owned Cobrust `Str` buffer, freed once by the existing
/// `__cobrust_str_drop` at the `.cb` scope exit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fang_hash_password(pw: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let password = unsafe { read_str_buf(pw) };
    // Fresh random salt from the OS CSPRNG (PHC discipline: a new salt
    // per hash so identical passwords produce distinct hashes).
    let salt = SaltString::generate(&mut OsRng);
    // `Argon2::default()` is argon2id with OWASP-recommended params.
    match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(hash) => alloc_str_buffer(&hash.to_string()),
        // Fail clean — no panic, no plaintext in the (absent) log.
        Err(_) => alloc_str_buffer(""),
    }
}

/// `fang.verify_password(pw, hash) -> bool`. Constant-time verification
/// of `pw` against the PHC `hash` string produced by
/// [`__cobrust_fang_hash_password`].
///
/// Returns `true` iff `pw` matches; a WRONG password (or a malformed /
/// empty `hash`) is a normal `false` return — NOT a panic, NOT an error
/// across the boundary (a mismatch is expected control flow, not an
/// exceptional condition). No plaintext password is ever logged.
///
/// # Safety
///
/// `pw` and `hash` must each be null or a valid Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fang_verify_password(pw: *mut u8, hash: *mut u8) -> bool {
    // SAFETY: caller-attestation per `# Safety`.
    let password = unsafe { read_str_buf(pw) };
    // SAFETY: caller-attestation per `# Safety`.
    let phc = unsafe { read_str_buf(hash) };
    // A malformed / empty PHC string is a non-match, not an error.
    let Ok(parsed) = PasswordHash::new(&phc) else {
        return false;
    };
    // Constant-time verify; `Ok(())` => match, any `Err` => non-match.
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
mod tests {
    use super::*;

    // The Str-buffer ABI is exported by cobrust-stdlib (a workspace
    // crate). For these unit tests we link it as a dev-dependency so the
    // `extern "C"` decls above resolve (in production the symbols come
    // from libcobrust_stdlib.a at the `cobrust build` link step). The
    // `extern crate` + the `#[used]` static anchor forces cargo to put
    // the rlib on the test link line — a bare `extern "C"` decl alone
    // does not create a crate-dependency link edge.
    extern crate cobrust_stdlib;
    #[used]
    static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
        cobrust_stdlib::fmt::__cobrust_str_new;

    // The Str drop shim from libcobrust_stdlib (used to free the
    // buffers we hand out under test).
    unsafe extern "C" {
        fn __cobrust_str_drop(buf: *mut u8);
    }
    unsafe fn drop_str_for_test(buf: *mut u8) {
        unsafe { __cobrust_str_drop(buf) }
    }

    /// Round-trip: a password hashed with `hash_password` verifies TRUE
    /// against itself — the core argon2 contract, exercised exactly as a
    /// compiled `.cb` program would call the shims.
    #[test]
    fn cabi_hash_then_verify_round_trip_true() {
        unsafe {
            let pw = alloc_str_buffer("hunter2");
            let h = __cobrust_fang_hash_password(pw);
            assert!(
                __cobrust_fang_verify_password(pw, h),
                "right pw must verify"
            );
            drop_str_for_test(pw);
            drop_str_for_test(h);
        }
    }

    /// Wrong-password-rejects — the security property that matters: a
    /// hash of `"hunter2"` must NOT verify against `"wrong"`. A mismatch
    /// is a clean `false`, never a panic.
    #[test]
    fn cabi_wrong_password_rejects_false() {
        unsafe {
            let pw = alloc_str_buffer("hunter2");
            let wrong = alloc_str_buffer("wrong");
            let h = __cobrust_fang_hash_password(pw);
            assert!(
                !__cobrust_fang_verify_password(wrong, h),
                "wrong pw must NOT verify"
            );
            drop_str_for_test(pw);
            drop_str_for_test(wrong);
            drop_str_for_test(h);
        }
    }

    /// Hash-is-argon2id-PHC — the returned string starts with the
    /// `$argon2id$` prefix, proving the algorithm (not a weak
    /// argon2i/argon2d or an unsalted digest) AND that the salt is
    /// embedded in the self-describing PHC string.
    #[test]
    fn cabi_hash_is_argon2id_phc() {
        unsafe {
            let pw = alloc_str_buffer("hunter2");
            let h = __cobrust_fang_hash_password(pw);
            let rendered = read_str_buf(h);
            assert!(
                rendered.starts_with("$argon2id$"),
                "expected argon2id PHC prefix, got: {rendered}"
            );
            drop_str_for_test(pw);
            drop_str_for_test(h);
        }
    }

    /// Hash-is-nondeterministic — two hashes of the SAME password differ
    /// (fresh random salt per call) yet both verify TRUE.
    #[test]
    fn cabi_hash_is_nondeterministic_both_verify() {
        unsafe {
            let pw = alloc_str_buffer("x");
            let h1 = __cobrust_fang_hash_password(pw);
            let h2 = __cobrust_fang_hash_password(pw);
            assert_ne!(
                read_str_buf(h1),
                read_str_buf(h2),
                "two hashes of the same pw must differ (random salt)"
            );
            assert!(__cobrust_fang_verify_password(pw, h1));
            assert!(__cobrust_fang_verify_password(pw, h2));
            drop_str_for_test(pw);
            drop_str_for_test(h1);
            drop_str_for_test(h2);
        }
    }

    /// A malformed / empty PHC `hash` is a clean `false`, never a panic.
    #[test]
    fn cabi_verify_with_malformed_hash_is_false_not_panic() {
        unsafe {
            let pw = alloc_str_buffer("hunter2");
            let bogus = alloc_str_buffer("not-a-phc-string");
            assert!(!__cobrust_fang_verify_password(pw, bogus));
            // Empty / null hash is also a clean false.
            assert!(!__cobrust_fang_verify_password(pw, std::ptr::null_mut()));
            drop_str_for_test(pw);
            drop_str_for_test(bogus);
        }
    }
}
