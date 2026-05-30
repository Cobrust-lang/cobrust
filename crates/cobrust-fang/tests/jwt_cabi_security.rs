//! ADSD TEST-first corpus — the BYTE-PRECISE JWT security footguns, at
//! the C-ABI shim level, with the real signing shim as an oracle.
//!
//! These two cases need byte-level control over the token (which pure
//! `.cb` cannot express — `.cb` has no char-indexing / base64url), so per
//! the TEST-author brief they live here as a Rust integration test rather
//! than in `ecosystem_fang_jwt_e2e.rs`:
//!
//! 1. **payload-segment byte tamper** — mint a genuinely-signed token via
//!    the real `__cobrust_fang_jwt_encode`, flip ONE byte in the MIDDLE
//!    dot-separated part (the base64url payload), and assert
//!    `__cobrust_fang_jwt_verify` is FALSE (the signature was computed
//!    over the ORIGINAL payload; mutating the payload breaks the MAC). A
//!    naive verifier that decodes the payload without re-checking the MAC
//!    over the received header.payload would wrongly ACCEPT it.
//!
//! 2. **`alg:none` forgery — THE key JWT footgun** — hand-build a token
//!    `base64url({"alg":"none","typ":"JWT"}) . base64url(claims) .` (an
//!    EMPTY signature segment) and assert `__cobrust_fang_jwt_verify` is
//!    FALSE. The CVE-class footgun (e.g. CVE-2015-9235 and the entire
//!    "JWT alg:none" family): a verifier that TRUSTS the header's `alg`
//!    field will, for `alg:none`, skip signature checking entirely and
//!    accept ANY claims an attacker writes. A correct verifier pins the
//!    expected algorithm (HS256) and rejects `none` outright.
//!
//! This file is RED at HEAD `8b1a1fe`: the `fang::cabi::__cobrust_fang_
//! jwt_*` symbols DO NOT EXIST yet (the impl must add them alongside
//! `__cobrust_fang_hash_password` / `__cobrust_fang_verify_password`), so
//! the crate fails to compile (`E0425`/`E0433` unresolved path
//! `fang::cabi::__cobrust_fang_jwt_encode`). See module-bottom note.
//!
//! Harness mirrors `cobrust-fang/src/cabi.rs`'s in-crate `#[cfg(test)]`
//! module: the Cobrust `__cobrust_str_*` ABI is exported by
//! `cobrust-stdlib` (a dev-dependency); the `extern crate` + `#[used]`
//! anchor forces cargo to put its rlib on the test link line so the
//! `extern "C"` decls below resolve under `cargo test`. base64url is
//! implemented inline (no crate dep) so this test does not presuppose the
//! base64 crate the impl ends up choosing.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::undocumented_unsafe_blocks)]
// The Cobrust `Str`-buffer ABI casts (`usize` <-> `i64` lengths) and the
// inline base64url encoder (`u8` -> `u32`) are intrinsic to this test's
// hand-rolled ABI/token scaffolding — mirror the crate-level cast allows
// on the production `cabi.rs` (the casts are correct here: Str lengths
// are non-negative and well under `i64::MAX` on the 64-bit AOT targets).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]

// The Str-buffer ABI is exported by cobrust-stdlib. The `extern crate` +
// `#[used]` static anchor forces cargo to put the rlib on the test link
// line (a bare `extern "C"` decl alone does not create a link edge), so
// the `__cobrust_str_*` symbols the fang shims call resolve under test.
extern crate cobrust_stdlib;
#[used]
static _STDLIB_LINK_ANCHOR: unsafe extern "C" fn() -> *mut u8 =
    cobrust_stdlib::fmt::__cobrust_str_new;

unsafe extern "C" {
    fn __cobrust_str_new() -> *mut u8;
    fn __cobrust_str_push_static(buf: *mut u8, ptr: *const u8, len: i64);
    fn __cobrust_str_ptr(buf: *mut u8) -> *const u8;
    fn __cobrust_str_len(buf: *mut u8) -> i64;
    fn __cobrust_str_drop(buf: *mut u8);
}

/// Allocate a fresh Cobrust `Str` buffer carrying `s`'s bytes (mirrors
/// `cabi.rs::alloc_str_buffer`).
fn alloc_str_buffer(s: &str) -> *mut u8 {
    unsafe {
        let buf = __cobrust_str_new();
        if !s.is_empty() {
            __cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// Read a Cobrust `Str` buffer pointer into an owned `String` (mirrors
/// `cabi.rs::read_str_buf`).
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
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

unsafe fn drop_str(buf: *mut u8) {
    unsafe { __cobrust_str_drop(buf) }
}

/// Standalone base64url-no-pad encoder (RFC 7515 §2 / JWT segment
/// encoding). Inline so this test has NO dependency on whatever base64
/// crate the impl selects — the forgery token is constructed exactly the
/// way a real attacker (or a JWT library) would assemble the segments.
fn b64url_nopad(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().map_or(0u32, u32::from);
        let b2 = chunk.get(2).copied().map_or(0u32, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 0x3f) as usize] as char);
        }
    }
    out
}

/// Convenience: call the real `__cobrust_fang_jwt_verify` shim on `&str`
/// arguments, handling Str-buffer alloc/free.
fn jwt_verify(token: &str, secret: &str) -> bool {
    unsafe {
        let t = alloc_str_buffer(token);
        let s = alloc_str_buffer(secret);
        let ok = fang::cabi::__cobrust_fang_jwt_verify(t, s);
        drop_str(t);
        drop_str(s);
        ok
    }
}

/// Convenience: call the real `__cobrust_fang_jwt_encode` shim, returning
/// the minted token as an owned `String` (buffer freed).
fn jwt_encode(claims_json: &str, secret: &str) -> String {
    unsafe {
        let c = alloc_str_buffer(claims_json);
        let s = alloc_str_buffer(secret);
        let tok_buf = fang::cabi::__cobrust_fang_jwt_encode(c, s);
        let tok = read_str_buf(tok_buf);
        drop_str(c);
        drop_str(s);
        drop_str(tok_buf);
        tok
    }
}

/// SANITY (so the security asserts below are meaningful, not vacuous): a
/// freshly minted token round-trips — `jwt_verify(t, secret)` is TRUE for
/// the right secret. If THIS fails, the tamper/forgery FALSE-asserts
/// would be trivially satisfied by an always-false verifier; this pins
/// that the verifier accepts a legitimate token.
#[test]
fn cabi_jwt_round_trip_verifies_true() {
    let t = jwt_encode("{\"sub\":\"alice\"}", "s3cret");
    assert!(
        t.split('.').count() == 3,
        "a JWT is three dot-separated segments, got {t:?}"
    );
    assert!(
        jwt_verify(&t, "s3cret"),
        "a freshly minted token must verify"
    );
}

/// SECURITY 1 (load-bearing) — payload-segment byte tamper: mint a real
/// token, flip ONE byte in the MIDDLE (payload) segment, assert verify is
/// FALSE. The signature was computed over `header.ORIGINAL_payload`;
/// changing the payload makes the recomputed MAC differ from the
/// transmitted signature, so a correct verifier rejects it. A verifier
/// that decodes the payload without re-checking the MAC over the RECEIVED
/// header.payload would wrongly accept the mutated claims.
#[test]
fn cabi_jwt_payload_segment_tamper_rejects() {
    let t = jwt_encode("{\"sub\":\"alice\"}", "s3cret");
    let parts: Vec<&str> = t.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT must have 3 segments, got {t:?}");

    // Mutate one byte in the MIDDLE (payload) segment to a DIFFERENT
    // base64url character, keeping it a valid base64url char (so the
    // breakage is the MAC mismatch, not a decode failure — exactly the
    // attack a naive verifier is vulnerable to).
    let payload = parts[1];
    assert!(!payload.is_empty(), "payload segment must be non-empty");
    let mut bytes = payload.as_bytes().to_vec();
    // Flip a byte roughly mid-segment. 'A' <-> 'B' are both base64url
    // chars; pick whichever differs from the current byte.
    let idx = bytes.len() / 2;
    bytes[idx] = if bytes[idx] == b'A' { b'B' } else { b'A' };
    let tampered_payload = std::str::from_utf8(&bytes).unwrap();
    let tampered = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);
    assert_ne!(tampered, t, "tamper must actually change the token");

    assert!(
        !jwt_verify(&tampered, "s3cret"),
        "a token with a tampered payload segment must NOT verify \
         (signature was over the original payload)"
    );
}

/// SECURITY 2 (THE key JWT footgun, load-bearing) — `alg:none` forgery:
/// hand-build `b64url({"alg":"none","typ":"JWT"}) . b64url(claims) .`
/// (an EMPTY signature segment) and assert verify is FALSE. This is the
/// canonical JWT vulnerability class ("alg:none"): a verifier that trusts
/// the header's `alg` will, seeing `none`, skip signature verification and
/// accept attacker-chosen claims. A correct verifier PINS the expected
/// algorithm (HS256) and rejects `none` outright — regardless of secret.
#[test]
fn cabi_jwt_alg_none_forgery_rejects() {
    let header = b64url_nopad(br#"{"alg":"none","typ":"JWT"}"#);
    // Attacker-chosen claims — an admin escalation, the whole point of
    // the forgery.
    let payload = b64url_nopad(br#"{"sub":"attacker","admin":true}"#);
    // alg:none tokens carry an EMPTY signature segment (the trailing dot
    // with nothing after it).
    let forged = format!("{header}.{payload}.");

    assert!(
        !jwt_verify(&forged, "s3cret"),
        "an alg:none token MUST be rejected (naive verifiers that trust \
         the header alg accept it — the canonical JWT footgun)"
    );
    // It must ALSO be rejected against any other secret, and against the
    // empty secret (an attacker controls neither, but a sloppy
    // none-handling path might special-case an empty key).
    assert!(
        !jwt_verify(&forged, ""),
        "alg:none must be rejected even against an empty secret"
    );
}

/// SECURITY 3 — `alg:none` with a TRAILING garbage signature is ALSO
/// rejected: some forgery variants append a junk signature segment rather
/// than leaving it empty. A verifier that branches on `alg=="none"` and
/// ignores the signature entirely would accept this too. Pinning the
/// algorithm to HS256 rejects it.
#[test]
fn cabi_jwt_alg_none_with_garbage_sig_rejects() {
    let header = b64url_nopad(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = b64url_nopad(br#"{"sub":"attacker","admin":true}"#);
    let forged = format!("{header}.{payload}.AAAA");
    assert!(
        !jwt_verify(&forged, "s3cret"),
        "alg:none with a junk signature must still be rejected"
    );
}

/// SECURITY 4 — malformed inputs never panic (the C-ABI contract): the
/// not-a-JWT string, the empty string, a two-segment string, and a token
/// with non-base64url bytes in the signature each return a clean FALSE.
/// (The `.cb` E2E covers the not-a-JWT / empty path too; this pins the
/// shim-level no-panic contract for the structural-garbage variants.)
#[test]
fn cabi_jwt_malformed_inputs_are_false_not_panic() {
    assert!(!jwt_verify("not.a.jwt", "s3cret"));
    assert!(!jwt_verify("", "s3cret"));
    assert!(!jwt_verify("only.two", "s3cret"));
    assert!(!jwt_verify("a.b.c.d", "s3cret")); // too many segments
    assert!(!jwt_verify("###.###.###", "s3cret")); // non-base64url
}

// =====================================================================
// RED EXPECTATION at HEAD 8b1a1fe
// ---------------------------------------------------------------------
// `fang::cabi::__cobrust_fang_jwt_encode` / `__cobrust_fang_jwt_verify`
// DO NOT EXIST yet, so `cargo test -p cobrust-fang` fails to COMPILE with
// an unresolved-path error (E0425 / E0433) on the
// `fang::cabi::__cobrust_fang_jwt_*` call sites above. Once the impl adds
// the three shims to `cobrust-fang/src/cabi.rs` (twins of the
// hash/verify shims), this file compiles and these asserts gate the
// security behavior.
// =====================================================================
