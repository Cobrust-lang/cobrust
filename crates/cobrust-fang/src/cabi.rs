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
//! - The JWT surface (`jwt_encode` / `jwt_verify` / `jwt_decode`) **PINS
//!   the algorithm to HS256** ([`hs256_validation`]). The token's own
//!   `alg` header is NEVER trusted, so an `alg:none` / alg-swapped (e.g.
//!   RS256-header) forgery is REJECTED — the classic JWT algorithm-
//!   confusion footgun (CVE-2015-9235 family) is closed by construction.
//!   A tampered / wrong-secret / malformed / `alg:none` token is a clean
//!   `false` (verify) or the empty-string sentinel (decode) — NEVER a
//!   panic, NEVER an accept. No secret / claim / token is ever logged.
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
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
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

// =====================================================================
// fang JWT surface — HS256-signed JSON Web Tokens.
// =====================================================================

/// Build the HS256-PINNED [`Validation`] used by `jwt_verify` /
/// `jwt_decode` — **the single security-critical knob on this surface**.
///
/// # Algorithm pinning (the load-bearing security property)
///
/// `Validation::new(Algorithm::HS256)` sets `algorithms = [HS256]`. The
/// `decode` path checks the token header's `alg` against this list AND
/// selects the verifier by the EXPECTED algorithm — the token's own
/// `alg` header is NEVER trusted to choose the verification path. Hence:
///
/// - an **`alg:none`** token (header `{"alg":"none"}`, empty signature)
///   is REJECTED — `none ∉ [HS256]`, and there is no
///   `insecure_disable_signature_validation` here;
/// - an **alg-swapped** token (e.g. an RS256 header) is REJECTED —
///   `RS256 ∉ [HS256]`, closing the RSA-pubkey-as-HMAC-secret confusion;
/// - a **tampered payload / wrong secret** fails the HMAC check.
///
/// This is the canonical JWT algorithm-confusion footgun (CVE-2015-9235
/// and the whole "JWT alg:none" family), closed by construction.
///
/// # What is intentionally relaxed (and why it is SAFE)
///
/// The crate default `Validation` additionally REQUIRES an `exp` claim
/// and validates expiry / audience. The `fang` surface makes NO claim-
/// schema demand on `.cb` authors (claims are an arbitrary JSON object),
/// so a bare `{"sub":"alice"}` token MUST round-trip. We therefore clear
/// `required_spec_claims` and disable `validate_exp` / `validate_aud`.
/// The signature gate stays on and `algorithms` stays `[HS256]` — the
/// relaxations touch ONLY claim-policy, never the signature/alg gate. (A
/// future `fang` surface may add an opt-in `exp` policy; the secure
/// default here is "the signature is always checked, pinned to HS256".)
fn hs256_validation() -> Validation {
    let mut validation = Validation::new(Algorithm::HS256);
    // No mandatory claims — `.cb` authors pass arbitrary claim objects.
    validation.required_spec_claims.clear();
    // Expiry / audience are application policy, not a `fang` mandate.
    validation.validate_exp = false;
    validation.validate_aud = false;
    // Defensive: the algorithm pin is the security core and must stay
    // exactly `[HS256]`. (There is NO public API on this surface to
    // disable signature verification, which is precisely why it is safe:
    // a forged `alg:none` token cannot route around the HMAC check.)
    debug_assert_eq!(
        validation.algorithms,
        vec![Algorithm::HS256],
        "algorithm must stay pinned to HS256 (no alg-confusion)"
    );
    validation
}

/// `fang.jwt_encode(claims_json, secret) -> str`. Mints an **HS256**
/// JSON Web Token whose payload is the JSON object parsed from
/// `claims_json`, signed with `secret`, and returns the compact
/// `header.payload.signature` token as a freshly-allocated Cobrust `Str`
/// buffer.
///
/// The header algorithm is fixed to HS256 ([`Header::new`]) — no
/// algorithm knob is exposed, so a `.cb` author cannot mint an
/// `alg:none` or otherwise-weak token by accident.
///
/// On a **malformed** `claims_json` (not valid JSON), or on the
/// (effectively impossible) internal signing error, the returned buffer
/// carries the **empty-string sentinel** — matching `hash_password`'s
/// fail-clean convention. NO panic, NO null across the boundary, NO
/// secret / claim logging.
///
/// # Safety
///
/// `claims_json` and `secret` must each be null or a valid Cobrust `Str`
/// buffer. The returned pointer is an owned Cobrust `Str` buffer, freed
/// once by `__cobrust_str_drop` at the `.cb` scope exit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fang_jwt_encode(
    claims_json: *mut u8,
    secret: *mut u8,
) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let claims_src = unsafe { read_str_buf(claims_json) };
    // SAFETY: caller-attestation per `# Safety`.
    let secret_bytes = unsafe { read_str_buf(secret) };
    // Parse the claims to an arbitrary JSON value; malformed => sentinel.
    let Ok(claims) = serde_json::from_str::<serde_json::Value>(&claims_src) else {
        return alloc_str_buffer("");
    };
    // HS256 header (algorithm fixed); HMAC key from the raw secret bytes.
    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(secret_bytes.as_bytes());
    match encode(&header, &claims, &key) {
        Ok(token) => alloc_str_buffer(&token),
        // Fail clean — no panic, no secret/claim in the (absent) log.
        Err(_) => alloc_str_buffer(""),
    }
}

/// `fang.jwt_verify(token, secret) -> bool`. Returns `true` iff `token`
/// is a well-formed **HS256** JWT whose signature validates against
/// `secret` (algorithm PINNED to HS256 via [`hs256_validation`]).
///
/// Returns `false` — never a panic — for ANY of: a tampered payload, the
/// wrong secret, a malformed / empty / wrong-segment-count token, or an
/// **`alg:none` / alg-swapped forgery** (the token's `alg` header is not
/// trusted; only HS256 is accepted). A failed verification is normal
/// control flow (CLAUDE.md §2.2), not an exceptional condition. No
/// secret / token is ever logged.
///
/// # Safety
///
/// `token` and `secret` must each be null or a valid Cobrust `Str`
/// buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fang_jwt_verify(token: *mut u8, secret: *mut u8) -> bool {
    // SAFETY: caller-attestation per `# Safety`.
    let token_str = unsafe { read_str_buf(token) };
    // SAFETY: caller-attestation per `# Safety`.
    let secret_bytes = unsafe { read_str_buf(secret) };
    let key = DecodingKey::from_secret(secret_bytes.as_bytes());
    // Decode into an arbitrary JSON object; `Ok` => signature valid AND
    // algorithm == HS256, any `Err` (bad sig, alg:none, malformed, …) =>
    // false. NEVER unwrap — a malformed token is `Err`, not a panic.
    decode::<serde_json::Value>(&token_str, &key, &hs256_validation()).is_ok()
}

/// `fang.jwt_decode(token, secret) -> str`. Verifies `token` exactly as
/// [`__cobrust_fang_jwt_verify`] does (HS256 signature, algorithm pinned)
/// and, on success, returns the claims re-serialized to a JSON object
/// string as a freshly-allocated Cobrust `Str` buffer.
///
/// On a token that does NOT verify (tampered / wrong-secret / malformed /
/// `alg:none`), returns the **empty-string sentinel** — mirroring
/// `hash_password`'s fail-clean convention. A decode therefore NEVER
/// surfaces unverified claims: an attacker-supplied unsigned token yields
/// the empty string, not its forged payload. NO panic, NO secret / token
/// logging.
///
/// Note: the returned JSON is re-serialized from the parsed claims, so
/// key order / whitespace may differ from the originally-encoded payload
/// (a JWT payload carries no canonical text form).
///
/// # Safety
///
/// `token` and `secret` must each be null or a valid Cobrust `Str`
/// buffer. The returned pointer is an owned Cobrust `Str` buffer, freed
/// once by `__cobrust_str_drop` at the `.cb` scope exit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fang_jwt_decode(token: *mut u8, secret: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let token_str = unsafe { read_str_buf(token) };
    // SAFETY: caller-attestation per `# Safety`.
    let secret_bytes = unsafe { read_str_buf(secret) };
    let key = DecodingKey::from_secret(secret_bytes.as_bytes());
    // Same pinned-HS256 decode; only a VALID token's claims are surfaced.
    match decode::<serde_json::Value>(&token_str, &key, &hs256_validation()) {
        // Re-serialize the verified claims object. The serializer cannot
        // fail for a value that just deserialized from JSON, but stay
        // fail-clean anyway (empty sentinel, never a panic).
        Ok(data) => match serde_json::to_string(&data.claims) {
            Ok(json) => alloc_str_buffer(&json),
            Err(_) => alloc_str_buffer(""),
        },
        // Invalid token => empty sentinel (no unverified claims leak out).
        Err(_) => alloc_str_buffer(""),
    }
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
