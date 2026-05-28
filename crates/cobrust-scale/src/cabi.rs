//! C-ABI shims — the runtime surface a compiled `.cb` program binds
//! onto when it does `import scale` and calls `scale.dumps_str(json)` /
//! `scale.loads_str(packed)` (ADR-0072 fourth-module generalization —
//! msgpack, rebrand of `msgpack-python`).
//!
//! # The chain
//!
//! These `#[no_mangle] extern "C"` shims are the L4 (runtime) layer of
//! the ecosystem-import chain. The Cobrust toolchain (L1 typecheck → L2
//! MIR intrinsic-rewrite → L3 codegen externs) retargets the source
//! calls onto these exact symbols; `cobrust build` static-links the
//! resulting `libscale.a` after `libcobrust_stdlib.a` (Linux wraps both
//! in `--start-group/--end-group` so the `__cobrust_str_*` forward
//! references below resolve under single-pass GNU ld too — per
//! ADR-0072 Q5).
//!
//! # ABI — value pattern (str→str), no handles
//!
//! - **Strings** cross as Cobrust `Str` buffers. Per ADR-0072 Q5 this
//!   crate has **no Rust-level dependency on `cobrust-stdlib`** in the
//!   production build; the `__cobrust_str_*` primitives are declared
//!   `extern "C"` here and resolved from the always-linked
//!   `libcobrust_stdlib.a`. (For in-crate unit tests, `cobrust-stdlib`
//!   is a dev-dependency so the `extern "C"` decls resolve under
//!   `cargo test`.)
//!
//! # First-proof shape
//!
//! The first proof keeps the surface **pure value-in-value-out**
//! (mirrors `nest`, third-module-proof value pattern):
//! - `dumps_str(json_input)` parses `json_input` as JSON, encodes the
//!   value tree to msgpack bytes via the native `pack_to_vec`, then
//!   renders the bytes as lowercase HEX in a Cobrust `Str` (printable
//!   on stdout, round-trippable). A raw `*mut u8` bytes ABI is a
//!   tracked follow-up — keeping the first proof in the chain's
//!   already-proven str→str shape lets us verify the chain generalizes
//!   to a FOURTH module without first inventing a new bytes ABI surface.
//! - `loads_str(packed_hex)` decodes the HEX into msgpack bytes,
//!   `unpack`s the value tree, then renders the value back to canonical
//!   JSON via `MsgValue::to_json` + `serde_json::to_string`.
//!
//! On any error (invalid JSON, invalid HEX, malformed msgpack) the
//! returned Cobrust `Str` carries the empty-string sentinel — matching
//! the std.json / F59 fail-clean convention. NO panic, NO null across
//! the C-ABI boundary.
//!
//! # Ownership
//!
//! Pure value-in-value-out (`Str → Str`): the input Str is BORROWED
//! (read into an owned Rust `String` then released — the `.cb` caller's
//! drop schedule frees the input buffer at scope exit). The returned
//! `*mut u8` Str buffer is owned by the caller and freed exactly once
//! by the existing `__cobrust_str_drop` at scope exit. NO handles, NO
//! callbacks — the chain handles this case natively (ADR-0072 §5 risk 1
//! "scope-local handle drop" is a non-concern here because there is no
//! handle).

// C-ABI-boundary cast allows — mirror `cobrust-nest/src/cabi.rs`'s
// crate-level allows (the casts are intrinsic to the opaque-pointer /
// length ABI and are correct here):
// - `i64 <-> usize` length round-trips: Cobrust `Str` lengths are
//   non-negative and well under `usize::MAX` on the 64-bit targets the
//   AOT backend supports.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]

use crate::parser::{MsgValue, pack_to_vec, unpack};

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
/// null / empty. Mirrors `cobrust-nest/src/cabi.rs::read_str_buf`.
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
// JSON ↔ MsgValue conversion (first-proof str→str round trip).
// =====================================================================

/// Convert a `serde_json::Value` to a `MsgValue`. The mapping is the
/// natural one for the M6 scope surface: null/bool/number/string/array/
/// object → Nil/Bool/Int|UInt|Float/Str/Array/Map. Integer JSON values
/// route to `MsgValue::Int` (negative) or `MsgValue::UInt` (non-neg-i64
/// overflow). Floats route to `MsgValue::Float`.
fn json_to_msgvalue(v: &serde_json::Value) -> MsgValue {
    match v {
        serde_json::Value::Null => MsgValue::Nil,
        serde_json::Value::Bool(b) => MsgValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                MsgValue::Int(i)
            } else if let Some(u) = n.as_u64() {
                MsgValue::UInt(u)
            } else if let Some(f) = n.as_f64() {
                MsgValue::Float(f)
            } else {
                // Unrepresentable number — fall back to Nil (sentinel).
                MsgValue::Nil
            }
        }
        serde_json::Value::String(s) => MsgValue::Str(s.clone()),
        serde_json::Value::Array(items) => {
            MsgValue::Array(items.iter().map(json_to_msgvalue).collect())
        }
        serde_json::Value::Object(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (k, v) in items {
                out.push((k.clone(), json_to_msgvalue(v)));
            }
            MsgValue::Map(out)
        }
    }
}

/// Encode a byte slice as lowercase hex. Inline to keep the first proof
/// dependency-free (no `hex` / `base16ct` etc.). Mirrors the canonical
/// lowercase shape `hex::encode` produces so the round-trip with
/// `decode_hex` below is exact.
fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(b & 0x0f), 16).unwrap_or('0'));
    }
    out
}

/// Decode a lowercase / uppercase hex string into bytes. Returns `None`
/// on any non-hex character or odd length. The fail-clean caller maps
/// `None` to the empty-Str sentinel — no panic.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

// =====================================================================
// scale C-ABI surface.
// =====================================================================

/// `scale.dumps_str(json_input) -> str`. Parses `json_input` as JSON,
/// msgpack-encodes the value tree, and returns a freshly-allocated
/// Cobrust `Str` buffer carrying the lowercase HEX rendering of the
/// msgpack bytes. The HEX rendering keeps the surface str→str (a raw
/// bytes ABI is a tracked follow-up).
///
/// Returns the empty Str on any error (malformed JSON / pack failure) —
/// the std.json / F59 fail-clean convention. NO panic, NO null.
///
/// # Safety
///
/// `json_input` must be null or a valid Cobrust `Str` buffer. The
/// returned pointer is an owned Cobrust `Str` buffer, freed once by the
/// existing `__cobrust_str_drop` at the `.cb` scope exit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_scale_dumps_str(json_input: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { read_str_buf(json_input) };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&src) else {
        return alloc_str_buffer("");
    };
    let msg = json_to_msgvalue(&value);
    let Ok(bytes) = pack_to_vec(&msg) else {
        return alloc_str_buffer("");
    };
    alloc_str_buffer(&encode_hex(&bytes))
}

/// `scale.loads_str(packed) -> str`. Decodes the HEX-rendered msgpack
/// bytes in `packed`, unpacks the value tree, and re-renders it as
/// canonical (compact) JSON via `MsgValue::to_json + serde_json::to_string`.
///
/// Returns the empty Str on any error (malformed HEX / unpack failure /
/// non-utf8 string field) — same fail-clean convention as `dumps_str`.
///
/// # Safety
///
/// `packed` must be null or a valid Cobrust `Str` buffer. The returned
/// pointer is an owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_scale_loads_str(packed: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety`.
    let hex = unsafe { read_str_buf(packed) };
    let Some(bytes) = decode_hex(&hex) else {
        return alloc_str_buffer("");
    };
    let Ok(value) = unpack(&bytes) else {
        return alloc_str_buffer("");
    };
    let json = value.to_json();
    let Ok(rendered) = serde_json::to_string(&json) else {
        return alloc_str_buffer("");
    };
    alloc_str_buffer(&rendered)
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

    /// End-to-end through the cabi shims, exactly as a compiled `.cb`
    /// program would call them — proving JSON → msgpack hex → JSON
    /// round-trips bit-faithfully.
    #[test]
    fn cabi_round_trip_dumps_then_loads_simple_object() {
        unsafe {
            let input = alloc_str_buffer(r#"{"key":"value"}"#);
            let packed = __cobrust_scale_dumps_str(input);
            let hex_rendered = read_str_buf(packed);
            // The packed hex must be non-empty (a proper msgpack
            // encoding of the single-key string-value map). Then
            // re-render through loads_str and assert it matches the
            // canonical JSON.
            assert!(!hex_rendered.is_empty(), "dumps_str returned empty Str");
            let back = __cobrust_scale_loads_str(packed);
            let rendered = read_str_buf(back);
            assert_eq!(rendered, r#"{"key":"value"}"#);
            drop_str_for_test(input);
            drop_str_for_test(packed);
            drop_str_for_test(back);
        }
    }

    #[test]
    fn cabi_round_trip_dumps_then_loads_nested_array() {
        unsafe {
            let input = alloc_str_buffer(r#"{"items":[1,2,3],"name":"x"}"#);
            let packed = __cobrust_scale_dumps_str(input);
            assert!(!read_str_buf(packed).is_empty());
            let back = __cobrust_scale_loads_str(packed);
            let rendered = read_str_buf(back);
            assert_eq!(rendered, r#"{"items":[1,2,3],"name":"x"}"#);
            drop_str_for_test(input);
            drop_str_for_test(packed);
            drop_str_for_test(back);
        }
    }

    #[test]
    fn cabi_dumps_str_with_invalid_json_yields_empty_sentinel() {
        unsafe {
            // Unterminated JSON object — serde_json::from_str fails.
            let input = alloc_str_buffer(r#"{"key":"#);
            let packed = __cobrust_scale_dumps_str(input);
            assert_eq!(
                read_str_buf(packed),
                "",
                "invalid JSON must yield empty-Str sentinel"
            );
            drop_str_for_test(input);
            drop_str_for_test(packed);
        }
    }

    #[test]
    fn cabi_loads_str_with_invalid_hex_yields_empty_sentinel() {
        unsafe {
            // "xyz" — non-hex characters; decode_hex returns None.
            let input = alloc_str_buffer("xyz");
            let back = __cobrust_scale_loads_str(input);
            assert_eq!(
                read_str_buf(back),
                "",
                "invalid hex must yield empty-Str sentinel"
            );
            drop_str_for_test(input);
            drop_str_for_test(back);
        }
    }

    #[test]
    fn cabi_null_input_yields_empty_sentinel() {
        // Null / empty JSON parses to a value-less serde error → empty
        // sentinel. Null / empty hex decodes to a zero-byte msgpack
        // stream → unpack error → empty sentinel.
        unsafe {
            let dumps = __cobrust_scale_dumps_str(std::ptr::null_mut());
            assert_eq!(read_str_buf(dumps), "");
            drop_str_for_test(dumps);
            let loads = __cobrust_scale_loads_str(std::ptr::null_mut());
            assert_eq!(read_str_buf(loads), "");
            drop_str_for_test(loads);
        }
    }

    #[test]
    fn json_to_msgvalue_handles_all_atoms() {
        assert_eq!(json_to_msgvalue(&serde_json::Value::Null), MsgValue::Nil);
        assert_eq!(
            json_to_msgvalue(&serde_json::Value::Bool(true)),
            MsgValue::Bool(true)
        );
        assert_eq!(
            json_to_msgvalue(&serde_json::json!(-7i64)),
            MsgValue::Int(-7)
        );
        // Non-negative numbers route to Int (serde_json prefers i64 when
        // it fits — Int(7) is canonical).
        assert_eq!(json_to_msgvalue(&serde_json::json!(7i64)), MsgValue::Int(7));
        assert_eq!(
            json_to_msgvalue(&serde_json::json!(1.5)),
            MsgValue::Float(1.5)
        );
        assert_eq!(
            json_to_msgvalue(&serde_json::Value::String("hi".into())),
            MsgValue::Str("hi".into())
        );
    }

    #[test]
    fn encode_then_decode_hex_round_trips() {
        let bytes = vec![0x00, 0x7f, 0x80, 0xff, 0x12, 0x34];
        let hex = encode_hex(&bytes);
        assert_eq!(hex, "007f80ff1234");
        assert_eq!(decode_hex(&hex), Some(bytes));
        // Odd length rejected.
        assert_eq!(decode_hex("abc"), None);
        // Non-hex chars rejected.
        assert_eq!(decode_hex("zz"), None);
    }
}
