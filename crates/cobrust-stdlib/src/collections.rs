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

// =====================================================================
// C-ABI runtime helpers for codegen Aggregate lowering (ADR-0027 §1)
// =====================================================================
//
// The Cranelift backend's `Rvalue::Aggregate` lowering emits calls
// to these symbols. The signatures mirror the table in ADR-0027 §1:
//   __cobrust_list_new(elem_size, len) -> *mut ListLayout
//   __cobrust_list_set(list, i, v_i64)
//   __cobrust_list_get(list, i) -> i64
//   __cobrust_list_len(list) -> i64
//   __cobrust_list_drop(list)
//
// At M12.x the storage element type is fixed at i64 (matches the
// codegen integer width); Phase F widens to per-type elem_size
// dispatch.

#[repr(C)]
struct ListI64Layout {
    items: *mut i64,
    len: i64,
    cap: i64,
}

/// Allocate a new `List<i64>` with reserved capacity for `len`
/// elements. Returns an opaque pointer the codegen passes to
/// `__cobrust_list_set` and `__cobrust_list_get`.
///
/// # Safety
///
/// The caller must eventually pass the returned pointer to
/// [`__cobrust_list_drop`] exactly once. `_elem_size` is reserved
/// for Phase F per-type dispatch; M12.x ignores it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_new(_elem_size: i64, len: i64) -> *mut u8 {
    let cap = len.max(0);
    let layout = Box::new(ListI64Layout {
        items: if cap == 0 {
            std::ptr::null_mut()
        } else {
            // SAFETY: always-zero initialised + 8-byte aligned.
            let l = std::alloc::Layout::array::<i64>(cap as usize).expect("layout");
            // SAFETY: layout valid + non-zero.
            let p = unsafe { std::alloc::alloc_zeroed(l) }.cast::<i64>();
            if p.is_null() {
                std::alloc::handle_alloc_error(l);
            }
            p
        },
        len: cap,
        cap,
    });
    Box::into_raw(layout).cast::<u8>()
}

/// Set `list[i] = v`. M12.x semantics: `i` must be in
/// `[0, list.len())`; out-of-bounds writes are silently dropped.
///
/// # Safety
///
/// `list` must be a non-null pointer returned by
/// [`__cobrust_list_new`] and not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_set(list: *mut u8, i: i64, v: i64) {
    if list.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &mut *list.cast::<ListI64Layout>() };
    if i < 0 || i >= layout.len {
        return;
    }
    // SAFETY: bounds-checked above; items is non-null when len>0.
    unsafe {
        *layout.items.add(i as usize) = v;
    }
}

/// Read `list[i]`. Returns 0 on out-of-bounds (M12.x conservative).
///
/// # Safety
///
/// Same as [`__cobrust_list_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_get(list: *mut u8, i: i64) -> i64 {
    if list.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*list.cast::<ListI64Layout>() };
    if i < 0 || i >= layout.len {
        return 0;
    }
    // SAFETY: bounds-checked above.
    unsafe { *layout.items.add(i as usize) }
}

/// Read `list.len()`. Returns 0 for null.
///
/// # Safety
///
/// `list` must be non-null and a valid layout, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_len(list: *mut u8) -> i64 {
    if list.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*list.cast::<ListI64Layout>() };
    layout.len
}

/// Append `v` to `list`, growing capacity if needed (doubling
/// strategy). ADR-0041 §H6 prerequisite: comprehension MIR
/// desugaring needs an `append` runtime helper.
///
/// # Safety
///
/// `list` must be a non-null pointer returned by
/// [`__cobrust_list_new`] and not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_append(list: *mut u8, v: i64) {
    if list.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &mut *list.cast::<ListI64Layout>() };
    if layout.len >= layout.cap {
        // Grow: double capacity, minimum 4.
        let new_cap = if layout.cap == 0 { 4 } else { layout.cap * 2 };
        let new_layout = std::alloc::Layout::array::<i64>(new_cap as usize).expect("layout");
        // SAFETY: layout valid + non-zero.
        let new_items = unsafe { std::alloc::alloc_zeroed(new_layout) }.cast::<i64>();
        if new_items.is_null() {
            std::alloc::handle_alloc_error(new_layout);
        }
        if !layout.items.is_null() && layout.cap > 0 {
            // SAFETY: copy old items into new buffer; both non-overlapping
            // (we just allocated new). Old buffer was zero-initialised
            // and len ≤ cap items are valid.
            unsafe {
                std::ptr::copy_nonoverlapping(layout.items, new_items, layout.len as usize);
                let old_layout =
                    std::alloc::Layout::array::<i64>(layout.cap as usize).expect("layout");
                std::alloc::dealloc(layout.items.cast::<u8>(), old_layout);
            }
        }
        layout.items = new_items;
        layout.cap = new_cap;
    }
    // SAFETY: bounds-checked: len < cap by the grow path above.
    unsafe {
        *layout.items.add(layout.len as usize) = v;
    }
    layout.len += 1;
}

/// Drop a list (free items + free the layout box).
///
/// # Safety
///
/// `list` must be the pointer returned by [`__cobrust_list_new`]
/// and not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_drop(list: *mut u8) {
    if list.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let boxed = unsafe { Box::from_raw(list.cast::<ListI64Layout>()) };
    if !boxed.items.is_null() && boxed.cap > 0 {
        let l = std::alloc::Layout::array::<i64>(boxed.cap as usize).expect("layout");
        // SAFETY: items came from `alloc_zeroed` with the same layout.
        unsafe { std::alloc::dealloc(boxed.items.cast::<u8>(), l) };
    }
    drop(boxed);
}

/// Drop a list whose i64 slots store owned pointer values. Iterates
/// each slot, casts it to `*mut u8`, calls `elem_drop_fn(slot)`, then
/// frees the list container.
///
/// ADR-0050c §"Phase 3": this is the codegen-emitted drop call for
/// `list[str]` and `list[list[T]]` typed locals. The element-drop
/// function pointer is supplied by codegen based on the element
/// type:
///
/// - `list[str]` → `__cobrust_str_drop` per slot.
/// - `list[list[T]]` → a fn pointer that recursively calls
///   `__cobrust_list_drop_elems` with the inner element drop fn (or
///   `__cobrust_list_drop` for `list[i64]`).
///
/// # Safety
///
/// `list` must be the pointer returned by [`__cobrust_list_new`] and
/// not yet dropped, OR `list` may be NULL. `elem_drop_fn` must be a
/// valid C-ABI fn pointer that accepts an `*mut u8` slot value and
/// is safe to call once on each slot. Zero-valued slots (i64 0) are
/// skipped (mirrors the standard NULL-pointer convention).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_drop_elems(
    list: *mut u8,
    elem_drop_fn: unsafe extern "C" fn(*mut u8),
) {
    if list.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*list.cast::<ListI64Layout>() };
    let len = layout.len;
    if !layout.items.is_null() && len > 0 {
        for i in 0..len {
            // SAFETY: bounds-checked by `len`; items is non-null when len>0.
            let slot = unsafe { *layout.items.add(i as usize) };
            if slot != 0 {
                // SAFETY: caller-attestation — `elem_drop_fn` is valid.
                unsafe { elem_drop_fn(slot as *mut u8) };
            }
        }
    }
    // Free the list container itself.
    // SAFETY: list was non-null and from `__cobrust_list_new`.
    unsafe { __cobrust_list_drop(list) };
}

/// Returns 1 if the list is empty (`len == 0`), 0 otherwise. NULL is
/// treated as empty (mirrors `__cobrust_list_len(NULL) == 0`).
///
/// ADR-0050c §"Phase 6" / F5 §2.2 uniformity addendum: complements
/// the future `__cobrust_dict_is_empty` shim from ADR-0050d so users
/// have one canonical "is empty" predicate per collection. Honors
/// §2.2 implicit-truthy ban: `if list_is_empty(xs):` is the canonical
/// pattern; `if xs:` is rejected at type-check time.
///
/// # Safety
///
/// `list` must be a non-null pointer returned by
/// [`__cobrust_list_new`] and not yet dropped, OR `list` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_list_is_empty(list: *mut u8) -> i64 {
    if list.is_null() {
        return 1;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*list.cast::<ListI64Layout>() };
    i64::from(layout.len == 0)
}

// --- Dict<i64, i64> ---------------------------------------------------

#[repr(C)]
struct DictI64Layout {
    map: *mut std::collections::HashMap<i64, i64>,
}

/// Allocate a new `Dict<i64, i64>` with reserved capacity for `len`
/// entries.
///
/// # Safety
///
/// Caller must eventually pass the result to
/// [`__cobrust_dict_drop`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dict_new(_k_size: i64, _v_size: i64, len: i64) -> *mut u8 {
    let cap = len.max(0) as usize;
    let m: std::collections::HashMap<i64, i64> = std::collections::HashMap::with_capacity(cap);
    let layout = Box::new(DictI64Layout {
        map: Box::into_raw(Box::new(m)),
    });
    Box::into_raw(layout).cast::<u8>()
}

/// Insert / replace `dict[k] = v`.
///
/// # Safety
///
/// `dict` must be a non-null pointer returned by
/// [`__cobrust_dict_new`] and not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dict_set(dict: *mut u8, k: i64, v: i64) {
    if dict.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*dict.cast::<DictI64Layout>() };
    if layout.map.is_null() {
        return;
    }
    // SAFETY: map pointer is owned by the layout.
    let map = unsafe { &mut *layout.map };
    map.insert(k, v);
}

/// Read `dict[k]`. Returns 0 if absent.
///
/// # Safety
///
/// Same as [`__cobrust_dict_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dict_get(dict: *mut u8, k: i64) -> i64 {
    if dict.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*dict.cast::<DictI64Layout>() };
    if layout.map.is_null() {
        return 0;
    }
    // SAFETY: map pointer is owned.
    let map = unsafe { &*layout.map };
    map.get(&k).copied().unwrap_or(0)
}

/// Read `dict.len()`.
///
/// # Safety
///
/// Same as [`__cobrust_dict_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dict_len(dict: *mut u8) -> i64 {
    if dict.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*dict.cast::<DictI64Layout>() };
    if layout.map.is_null() {
        return 0;
    }
    // SAFETY: map pointer is owned.
    let map = unsafe { &*layout.map };
    map.len() as i64
}

/// Drop a dict (free entries + free the layout).
///
/// # Safety
///
/// Same as [`__cobrust_dict_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_dict_drop(dict: *mut u8) {
    if dict.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { Box::from_raw(dict.cast::<DictI64Layout>()) };
    if !layout.map.is_null() {
        // SAFETY: map pointer is owned.
        let _ = unsafe { Box::from_raw(layout.map) };
    }
    drop(layout);
}

// --- Set<i64> --------------------------------------------------------

#[repr(C)]
struct SetI64Layout {
    set: *mut std::collections::HashSet<i64>,
}

/// Allocate a new `Set<i64>` with reserved capacity for `len`
/// entries.
///
/// # Safety
///
/// Caller must eventually pass the result to
/// [`__cobrust_set_drop`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_set_new(_elem_size: i64, len: i64) -> *mut u8 {
    let cap = len.max(0) as usize;
    let s: std::collections::HashSet<i64> = std::collections::HashSet::with_capacity(cap);
    let layout = Box::new(SetI64Layout {
        set: Box::into_raw(Box::new(s)),
    });
    Box::into_raw(layout).cast::<u8>()
}

/// `set.insert(v)`. Idempotent for duplicates.
///
/// # Safety
///
/// `set` must be a non-null pointer returned by
/// [`__cobrust_set_new`] and not yet dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_set_insert(set: *mut u8, v: i64) {
    if set.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*set.cast::<SetI64Layout>() };
    if layout.set.is_null() {
        return;
    }
    // SAFETY: set pointer is owned.
    let s = unsafe { &mut *layout.set };
    s.insert(v);
}

/// `set.contains(v)`. Returns 0 / 1.
///
/// # Safety
///
/// Same as [`__cobrust_set_insert`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_set_contains(set: *mut u8, v: i64) -> i64 {
    if set.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*set.cast::<SetI64Layout>() };
    if layout.set.is_null() {
        return 0;
    }
    // SAFETY: set pointer is owned.
    let s = unsafe { &*layout.set };
    i64::from(s.contains(&v))
}

/// `set.len()`.
///
/// # Safety
///
/// Same as [`__cobrust_set_insert`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_set_len(set: *mut u8) -> i64 {
    if set.is_null() {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { &*set.cast::<SetI64Layout>() };
    if layout.set.is_null() {
        return 0;
    }
    // SAFETY: set pointer is owned.
    let s = unsafe { &*layout.set };
    s.len() as i64
}

/// Drop a set.
///
/// # Safety
///
/// Same as [`__cobrust_set_insert`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_set_drop(set: *mut u8) {
    if set.is_null() {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    let layout = unsafe { Box::from_raw(set.cast::<SetI64Layout>()) };
    if !layout.set.is_null() {
        // SAFETY: set pointer is owned.
        let _ = unsafe { Box::from_raw(layout.set) };
    }
    drop(layout);
}

// --- Tuple<i64, ...> -------------------------------------------------
// Tuple uses a flat struct backed by a heap allocation of N i64
// slots. M12.x is uniform-element only (matches Aggregate kind shape
// in MIR); Phase F widens to per-element typing.

/// Allocate a tuple slot of `n` i64 elements.
///
/// # Safety
///
/// Caller must eventually pass result to [`__cobrust_tuple_drop`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tuple_new(n: i64) -> *mut u8 {
    if n <= 0 {
        return std::ptr::NonNull::<u8>::dangling().as_ptr();
    }
    let l = std::alloc::Layout::array::<i64>(n as usize).expect("layout");
    // SAFETY: layout valid + non-zero.
    let p = unsafe { std::alloc::alloc_zeroed(l) };
    if p.is_null() {
        std::alloc::handle_alloc_error(l);
    }
    p
}

/// Set tuple slot `i` to `v`.
///
/// # Safety
///
/// `tup` must be a non-null pointer returned by
/// [`__cobrust_tuple_new`] with size at least `i+1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tuple_set(tup: *mut u8, i: i64, v: i64) {
    if tup.is_null() || i < 0 {
        return;
    }
    // SAFETY: caller-attestation per `# Safety`.
    unsafe {
        *tup.cast::<i64>().add(i as usize) = v;
    }
}

/// Read tuple slot `i`.
///
/// # Safety
///
/// Same as [`__cobrust_tuple_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tuple_get(tup: *mut u8, i: i64) -> i64 {
    if tup.is_null() || i < 0 {
        return 0;
    }
    // SAFETY: caller-attestation per `# Safety`.
    unsafe { *tup.cast::<i64>().add(i as usize) }
}

/// Drop a tuple.
///
/// # Safety
///
/// Same as [`__cobrust_tuple_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_tuple_drop(tup: *mut u8, n: i64) {
    if tup.is_null() || n <= 0 {
        return;
    }
    let l = std::alloc::Layout::array::<i64>(n as usize).expect("layout");
    // SAFETY: tup came from alloc_zeroed with same layout.
    unsafe { std::alloc::dealloc(tup, l) };
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

    // -- C-ABI runtime helpers (M12.x ADR-0027 §1) -----------------

    #[test]
    fn cabi_list_new_set_get_drop() {
        // SAFETY: documented contract.
        unsafe {
            let l = __cobrust_list_new(8, 3);
            assert!(!l.is_null());
            __cobrust_list_set(l, 0, 100);
            __cobrust_list_set(l, 1, 200);
            __cobrust_list_set(l, 2, 300);
            assert_eq!(__cobrust_list_get(l, 0), 100);
            assert_eq!(__cobrust_list_get(l, 1), 200);
            assert_eq!(__cobrust_list_get(l, 2), 300);
            assert_eq!(__cobrust_list_len(l), 3);
            __cobrust_list_drop(l);
        }
    }

    #[test]
    fn cabi_list_zero_len_no_alloc() {
        // SAFETY: contract.
        unsafe {
            let l = __cobrust_list_new(8, 0);
            assert!(!l.is_null());
            assert_eq!(__cobrust_list_len(l), 0);
            __cobrust_list_drop(l);
        }
    }

    #[test]
    fn cabi_list_get_out_of_bounds_returns_zero() {
        // SAFETY: contract.
        unsafe {
            let l = __cobrust_list_new(8, 1);
            __cobrust_list_set(l, 0, 42);
            assert_eq!(__cobrust_list_get(l, 99), 0);
            __cobrust_list_drop(l);
        }
    }

    #[test]
    fn cabi_list_handles_null() {
        // SAFETY: documented null-arg path.
        unsafe {
            __cobrust_list_set(std::ptr::null_mut(), 0, 0);
            assert_eq!(__cobrust_list_get(std::ptr::null_mut(), 0), 0);
            assert_eq!(__cobrust_list_len(std::ptr::null_mut()), 0);
            __cobrust_list_drop(std::ptr::null_mut());
        }
    }

    #[test]
    fn cabi_dict_new_set_get_drop() {
        // SAFETY: contract.
        unsafe {
            let d = __cobrust_dict_new(8, 8, 4);
            assert!(!d.is_null());
            __cobrust_dict_set(d, 1, 10);
            __cobrust_dict_set(d, 2, 20);
            __cobrust_dict_set(d, 3, 30);
            assert_eq!(__cobrust_dict_get(d, 1), 10);
            assert_eq!(__cobrust_dict_get(d, 2), 20);
            assert_eq!(__cobrust_dict_get(d, 3), 30);
            assert_eq!(__cobrust_dict_len(d), 3);
            __cobrust_dict_drop(d);
        }
    }

    #[test]
    fn cabi_dict_replace_value() {
        // SAFETY: contract.
        unsafe {
            let d = __cobrust_dict_new(8, 8, 1);
            __cobrust_dict_set(d, 1, 10);
            __cobrust_dict_set(d, 1, 99);
            assert_eq!(__cobrust_dict_get(d, 1), 99);
            assert_eq!(__cobrust_dict_len(d), 1);
            __cobrust_dict_drop(d);
        }
    }

    #[test]
    fn cabi_dict_get_missing_returns_zero() {
        // SAFETY: contract.
        unsafe {
            let d = __cobrust_dict_new(8, 8, 0);
            assert_eq!(__cobrust_dict_get(d, 42), 0);
            __cobrust_dict_drop(d);
        }
    }

    #[test]
    fn cabi_set_insert_dedups() {
        // SAFETY: contract.
        unsafe {
            let s = __cobrust_set_new(8, 4);
            __cobrust_set_insert(s, 1);
            __cobrust_set_insert(s, 2);
            __cobrust_set_insert(s, 1);
            assert_eq!(__cobrust_set_len(s), 2);
            assert_eq!(__cobrust_set_contains(s, 1), 1);
            assert_eq!(__cobrust_set_contains(s, 2), 1);
            assert_eq!(__cobrust_set_contains(s, 3), 0);
            __cobrust_set_drop(s);
        }
    }

    #[test]
    fn cabi_set_handles_null() {
        // SAFETY: documented null path.
        unsafe {
            __cobrust_set_insert(std::ptr::null_mut(), 1);
            assert_eq!(__cobrust_set_contains(std::ptr::null_mut(), 1), 0);
            assert_eq!(__cobrust_set_len(std::ptr::null_mut()), 0);
            __cobrust_set_drop(std::ptr::null_mut());
        }
    }

    #[test]
    fn cabi_tuple_new_set_get_drop() {
        // SAFETY: contract.
        unsafe {
            let t = __cobrust_tuple_new(3);
            __cobrust_tuple_set(t, 0, 11);
            __cobrust_tuple_set(t, 1, 22);
            __cobrust_tuple_set(t, 2, 33);
            assert_eq!(__cobrust_tuple_get(t, 0), 11);
            assert_eq!(__cobrust_tuple_get(t, 1), 22);
            assert_eq!(__cobrust_tuple_get(t, 2), 33);
            __cobrust_tuple_drop(t, 3);
        }
    }

    #[test]
    fn cabi_tuple_zero_size_no_alloc() {
        // SAFETY: contract — n<=0 returns dangling ptr.
        unsafe {
            let t = __cobrust_tuple_new(0);
            assert!(!t.is_null());
            __cobrust_tuple_drop(t, 0);
        }
    }
}
