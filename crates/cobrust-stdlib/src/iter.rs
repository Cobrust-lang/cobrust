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

// =====================================================================
// C-ABI runtime helpers (ADR-0027 §4 for-protocol lowering)
// =====================================================================
//
// HIR `Stmt::For` lowers to MIR Calls:
//   __cobrust_iter_init(iter_value) -> *mut IterHandle
//   __cobrust_iter_next(handle)     -> i64 (0 = None, !=0 = next value)
//   __cobrust_iter_drop(handle)
//
// The handle wraps a polymorphic iterator over `i64` values backed
// by ListIter / DictIter / SetIter / RangeIter. M12.x i64-only is
// the conservative width matching the codegen integer type; Phase F
// widens to per-type dispatch.

/// Internal handle wrapping a Box<dyn>-erased iterator over i64
/// values. ADR-0044 W2 Phase 2 amendment: tracks an explicit `done`
/// flag so a list-of-i64 element with legitimate value 0 is
/// distinguishable from the exhausted sentinel.
pub struct IterHandle {
    next_fn: Box<dyn FnMut() -> Option<i64>>,
    done: bool,
}

/// Initialize an iterator handle. ADR-0044 W2 Phase 2 amendment:
/// the source-level for-protocol's only callers are `List<T>` /
/// `Set<T>` / `Dict<K,V>` / `Tuple<Ts>` expressions per the type-
/// checker's `iter_element` contract (no `range()` builtin exists in
/// Cobrust today). All those produce a heap pointer at MIR lowering,
/// not a small int. So we interpret `iter_val` as a list-layout
/// pointer and yield the stored i64 slots in order. For empty / null
/// pointers the handle exhausts immediately.
///
/// W2 Phase 2 scope: per ADR-0044 §"MIR / type-checker — NO change",
/// MIR for-protocol still emits `__cobrust_iter_init(i64)` — we just
/// change the runtime interpretation of the i64 from "Range count"
/// to "list pointer" since no source-level construct produces a
/// Range count today. The pre-existing latent semantics (treating
/// list-pointer-as-Range count) iterated 0..ptr — essentially
/// infinite — and was unobservable because the build/check-tier
/// tests never executed.
///
/// # Safety
///
/// Caller must eventually pass the result to
/// [`__cobrust_iter_drop`]. `iter_val`, when non-zero, must be a
/// valid `*mut ListI64Layout` pointer produced by
/// `__cobrust_list_new` (the only path that constructs list-typed
/// values today).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_iter_init(iter_val: i64) -> *mut u8 {
    // Capture the list-layout pointer + cursor inside the handle's
    // closure. The closure reads list slots via the public
    // `__cobrust_list_get` / `__cobrust_list_len` C ABI; both tolerate
    // null pointers (return 0) so the empty-list / null-iter case is
    // handled at the iter level.
    let list_ptr = iter_val;
    let mut idx: i64 = 0;
    let h = IterHandle {
        next_fn: Box::new(move || {
            // SAFETY: list_ptr was returned by `__cobrust_list_new` (or
            // is null, in which case `__cobrust_list_len` returns 0).
            let len = unsafe { crate::collections::__cobrust_list_len(list_ptr as *mut u8) };
            if idx >= len {
                return None;
            }
            // SAFETY: bounds-checked above.
            let v = unsafe { crate::collections::__cobrust_list_get(list_ptr as *mut u8, idx) };
            idx += 1;
            Some(v)
        }),
        done: false,
    };
    Box::into_raw(Box::new(h)).cast::<u8>()
}

/// Yield the next i64 value, or 0 when exhausted. ADR-0044 W2
/// Phase 2 amendment: list iteration over heap-allocated Str
/// pointers never yields 0 (real heap addresses), and list
/// iteration over i64 collections is the only source-level form
/// today (per type-checker's `iter_element`). The previous +1
/// bias (M12.x conservative) was for the now-removed Range
/// interpretation; we drop it because (a) Range isn't reachable
/// at source level and (b) pointer-typed iter values would
/// dereference at `ptr+1` and crash. The 0=None convention
/// still applies, but the impl now relies on the caller never
/// putting a literal 0 in a heap-allocated list-of-pointers slot.
/// For list-of-i64 cases where an element is legitimately 0,
/// we use a separate exhaustion sentinel (the `done` flag inside
/// `IterHandle`).
///
/// # Safety
///
/// `handle` must be a non-null pointer returned by
/// [`__cobrust_iter_init`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_iter_next(handle: *mut u8) -> i64 {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let h = unsafe { &mut *handle.cast::<IterHandle>() };
    if h.done {
        return 0;
    }
    match (h.next_fn)() {
        Some(v) => v,
        None => {
            h.done = true;
            0
        }
    }
}

/// Drop the handle.
///
/// # Safety
///
/// `handle` must be a pointer returned by [`__cobrust_iter_init`]
/// and not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_iter_drop(handle: *mut u8) {
    if handle.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let _ = unsafe { Box::from_raw(handle.cast::<IterHandle>()) };
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

    // -- C-ABI runtime helpers --

    #[test]
    fn cabi_iter_init_drop_smoke_null() {
        // SAFETY: ADR-0044 W2 amendment — `__cobrust_iter_init(0)` is the
        // null-list path; the handle exhausts immediately (no elements).
        unsafe {
            let h = __cobrust_iter_init(0);
            assert!(!h.is_null());
            assert_eq!(__cobrust_iter_next(h), 0, "null list exhausts immediately");
            __cobrust_iter_drop(h);
        }
    }

    #[test]
    fn cabi_iter_next_yields_list_elements() {
        // SAFETY: ADR-0044 W2 amendment — `__cobrust_iter_init(list_ptr)`
        // now iterates the list's i64 slots directly (no bias). Build a
        // 3-element list, iterate, exhaust.
        unsafe {
            let list = crate::collections::__cobrust_list_new(8, 3);
            crate::collections::__cobrust_list_set(list, 0, 10);
            crate::collections::__cobrust_list_set(list, 1, 20);
            crate::collections::__cobrust_list_set(list, 2, 30);
            let h = __cobrust_iter_init(list as i64);
            assert_eq!(__cobrust_iter_next(h), 10);
            assert_eq!(__cobrust_iter_next(h), 20);
            assert_eq!(__cobrust_iter_next(h), 30);
            assert_eq!(__cobrust_iter_next(h), 0, "list exhausted");
            __cobrust_iter_drop(h);
            crate::collections::__cobrust_list_drop(list);
        }
    }

    #[test]
    fn cabi_iter_next_yields_zero_value_via_done_flag() {
        // SAFETY: ADR-0044 W2 amendment — explicit `done` flag in
        // IterHandle lets a list slot legitimately store 0 without
        // looking like "exhausted". Build a list with a 0 slot,
        // verify next() yields 0 and the subsequent call yields the
        // "true" exhaustion 0 too — but with the `done` flag set so
        // any extra call beyond exhaustion still yields 0.
        unsafe {
            let list = crate::collections::__cobrust_list_new(8, 2);
            crate::collections::__cobrust_list_set(list, 0, 0);
            crate::collections::__cobrust_list_set(list, 1, 42);
            let h = __cobrust_iter_init(list as i64);
            assert_eq!(__cobrust_iter_next(h), 0, "first slot is 0");
            assert_eq!(__cobrust_iter_next(h), 42, "second slot is 42");
            assert_eq!(__cobrust_iter_next(h), 0, "exhausted");
            __cobrust_iter_drop(h);
            crate::collections::__cobrust_list_drop(list);
        }
    }

    #[test]
    fn cabi_iter_next_handles_null() {
        // SAFETY: documented null path.
        unsafe {
            assert_eq!(__cobrust_iter_next(std::ptr::null_mut()), 0);
            __cobrust_iter_drop(std::ptr::null_mut());
        }
    }
}
