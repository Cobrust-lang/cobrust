//! `std.iter` — iterator protocol surface.
//!
//! ADR-0027 §4 binds: HIR `Stmt::For { var, iter_expr, body }`
//! lowers to MIR
//!
//! ```mir
//! let it = iter_expr.iter();
//! loop:
//!   let opt = it.next();
//!   if opt.is_none() { break }
//!   let var = opt.unwrap();
//!   body
//!   goto loop
//! ```
//!
//! The four stdlib iter types — [`ListIter`], [`DictIter`],
//! [`SetIter`], [`RangeIter`] — implement the [`Iterator`] trait
//! defined here. User-defined types implementing the trait are
//! Phase F (deferred per ADR-0027 §"Deferred to Phase F").
//!
//! Constitution §2.2 binds `Result<T, E>` over panic for user-driven
//! errors; iterators here use [`Option<T>`] (`None` means exhausted)
//! per Rust convention, and the for-protocol lowering panics only
//! when the host-language type system is misused (e.g. iterating a
//! non-iterable).

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

// =====================================================================
// Iterator trait — the for-protocol surface
// =====================================================================

/// The iteration protocol. Cobrust source `for x in expr:` lowers
/// to MIR that calls `iter_expr.iter()` once and `it.next()` per
/// loop turn (per ADR-0027 §4).
///
/// At M12.x the trait surface is closed-world — only the four types
/// in this module implement it. Phase F lifts to user-defined types.
pub trait Iterator {
    type Item;

    /// Yield the next element, or `None` if exhausted.
    fn next(&mut self) -> Option<Self::Item>;
}

// =====================================================================
// ListIter<T>
// =====================================================================

/// Iterator over a [`crate::collections::List`]'s elements.
///
/// Construct via `List::iter_proto()` (the source-level protocol
/// adapter) or directly. Yields owned values; for borrow iteration
/// users use `List::iter()` from the standard library.
pub struct ListIter<T> {
    items: Vec<T>,
    idx: usize,
}

impl<T> ListIter<T> {
    /// New iterator over an owned vector. Consuming the source list
    /// is the M12.x semantic for the for-protocol; Phase F adds
    /// borrow-iteration via reference types.
    pub fn new(items: Vec<T>) -> Self {
        Self { items, idx: 0 }
    }

    /// Length of the remaining sequence.
    pub fn remaining(&self) -> usize {
        self.items.len().saturating_sub(self.idx)
    }
}

impl<T: Clone> Iterator for ListIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.idx >= self.items.len() {
            return None;
        }
        let v = self.items[self.idx].clone();
        self.idx += 1;
        Some(v)
    }
}

// =====================================================================
// DictIter<K, V>
// =====================================================================

/// Iterator over a [`crate::collections::Dict`]'s `(key, value)`
/// pairs. The order matches the underlying `HashMap`'s iteration
/// order (unspecified per Rust's `HashMap` contract).
pub struct DictIter<K: Eq + Hash, V> {
    entries: Vec<(K, V)>,
    idx: usize,
}

impl<K: Eq + Hash, V> DictIter<K, V> {
    /// Build from an owned `HashMap`.
    pub fn new(map: HashMap<K, V>) -> Self {
        let entries: Vec<(K, V)> = map.into_iter().collect();
        Self { entries, idx: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.entries.len().saturating_sub(self.idx)
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Iterator for DictIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<(K, V)> {
        if self.idx >= self.entries.len() {
            return None;
        }
        let (k, v) = (
            self.entries[self.idx].0.clone(),
            self.entries[self.idx].1.clone(),
        );
        self.idx += 1;
        Some((k, v))
    }
}

// =====================================================================
// SetIter<T>
// =====================================================================

/// Iterator over a [`crate::collections::Set`]'s elements. Order
/// matches the underlying `HashSet`'s iteration order.
pub struct SetIter<T: Eq + Hash> {
    items: Vec<T>,
    idx: usize,
}

impl<T: Eq + Hash> SetIter<T> {
    pub fn new(set: HashSet<T>) -> Self {
        let items: Vec<T> = set.into_iter().collect();
        Self { items, idx: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.items.len().saturating_sub(self.idx)
    }
}

impl<T: Eq + Hash + Clone> Iterator for SetIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.idx >= self.items.len() {
            return None;
        }
        let v = self.items[self.idx].clone();
        self.idx += 1;
        Some(v)
    }
}

// =====================================================================
// RangeIter — `for i in range(start, stop, step):`
// =====================================================================

/// Half-open arithmetic range with optional step. Mirrors Python's
/// `range(start, stop, step)` semantics (stop is exclusive). Steps
/// of 0 panic — the Cobrust type system rejects literal-0 steps but
/// runtime-computed ones can still hit this path.
pub struct RangeIter {
    cur: i64,
    stop: i64,
    step: i64,
}

impl RangeIter {
    /// `range(stop)` — `0..stop` with step `1`.
    pub fn unbounded(stop: i64) -> Self {
        Self {
            cur: 0,
            stop,
            step: 1,
        }
    }

    /// `range(start, stop)` — `start..stop` with step `1`.
    pub fn bounded(start: i64, stop: i64) -> Self {
        Self {
            cur: start,
            stop,
            step: 1,
        }
    }

    /// `range(start, stop, step)`.
    ///
    /// # Panics
    ///
    /// Panics if `step == 0`. The Cobrust type checker forbids
    /// literal-0 steps; this guard catches runtime-computed misuse.
    pub fn stepped(start: i64, stop: i64, step: i64) -> Self {
        assert!(step != 0, "RangeIter step must be non-zero");
        Self {
            cur: start,
            stop,
            step,
        }
    }
}

impl Iterator for RangeIter {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        if self.step > 0 {
            if self.cur >= self.stop {
                return None;
            }
        } else if self.cur <= self.stop {
            return None;
        }
        let v = self.cur;
        self.cur = self.cur.saturating_add(self.step);
        Some(v)
    }
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
    fn list_iter_empty() {
        let mut it: ListIter<i64> = ListIter::new(Vec::new());
        assert_eq!(it.next(), None);
    }

    #[test]
    fn list_iter_single() {
        let mut it = ListIter::new(vec![7]);
        assert_eq!(it.next(), Some(7));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn list_iter_multiple() {
        let mut it = ListIter::new(vec![1, 2, 3]);
        let v: Vec<i64> = std::iter::from_fn(|| it.next()).collect();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn list_iter_remaining_decreases() {
        let mut it = ListIter::new(vec![1, 2, 3]);
        assert_eq!(it.remaining(), 3);
        it.next();
        assert_eq!(it.remaining(), 2);
    }

    #[test]
    fn dict_iter_count() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), 1);
        m.insert("b".to_string(), 2);
        let mut it = DictIter::new(m);
        let mut count = 0;
        while it.next().is_some() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn dict_iter_empty() {
        let mut it: DictIter<String, i64> = DictIter::new(HashMap::new());
        assert!(it.next().is_none());
    }

    #[test]
    fn set_iter_count() {
        let mut s: HashSet<i64> = HashSet::new();
        s.insert(1);
        s.insert(2);
        s.insert(3);
        let mut it = SetIter::new(s);
        let mut count = 0;
        while it.next().is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
    }

    #[test]
    fn set_iter_empty() {
        let mut it: SetIter<i64> = SetIter::new(HashSet::new());
        assert!(it.next().is_none());
    }

    #[test]
    fn range_iter_unbounded() {
        let mut r = RangeIter::unbounded(3);
        assert_eq!(r.next(), Some(0));
        assert_eq!(r.next(), Some(1));
        assert_eq!(r.next(), Some(2));
        assert_eq!(r.next(), None);
    }

    #[test]
    fn range_iter_bounded() {
        let mut r = RangeIter::bounded(2, 5);
        let v: Vec<i64> = std::iter::from_fn(|| r.next()).collect();
        assert_eq!(v, vec![2, 3, 4]);
    }

    #[test]
    fn range_iter_stepped_positive() {
        let mut r = RangeIter::stepped(0, 10, 2);
        let v: Vec<i64> = std::iter::from_fn(|| r.next()).collect();
        assert_eq!(v, vec![0, 2, 4, 6, 8]);
    }

    #[test]
    fn range_iter_stepped_negative() {
        let mut r = RangeIter::stepped(5, 0, -1);
        let v: Vec<i64> = std::iter::from_fn(|| r.next()).collect();
        assert_eq!(v, vec![5, 4, 3, 2, 1]);
    }

    #[test]
    fn range_iter_empty_when_start_eq_stop() {
        let mut r = RangeIter::bounded(3, 3);
        assert_eq!(r.next(), None);
    }

    #[test]
    fn range_iter_empty_negative_no_progress() {
        // start < stop with positive step empty when start >= stop.
        let mut r = RangeIter::bounded(5, 2);
        assert_eq!(r.next(), None);
    }

    #[test]
    #[should_panic(expected = "RangeIter step must be non-zero")]
    fn range_iter_stepped_zero_panics() {
        let _ = RangeIter::stepped(0, 5, 0);
    }
}
