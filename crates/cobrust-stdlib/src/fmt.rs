//! `std.fmt` — f-string runtime helpers.
//!
//! ADR-0025 §"Public surface (binding)" pins the API. ADR-0019
//! §"M11 — Standard library" §"Modules" describes:
//!
//! > `std.fmt` | f-string runtime helpers (already lowered in
//! > HIR; this is the runtime side)
//!
//! HIR-tier f-string lowering decomposes `f"{x}"` into a sequence
//! of static text + value-formatting calls. The runtime helpers
//! here are what those calls land on.

// =====================================================================
// Per-type formatters
// =====================================================================

/// Format an integer as a decimal string. Cobrust's `f"{i}"`
/// lowers to a call here.
pub fn format_int(i: i64) -> String {
    i.to_string()
}

/// Format a float as a string. Uses the `FormatArg::Float`
/// strategy: integer-valued floats display with `.0`; non-integer
/// values use the shortest round-trip repr.
pub fn format_float(x: f64) -> String {
    if x.fract() == 0.0 && x.is_finite() {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

/// Format a bool as `True` / `False` (matches Python's repr).
pub fn format_bool(b: bool) -> String {
    if b { "True".into() } else { "False".into() }
}

/// Identity. Provided for completeness in the f-string codegen.
pub fn format_str(s: &str) -> String {
    s.to_string()
}

// =====================================================================
// C-ABI runtime helpers (ADR-0027 §5: HIR-tier f-string lowering)
// =====================================================================
//
// HIR `Expr::FString { parts }` lowers to MIR via the table below:
//
// 1. Allocate an empty `String` at start.
// 2. For each Static(s):  call __cobrust_str_push_static(buf, s, len)
// 3. For each Interp(e), depending on type:
//    - i32/i64 → __cobrust_fmt_int(buf, v)
//    - f32/f64 → __cobrust_fmt_float(buf, v)
//    - bool   → __cobrust_fmt_bool(buf, v)
//    - str    → __cobrust_fmt_str(buf, v_ptr, v_len)
//    - List/Dict/Set → __cobrust_fmt_repr(buf, v_ptr, type_id)
// 4. Drop schedule registers buf for __cobrust_str_drop at scope end.
//
// The runtime String is a heap-allocated `Vec<u8>` boxed into a
// fixed-shape opaque pointer the codegen passes around.

#[repr(C)]
struct StringBuffer {
    /// Heap-allocated UTF-8 byte buffer.
    bytes: Vec<u8>,
}

/// Allocate a fresh empty string buffer for f-string composition.
///
/// # Safety
///
/// Caller must eventually pass result to [`__cobrust_str_drop`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_new() -> *mut u8 {
    let buf = Box::new(StringBuffer { bytes: Vec::new() });
    Box::into_raw(buf).cast::<u8>()
}

/// Push a static `(*const u8, usize)` payload onto the buffer.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`] and
/// not yet dropped. `ptr`/`len` must describe a valid UTF-8 byte
/// slice; codegen always emits this from `.rodata`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_push_static(buf: *mut u8, ptr: *const u8, len: i64) {
    if buf.is_null() || ptr.is_null() || len <= 0 {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    // SAFETY: caller-attestation per `# Safety`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    b.bytes.extend_from_slice(bytes);
}

/// Append the decimal representation of an i64 to the buffer.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fmt_int(buf: *mut u8, v: i64) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    let s = format_int(v);
    b.bytes.extend_from_slice(s.as_bytes());
}

/// Append the f64 repr to the buffer.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fmt_float(buf: *mut u8, v: f64) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    let s = format_float(v);
    b.bytes.extend_from_slice(s.as_bytes());
}

/// Append a float with a fixed-precision format spec (M-F.3.3 gap c).
///
/// `spec_ptr` / `spec_len` describe a UTF-8 format spec string such as
/// `.2f`, `e`, or `g`. A leading `.` followed by digits and `f` means
/// fixed-decimal with that many places; `e` means scientific notation;
/// `g` means shortest-repr (default). Other values fall back to default.
///
/// # Safety
///
/// - `buf` must be a pointer returned by [`__cobrust_str_new`].
/// - `spec_ptr`/`spec_len` must describe a valid UTF-8 slice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fmt_float_prec(
    buf: *mut u8,
    v: f64,
    spec_ptr: *const u8,
    spec_len: i64,
) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    let s = if spec_ptr.is_null() || spec_len <= 0 {
        format_float(v)
    } else {
        // SAFETY: caller-attestation.
        let spec_bytes = unsafe { std::slice::from_raw_parts(spec_ptr, spec_len as usize) };
        let spec = std::str::from_utf8(spec_bytes).unwrap_or("");
        format_float_with_spec(v, spec)
    };
    b.bytes.extend_from_slice(s.as_bytes());
}

/// Format a float using a Python-style format spec string (`.Nf`, `e`, `g`).
pub fn format_float_with_spec(x: f64, spec: &str) -> String {
    // Strip leading `.` if present; check for form `.Nf`, `.Ne`, `e`, `g`.
    let spec = spec.trim_start_matches('.');
    if let Some(rest) = spec.strip_suffix('f') {
        // Fixed-point: e.g. "2f" from ".2f".
        if rest.is_empty() {
            // `.f` without precision — default to 6 decimal places.
            return format!("{x:.6}");
        }
        if let Ok(prec) = rest.parse::<usize>() {
            return format!("{x:.prec$}");
        }
    } else if spec.ends_with('e') || spec == "e" {
        // Scientific notation.
        return format!("{x:e}");
    } else if spec.ends_with('g') || spec == "g" {
        // General / shortest repr.
        return format_float(x);
    }
    // Fallback: default float repr.
    format_float(x)
}

/// Append `True`/`False`.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fmt_bool(buf: *mut u8, v: i64) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    let s = if v != 0 { "True" } else { "False" };
    b.bytes.extend_from_slice(s.as_bytes());
}

/// Append a runtime str (`(*const u8, usize)`) to the buffer.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`].
/// `ptr`/`len` must describe a valid UTF-8 slice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fmt_str(buf: *mut u8, ptr: *const u8, len: i64) {
    if buf.is_null() || ptr.is_null() || len <= 0 {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    // SAFETY: caller-attestation per `# Safety`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    b.bytes.extend_from_slice(bytes);
}

/// Append a debug repr placeholder for List/Dict/Set. M12.x emits
/// the literal `"<{type_id}>"` token; Phase F widens to real repr.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_fmt_repr(buf: *mut u8, _ptr: *mut u8, type_id: i64) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &mut *buf.cast::<StringBuffer>() };
    let s = format!("<{type_id}>");
    b.bytes.extend_from_slice(s.as_bytes());
}

/// Read the buffer's UTF-8 length without consuming it.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`] and
/// not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_len(buf: *mut u8) -> i64 {
    if buf.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &*buf.cast::<StringBuffer>() };
    b.bytes.len() as i64
}

/// Read the buffer's bytes pointer without consuming it. Returns
/// `null` for empty strings.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`] and
/// not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_ptr(buf: *mut u8) -> *const u8 {
    if buf.is_null() {
        return std::ptr::null();
    }
    // SAFETY: caller-attestation per `# Safety`.
    let b = unsafe { &*buf.cast::<StringBuffer>() };
    if b.bytes.is_empty() {
        std::ptr::null()
    } else {
        b.bytes.as_ptr()
    }
}

/// Free a string buffer.
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`] and
/// not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_drop(buf: *mut u8) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let _ = unsafe { Box::from_raw(buf.cast::<StringBuffer>()) };
}

/// Deep-copy a string buffer. Allocates a fresh `StringBuffer`, copies
/// the source bytes, returns the new pointer. NULL → NULL.
///
/// ADR-0050c §"Phase 3": explicit clone for the shared-ownership
/// escape hatch. Phase 4 emits this at MIR operand-lowering when a
/// Str-typed local would otherwise be read twice (`let a = s; let b
/// = s` source-level pattern).
///
/// # Safety
///
/// `buf` must be a pointer returned by [`__cobrust_str_new`] and not
/// yet dropped, OR `buf` may be NULL. Returned pointer must be passed
/// to [`__cobrust_str_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_clone(buf: *mut u8) -> *mut u8 {
    if buf.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller-attestation per `# Safety`.
    let src = unsafe { &*buf.cast::<StringBuffer>() };
    let copy = Box::new(StringBuffer {
        bytes: src.bytes.clone(),
    });
    Box::into_raw(copy).cast::<u8>()
}

/// Concatenate two string buffers into a fresh `StringBuffer` carrying
/// `a`'s bytes followed by `b`'s bytes. NULL operands are treated as the
/// empty string. The inputs are BORROWED (read-only) — the caller's drop
/// schedule still frees them. The returned pointer is a freshly-allocated
/// buffer the caller owns and must free exactly once via
/// [`__cobrust_str_drop`].
///
/// The runtime target of the `.cb` `str + str` operator (the natural
/// concatenation form, sibling of `__cobrust_str_eq` for `str == str`).
///
/// # Safety
///
/// `a` and `b` must each be NULL or a pointer returned by
/// [`__cobrust_str_new`] and not yet dropped. The returned pointer must be
/// passed to [`__cobrust_str_drop`] exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_concat(a: *mut u8, b: *mut u8) -> *mut u8 {
    let mut bytes: Vec<u8> = Vec::new();
    if !a.is_null() {
        // SAFETY: caller-attestation — `a` is a valid StringBuffer.
        let sa = unsafe { &*a.cast::<StringBuffer>() };
        bytes.extend_from_slice(&sa.bytes);
    }
    if !b.is_null() {
        // SAFETY: caller-attestation — `b` is a valid StringBuffer.
        let sb = unsafe { &*b.cast::<StringBuffer>() };
        bytes.extend_from_slice(&sb.bytes);
    }
    let out = Box::new(StringBuffer { bytes });
    Box::into_raw(out).cast::<u8>()
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::format_push_string,
    clippy::let_unit_value,
    clippy::ignored_unit_patterns,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::manual_is_multiple_of,
    clippy::redundant_closure_for_method_calls,
    clippy::match_same_arms,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn format_int_basic() {
        assert_eq!(format_int(0), "0");
        assert_eq!(format_int(42), "42");
        assert_eq!(format_int(-7), "-7");
    }

    #[test]
    fn format_int_max() {
        assert_eq!(format_int(i64::MAX), "9223372036854775807");
    }

    #[test]
    fn format_int_min() {
        assert_eq!(format_int(i64::MIN), "-9223372036854775808");
    }

    #[test]
    fn format_float_integer_value() {
        assert_eq!(format_float(3.0), "3.0");
        assert_eq!(format_float(-7.0), "-7.0");
    }

    #[test]
    fn format_float_fractional() {
        let s = format_float(3.14);
        assert!(s.starts_with("3.14"));
    }

    #[test]
    fn format_float_zero() {
        assert_eq!(format_float(0.0), "0.0");
    }

    #[test]
    fn format_float_neg_zero() {
        // -0.0.fract() == -0.0, but we render with "{:.1}" so
        // representation may be "-0.0". Implementation-defined;
        // accept either.
        let s = format_float(-0.0);
        assert!(s == "-0.0" || s == "0.0");
    }

    #[test]
    fn format_float_nan_or_inf_does_not_panic() {
        let _s1 = format_float(f64::NAN);
        let _s2 = format_float(f64::INFINITY);
        let _s3 = format_float(f64::NEG_INFINITY);
    }

    #[test]
    fn format_bool_true() {
        assert_eq!(format_bool(true), "True");
    }

    #[test]
    fn format_bool_false() {
        assert_eq!(format_bool(false), "False");
    }

    #[test]
    fn format_str_identity() {
        assert_eq!(format_str("hi"), "hi");
        assert_eq!(format_str(""), "");
    }

    // -- C-ABI runtime helpers (M12.x ADR-0027 §5) -----------------

    #[test]
    fn cabi_str_new_drop_smoke() {
        // SAFETY: documented contract.
        let buf = unsafe { __cobrust_str_new() };
        assert!(!buf.is_null());
        // SAFETY: contract.
        unsafe { __cobrust_str_drop(buf) };
    }

    #[test]
    fn cabi_str_push_static_then_read() {
        // SAFETY: documented contract.
        unsafe {
            let buf = __cobrust_str_new();
            let bytes = b"hello";
            __cobrust_str_push_static(buf, bytes.as_ptr(), bytes.len() as i64);
            assert_eq!(__cobrust_str_len(buf), 5);
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_fmt_int_appends_decimal() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            __cobrust_fmt_int(buf, 42);
            assert_eq!(__cobrust_str_len(buf), 2);
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_fmt_float_appends_repr() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            __cobrust_fmt_float(buf, 3.14);
            assert!(__cobrust_str_len(buf) > 0);
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_fmt_bool_true() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            __cobrust_fmt_bool(buf, 1);
            assert_eq!(__cobrust_str_len(buf), 4);
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_fmt_bool_false() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            __cobrust_fmt_bool(buf, 0);
            assert_eq!(__cobrust_str_len(buf), 5);
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_fmt_str_appends() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            let bytes = b"world";
            __cobrust_fmt_str(buf, bytes.as_ptr(), bytes.len() as i64);
            assert_eq!(__cobrust_str_len(buf), 5);
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_fmt_repr_format_placeholder() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            __cobrust_fmt_repr(buf, std::ptr::null_mut(), 7);
            assert_eq!(__cobrust_str_len(buf), 3); // "<7>"
            __cobrust_str_drop(buf);
        }
    }

    #[test]
    fn cabi_str_handles_null_safely() {
        // SAFETY: documented null-arg path on every helper.
        unsafe {
            __cobrust_str_drop(std::ptr::null_mut());
            __cobrust_str_push_static(std::ptr::null_mut(), b"x".as_ptr(), 1);
            assert_eq!(__cobrust_str_len(std::ptr::null_mut()), 0);
            __cobrust_fmt_int(std::ptr::null_mut(), 1);
            __cobrust_fmt_float(std::ptr::null_mut(), 1.0);
            __cobrust_fmt_bool(std::ptr::null_mut(), 1);
            __cobrust_fmt_str(std::ptr::null_mut(), b"x".as_ptr(), 1);
            __cobrust_fmt_repr(std::ptr::null_mut(), std::ptr::null_mut(), 1);
        }
    }

    #[test]
    fn cabi_str_compose_full_fstring() {
        // SAFETY: contract.
        unsafe {
            let buf = __cobrust_str_new();
            let prefix = b"x = ";
            __cobrust_str_push_static(buf, prefix.as_ptr(), prefix.len() as i64);
            __cobrust_fmt_int(buf, 42);
            // Expect "x = 42" -> 6 bytes.
            assert_eq!(__cobrust_str_len(buf), 6);
            __cobrust_str_drop(buf);
        }
    }
}
