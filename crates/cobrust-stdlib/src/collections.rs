//! `std.collections` — `List<T>`, `Dict<K, V>`, `Set<T>`.
//!
//! ADR-0025 §"Public surface (binding)" pins the API. ADR-0019
//! §"M11 — Standard library" §"Modules" requires:
//!
//! > `std.collections` | `List<T>` / `Dict<K, V>` / `Set<T>` (with
//! > the constitution's "no implicit truthiness"); iteration via
//! > `mod:frontend`'s for-protocol
//!
//! The Rust shim wraps `Vec`, `HashMap`, and `HashSet` (per ADR-0012
//! "translate the surface, bind the core"). The Cobrust source-level
//! API:
//!
//! - `List<T>` — `len`, `is_empty`, `push`, `pop`, `get`, `iter`.
//! - `Dict<K, V>` — `len`, `is_empty`, `insert`, `get`, `remove`,
//!   `contains_key`, `iter`.
//! - `Set<T>` — `len`, `is_empty`, `insert`, `remove`, `contains`,
//!   `iter`.
//!
//! Constitution §2.2 binds "no implicit truthiness" — every
//! collection has `is_empty()`; users write `if list.is_empty()`,
//! never `if list`.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::runtime::Error;

// =====================================================================
// List<T>
// =====================================================================

/// Cobrust `List[T]` — a homogeneous, growable sequence.
///
/// Backed by `Vec<T>`. The Cobrust source-level API matches Python's
/// `list` for operations that exist in both (push = append, pop, etc.)
/// and uses Rust-idiomatic names (`is_empty`) elsewhere.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct List<T> {
    inner: Vec<T>,
}

impl<T> List<T> {
    /// Empty list.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Empty list with reserved capacity.
    pub fn with_capacity(n: usize) -> Self {
        Self {
            inner: Vec::with_capacity(n),
        }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True iff zero elements. Constitution §2.2 — explicit
    /// emptiness check; no implicit truthiness.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Append `value`.
    pub fn push(&mut self, value: T) {
        self.inner.push(value);
    }

    /// Remove + return the last element. `None` if empty.
    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    /// Get a reference to the element at `idx`. Returns `Err` rather
    /// than panicking — constitution §2.2 binds Result over panic
    /// for user-driven errors.
    pub fn get(&self, idx: usize) -> Result<&T, Error> {
        self.inner.get(idx).ok_or_else(|| {
            Error::out_of_bounds(format!(
                "index {idx} out of bounds for list of length {}",
                self.inner.len()
            ))
        })
    }

    /// Get a mutable reference to the element at `idx`.
    pub fn get_mut(&mut self, idx: usize) -> Result<&mut T, Error> {
        let n = self.inner.len();
        self.inner.get_mut(idx).ok_or_else(|| {
            Error::out_of_bounds(format!("index {idx} out of bounds for list of length {n}"))
        })
    }

    /// Iterator over references.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.inner.iter()
    }

    /// Mutable iterator.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.inner.iter_mut()
    }

    /// Drop every element.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Convert to a `Vec<T>` for callers that need direct access.
    pub fn into_vec(self) -> Vec<T> {
        self.inner
    }

    /// Construct from a `Vec<T>`.
    pub fn from_vec(v: Vec<T>) -> Self {
        Self { inner: v }
    }

    /// Insert `value` at `idx`, shifting later elements right.
    pub fn insert(&mut self, idx: usize, value: T) -> Result<(), Error> {
        if idx > self.inner.len() {
            return Err(Error::out_of_bounds(format!(
                "insert index {idx} out of bounds for list of length {}",
                self.inner.len()
            )));
        }
        self.inner.insert(idx, value);
        Ok(())
    }

    /// Remove + return the element at `idx`.
    pub fn remove(&mut self, idx: usize) -> Result<T, Error> {
        if idx >= self.inner.len() {
            return Err(Error::out_of_bounds(format!(
                "remove index {idx} out of bounds for list of length {}",
                self.inner.len()
            )));
        }
        Ok(self.inner.remove(idx))
    }
}

impl<T: Ord> List<T> {
    /// Sort in ascending order.
    pub fn sort(&mut self) {
        self.inner.sort();
    }
}

impl<T: PartialEq> List<T> {
    /// True if any element equals `target`.
    pub fn contains(&self, target: &T) -> bool {
        self.inner.contains(target)
    }
}

impl<T> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<T> FromIterator<T> for List<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}

// =====================================================================
// Dict<K, V>
// =====================================================================

/// Cobrust `Dict[K, V]` — a homogeneous mapping.
///
/// Backed by `HashMap<K, V>`. `K: Eq + Hash` is the constraint;
/// the Cobrust type checker enforces this at the source level
/// (M2 + M11.x).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Dict<K, V>
where
    K: Eq + Hash,
{
    inner: HashMap<K, V>,
}

impl<K, V> Dict<K, V>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            inner: HashMap::with_capacity(n),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Insert / replace. Returns the previous value if present.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.inner.insert(key, value)
    }

    pub fn get<Q: ?Sized>(&self, key: &Q) -> Result<&V, Error>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + std::fmt::Debug,
    {
        self.inner
            .get(key)
            .ok_or_else(|| Error::key_not_found(format!("{key:?}")))
    }

    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash,
    {
        self.inner.contains_key(key)
    }

    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash,
    {
        self.inner.remove(key)
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, K, V> {
        self.inner.iter()
    }

    pub fn keys(&self) -> std::collections::hash_map::Keys<'_, K, V> {
        self.inner.keys()
    }

    pub fn values(&self) -> std::collections::hash_map::Values<'_, K, V> {
        self.inner.values()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<K, V> FromIterator<(K, V)> for Dict<K, V>
where
    K: Eq + Hash,
{
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}

// =====================================================================
// Set<T>
// =====================================================================

/// Cobrust `Set[T]` — a homogeneous unordered collection of unique
/// elements. Backed by `HashSet<T>`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Set<T>
where
    T: Eq + Hash,
{
    inner: HashSet<T>,
}

impl<T> Set<T>
where
    T: Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            inner: HashSet::with_capacity(n),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Insert; returns `true` if the value was new.
    pub fn insert(&mut self, value: T) -> bool {
        self.inner.insert(value)
    }

    pub fn contains(&self, value: &T) -> bool {
        self.inner.contains(value)
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.inner.remove(value)
    }

    pub fn iter(&self) -> std::collections::hash_set::Iter<'_, T> {
        self.inner.iter()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T> FromIterator<T> for Set<T>
where
    T: Eq + Hash,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
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

    // --- List ---------------------------------------------------------------

    #[test]
    fn list_new_is_empty() {
        let l: List<i64> = List::new();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn list_push_pop() {
        let mut l = List::new();
        l.push(1);
        l.push(2);
        l.push(3);
        assert_eq!(l.len(), 3);
        assert!(!l.is_empty());
        assert_eq!(l.pop(), Some(3));
        assert_eq!(l.pop(), Some(2));
        assert_eq!(l.pop(), Some(1));
        assert_eq!(l.pop(), None);
    }

    #[test]
    fn list_get_in_bounds() {
        let mut l = List::new();
        l.push("a");
        assert_eq!(*l.get(0).unwrap(), "a");
    }

    #[test]
    fn list_get_out_of_bounds() {
        let l: List<i64> = List::new();
        let res = l.get(0);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().kind(),
            &crate::runtime::ErrorKind::OutOfBounds
        );
    }

    #[test]
    fn list_with_capacity_is_empty() {
        let l: List<i64> = List::with_capacity(64);
        assert!(l.is_empty());
    }

    #[test]
    fn list_iter() {
        let l: List<i64> = vec![1, 2, 3].into_iter().collect();
        let v: Vec<i64> = l.iter().copied().collect();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn list_into_iter() {
        let l: List<i64> = vec![10, 20].into_iter().collect();
        let v: Vec<i64> = l.into_iter().collect();
        assert_eq!(v, vec![10, 20]);
    }

    #[test]
    fn list_clear() {
        let mut l: List<i64> = vec![1, 2, 3].into_iter().collect();
        l.clear();
        assert!(l.is_empty());
    }

    #[test]
    fn list_sort_ascending() {
        let mut l: List<i64> = vec![3, 1, 2].into_iter().collect();
        l.sort();
        assert_eq!(l.into_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn list_contains() {
        let l: List<i64> = vec![1, 2, 3].into_iter().collect();
        assert!(l.contains(&2));
        assert!(!l.contains(&99));
    }

    #[test]
    fn list_insert_in_middle() {
        let mut l: List<i64> = vec![1, 3].into_iter().collect();
        l.insert(1, 2).unwrap();
        assert_eq!(l.into_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn list_insert_out_of_bounds() {
        let mut l: List<i64> = List::new();
        let res = l.insert(5, 1);
        assert!(res.is_err());
    }

    #[test]
    fn list_remove() {
        let mut l: List<i64> = vec![1, 2, 3].into_iter().collect();
        let removed = l.remove(1).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(l.into_vec(), vec![1, 3]);
    }

    #[test]
    fn list_remove_out_of_bounds() {
        let mut l: List<i64> = List::new();
        assert!(l.remove(0).is_err());
    }

    #[test]
    fn list_from_iter_collect() {
        let l: List<i64> = (1..=5).collect();
        assert_eq!(l.len(), 5);
    }

    #[test]
    fn list_default() {
        let l: List<i64> = List::default();
        assert!(l.is_empty());
    }

    // --- Dict ---------------------------------------------------------------

    #[test]
    fn dict_new_is_empty() {
        let d: Dict<&str, i64> = Dict::new();
        assert!(d.is_empty());
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn dict_insert_get() {
        let mut d: Dict<String, i64> = Dict::new();
        d.insert("a".into(), 1);
        d.insert("b".into(), 2);
        assert_eq!(*d.get("a").unwrap(), 1);
        assert_eq!(*d.get("b").unwrap(), 2);
    }

    #[test]
    fn dict_insert_replaces() {
        let mut d: Dict<String, i64> = Dict::new();
        let prev = d.insert("k".into(), 1);
        assert!(prev.is_none());
        let prev = d.insert("k".into(), 2);
        assert_eq!(prev, Some(1));
        assert_eq!(*d.get("k").unwrap(), 2);
    }

    #[test]
    fn dict_get_missing() {
        let d: Dict<String, i64> = Dict::new();
        let res = d.get("missing");
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().kind(),
            &crate::runtime::ErrorKind::KeyNotFound
        );
    }

    #[test]
    fn dict_contains_key() {
        let mut d: Dict<String, i64> = Dict::new();
        d.insert("k".into(), 1);
        assert!(d.contains_key("k"));
        assert!(!d.contains_key("nope"));
    }

    #[test]
    fn dict_remove() {
        let mut d: Dict<String, i64> = Dict::new();
        d.insert("k".into(), 5);
        let v = d.remove("k");
        assert_eq!(v, Some(5));
        assert!(!d.contains_key("k"));
    }

    #[test]
    fn dict_iter_count() {
        let d: Dict<String, i64> = vec![("a".to_string(), 1), ("b".to_string(), 2)]
            .into_iter()
            .collect();
        assert_eq!(d.iter().count(), 2);
    }

    #[test]
    fn dict_keys_values() {
        let d: Dict<String, i64> = vec![("a".to_string(), 1)].into_iter().collect();
        assert_eq!(d.keys().count(), 1);
        assert_eq!(d.values().count(), 1);
    }

    #[test]
    fn dict_with_capacity() {
        let d: Dict<String, i64> = Dict::with_capacity(8);
        assert!(d.is_empty());
    }

    #[test]
    fn dict_clear() {
        let mut d: Dict<String, i64> = vec![("a".to_string(), 1)].into_iter().collect();
        d.clear();
        assert!(d.is_empty());
    }

    // --- Set ----------------------------------------------------------------

    #[test]
    fn set_new_is_empty() {
        let s: Set<i64> = Set::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn set_insert_dedups() {
        let mut s: Set<i64> = Set::new();
        assert!(s.insert(1));
        assert!(!s.insert(1));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn set_contains() {
        let mut s: Set<i64> = Set::new();
        s.insert(1);
        assert!(s.contains(&1));
        assert!(!s.contains(&2));
    }

    #[test]
    fn set_remove() {
        let mut s: Set<i64> = Set::new();
        s.insert(1);
        assert!(s.remove(&1));
        assert!(!s.remove(&1));
    }

    #[test]
    fn set_iter() {
        let s: Set<i64> = (1..=3).collect();
        assert_eq!(s.iter().count(), 3);
    }

    #[test]
    fn set_with_capacity() {
        let s: Set<i64> = Set::with_capacity(8);
        assert!(s.is_empty());
    }

    #[test]
    fn set_clear() {
        let mut s: Set<i64> = (1..=3).collect();
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn collections_no_implicit_truthiness_via_is_empty() {
        // Constitution §2.2 binding: every collection has is_empty.
        let l: List<i64> = List::new();
        let d: Dict<String, i64> = Dict::new();
        let s: Set<i64> = Set::new();
        assert!(l.is_empty());
        assert!(d.is_empty());
        assert!(s.is_empty());
    }
}
