//! M12.x f-string corpus (per ADR-0027 §5).
//!
//! Each test exercises one path of the f-string runtime helper
//! suite: `__cobrust_str_new`, `__cobrust_str_push_static`,
//! `__cobrust_fmt_int|float|bool|str|repr`, `__cobrust_str_drop`.
//!
//! These are unit-level tests of the runtime ABI; the codegen side
//! (HIR → MIR → Cranelift) is gated by `aggregate_corpus` whose
//! `agg_*_format_string` would land in a Phase F end-to-end suite.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
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

use cobrust_stdlib::fmt::{
    __cobrust_fmt_bool, __cobrust_fmt_float, __cobrust_fmt_int, __cobrust_fmt_repr,
    __cobrust_fmt_str, __cobrust_str_drop, __cobrust_str_len, __cobrust_str_new, __cobrust_str_ptr,
    __cobrust_str_push_static, format_bool, format_float, format_int, format_str,
};

// =====================================================================
// Per-formatter pure helpers
// =====================================================================

#[test]
fn fstr_format_int_zero() {
    assert_eq!(format_int(0), "0");
}

#[test]
fn fstr_format_int_positive() {
    assert_eq!(format_int(42), "42");
}

#[test]
fn fstr_format_int_negative() {
    assert_eq!(format_int(-7), "-7");
}

#[test]
fn fstr_format_int_max() {
    assert_eq!(format_int(i64::MAX), "9223372036854775807");
}

#[test]
fn fstr_format_int_min() {
    assert_eq!(format_int(i64::MIN), "-9223372036854775808");
}

#[test]
fn fstr_format_float_integer() {
    assert_eq!(format_float(3.0), "3.0");
}

#[test]
fn fstr_format_float_fraction() {
    assert!(format_float(3.14).starts_with("3.14"));
}

#[test]
fn fstr_format_float_zero() {
    assert_eq!(format_float(0.0), "0.0");
}

#[test]
fn fstr_format_float_neg() {
    assert_eq!(format_float(-1.0), "-1.0");
}

#[test]
fn fstr_format_bool_true() {
    assert_eq!(format_bool(true), "True");
}

#[test]
fn fstr_format_bool_false() {
    assert_eq!(format_bool(false), "False");
}

#[test]
fn fstr_format_str_identity() {
    assert_eq!(format_str("hello"), "hello");
}

// =====================================================================
// C-ABI buffer composition
// =====================================================================

unsafe fn buf_to_str(buf: *mut u8) -> String {
    // SAFETY: caller-attestation per `# Safety`. buf must be from
    // __cobrust_str_new and live for the duration of the call.
    let len = unsafe { __cobrust_str_len(buf) };
    let ptr = unsafe { __cobrust_str_ptr(buf) };
    if ptr.is_null() || len <= 0 {
        return String::new();
    }
    // SAFETY: buf is alive; ptr is its bytes.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf8_lossy(bytes).to_string()
}

#[test]
fn fstr_cabi_static_only() {
    // f"hello"
    // SAFETY: documented contract.
    unsafe {
        let buf = __cobrust_str_new();
        let bytes = b"hello";
        __cobrust_str_push_static(buf, bytes.as_ptr(), bytes.len() as i64);
        assert_eq!(buf_to_str(buf), "hello");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_int_only() {
    // f"{42}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        __cobrust_fmt_int(buf, 42);
        assert_eq!(buf_to_str(buf), "42");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_static_then_int() {
    // f"x = {42}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let bytes = b"x = ";
        __cobrust_str_push_static(buf, bytes.as_ptr(), bytes.len() as i64);
        __cobrust_fmt_int(buf, 42);
        assert_eq!(buf_to_str(buf), "x = 42");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_static_then_int_then_static() {
    // f"a = {1} and b = {2}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let p1 = b"a = ";
        let p2 = b" and b = ";
        __cobrust_str_push_static(buf, p1.as_ptr(), p1.len() as i64);
        __cobrust_fmt_int(buf, 1);
        __cobrust_str_push_static(buf, p2.as_ptr(), p2.len() as i64);
        __cobrust_fmt_int(buf, 2);
        assert_eq!(buf_to_str(buf), "a = 1 and b = 2");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_float_only() {
    // f"{3.14}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        __cobrust_fmt_float(buf, 3.14);
        let s = buf_to_str(buf);
        assert!(s.starts_with("3.14"));
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_bool_true() {
    // f"{True}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        __cobrust_fmt_bool(buf, 1);
        assert_eq!(buf_to_str(buf), "True");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_bool_false() {
    // f"{False}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        __cobrust_fmt_bool(buf, 0);
        assert_eq!(buf_to_str(buf), "False");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_str_interp() {
    // f"hello, {name}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let prefix = b"hello, ";
        __cobrust_str_push_static(buf, prefix.as_ptr(), prefix.len() as i64);
        let val = b"world";
        __cobrust_fmt_str(buf, val.as_ptr(), val.len() as i64);
        assert_eq!(buf_to_str(buf), "hello, world");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_repr_placeholder() {
    // f"{some_list}" — repr placeholder.
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        __cobrust_fmt_repr(buf, std::ptr::null_mut(), 99);
        assert_eq!(buf_to_str(buf), "<99>");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_full_compose() {
    // f"name={n}, age={a}, ok={ok}, pi={pi}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let p1 = b"name=";
        let p2 = b", age=";
        let p3 = b", ok=";
        let p4 = b", pi=";
        let n = b"alice";
        __cobrust_str_push_static(buf, p1.as_ptr(), p1.len() as i64);
        __cobrust_fmt_str(buf, n.as_ptr(), n.len() as i64);
        __cobrust_str_push_static(buf, p2.as_ptr(), p2.len() as i64);
        __cobrust_fmt_int(buf, 30);
        __cobrust_str_push_static(buf, p3.as_ptr(), p3.len() as i64);
        __cobrust_fmt_bool(buf, 1);
        __cobrust_str_push_static(buf, p4.as_ptr(), p4.len() as i64);
        __cobrust_fmt_float(buf, 3.14);
        let s = buf_to_str(buf);
        assert!(s.starts_with("name=alice, age=30, ok=True, pi=3.14"));
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_empty_buffer_is_empty_str() {
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        assert_eq!(buf_to_str(buf), "");
        assert_eq!(__cobrust_str_len(buf), 0);
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_consecutive_ints() {
    // f"{1}{2}{3}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        __cobrust_fmt_int(buf, 1);
        __cobrust_fmt_int(buf, 2);
        __cobrust_fmt_int(buf, 3);
        assert_eq!(buf_to_str(buf), "123");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_neg_int_in_buf() {
    // f"x={-42}"
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let p = b"x=";
        __cobrust_str_push_static(buf, p.as_ptr(), p.len() as i64);
        __cobrust_fmt_int(buf, -42);
        assert_eq!(buf_to_str(buf), "x=-42");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_unicode_string_static() {
    // f"hello, 世界" — UTF-8 byte payload preserved.
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let s = "hello, 世界".as_bytes();
        __cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        assert_eq!(buf_to_str(buf), "hello, 世界");
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_handles_null_buf_safely() {
    // SAFETY: documented null path on every helper.
    unsafe {
        __cobrust_str_drop(std::ptr::null_mut());
        __cobrust_str_push_static(std::ptr::null_mut(), b"x".as_ptr(), 1);
        assert_eq!(__cobrust_str_len(std::ptr::null_mut()), 0);
        __cobrust_fmt_int(std::ptr::null_mut(), 1);
        __cobrust_fmt_float(std::ptr::null_mut(), 1.0);
        __cobrust_fmt_bool(std::ptr::null_mut(), 1);
        __cobrust_fmt_str(std::ptr::null_mut(), b"x".as_ptr(), 1);
        __cobrust_fmt_repr(std::ptr::null_mut(), std::ptr::null_mut(), 1);
        assert!(__cobrust_str_ptr(std::ptr::null_mut()).is_null());
    }
}

#[test]
fn fstr_cabi_handles_zero_len_static_push() {
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        let bytes = b"";
        __cobrust_str_push_static(buf, bytes.as_ptr(), 0);
        assert_eq!(__cobrust_str_len(buf), 0);
        __cobrust_str_drop(buf);
    }
}

#[test]
fn fstr_cabi_long_buffer_growth() {
    // SAFETY: contract.
    unsafe {
        let buf = __cobrust_str_new();
        for i in 0..50 {
            __cobrust_fmt_int(buf, i);
        }
        let s = buf_to_str(buf);
        assert!(s.starts_with("0123456789"));
        __cobrust_str_drop(buf);
    }
}
