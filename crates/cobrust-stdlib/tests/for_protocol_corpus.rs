//! M12.x for-protocol corpus (per ADR-0027 §4).
//!
//! Each test exercises the iter / next runtime ABI through the
//! `ListIter / DictIter / SetIter / RangeIter` types. The codegen
//! lowering (in cobrust-mir + cobrust-codegen) emits Calls into
//! these helpers; here we exercise them directly to verify the
//! Rust-side semantics independently of the codegen pipeline.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::approx_constant)]
#![allow(clippy::stable_sort_primitive)]
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

use std::collections::{HashMap, HashSet};

use cobrust_stdlib::iter::{__cobrust_iter_drop, __cobrust_iter_init, __cobrust_iter_next};
use cobrust_stdlib::iter::{DictIter, Iterator, ListIter, RangeIter, SetIter};

// =====================================================================
// ListIter
// =====================================================================

#[test]
fn for_list_iter_empty() {
    let mut it: ListIter<i64> = ListIter::new(Vec::new());
    assert!(it.next().is_none());
}

#[test]
fn for_list_iter_one() {
    let mut it = ListIter::new(vec![7]);
    assert_eq!(it.next(), Some(7));
    assert_eq!(it.next(), None);
}

#[test]
fn for_list_iter_three() {
    let mut it = ListIter::new(vec![1, 2, 3]);
    let v: Vec<_> = std::iter::from_fn(|| it.next()).collect();
    assert_eq!(v, vec![1, 2, 3]);
}

#[test]
fn for_list_iter_strings() {
    let mut it = ListIter::new(vec!["a".to_string(), "b".to_string()]);
    assert_eq!(it.next().as_deref(), Some("a"));
    assert_eq!(it.next().as_deref(), Some("b"));
    assert_eq!(it.next(), None);
}

#[test]
fn for_list_iter_remaining_count() {
    let mut it = ListIter::new(vec![1, 2, 3]);
    assert_eq!(it.remaining(), 3);
    it.next();
    assert_eq!(it.remaining(), 2);
    it.next();
    it.next();
    assert_eq!(it.remaining(), 0);
}

#[test]
fn for_list_iter_negative_values() {
    let mut it = ListIter::new(vec![-3, -2, -1, 0, 1]);
    let mut count = 0;
    while it.next().is_some() {
        count += 1;
    }
    assert_eq!(count, 5);
}

#[test]
fn for_list_iter_after_exhaustion_stays_none() {
    let mut it = ListIter::new(vec![1]);
    it.next();
    assert!(it.next().is_none());
    assert!(it.next().is_none());
    assert!(it.next().is_none());
}

#[test]
fn for_list_iter_collect_pattern() {
    let it = ListIter::new(vec![10, 20, 30]);
    let collected: Vec<i64> = std::iter::successors(Some(()), |_| Some(()))
        .scan(it, |it, _| it.next())
        .collect();
    assert_eq!(collected, vec![10, 20, 30]);
}

#[test]
fn for_list_iter_long() {
    let v: Vec<i64> = (0..100).collect();
    let mut it = ListIter::new(v);
    let mut sum = 0i64;
    while let Some(x) = it.next() {
        sum += x;
    }
    assert_eq!(sum, (0..100).sum::<i64>());
}

#[test]
fn for_list_iter_clone_each_call() {
    let mut it = ListIter::new(vec!["x".to_string()]);
    let first = it.next().unwrap();
    assert_eq!(first, "x");
    assert!(it.next().is_none());
}

// =====================================================================
// DictIter
// =====================================================================

#[test]
fn for_dict_iter_empty() {
    let mut it: DictIter<String, i64> = DictIter::new(HashMap::new());
    assert!(it.next().is_none());
}

#[test]
fn for_dict_iter_single() {
    let mut m = HashMap::new();
    m.insert(1, 10);
    let mut it = DictIter::new(m);
    let pair = it.next().unwrap();
    assert_eq!(pair, (1, 10));
    assert!(it.next().is_none());
}

#[test]
fn for_dict_iter_count() {
    let mut m = HashMap::new();
    m.insert(1, 10);
    m.insert(2, 20);
    m.insert(3, 30);
    let mut it = DictIter::new(m);
    let mut count = 0;
    while it.next().is_some() {
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn for_dict_iter_collect_keys() {
    let mut m = HashMap::new();
    m.insert(1, 10);
    m.insert(2, 20);
    let mut it = DictIter::new(m);
    let mut keys = Vec::new();
    while let Some((k, _v)) = it.next() {
        keys.push(k);
    }
    keys.sort();
    assert_eq!(keys, vec![1, 2]);
}

#[test]
fn for_dict_iter_collect_values() {
    let mut m = HashMap::new();
    m.insert("a".to_string(), 100);
    m.insert("b".to_string(), 200);
    let mut it = DictIter::new(m);
    let mut vals = Vec::new();
    while let Some((_k, v)) = it.next() {
        vals.push(v);
    }
    vals.sort();
    assert_eq!(vals, vec![100, 200]);
}

#[test]
fn for_dict_iter_remaining_decreases() {
    let mut m = HashMap::new();
    m.insert(1, 10);
    m.insert(2, 20);
    let mut it = DictIter::new(m);
    assert_eq!(it.remaining(), 2);
    it.next();
    assert_eq!(it.remaining(), 1);
    it.next();
    assert_eq!(it.remaining(), 0);
}

// =====================================================================
// SetIter
// =====================================================================

#[test]
fn for_set_iter_empty() {
    let mut it: SetIter<i64> = SetIter::new(HashSet::new());
    assert!(it.next().is_none());
}

#[test]
fn for_set_iter_unique_count() {
    let mut s = HashSet::new();
    s.insert(1);
    s.insert(2);
    s.insert(2);
    let mut it = SetIter::new(s);
    let mut count = 0;
    while it.next().is_some() {
        count += 1;
    }
    assert_eq!(count, 2);
}

#[test]
fn for_set_iter_collect_sorted() {
    let mut s = HashSet::new();
    s.insert(5);
    s.insert(3);
    s.insert(1);
    let mut it = SetIter::new(s);
    let mut v = Vec::new();
    while let Some(x) = it.next() {
        v.push(x);
    }
    v.sort();
    assert_eq!(v, vec![1, 3, 5]);
}

#[test]
fn for_set_iter_strings() {
    let mut s = HashSet::new();
    s.insert("alpha".to_string());
    s.insert("beta".to_string());
    let mut it = SetIter::new(s);
    let mut found = 0;
    while it.next().is_some() {
        found += 1;
    }
    assert_eq!(found, 2);
}

#[test]
fn for_set_iter_after_exhaust_stays_none() {
    let mut s = HashSet::new();
    s.insert(1);
    let mut it = SetIter::new(s);
    it.next();
    assert!(it.next().is_none());
    assert!(it.next().is_none());
}

// =====================================================================
// RangeIter
// =====================================================================

#[test]
fn for_range_unbounded_zero() {
    let mut r = RangeIter::unbounded(0);
    assert!(r.next().is_none());
}

#[test]
fn for_range_unbounded_three() {
    let mut r = RangeIter::unbounded(3);
    assert_eq!(r.next(), Some(0));
    assert_eq!(r.next(), Some(1));
    assert_eq!(r.next(), Some(2));
    assert_eq!(r.next(), None);
}

#[test]
fn for_range_bounded_basic() {
    let mut r = RangeIter::bounded(5, 8);
    assert_eq!(r.next(), Some(5));
    assert_eq!(r.next(), Some(6));
    assert_eq!(r.next(), Some(7));
    assert_eq!(r.next(), None);
}

#[test]
fn for_range_stepped_two() {
    let mut r = RangeIter::stepped(0, 10, 2);
    let v: Vec<_> = std::iter::from_fn(|| r.next()).collect();
    assert_eq!(v, vec![0, 2, 4, 6, 8]);
}

#[test]
fn for_range_stepped_negative() {
    let mut r = RangeIter::stepped(10, 0, -2);
    let v: Vec<_> = std::iter::from_fn(|| r.next()).collect();
    assert_eq!(v, vec![10, 8, 6, 4, 2]);
}

#[test]
fn for_range_long() {
    let mut r = RangeIter::unbounded(100);
    let mut sum = 0i64;
    while let Some(x) = r.next() {
        sum += x;
    }
    assert_eq!(sum, (0..100).sum::<i64>());
}

#[test]
fn for_range_overflow_safe() {
    // saturating add inside RangeIter::next prevents UB.
    let mut r = RangeIter::stepped(i64::MAX - 2, i64::MAX, 1);
    assert_eq!(r.next(), Some(i64::MAX - 2));
    assert_eq!(r.next(), Some(i64::MAX - 1));
    assert!(r.next().is_none());
}

// =====================================================================
// C-ABI iter handle
// =====================================================================

// ADR-0044 W2 Phase 2: `__cobrust_iter_init` now interprets its arg
// as a list-layout pointer (not a Range count) since no source-level
// construct in Cobrust today produces a Range. The previous tests
// asserted the dead RangeIter semantics; below we exercise the new
// list-iter contract.

#[test]
fn for_cabi_iter_runs_to_exhaustion() {
    // SAFETY: documented contract.
    unsafe {
        let list = cobrust_stdlib::collections::__cobrust_list_new(8, 5);
        for i in 0..5 {
            cobrust_stdlib::collections::__cobrust_list_set(list, i, i);
        }
        let h = __cobrust_iter_init(list as i64);
        let mut found = Vec::new();
        // Iter contract: yields slot values; 0 = exhaustion sentinel
        // (with `done` flag in IterHandle distinguishing a legit-0 slot).
        // For this test the list has values 0..5; the first call yields
        // 0 (slot[0]), then the iter handle's `done` flag is NOT set yet
        // since the source had more slots. Subsequent calls yield 1..4.
        // The exhaustion call after slot[4] returns 0 with `done` set.
        for _ in 0..5 {
            found.push(__cobrust_iter_next(h));
        }
        // Final call after all 5 slots → exhausted.
        assert_eq!(__cobrust_iter_next(h), 0);
        assert_eq!(found, vec![0, 1, 2, 3, 4]);
        __cobrust_iter_drop(h);
        cobrust_stdlib::collections::__cobrust_list_drop(list);
    }
}

#[test]
fn for_cabi_iter_zero_count_immediate_exhaust() {
    // SAFETY: ADR-0044 amendment — a null list pointer exhausts
    // immediately (__cobrust_list_len returns 0 for null).
    unsafe {
        let h = __cobrust_iter_init(0);
        assert_eq!(__cobrust_iter_next(h), 0);
        __cobrust_iter_drop(h);
    }
}

#[test]
fn for_cabi_iter_empty_list_immediate_exhaust() {
    // SAFETY: ADR-0044 amendment — an empty list exhausts on the
    // first __cobrust_iter_next call (len == 0). Replaces the now-
    // dead `negative_treated_as_zero` test that asserted RangeIter
    // semantics (which never reached source level).
    unsafe {
        let list = cobrust_stdlib::collections::__cobrust_list_new(8, 0);
        let h = __cobrust_iter_init(list as i64);
        assert_eq!(__cobrust_iter_next(h), 0);
        __cobrust_iter_drop(h);
        cobrust_stdlib::collections::__cobrust_list_drop(list);
    }
}
