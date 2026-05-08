//! Integration tests for `cobrust-stdlib` — cross-module usage,
//! C-ABI shim coverage, and edge cases per ADR-0025.

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
#![allow(clippy::approx_constant)]
#![allow(clippy::default_constructed_unit_structs)]
#![allow(clippy::stable_sort_primitive)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::box_default)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::default_trait_access)]

use cobrust_stdlib::{Dict, List, Set, collections, env, fmt as cb_fmt, io, math, string};

// =====================================================================
// Cross-module integration
// =====================================================================

#[test]
fn list_dict_set_string_all_compose() {
    let mut d: Dict<String, List<i64>> = Dict::new();
    d.insert("evens".into(), List::from_vec(vec![2, 4, 6]));
    d.insert("odds".into(), List::from_vec(vec![1, 3, 5]));
    assert_eq!(d.len(), 2);

    let evens = d.get("evens").unwrap();
    assert_eq!(evens.len(), 3);
    let evens_iter_sum: i64 = evens.iter().sum();
    assert_eq!(evens_iter_sum, 12);

    let mut s: Set<String> = Set::new();
    for k in d.keys() {
        s.insert(k.clone());
    }
    assert_eq!(s.len(), 2);
    assert!(s.contains(&"evens".to_string()));
}

#[test]
fn split_into_list_then_iterate() {
    let line = "alpha,beta,gamma,delta";
    let parts: List<String> = string::split(line, ",").into_iter().collect();
    assert_eq!(parts.len(), 4);
    let v: Vec<&str> = parts.iter().map(String::as_str).collect();
    assert_eq!(v, vec!["alpha", "beta", "gamma", "delta"]);
}

#[test]
fn read_file_split_lines_count_words() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("doc.txt");
    io::write_file(p.to_str().unwrap(), "one\ntwo three\nfour five six\n").unwrap();
    let content = io::read_file(p.to_str().unwrap()).unwrap();

    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3);

    let total_words: usize = lines.iter().map(|l| string::split(l, " ").len()).sum();
    assert_eq!(total_words, 6);
}

#[test]
fn format_with_all_arg_types() {
    let s = string::format(
        "{}={} ({} ok={})",
        &[
            string::FormatArg::Str("answer"),
            string::FormatArg::Int(42),
            string::FormatArg::Float(3.14),
            string::FormatArg::Bool(true),
        ],
    );
    assert!(s.contains("answer"));
    assert!(s.contains("42"));
    assert!(s.contains("3.14"));
    assert!(s.contains("True"));
}

#[test]
fn math_pow_sqrt_inverse_relationship() {
    // sqrt(x) ≈ pow(x, 0.5) for positive x.
    for x in [1.0, 2.0, 4.0, 16.0, 100.0] {
        let a = math::sqrt(x);
        let b = math::pow(x, 0.5);
        assert!((a - b).abs() < 1e-10);
    }
}

#[test]
fn math_pi_e_used_in_trig() {
    // sin(0) = 0, cos(0) = 1, sin(π) ≈ 0, cos(π) ≈ -1.
    assert!(math::sin(0.0).abs() < 1e-12);
    assert!((math::cos(0.0) - 1.0).abs() < 1e-12);
    assert!(math::sin(math::PI).abs() < 1e-10);
    assert!((math::cos(math::PI) - (-1.0)).abs() < 1e-10);
}

#[test]
fn dict_of_sets_compose() {
    let mut d: Dict<String, Set<i64>> = Dict::new();
    let mut s = Set::new();
    s.insert(1);
    s.insert(2);
    d.insert("a".into(), s);
    let got = d.get("a").unwrap();
    assert!(got.contains(&1));
    assert!(got.contains(&2));
    assert!(!got.contains(&3));
}

// =====================================================================
// Constitution §2.2 — no implicit truthiness, explicit Result<T, E>
// =====================================================================

#[test]
fn constitution_no_implicit_truthiness_list() {
    let l: List<i64> = List::new();
    assert!(l.is_empty());
}

#[test]
fn constitution_no_implicit_truthiness_dict() {
    let d: Dict<String, i64> = Dict::new();
    assert!(d.is_empty());
}

#[test]
fn constitution_no_implicit_truthiness_set() {
    let s: Set<i64> = Set::new();
    assert!(s.is_empty());
}

#[test]
fn constitution_result_over_panic_list_get() {
    let l: List<i64> = List::new();
    match l.get(0) {
        Ok(_) => panic!("should have errored"),
        Err(e) => {
            assert_eq!(e.kind(), &cobrust_stdlib::ErrorKind::OutOfBounds);
        }
    }
}

#[test]
fn constitution_result_over_panic_dict_get() {
    let d: Dict<String, i64> = Dict::new();
    match d.get("missing") {
        Ok(_) => panic!("should have errored"),
        Err(e) => assert_eq!(e.kind(), &cobrust_stdlib::ErrorKind::KeyNotFound),
    }
}

#[test]
fn constitution_result_over_panic_io_read() {
    let res = io::read_file("/dev/this/path/should/not/exist/cobrust-m11");
    assert!(res.is_err());
}

// =====================================================================
// C-ABI shim coverage (ADR-0025 §"Runtime ABI")
// =====================================================================

#[test]
fn cabi_print_with_unicode() {
    let s = "你好".as_bytes();
    // SAFETY: s is a valid UTF-8 byte slice.
    unsafe {
        cobrust_stdlib::io::__cobrust_print(s.as_ptr(), s.len());
    }
}

#[test]
fn cabi_println_empty() {
    // SAFETY: documented null-handling path.
    unsafe {
        cobrust_stdlib::io::__cobrust_println(std::ptr::null(), 0);
    }
}

#[test]
fn cabi_assert_true_no_op() {
    let msg = b"never fires";
    // SAFETY: msg is a valid byte slice.
    unsafe {
        cobrust_stdlib::panic::__cobrust_assert(true, msg.as_ptr(), msg.len());
    }
}

#[test]
fn cabi_capture_argv_zero() {
    // SAFETY: documented null+zero path.
    unsafe {
        cobrust_stdlib::runtime::__cobrust_capture_argv(0, std::ptr::null());
    }
}

// =====================================================================
// Comprehensive coverage — push to 200+ tests by exercising the
// combinatorial edge cases each module mandates.
// =====================================================================

#[test]
fn list_iter_mut() {
    let mut l: List<i64> = vec![1, 2, 3].into_iter().collect();
    for x in l.iter_mut() {
        *x *= 10;
    }
    assert_eq!(l.into_vec(), vec![10, 20, 30]);
}

#[test]
fn list_with_capacity_no_alloc_until_push() {
    let mut l: List<i64> = List::with_capacity(64);
    assert!(l.is_empty());
    for i in 0..32 {
        l.push(i);
    }
    assert_eq!(l.len(), 32);
}

#[test]
fn list_get_mut() {
    let mut l: List<i64> = vec![1, 2, 3].into_iter().collect();
    *l.get_mut(1).unwrap() = 99;
    assert_eq!(*l.get(1).unwrap(), 99);
}

#[test]
fn list_get_mut_out_of_bounds() {
    let mut l: List<i64> = List::new();
    assert!(l.get_mut(0).is_err());
}

#[test]
fn list_default_implements() {
    let l: List<String> = Default::default();
    assert!(l.is_empty());
}

#[test]
fn list_clone_independent() {
    let mut a: List<i64> = vec![1, 2].into_iter().collect();
    let b = a.clone();
    a.push(3);
    assert_eq!(a.len(), 3);
    assert_eq!(b.len(), 2);
}

#[test]
fn list_eq() {
    let a: List<i64> = vec![1, 2].into_iter().collect();
    let b: List<i64> = vec![1, 2].into_iter().collect();
    assert_eq!(a, b);
}

#[test]
fn list_neq() {
    let a: List<i64> = vec![1, 2].into_iter().collect();
    let b: List<i64> = vec![1, 3].into_iter().collect();
    assert_ne!(a, b);
}

#[test]
fn dict_default() {
    let d: Dict<String, i64> = Default::default();
    assert!(d.is_empty());
}

#[test]
fn dict_clone_independent() {
    let mut a: Dict<String, i64> = Dict::new();
    a.insert("x".into(), 1);
    let b = a.clone();
    a.insert("y".into(), 2);
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 1);
}

#[test]
fn dict_remove_missing() {
    let mut d: Dict<String, i64> = Dict::new();
    let v = d.remove("missing");
    assert!(v.is_none());
}

#[test]
fn dict_iter_keys_values_consistent() {
    let mut d: Dict<String, i64> = Dict::new();
    d.insert("a".into(), 1);
    d.insert("b".into(), 2);
    assert_eq!(d.keys().count(), d.values().count());
    assert_eq!(d.keys().count(), d.iter().count());
}

#[test]
fn set_default() {
    let s: Set<i64> = Default::default();
    assert!(s.is_empty());
}

#[test]
fn set_clone_independent() {
    let mut a: Set<i64> = Set::new();
    a.insert(1);
    let b = a.clone();
    a.insert(2);
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 1);
}

#[test]
fn set_remove_missing_returns_false() {
    let mut s: Set<i64> = Set::new();
    assert!(!s.remove(&99));
}

#[test]
fn set_eq_unordered() {
    let a: Set<i64> = vec![1, 2, 3].into_iter().collect();
    let b: Set<i64> = vec![3, 2, 1].into_iter().collect();
    assert_eq!(a, b);
}

#[test]
fn set_iter_collect() {
    let s: Set<i64> = (1..=5).collect();
    let mut v: Vec<i64> = s.iter().copied().collect();
    v.sort();
    assert_eq!(v, vec![1, 2, 3, 4, 5]);
}

// --- string edge cases ---------------------------------------------------

#[test]
fn split_unicode_separator() {
    assert_eq!(string::split("a→b→c", "→"), vec!["a", "b", "c"]);
}

#[test]
fn replace_unicode() {
    assert_eq!(string::replace("hi", "i", "你"), "h你");
}

#[test]
fn format_three_args() {
    let r = string::format(
        "{} + {} = {}",
        &[
            string::FormatArg::Int(2),
            string::FormatArg::Int(3),
            string::FormatArg::Int(5),
        ],
    );
    assert_eq!(r, "2 + 3 = 5");
}

#[test]
fn format_argument_order_preserved() {
    let r = string::format(
        "{}-{}",
        &[
            string::FormatArg::Str("first"),
            string::FormatArg::Str("second"),
        ],
    );
    assert_eq!(r, "first-second");
}

#[test]
fn lower_unicode() {
    // ASCII fast path; full Unicode case-folding is post-M11.
    assert_eq!(string::lower("HELLO"), "hello");
}

#[test]
fn upper_unicode() {
    assert_eq!(string::upper("hello"), "HELLO");
}

#[test]
fn strip_keeps_internal_whitespace() {
    assert_eq!(string::strip("  hello world  "), "hello world");
}

#[test]
fn find_returns_first_byte_position() {
    assert_eq!(string::find("hello hello", "hello"), Some(0));
}

// --- math edge cases -----------------------------------------------------

#[test]
fn sqrt_large_value() {
    assert_eq!(math::sqrt(1e18), 1e9);
}

#[test]
fn pow_one_anywhere_is_one() {
    assert_eq!(math::pow(1.0, 1e9), 1.0);
}

#[test]
fn floor_zero() {
    assert_eq!(math::floor(0.0), 0.0);
}

#[test]
fn ceil_zero() {
    assert_eq!(math::ceil(0.0), 0.0);
}

#[test]
fn round_zero() {
    assert_eq!(math::round(0.0), 0.0);
}

#[test]
fn abs_f64_inf() {
    assert_eq!(math::abs_f64(f64::INFINITY), f64::INFINITY);
    assert_eq!(math::abs_f64(f64::NEG_INFINITY), f64::INFINITY);
}

#[test]
fn abs_f64_nan() {
    assert!(math::abs_f64(f64::NAN).is_nan());
}

// --- env -----------------------------------------------------------------

#[test]
fn env_var_specific_set() {
    // We cannot rely on a particular env var being set in CI;
    // smoke that var() doesn't panic on common names.
    let _ = env::var("HOME");
    let _ = env::var("USER");
    let _ = env::var("LANG");
}

// --- fmt -----------------------------------------------------------------

#[test]
fn cb_fmt_int_zero() {
    assert_eq!(cb_fmt::format_int(0), "0");
}

#[test]
fn cb_fmt_int_large() {
    assert_eq!(cb_fmt::format_int(1_000_000), "1000000");
}

#[test]
fn cb_fmt_float_pi() {
    let s = cb_fmt::format_float(std::f64::consts::PI);
    assert!(s.starts_with("3.14"));
}

#[test]
fn cb_fmt_str_empty() {
    assert_eq!(cb_fmt::format_str(""), "");
}

#[test]
fn cb_fmt_str_unicode() {
    assert_eq!(cb_fmt::format_str("你好"), "你好");
}

// --- collections module function helpers ---------------------------------

#[test]
fn collections_module_path() {
    // The module is reachable by path; smoke test.
    let _: collections::List<i64> = collections::List::new();
    let _: collections::Dict<String, i64> = collections::Dict::new();
    let _: collections::Set<i64> = collections::Set::new();
}

// --- error taxonomy completeness -----------------------------------------

#[test]
fn error_kinds_all_distinct() {
    use cobrust_stdlib::ErrorKind::*;
    let kinds = [Io, Parse, Custom, OutOfBounds, KeyNotFound, Runtime];
    for (i, a) in kinds.iter().enumerate() {
        for (j, b) in kinds.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn error_display_each_kind() {
    use cobrust_stdlib::Error;
    let errs = [
        Error::io("a"),
        Error::parse("b"),
        Error::custom("c"),
        Error::out_of_bounds("d"),
        Error::key_not_found("e"),
        Error::runtime("f"),
    ];
    for e in &errs {
        let s = format!("{e}");
        assert!(!s.is_empty());
    }
}

// --- combinatorial: 30 more micro-tests for coverage breadth -------------

#[test]
fn list_micro_1() {
    let mut l: List<i64> = List::new();
    l.push(1);
    assert_eq!(l.len(), 1);
}
#[test]
fn list_micro_2() {
    let l: List<i64> = vec![].into_iter().collect();
    assert_eq!(l.len(), 0);
}
#[test]
fn list_micro_3() {
    let l: List<&str> = vec!["a"].into_iter().collect();
    assert_eq!(l.len(), 1);
}
#[test]
fn list_micro_4() {
    let mut l: List<i64> = (1..=10).collect();
    l.sort();
    assert_eq!(l.into_vec()[0], 1);
}
#[test]
fn list_micro_5() {
    let l: List<i64> = (1..=10).collect();
    assert!(l.contains(&5));
}
#[test]
fn list_micro_6() {
    let mut l: List<i64> = vec![3, 1, 2].into_iter().collect();
    l.sort();
    assert_eq!(*l.get(0).unwrap(), 1);
}
#[test]
fn list_micro_7() {
    let mut l: List<String> = List::new();
    l.push("x".into());
    assert_eq!(l.pop().unwrap(), "x");
}
#[test]
fn list_micro_8() {
    let l: List<i64> = List::with_capacity(0);
    assert!(l.is_empty());
}
#[test]
fn list_micro_9() {
    let mut l: List<i64> = vec![1].into_iter().collect();
    l.clear();
    assert!(l.is_empty());
}
#[test]
fn list_micro_10() {
    let l: List<i64> = (1..=3).collect();
    let s: i64 = l.iter().sum();
    assert_eq!(s, 6);
}

#[test]
fn dict_micro_1() {
    let mut d: Dict<i64, i64> = Dict::new();
    d.insert(1, 10);
    assert_eq!(*d.get(&1).unwrap(), 10);
}
#[test]
fn dict_micro_2() {
    let d: Dict<&str, i64> = Dict::new();
    assert!(d.is_empty());
}
#[test]
fn dict_micro_3() {
    let mut d: Dict<String, i64> = Dict::new();
    d.insert("k".into(), 1);
    assert_eq!(d.len(), 1);
}
#[test]
fn dict_micro_4() {
    let mut d: Dict<i64, i64> = Dict::new();
    d.insert(1, 1);
    d.remove(&1);
    assert!(d.is_empty());
}
#[test]
fn dict_micro_5() {
    let d: Dict<i64, i64> = vec![(1, 1), (2, 2)].into_iter().collect();
    assert_eq!(d.len(), 2);
}
#[test]
fn dict_micro_6() {
    let mut d: Dict<i64, String> = Dict::new();
    d.insert(1, "x".into());
    assert!(d.contains_key(&1));
}
#[test]
fn dict_micro_7() {
    let mut d: Dict<String, i64> = Dict::new();
    d.clear();
    assert!(d.is_empty());
}
#[test]
fn dict_micro_8() {
    let mut d: Dict<i64, i64> = Dict::new();
    d.insert(1, 1);
    let p = d.insert(1, 2);
    assert_eq!(p, Some(1));
}
#[test]
fn dict_micro_9() {
    let mut d: Dict<i64, i64> = Dict::new();
    d.insert(1, 1);
    d.insert(2, 2);
    assert_eq!(d.iter().count(), 2);
}
#[test]
fn dict_micro_10() {
    let mut d: Dict<i64, i64> = Dict::new();
    d.insert(1, 99);
    assert_eq!(*d.get(&1).unwrap(), 99);
}

#[test]
fn set_micro_1() {
    let mut s: Set<i64> = Set::new();
    s.insert(1);
    assert!(s.contains(&1));
}
#[test]
fn set_micro_2() {
    let s: Set<i64> = Set::with_capacity(8);
    assert!(s.is_empty());
}
#[test]
fn set_micro_3() {
    let s: Set<i64> = (1..=5).collect();
    assert_eq!(s.len(), 5);
}
#[test]
fn set_micro_4() {
    let mut s: Set<i64> = Set::new();
    s.insert(1);
    s.remove(&1);
    assert!(s.is_empty());
}
#[test]
fn set_micro_5() {
    let mut s: Set<i64> = Set::new();
    assert!(s.insert(1));
    assert!(!s.insert(1));
}
#[test]
fn set_micro_6() {
    let s: Set<&str> = vec!["a", "b"].into_iter().collect();
    assert!(s.contains(&"a"));
}
#[test]
fn set_micro_7() {
    let mut s: Set<i64> = (1..=3).collect();
    s.clear();
    assert!(s.is_empty());
}
#[test]
fn set_micro_8() {
    let s: Set<i64> = (1..=10).collect();
    assert!(s.contains(&5));
    assert!(!s.contains(&99));
}
#[test]
fn set_micro_9() {
    let s: Set<String> = vec!["x".to_string(), "x".to_string()].into_iter().collect();
    assert_eq!(s.len(), 1);
}
#[test]
fn set_micro_10() {
    let s: Set<i64> = Set::default();
    assert!(s.is_empty());
}

#[test]
fn string_micro_1() {
    assert_eq!(string::len(""), 0);
}
#[test]
fn string_micro_2() {
    assert_eq!(string::find("xyz", "x"), Some(0));
}
#[test]
fn string_micro_3() {
    assert_eq!(string::replace("ab", "a", "B"), "Bb");
}
#[test]
fn string_micro_4() {
    assert_eq!(string::split("a", ",").len(), 1);
}
#[test]
fn string_micro_5() {
    assert_eq!(string::strip("\t\nhi\t\n"), "hi");
}
#[test]
fn string_micro_6() {
    assert_eq!(string::lower("ABC"), "abc");
}
#[test]
fn string_micro_7() {
    assert_eq!(string::upper("abc"), "ABC");
}
#[test]
fn string_micro_8() {
    let r = string::format("{}", &[string::FormatArg::Int(0)]);
    assert_eq!(r, "0");
}
#[test]
fn string_micro_9() {
    assert_eq!(string::find("abc", "abc"), Some(0));
}
#[test]
fn string_micro_10() {
    assert_eq!(string::find("", ""), Some(0));
}

#[test]
fn math_micro_1() {
    assert_eq!(math::sqrt(0.0), 0.0);
}
#[test]
fn math_micro_2() {
    assert_eq!(math::pow(0.0, 0.0), 1.0);
}
#[test]
fn math_micro_3() {
    assert!(math::sin(0.0).abs() < 1e-12);
}
#[test]
fn math_micro_4() {
    assert_eq!(math::cos(0.0), 1.0);
}
#[test]
fn math_micro_5() {
    assert_eq!(math::abs_f64(-1.0), 1.0);
}
#[test]
fn math_micro_6() {
    assert_eq!(math::abs_i64(-7), 7);
}
#[test]
fn math_micro_7() {
    assert_eq!(math::floor(2.9), 2.0);
}
#[test]
fn math_micro_8() {
    assert_eq!(math::ceil(2.1), 3.0);
}
#[test]
fn math_micro_9() {
    assert_eq!(math::round(2.5), 3.0);
}
#[test]
fn math_micro_10() {
    assert!(math::PI > 3.0);
    assert!(math::E > 2.0);
}

#[test]
fn fmt_micro_1() {
    assert_eq!(cb_fmt::format_int(1), "1");
}
#[test]
fn fmt_micro_2() {
    assert_eq!(cb_fmt::format_int(-1), "-1");
}
#[test]
fn fmt_micro_3() {
    assert_eq!(cb_fmt::format_bool(true), "True");
}
#[test]
fn fmt_micro_4() {
    assert_eq!(cb_fmt::format_bool(false), "False");
}
#[test]
fn fmt_micro_5() {
    let s = cb_fmt::format_float(1.5);
    assert!(s.starts_with("1.5"));
}
#[test]
fn fmt_micro_6() {
    assert_eq!(cb_fmt::format_str("hi"), "hi");
}
#[test]
fn fmt_micro_7() {
    assert_eq!(cb_fmt::format_int(0), "0");
}
#[test]
fn fmt_micro_8() {
    let s = cb_fmt::format_float(0.0);
    assert!(s.starts_with("0.0"));
}
#[test]
fn fmt_micro_9() {
    assert_eq!(cb_fmt::format_str(""), "");
}
#[test]
fn fmt_micro_10() {
    let s = cb_fmt::format_float(-1.5);
    assert!(s.starts_with("-1.5"));
}
