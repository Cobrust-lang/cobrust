//! Cobrust-cb arena-form mirror of `cobrust_types` — ADR-0055a + ADR-0055b Phase H Wave-2.
//!
//! # Scope (F28 strict-separation — DEV impl, Wave-2)
//!
//! This crate ships the cb mirror of the Rust `Ty` universe under the
//! arena workaround (ADR-0055a §3). Contract types + corpus locked by
//! TEST at commit `2e7ccb2`; DEV (this commit) implements:
//!
//! - `ty_cb_arena_from_rust` — recursive Rust `Ty` → cb arena conversion.
//! - `record_from_pairs` / `fn_ty_arity` — surface-invariant utilities.
//! - `is_mutable_container` / `is_hashable` — predicates over arena handles.
//! - `display_ty` — byte-identical to Rust `impl Display for Ty`.
//! - `clone_into_arena` / `subst_var` / `free_vars` — arena-aware ports.
//! - `Canonicalize for TyEntry` — post-order traversal feeding the
//!   ADR-0055e parity harness (§3 5-namespace dense-pack contract).
//!
//! # Arena-form design (ADR-0055a §3)
//!
//! `Ty` in Rust uses recursive `Box<Ty>` and `Vec<Ty>`. Cobrust M2 does not
//! have recursive `enum` without arena indirection (Phase 7.5 deferred per
//! ADR-0055 §3.2). This crate represents the cb mirror under the arena
//! workaround:
//!
//! - `TyId` = `i64` — arena handle (Cobrust ints are M2-single-width `i64`
//!   per ADR-0006 §"Numeric").
//! - `TyArena` = dense-pack vec-arena bundling `Vec<TyEntry>` +
//!   `Vec<FnTyEntry>` + `Vec<RecordEntry>` (parallel arenas embedded;
//!   the `(TyId, TyArena)` return shape of `ty_cb_arena_from_rust` is
//!   the locked TEST contract — parallel arenas live inside per
//!   ADR-0055a §3 parallel-arenas paragraph).
//! - `TyEntry` = the `Ty` enum mirrored 1:1 with recursive children as
//!   `i64` handles instead of `Box<Ty>` / `Vec<Ty>`.
//!
//! # F34 symbol anchors
//!
//! - `TyEntry::Tuple` — list-of-TyId payload per ADR-0055a §3 table row 1.
//! - `TyEntry::Ref` — single TyId payload per ADR-0055a §3 table row 9.
//! - `TyArena::insert` — dense-pack push returning the new TyId handle.
//! - `ty_cb_arena_from_rust` — conversion bridge: post-order Rust→cb walk.
//! - `display_ty` — byte-equal to Rust `impl Display for Ty`.
//!
//! ## Re-export surface (mirrors `cobrust-types::lib.rs`)
//!
//! Every `pub use` in Rust `lib.rs` is reproduced here per ADR-0055b §4
//! risk 3 mitigation: Tier-2 ports (`0055c` `infer.rs`, `0055d` `check.rs`)
//! import from this crate with identical name shapes.
//!
//! ADR-0055b §9.4 doc mandate: agent docs in `docs/agent/modules/types-cb.md`.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
// Recursive arena dispatch generates large match arms; the variant-by-variant
// structure is the load-bearing parity contract per ADR-0055a §4 — refactoring
// to shorter helpers would obscure variant-name alignment with `ty.rs::Ty`.
#![allow(clippy::too_many_lines)]
// `TyEntry::Var(_)` (non-target) and the leaf-variant arms both return
// `src_id` unchanged; merging the arms would obscure the §4 "Var-target vs
// non-target" semantic distinction documented inline.
#![allow(clippy::match_same_arms)]

use cobrust_types::{Record as RustRecord, Ty};
use cobrust_types_parity::{Canonicalize, CanonicalKey, TyArena as ParityArena};

// =====================================================================
// Arena handle type
// =====================================================================

/// Arena handle for a `TyEntry` in [`TyArena`].
///
/// `i64` per ADR-0055a §2 + ADR-0006 §"Numeric" (Cobrust M2 single-width int).
/// Negative values are reserved as sentinels (e.g. `-1` = null handle);
/// valid handles are `>= 0`. DEV enforces this invariant in the impl.
pub type TyId = i64;

/// Arena handle for a [`FnTyEntry`] in `TyArena.fn_entries`.
pub type FnTyId = i64;

/// Arena handle for a [`RecordEntry`] in `TyArena.record_entries`.
pub type RecordId = i64;

// =====================================================================
// TyEntry — arena-form mirror of `cobrust_types::Ty`
// =====================================================================

/// Arena-form mirror of `cobrust_types::Ty`.
///
/// Recursive variants substitute `TyId` (arena handle) for `Box<Ty>` /
/// `Vec<Ty>` per ADR-0055a §3. Parallel-arena variants (`Fn`, `Record`)
/// carry a `FnTyId` / `RecordId` handle instead of an inline payload.
///
/// Every variant name is **identical** to the corresponding `Ty` variant
/// per ADR-0055a §4 invariant 1 ("identical name + identical payload shape
/// modulo arena-id substitution").
///
/// # Invariant: 1-tuple trailing comma
///
/// `TyEntry::Tuple` with a single element canonicalizes to `(T,)` display
/// form (trailing comma per ADR-0055a §4 Display parity + `ty.rs` Tuple arm).
/// `display_ty` emits the trailing comma for `items.len() == 1`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TyEntry {
    // ---- Copy scalar leaves (ADR-0055a §3 — no arena indirection needed) ----
    /// `bool`.
    Bool,
    /// Integer (M2 single-width `i64`).
    Int,
    /// `f64`.
    Float,
    /// Imaginary literal stub.
    Imag,
    /// `str`.
    Str,
    /// `bytes`.
    Bytes,
    /// Unit type.
    None,
    /// Bottom — `raise` + never-returning calls (ADR-0006 §"`Never` as bottom").
    Never,

    // ---- Composite variants — recursive children as arena handles ----
    /// Positional fixed-size tuple: `list[TyId]`.
    ///
    /// Arena-form of `Ty::Tuple(Vec<Ty>)`. Single-element tuple MUST
    /// display with trailing comma: `(T,)` per ADR-0055a §4.
    Tuple(Vec<TyId>),

    /// Homogeneous list `List[T]`: single `TyId` handle.
    List(TyId),

    /// Homogeneous set `Set[T]`: single `TyId` handle.
    Set(TyId),

    /// Homogeneous dict `Dict[K, V]`: `(key_TyId, val_TyId)`.
    Dict(TyId, TyId),

    /// Closed structural record: `RecordId` handle into parallel record arena.
    ///
    /// Parallel arena per ADR-0055a §3 paragraph on parallel arenas.
    Record(RecordId),

    /// Function type: `FnTyId` handle into parallel fn arena.
    ///
    /// Named-separator in FnTy display form: `(a: Int, b: Str) -> Bool`
    /// per ADR-0055a §4 + `ty.rs::FnTy` Display arm.
    Fn(FnTyId),

    /// User-declared ADT: `(AdtId-as-TyId, list[TyId])`.
    ///
    /// `AdtId` stored as `i64` per ADR-0055a §2 newtype-as-i64 rule.
    Adt(TyId, Vec<TyId>),

    /// Transparent type-alias application: `(AliasId-as-TyId, list[TyId])`.
    Alias(TyId, Vec<TyId>),

    /// Universally quantified type-parameter use: `GenericVar`-as-i64.
    Generic(TyId),

    /// Inference unknown: `VarId`-as-i64.
    Var(TyId),

    /// ADR-0052a Wave-1 — `&T` immutable shared borrow: single `TyId` handle.
    ///
    /// `Ref` is not hashable in Wave-1 per ADR-0052a (inherits into
    /// `is_hashable`). Display glyph: `&{inner}`.
    Ref(TyId),
}

// =====================================================================
// FnTyEntry — parallel arena entry for function types
// =====================================================================

/// Arena-form mirror of `cobrust_types::FnTy`.
///
/// Lives in the parallel `TyArena.fn_entries` arena rather than the main
/// `TyEntry` vec because its field structure (positional + named +
/// var-positional + var-keyword + return) does not fit the uniform
/// `TyEntry` shape per ADR-0055a §3 parallel-arenas rationale.
///
/// Named-param separator in display: `(a: Int, b: Str) -> Bool` per ADR-0055a §4.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnTyEntry {
    /// Positional param types as `TyId` handles.
    pub positional: Vec<TyId>,
    /// Named params as `(name, TyId)` pairs.
    pub named: Vec<(String, TyId)>,
    /// `*args` variadic positional (optional).
    pub var_positional: Option<TyId>,
    /// `**kwargs` variadic keyword (optional).
    pub var_keyword: Option<TyId>,
    /// Return type as `TyId` handle.
    pub return_ty: TyId,
}

impl FnTyEntry {
    /// Arity: `positional.len() + named.len()` per `ty.rs::FnTy::arity`.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.positional.len() + self.named.len()
    }
}

// =====================================================================
// RecordEntry — parallel arena entry for record types
// =====================================================================

/// Arena-form mirror of `cobrust_types::Record`.
///
/// Lives in the parallel `TyArena.record_entries` arena (parallel arenas
/// per ADR-0055a §3). Fields are `(name, TyId)` pairs sorted by name for
/// canonical equality per ADR-0006 §"Record canonicalization".
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordEntry {
    /// Sorted-by-name field pairs: `(field_name, value_TyId)`.
    ///
    /// Sorted insertion via `record_from_pairs` per ADR-0055a §4
    /// `Record::from_pairs` surface invariant.
    pub fields: Vec<(String, TyId)>,
}

// =====================================================================
// TyArena — dense-pack arena bundle for TyEntry + FnTyEntry + RecordEntry
// =====================================================================

/// Dense-pack arena for [`TyEntry`] values, with embedded parallel arenas
/// for [`FnTyEntry`] and [`RecordEntry`] per ADR-0055a §3.
///
/// `TyId` handles are `i64` indices into `entries`. Negative handles are
/// invalid sentinels.
///
/// The parallel `fn_entries` and `record_entries` vecs live inside this
/// struct so that the locked `ty_cb_arena_from_rust(&Ty) -> (TyId, TyArena)`
/// return shape carries the full conversion in one return value.
///
/// # F34 anchor: `TyArena::insert`
///
/// `insert` is the only `TyEntry` mutation site. All arena construction
/// flows through it. `subst_var` and `clone_into_arena` both call `insert`
/// for fresh handles. `insert_fn` + `insert_record` mirror the API for the
/// parallel arenas.
#[derive(Clone, Debug, Default)]
pub struct TyArena {
    /// Dense-pack `TyEntry` entries; index = TyId handle.
    pub entries: Vec<TyEntry>,
    /// Dense-pack `FnTyEntry` entries; index = FnTyId handle.
    pub fn_entries: Vec<FnTyEntry>,
    /// Dense-pack `RecordEntry` entries; index = RecordId handle.
    pub record_entries: Vec<RecordEntry>,
}

impl TyArena {
    /// Create a fresh empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a `TyEntry` and return its fresh `TyId` handle.
    ///
    /// The returned handle is always `>= 0`. Panics if `entries.len()`
    /// exceeds `i64::MAX` (unreachable in practice).
    pub fn insert(&mut self, entry: TyEntry) -> TyId {
        let id = i64::try_from(self.entries.len())
            .expect("TyArena overflow: entries.len() > i64::MAX");
        self.entries.push(entry);
        id
    }

    /// Insert a `FnTyEntry` into the parallel fn arena and return its `FnTyId`.
    pub fn insert_fn(&mut self, entry: FnTyEntry) -> FnTyId {
        let id =
            i64::try_from(self.fn_entries.len()).expect("TyArena overflow: fn_entries > i64::MAX");
        self.fn_entries.push(entry);
        id
    }

    /// Insert a `RecordEntry` into the parallel record arena and return its `RecordId`.
    pub fn insert_record(&mut self, entry: RecordEntry) -> RecordId {
        let id = i64::try_from(self.record_entries.len())
            .expect("TyArena overflow: record_entries > i64::MAX");
        self.record_entries.push(entry);
        id
    }

    /// Look up a `TyEntry` by handle.
    ///
    /// Panics if `id < 0` or `id >= entries.len()` (dangling handle).
    #[must_use]
    pub fn lookup(&self, id: TyId) -> &TyEntry {
        let idx = usize::try_from(id).expect("TyArena::lookup: negative TyId");
        &self.entries[idx]
    }

    /// Look up a `FnTyEntry` by handle.
    #[must_use]
    pub fn lookup_fn(&self, id: FnTyId) -> &FnTyEntry {
        let idx = usize::try_from(id).expect("TyArena::lookup_fn: negative FnTyId");
        &self.fn_entries[idx]
    }

    /// Look up a `RecordEntry` by handle.
    #[must_use]
    pub fn lookup_record(&self, id: RecordId) -> &RecordEntry {
        let idx = usize::try_from(id).expect("TyArena::lookup_record: negative RecordId");
        &self.record_entries[idx]
    }
}

// =====================================================================
// FnTyArena + RecordArena — standalone parallel-arena wrappers (legacy API)
// =====================================================================

/// Standalone parallel arena for [`FnTyEntry`] values.
///
/// Provided for API compatibility with the locked `display_parity.rs`
/// helper signature `display_ty(&TyArena, &FnTyArena, &RecordArena, TyId)`.
/// In normal use, the `FnTyEntry` entries live inside `TyArena.fn_entries`;
/// this standalone struct is a thin wrapper for the helper signature only.
#[derive(Clone, Debug, Default)]
pub struct FnTyArena {
    pub entries: Vec<FnTyEntry>,
}

impl FnTyArena {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a `FnTyEntry`; return its `FnTyId` handle.
    pub fn insert(&mut self, entry: FnTyEntry) -> FnTyId {
        let id = i64::try_from(self.entries.len()).expect("FnTyArena overflow");
        self.entries.push(entry);
        id
    }

    /// Look up a `FnTyEntry` by handle.
    #[must_use]
    pub fn lookup(&self, id: FnTyId) -> &FnTyEntry {
        let idx = usize::try_from(id).expect("FnTyArena::lookup: negative FnTyId");
        &self.entries[idx]
    }
}

/// Standalone parallel arena for [`RecordEntry`] values.
///
/// See [`FnTyArena`] doc for the wrapper rationale.
#[derive(Clone, Debug, Default)]
pub struct RecordArena {
    pub entries: Vec<RecordEntry>,
}

impl RecordArena {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a `RecordEntry`; return its `RecordId` handle.
    pub fn insert(&mut self, entry: RecordEntry) -> RecordId {
        let id = i64::try_from(self.entries.len()).expect("RecordArena overflow");
        self.entries.push(entry);
        id
    }

    /// Look up a `RecordEntry` by handle.
    #[must_use]
    pub fn lookup(&self, id: RecordId) -> &RecordEntry {
        let idx = usize::try_from(id).expect("RecordArena::lookup: negative RecordId");
        &self.entries[idx]
    }
}

// =====================================================================
// ty_cb_arena_from_rust — recursive Rust Ty → cb arena conversion
// =====================================================================

/// Build a cb-form arena from a Rust `Ty` tree.
///
/// Returns `(root_TyId, arena)` where `root_TyId` is the handle for the
/// root entry corresponding to `rust_ty`. The conversion is a post-order
/// recursive traversal: children are inserted before parents so every
/// parent's `TyId` payloads point at already-existing entries (invariant:
/// for every inserted entry `e` at handle `h`, every `TyId` in `e.payload`
/// satisfies `0 <= id < h` — the "fresh handle is always > all referenced
/// handles" invariant per ADR-0055a §6 risk 1 mitigation + §4.1
/// arena-roundtrip property).
///
/// # F34 anchor: `ty_cb_arena_from_rust`
#[must_use]
pub fn ty_cb_arena_from_rust(rust: &Ty) -> (TyId, TyArena) {
    let mut arena = TyArena::new();
    let root = insert_rust_into_arena(rust, &mut arena);
    (root, arena)
}

/// Inner helper: insert a Rust `Ty` sub-tree into `arena` post-order.
///
/// Returns the new TyId for the root of the inserted sub-tree.
fn insert_rust_into_arena(rust: &Ty, arena: &mut TyArena) -> TyId {
    let entry = match rust {
        Ty::Bool => TyEntry::Bool,
        Ty::Int => TyEntry::Int,
        Ty::Float => TyEntry::Float,
        Ty::Imag => TyEntry::Imag,
        Ty::Str => TyEntry::Str,
        Ty::Bytes => TyEntry::Bytes,
        Ty::None => TyEntry::None,
        Ty::Never => TyEntry::Never,
        Ty::Tuple(items) => {
            let ids: Vec<TyId> = items
                .iter()
                .map(|t| insert_rust_into_arena(t, arena))
                .collect();
            TyEntry::Tuple(ids)
        }
        Ty::List(inner) => {
            let id = insert_rust_into_arena(inner, arena);
            TyEntry::List(id)
        }
        Ty::Set(inner) => {
            let id = insert_rust_into_arena(inner, arena);
            TyEntry::Set(id)
        }
        Ty::Dict(k, v) => {
            let k_id = insert_rust_into_arena(k, arena);
            let v_id = insert_rust_into_arena(v, arena);
            TyEntry::Dict(k_id, v_id)
        }
        Ty::Record(rec) => {
            // BTreeMap iteration is sorted by name already; preserve order.
            let fields: Vec<(String, TyId)> = rec
                .fields
                .iter()
                .map(|(name, t)| (name.clone(), insert_rust_into_arena(t, arena)))
                .collect();
            let rec_id = arena.insert_record(RecordEntry { fields });
            TyEntry::Record(rec_id)
        }
        Ty::Fn(fn_ty) => {
            let positional: Vec<TyId> = fn_ty
                .positional
                .iter()
                .map(|t| insert_rust_into_arena(t, arena))
                .collect();
            let named: Vec<(String, TyId)> = fn_ty
                .named
                .iter()
                .map(|(n, t)| (n.clone(), insert_rust_into_arena(t, arena)))
                .collect();
            let var_positional = fn_ty
                .var_positional
                .as_ref()
                .map(|t| insert_rust_into_arena(t, arena));
            let var_keyword = fn_ty
                .var_keyword
                .as_ref()
                .map(|t| insert_rust_into_arena(t, arena));
            let return_ty = insert_rust_into_arena(&fn_ty.return_ty, arena);
            let fn_id = arena.insert_fn(FnTyEntry {
                positional,
                named,
                var_positional,
                var_keyword,
                return_ty,
            });
            TyEntry::Fn(fn_id)
        }
        Ty::Adt(adt_id, args) => {
            let arg_ids: Vec<TyId> = args
                .iter()
                .map(|t| insert_rust_into_arena(t, arena))
                .collect();
            TyEntry::Adt(i64::from(adt_id.0), arg_ids)
        }
        Ty::Alias(alias_id, args) => {
            let arg_ids: Vec<TyId> = args
                .iter()
                .map(|t| insert_rust_into_arena(t, arena))
                .collect();
            TyEntry::Alias(i64::from(alias_id.0), arg_ids)
        }
        Ty::Generic(g) => TyEntry::Generic(i64::from(g.0)),
        Ty::Var(v) => TyEntry::Var(i64::from(v.0)),
        Ty::Ref(inner) => {
            let id = insert_rust_into_arena(inner, arena);
            TyEntry::Ref(id)
        }
    };
    arena.insert(entry)
}

// =====================================================================
// record_from_pairs — sorted insertion (ADR-0055a §4 surface invariant)
// =====================================================================

/// Insert sorted-by-name record pairs into the parallel record arena
/// inside `TyArena`.
///
/// Mirrors `cobrust_types::Record::from_pairs` per ADR-0055a §4
/// `Record::from_pairs` surface invariant. Returns the `RecordId` handle.
///
/// The provided pairs are sorted by `name` ascending before insertion to
/// match the Rust `BTreeMap` canonical ordering.
pub fn record_from_pairs(arena: &mut TyArena, mut pairs: Vec<(String, TyId)>) -> RecordId {
    // Sort by name to match Rust Record::from_pairs / BTreeMap ordering.
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    // Dedupe by name (last-wins to match BTreeMap::insert).
    let mut dedup: Vec<(String, TyId)> = Vec::with_capacity(pairs.len());
    for (name, id) in pairs {
        #[allow(clippy::collapsible_if)]
        if let Some(last) = dedup.last_mut() {
            if last.0 == name {
                last.1 = id;
                continue;
            }
        }
        dedup.push((name, id));
    }
    arena.insert_record(RecordEntry { fields: dedup })
}

// =====================================================================
// fn_ty_arity — arena-aware port of FnTy::arity
// =====================================================================

/// Arity of a `FnTyEntry` stored in `TyArena.fn_entries`.
///
/// Mirrors `ty.rs::FnTy::arity` per ADR-0055a §4 surface invariant.
#[must_use]
pub fn fn_ty_arity(arena: &TyArena, fn_id: FnTyId) -> i64 {
    let entry = arena.lookup_fn(fn_id);
    // arity = positional.len() + named.len() per FnTy::arity Rust impl.
    i64::try_from(entry.arity()).expect("fn_ty_arity: arity overflow")
}

// =====================================================================
// is_mutable_container — single-level arena lookup
// =====================================================================

/// `is_mutable_container` predicate over `TyArena`.
///
/// Mirrors `ty.rs::Ty::is_mutable_container` per ADR-0055a §4: returns
/// `true` iff the `TyEntry` at `id` is `List` | `Set` | `Dict`.
#[must_use]
pub fn is_mutable_container(arena: &TyArena, id: TyId) -> bool {
    matches!(
        arena.lookup(id),
        TyEntry::List(_) | TyEntry::Set(_) | TyEntry::Dict(_, _)
    )
}

// =====================================================================
// is_hashable — recursive arena walk
// =====================================================================

/// `is_hashable` predicate over `TyArena`.
///
/// Mirrors `ty.rs::Ty::is_hashable` per ADR-0055a §4 + ADR-0050d Decision 7A.
/// Tuple arm recurses through arena handles. `Ref` arm returns `false` per
/// ADR-0052a Wave-1.
#[must_use]
pub fn is_hashable(arena: &TyArena, id: TyId) -> bool {
    match arena.lookup(id) {
        TyEntry::Bool
        | TyEntry::Int
        | TyEntry::Str
        | TyEntry::Bytes
        | TyEntry::None
        | TyEntry::Never => true,
        TyEntry::Tuple(items) => {
            // Clone the handle list before recursing — we hold a borrow on
            // arena.lookup return; recursion needs &TyArena which is fine,
            // but the borrow checker only allows this if we don't hold an
            // active mut. Since lookup returns &TyEntry (shared), iter is OK.
            let ids: Vec<TyId> = items.clone();
            ids.iter().all(|child| is_hashable(arena, *child))
        }
        TyEntry::Float
        | TyEntry::Imag
        | TyEntry::List(_)
        | TyEntry::Set(_)
        | TyEntry::Dict(_, _)
        | TyEntry::Record(_)
        | TyEntry::Fn(_)
        | TyEntry::Adt(_, _)
        | TyEntry::Alias(_, _)
        | TyEntry::Generic(_)
        | TyEntry::Var(_)
        | TyEntry::Ref(_) => false,
    }
}

// =====================================================================
// display_ty — byte-identical to Rust impl Display for Ty
// =====================================================================

/// Recursive byte-equal display for arena-form types.
///
/// Mirrors `cobrust_types::Ty` Display arm-for-arm per ADR-0055a §4.
///
/// The `_fn_arena` and `_rec_arena` parameters are present for the locked
/// `display_parity.rs::assert_display` helper signature. The actual Fn /
/// Record payload data lives inside `arena.fn_entries` / `arena.record_entries`
/// (the arena returned by `ty_cb_arena_from_rust`); the standalone
/// `FnTyArena` / `RecordArena` parameters are only consulted as fallback
/// if `arena.fn_entries` / `arena.record_entries` are empty for the
/// requested handle.
#[must_use]
pub fn display_ty(
    arena: &TyArena,
    fn_arena: &FnTyArena,
    rec_arena: &RecordArena,
    id: TyId,
) -> String {
    let mut out = String::new();
    write_ty(arena, fn_arena, rec_arena, id, &mut out);
    out
}

/// Inner Display walk; writes into `out` to avoid intermediate allocations
/// per recursion level.
fn write_ty(
    arena: &TyArena,
    fn_arena: &FnTyArena,
    rec_arena: &RecordArena,
    id: TyId,
    out: &mut String,
) {
    use std::fmt::Write;

    let entry = arena.lookup(id);
    match entry {
        TyEntry::Bool => out.push_str("bool"),
        TyEntry::Int => out.push_str("i64"),
        TyEntry::Float => out.push_str("f64"),
        TyEntry::Imag => out.push_str("imag"),
        TyEntry::Str => out.push_str("str"),
        TyEntry::Bytes => out.push_str("bytes"),
        TyEntry::None => out.push_str("None"),
        TyEntry::Never => out.push_str("Never"),
        TyEntry::Tuple(items) => {
            let items = items.clone();
            out.push('(');
            for (i, child) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_ty(arena, fn_arena, rec_arena, *child, out);
            }
            // 1-tuple trailing comma — see ADR-0055a §4 invariant.
            if items.len() == 1 {
                out.push(',');
            }
            out.push(')');
        }
        TyEntry::List(inner) => {
            let child = *inner;
            out.push_str("List[");
            write_ty(arena, fn_arena, rec_arena, child, out);
            out.push(']');
        }
        TyEntry::Set(inner) => {
            let child = *inner;
            out.push_str("Set[");
            write_ty(arena, fn_arena, rec_arena, child, out);
            out.push(']');
        }
        TyEntry::Dict(k, v) => {
            let (k_id, v_id) = (*k, *v);
            out.push_str("Dict[");
            write_ty(arena, fn_arena, rec_arena, k_id, out);
            out.push_str(", ");
            write_ty(arena, fn_arena, rec_arena, v_id, out);
            out.push(']');
        }
        TyEntry::Record(rec_id) => {
            let rec_id = *rec_id;
            let rec_entry = lookup_record(arena, rec_arena, rec_id);
            out.push('{');
            for (i, (name, child)) in rec_entry.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write!(out, "{name}: ").expect("write to String");
                write_ty(arena, fn_arena, rec_arena, *child, out);
            }
            out.push('}');
        }
        TyEntry::Fn(fn_id) => {
            let fn_id = *fn_id;
            let fn_entry = lookup_fn(arena, fn_arena, fn_id);
            // Clone to drop the borrow on arena before the recursive
            // write_ty calls (which take &TyArena — shared).
            let positional = fn_entry.positional.clone();
            let named = fn_entry.named.clone();
            let return_ty = fn_entry.return_ty;
            out.push('(');
            for (i, child) in positional.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_ty(arena, fn_arena, rec_arena, *child, out);
            }
            for (i, (n, child)) in named.iter().enumerate() {
                // Rust arm: prepend ", " if positional non-empty OR not the first named.
                if i > 0 || !positional.is_empty() {
                    out.push_str(", ");
                }
                write!(out, "{n}: ").expect("write to String");
                write_ty(arena, fn_arena, rec_arena, *child, out);
            }
            out.push_str(") -> ");
            write_ty(arena, fn_arena, rec_arena, return_ty, out);
        }
        TyEntry::Adt(adt_id, args) => {
            let adt_id = *adt_id;
            let args = args.clone();
            write!(out, "Adt#{adt_id}").expect("write to String");
            if !args.is_empty() {
                out.push('[');
                for (i, child) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_ty(arena, fn_arena, rec_arena, *child, out);
                }
                out.push(']');
            }
        }
        TyEntry::Alias(alias_id, args) => {
            let alias_id = *alias_id;
            let args = args.clone();
            write!(out, "Alias#{alias_id}").expect("write to String");
            if !args.is_empty() {
                out.push('[');
                for (i, child) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_ty(arena, fn_arena, rec_arena, *child, out);
                }
                out.push(']');
            }
        }
        TyEntry::Generic(g) => {
            write!(out, "T{g}").expect("write to String");
        }
        TyEntry::Var(v) => {
            write!(out, "?{v}").expect("write to String");
        }
        TyEntry::Ref(inner) => {
            let child = *inner;
            out.push('&');
            write_ty(arena, fn_arena, rec_arena, child, out);
        }
    }
}

/// Look up a FnTyEntry preferring `arena.fn_entries`; fall back to
/// the standalone `fn_arena` wrapper if `arena.fn_entries` is empty.
fn lookup_fn<'a>(arena: &'a TyArena, fn_arena: &'a FnTyArena, id: FnTyId) -> &'a FnTyEntry {
    if arena.fn_entries.is_empty() {
        fn_arena.lookup(id)
    } else {
        arena.lookup_fn(id)
    }
}

/// Look up a RecordEntry preferring `arena.record_entries`; fall back
/// to the standalone `rec_arena` wrapper if `arena.record_entries` is empty.
fn lookup_record<'a>(
    arena: &'a TyArena,
    rec_arena: &'a RecordArena,
    id: RecordId,
) -> &'a RecordEntry {
    if arena.record_entries.is_empty() {
        rec_arena.lookup(id)
    } else {
        arena.lookup_record(id)
    }
}

// =====================================================================
// clone_into_arena — cross-arena deep copy
// =====================================================================

/// Cross-arena clone utility.
///
/// Deep-copies the sub-tree rooted at `src_id` from `src_arena` into
/// `dst_arena`. Returns the new handle in `dst_arena`. See ADR-0055a §6
/// risk 3 (clone semantics under arena).
pub fn clone_into_arena(src_arena: &TyArena, src_id: TyId, dst_arena: &mut TyArena) -> TyId {
    // Recursive deep clone: lookup source entry, recurse on each TyId
    // payload (mapping through src → dst), then insert the rebuilt entry.
    let src_entry = src_arena.lookup(src_id).clone();
    let new_entry = match src_entry {
        TyEntry::Bool => TyEntry::Bool,
        TyEntry::Int => TyEntry::Int,
        TyEntry::Float => TyEntry::Float,
        TyEntry::Imag => TyEntry::Imag,
        TyEntry::Str => TyEntry::Str,
        TyEntry::Bytes => TyEntry::Bytes,
        TyEntry::None => TyEntry::None,
        TyEntry::Never => TyEntry::Never,
        TyEntry::Tuple(items) => {
            let mapped: Vec<TyId> = items
                .iter()
                .map(|child| clone_into_arena(src_arena, *child, dst_arena))
                .collect();
            TyEntry::Tuple(mapped)
        }
        TyEntry::List(inner) => {
            let mapped = clone_into_arena(src_arena, inner, dst_arena);
            TyEntry::List(mapped)
        }
        TyEntry::Set(inner) => {
            let mapped = clone_into_arena(src_arena, inner, dst_arena);
            TyEntry::Set(mapped)
        }
        TyEntry::Dict(k, v) => {
            let mk = clone_into_arena(src_arena, k, dst_arena);
            let mv = clone_into_arena(src_arena, v, dst_arena);
            TyEntry::Dict(mk, mv)
        }
        TyEntry::Record(rid) => {
            let rec_entry = src_arena.lookup_record(rid).clone();
            let fields: Vec<(String, TyId)> = rec_entry
                .fields
                .into_iter()
                .map(|(name, child)| (name, clone_into_arena(src_arena, child, dst_arena)))
                .collect();
            let new_rid = dst_arena.insert_record(RecordEntry { fields });
            TyEntry::Record(new_rid)
        }
        TyEntry::Fn(fid) => {
            let fn_entry = src_arena.lookup_fn(fid).clone();
            let positional: Vec<TyId> = fn_entry
                .positional
                .iter()
                .map(|c| clone_into_arena(src_arena, *c, dst_arena))
                .collect();
            let named: Vec<(String, TyId)> = fn_entry
                .named
                .into_iter()
                .map(|(n, c)| (n, clone_into_arena(src_arena, c, dst_arena)))
                .collect();
            let var_positional = fn_entry
                .var_positional
                .map(|c| clone_into_arena(src_arena, c, dst_arena));
            let var_keyword = fn_entry
                .var_keyword
                .map(|c| clone_into_arena(src_arena, c, dst_arena));
            let return_ty = clone_into_arena(src_arena, fn_entry.return_ty, dst_arena);
            let new_fid = dst_arena.insert_fn(FnTyEntry {
                positional,
                named,
                var_positional,
                var_keyword,
                return_ty,
            });
            TyEntry::Fn(new_fid)
        }
        TyEntry::Adt(adt_id, args) => {
            let mapped: Vec<TyId> = args
                .iter()
                .map(|c| clone_into_arena(src_arena, *c, dst_arena))
                .collect();
            TyEntry::Adt(adt_id, mapped)
        }
        TyEntry::Alias(alias_id, args) => {
            let mapped: Vec<TyId> = args
                .iter()
                .map(|c| clone_into_arena(src_arena, *c, dst_arena))
                .collect();
            TyEntry::Alias(alias_id, mapped)
        }
        TyEntry::Generic(g) => TyEntry::Generic(g),
        TyEntry::Var(v) => TyEntry::Var(v),
        TyEntry::Ref(inner) => {
            let mapped = clone_into_arena(src_arena, inner, dst_arena);
            TyEntry::Ref(mapped)
        }
    };
    dst_arena.insert(new_entry)
}

// =====================================================================
// subst_var — arena-aware substitution
// =====================================================================

/// `subst_var` — substitute `VarId`-as-i64 throughout the arena sub-tree.
///
/// Returns a **fresh handle** (new entry inserted) for composite results;
/// returns a copy of an existing handle for leaf results that are not the
/// target var. Matches `ty.rs::Ty::subst_var` value semantics.
pub fn subst_var(
    arena: &mut TyArena,
    src_id: TyId,
    var_id: TyId,
    replacement_id: TyId,
) -> TyId {
    // Snapshot the src entry to drop the borrow before recursion.
    let entry = arena.lookup(src_id).clone();
    match entry {
        // The target Var: clone the replacement sub-tree (deep clone)
        // to preserve the fresh-handle invariant. The Rust impl returns
        // `replacement.clone()` directly; the arena form clones the
        // replacement sub-tree into the same arena and returns the new
        // root handle — semantically equivalent.
        TyEntry::Var(v) if v == var_id => {
            // Use a temporary swap: clone_into_arena requires &src + &mut dst,
            // but both are the same arena. Walk the sub-tree by hand using
            // a self-clone routine that reads + writes from the same arena.
            clone_within_arena(arena, replacement_id)
        }
        // Non-target Var: return src_id (no rebuild needed for leaves).
        TyEntry::Var(_) => src_id,
        // Leaves without payload: return src_id (immutable).
        TyEntry::Bool
        | TyEntry::Int
        | TyEntry::Float
        | TyEntry::Imag
        | TyEntry::Str
        | TyEntry::Bytes
        | TyEntry::None
        | TyEntry::Never
        | TyEntry::Generic(_) => src_id,
        // Composite variants: recurse + insert fresh.
        TyEntry::Tuple(items) => {
            let mapped: Vec<TyId> = items
                .iter()
                .map(|c| subst_var(arena, *c, var_id, replacement_id))
                .collect();
            arena.insert(TyEntry::Tuple(mapped))
        }
        TyEntry::List(inner) => {
            let mapped = subst_var(arena, inner, var_id, replacement_id);
            arena.insert(TyEntry::List(mapped))
        }
        TyEntry::Set(inner) => {
            let mapped = subst_var(arena, inner, var_id, replacement_id);
            arena.insert(TyEntry::Set(mapped))
        }
        TyEntry::Dict(k, v) => {
            let mk = subst_var(arena, k, var_id, replacement_id);
            let mv = subst_var(arena, v, var_id, replacement_id);
            arena.insert(TyEntry::Dict(mk, mv))
        }
        TyEntry::Record(rid) => {
            let rec_entry = arena.lookup_record(rid).clone();
            let fields: Vec<(String, TyId)> = rec_entry
                .fields
                .into_iter()
                .map(|(name, child)| (name, subst_var(arena, child, var_id, replacement_id)))
                .collect();
            let new_rid = arena.insert_record(RecordEntry { fields });
            arena.insert(TyEntry::Record(new_rid))
        }
        TyEntry::Fn(fid) => {
            let fn_entry = arena.lookup_fn(fid).clone();
            let positional: Vec<TyId> = fn_entry
                .positional
                .iter()
                .map(|c| subst_var(arena, *c, var_id, replacement_id))
                .collect();
            let named: Vec<(String, TyId)> = fn_entry
                .named
                .into_iter()
                .map(|(n, c)| (n, subst_var(arena, c, var_id, replacement_id)))
                .collect();
            let var_positional = fn_entry
                .var_positional
                .map(|c| subst_var(arena, c, var_id, replacement_id));
            let var_keyword = fn_entry
                .var_keyword
                .map(|c| subst_var(arena, c, var_id, replacement_id));
            let return_ty = subst_var(arena, fn_entry.return_ty, var_id, replacement_id);
            let new_fid = arena.insert_fn(FnTyEntry {
                positional,
                named,
                var_positional,
                var_keyword,
                return_ty,
            });
            arena.insert(TyEntry::Fn(new_fid))
        }
        TyEntry::Adt(adt_id, args) => {
            let mapped: Vec<TyId> = args
                .iter()
                .map(|c| subst_var(arena, *c, var_id, replacement_id))
                .collect();
            arena.insert(TyEntry::Adt(adt_id, mapped))
        }
        TyEntry::Alias(alias_id, args) => {
            let mapped: Vec<TyId> = args
                .iter()
                .map(|c| subst_var(arena, *c, var_id, replacement_id))
                .collect();
            arena.insert(TyEntry::Alias(alias_id, mapped))
        }
        TyEntry::Ref(inner) => {
            let mapped = subst_var(arena, inner, var_id, replacement_id);
            arena.insert(TyEntry::Ref(mapped))
        }
    }
}

/// Internal: deep-clone within a single arena.
///
/// Used by `subst_var` to materialize a copy of the replacement sub-tree
/// rooted at `src_id` and return a fresh root handle.
fn clone_within_arena(arena: &mut TyArena, src_id: TyId) -> TyId {
    let entry = arena.lookup(src_id).clone();
    let new_entry = match entry {
        TyEntry::Bool => TyEntry::Bool,
        TyEntry::Int => TyEntry::Int,
        TyEntry::Float => TyEntry::Float,
        TyEntry::Imag => TyEntry::Imag,
        TyEntry::Str => TyEntry::Str,
        TyEntry::Bytes => TyEntry::Bytes,
        TyEntry::None => TyEntry::None,
        TyEntry::Never => TyEntry::Never,
        TyEntry::Generic(g) => TyEntry::Generic(g),
        TyEntry::Var(v) => TyEntry::Var(v),
        TyEntry::Tuple(items) => {
            let mapped: Vec<TyId> = items
                .iter()
                .map(|c| clone_within_arena(arena, *c))
                .collect();
            TyEntry::Tuple(mapped)
        }
        TyEntry::List(inner) => {
            let mapped = clone_within_arena(arena, inner);
            TyEntry::List(mapped)
        }
        TyEntry::Set(inner) => {
            let mapped = clone_within_arena(arena, inner);
            TyEntry::Set(mapped)
        }
        TyEntry::Dict(k, v) => {
            let mk = clone_within_arena(arena, k);
            let mv = clone_within_arena(arena, v);
            TyEntry::Dict(mk, mv)
        }
        TyEntry::Record(rid) => {
            let rec_entry = arena.lookup_record(rid).clone();
            let fields: Vec<(String, TyId)> = rec_entry
                .fields
                .into_iter()
                .map(|(name, child)| (name, clone_within_arena(arena, child)))
                .collect();
            let new_rid = arena.insert_record(RecordEntry { fields });
            TyEntry::Record(new_rid)
        }
        TyEntry::Fn(fid) => {
            let fn_entry = arena.lookup_fn(fid).clone();
            let positional: Vec<TyId> = fn_entry
                .positional
                .iter()
                .map(|c| clone_within_arena(arena, *c))
                .collect();
            let named: Vec<(String, TyId)> = fn_entry
                .named
                .into_iter()
                .map(|(n, c)| (n, clone_within_arena(arena, c)))
                .collect();
            let var_positional = fn_entry.var_positional.map(|c| clone_within_arena(arena, c));
            let var_keyword = fn_entry.var_keyword.map(|c| clone_within_arena(arena, c));
            let return_ty = clone_within_arena(arena, fn_entry.return_ty);
            let new_fid = arena.insert_fn(FnTyEntry {
                positional,
                named,
                var_positional,
                var_keyword,
                return_ty,
            });
            TyEntry::Fn(new_fid)
        }
        TyEntry::Adt(adt_id, args) => {
            let mapped: Vec<TyId> = args.iter().map(|c| clone_within_arena(arena, *c)).collect();
            TyEntry::Adt(adt_id, mapped)
        }
        TyEntry::Alias(alias_id, args) => {
            let mapped: Vec<TyId> = args.iter().map(|c| clone_within_arena(arena, *c)).collect();
            TyEntry::Alias(alias_id, mapped)
        }
        TyEntry::Ref(inner) => {
            let mapped = clone_within_arena(arena, inner);
            TyEntry::Ref(mapped)
        }
    };
    arena.insert(new_entry)
}

// =====================================================================
// free_vars — deduplicated VarId-as-i64 walk
// =====================================================================

/// `free_vars` — deduplicated `VarId`-as-i64 list for the arena sub-tree.
///
/// Mirrors `ty.rs::Ty::free_vars` + `Ty::collect_vars` per ADR-0055a §4.
/// Output set is deduplicated (first-encounter order, same as Rust collect_vars).
#[must_use]
pub fn free_vars(arena: &TyArena, id: TyId) -> Vec<TyId> {
    let mut out = Vec::new();
    collect_vars(arena, id, &mut out);
    out
}

fn collect_vars(arena: &TyArena, id: TyId, out: &mut Vec<TyId>) {
    match arena.lookup(id) {
        TyEntry::Var(v) => {
            if !out.contains(v) {
                out.push(*v);
            }
        }
        TyEntry::Tuple(items) => {
            let items = items.clone();
            for child in items {
                collect_vars(arena, child, out);
            }
        }
        TyEntry::List(inner) | TyEntry::Set(inner) | TyEntry::Ref(inner) => {
            let inner = *inner;
            collect_vars(arena, inner, out);
        }
        TyEntry::Dict(k, v) => {
            let (kk, vv) = (*k, *v);
            collect_vars(arena, kk, out);
            collect_vars(arena, vv, out);
        }
        TyEntry::Record(rid) => {
            let rec_entry = arena.lookup_record(*rid).clone();
            for (_, child) in rec_entry.fields {
                collect_vars(arena, child, out);
            }
        }
        TyEntry::Fn(fid) => {
            let fn_entry = arena.lookup_fn(*fid).clone();
            for c in &fn_entry.positional {
                collect_vars(arena, *c, out);
            }
            for (_, c) in &fn_entry.named {
                collect_vars(arena, *c, out);
            }
            if let Some(c) = fn_entry.var_positional {
                collect_vars(arena, c, out);
            }
            if let Some(c) = fn_entry.var_keyword {
                collect_vars(arena, c, out);
            }
            collect_vars(arena, fn_entry.return_ty, out);
        }
        TyEntry::Adt(_, args) | TyEntry::Alias(_, args) => {
            let args = args.clone();
            for child in args {
                collect_vars(arena, child, out);
            }
        }
        _ => {}
    }
}

// =====================================================================
// Canonicalize for TyEntry — feeds the ADR-0055e parity harness
// =====================================================================

/// `Canonicalize` impl for the cb-side root `TyEntry`.
///
/// **API constraint**: `Canonicalize::canonicalize(&self, &mut ParityArena)`
/// receives `&TyEntry` only — it does NOT have access to the surrounding
/// `TyArena` (or `FnTyArena` / `RecordArena`). The cb-side parity entrypoint
/// is therefore designed for **leaf-shaped** `TyEntry` values: variants
/// with no `TyId` payload (`Bool`, `Int`, … `Never`) canonicalize directly;
/// composite variants emit a canonical key whose children are placeholder
/// leaves named after the arena handle (e.g. `"#7"` for `TyId(7)`).
///
/// This is the **harness contract**: parity tests construct cb arenas via
/// `ty_cb_arena_from_rust` and then drive parity through the `Ty` (Rust)
/// canonicalization path, NOT through `TyEntry::canonicalize`. The cb-side
/// `Canonicalize` impl exists as a trait-bound witness only — the actual
/// cb-side post-order traversal lives in `canonicalize_arena_root` below.
///
/// See ADR-0055e §3 amendment 2026-05-18 for the 5-namespace dense-pack
/// canonicalization protocol; `canonicalize_arena_root` implements it.
impl Canonicalize for TyEntry {
    fn canonicalize(&self, arena: &mut ParityArena) -> CanonicalKey {
        match self {
            TyEntry::Bool => CanonicalKey::leaf("Bool"),
            TyEntry::Int => CanonicalKey::leaf("Int"),
            TyEntry::Float => CanonicalKey::leaf("Float"),
            TyEntry::Imag => CanonicalKey::leaf("Imag"),
            TyEntry::Str => CanonicalKey::leaf("Str"),
            TyEntry::Bytes => CanonicalKey::leaf("Bytes"),
            TyEntry::None => CanonicalKey::leaf("None"),
            TyEntry::Never => CanonicalKey::leaf("Never"),
            // Composite variants: emit placeholder children naming the
            // arena handle. The full arena walk lives in
            // `canonicalize_arena_root`; this impl is for leaf-shaped
            // sanity coverage only.
            TyEntry::Tuple(ids) => CanonicalKey::node(
                "Tuple",
                ids.iter()
                    .map(|id| CanonicalKey::leaf(&format!("#{id}")))
                    .collect(),
            ),
            TyEntry::List(id) => {
                CanonicalKey::node("List", vec![CanonicalKey::leaf(&format!("#{id}"))])
            }
            TyEntry::Set(id) => {
                CanonicalKey::node("Set", vec![CanonicalKey::leaf(&format!("#{id}"))])
            }
            TyEntry::Dict(k, v) => CanonicalKey::node(
                "Dict",
                vec![
                    CanonicalKey::leaf(&format!("#{k}")),
                    CanonicalKey::leaf(&format!("#{v}")),
                ],
            ),
            TyEntry::Record(rid) => {
                let canon = arena.fresh_record_id();
                CanonicalKey::node(
                    &format!("Record#{canon}"),
                    vec![CanonicalKey::leaf(&format!("rid#{rid}"))],
                )
            }
            TyEntry::Fn(fid) => {
                let canon = arena.fresh_fn_ty_id();
                CanonicalKey::node(
                    &format!("Fn#{canon}"),
                    vec![CanonicalKey::leaf(&format!("fid#{fid}"))],
                )
            }
            TyEntry::Adt(id, args) => {
                use cobrust_types::AdtId;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let raw = AdtId(*id as u32);
                let canon = arena.adt_id(raw);
                CanonicalKey::node(
                    &format!("Adt#{canon}"),
                    args.iter()
                        .map(|id| CanonicalKey::leaf(&format!("#{id}")))
                        .collect(),
                )
            }
            TyEntry::Alias(id, args) => {
                use cobrust_types::AliasId;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let raw = AliasId(*id as u32);
                let canon = arena.alias_id(raw);
                CanonicalKey::node(
                    &format!("Alias#{canon}"),
                    args.iter()
                        .map(|id| CanonicalKey::leaf(&format!("#{id}")))
                        .collect(),
                )
            }
            TyEntry::Generic(g) => {
                use cobrust_types::GenericVar;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let raw = GenericVar(*g as u32);
                let canon = arena.generic_var(raw);
                CanonicalKey::leaf(&format!("Generic#{canon}"))
            }
            TyEntry::Var(v) => {
                use cobrust_types::VarId;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let raw = VarId(*v as u32);
                let canon = arena.var_id(raw);
                CanonicalKey::leaf(&format!("Var#{canon}"))
            }
            TyEntry::Ref(id) => CanonicalKey::node(
                "Ref",
                vec![CanonicalKey::leaf(&format!("#{id}"))],
            ),
        }
    }
}

/// Post-order arena-walking canonicalization for the cb-side root.
///
/// This is the **real** canonicalization the parity harness drives for cb
/// `(TyId, TyArena)` roots: it walks the arena tree post-order and emits a
/// `CanonicalKey` byte-identical to what `<Ty as Canonicalize>::canonicalize`
/// produces for the corresponding Rust `Ty`. The arena-id renaming is
/// performed in the supplied `ParityArena` (5-namespace dense-pack
/// allocators per ADR-0055e §3 amendment 2026-05-18).
#[must_use]
pub fn canonicalize_arena_root(
    arena: &TyArena,
    parity: &mut ParityArena,
    root: TyId,
) -> CanonicalKey {
    let entry = arena.lookup(root).clone();
    match entry {
        TyEntry::Bool => CanonicalKey::leaf("Bool"),
        TyEntry::Int => CanonicalKey::leaf("Int"),
        TyEntry::Float => CanonicalKey::leaf("Float"),
        TyEntry::Imag => CanonicalKey::leaf("Imag"),
        TyEntry::Str => CanonicalKey::leaf("Str"),
        TyEntry::Bytes => CanonicalKey::leaf("Bytes"),
        TyEntry::None => CanonicalKey::leaf("None"),
        TyEntry::Never => CanonicalKey::leaf("Never"),
        TyEntry::Tuple(items) => CanonicalKey::node(
            "Tuple",
            items
                .iter()
                .map(|c| canonicalize_arena_root(arena, parity, *c))
                .collect(),
        ),
        TyEntry::List(inner) => CanonicalKey::node(
            "List",
            vec![canonicalize_arena_root(arena, parity, inner)],
        ),
        TyEntry::Set(inner) => CanonicalKey::node(
            "Set",
            vec![canonicalize_arena_root(arena, parity, inner)],
        ),
        TyEntry::Dict(k, v) => CanonicalKey::node(
            "Dict",
            vec![
                canonicalize_arena_root(arena, parity, k),
                canonicalize_arena_root(arena, parity, v),
            ],
        ),
        TyEntry::Record(rid) => {
            let _rec_id = parity.fresh_record_id();
            let rec_entry = arena.lookup_record(rid).clone();
            let children: Vec<CanonicalKey> = rec_entry
                .fields
                .iter()
                .map(|(name, c)| {
                    CanonicalKey::node(
                        name.as_str(),
                        vec![canonicalize_arena_root(arena, parity, *c)],
                    )
                })
                .collect();
            CanonicalKey::node("Record", children)
        }
        TyEntry::Fn(fid) => {
            let _fn_id = parity.fresh_fn_ty_id();
            let fn_entry = arena.lookup_fn(fid).clone();
            let mut children: Vec<CanonicalKey> = fn_entry
                .positional
                .iter()
                .map(|c| canonicalize_arena_root(arena, parity, *c))
                .collect();
            for (name, c) in &fn_entry.named {
                children.push(CanonicalKey::node(
                    name.as_str(),
                    vec![canonicalize_arena_root(arena, parity, *c)],
                ));
            }
            if let Some(vp) = fn_entry.var_positional {
                children.push(CanonicalKey::node(
                    "*args",
                    vec![canonicalize_arena_root(arena, parity, vp)],
                ));
            }
            if let Some(vk) = fn_entry.var_keyword {
                children.push(CanonicalKey::node(
                    "**kwargs",
                    vec![canonicalize_arena_root(arena, parity, vk)],
                ));
            }
            children.push(CanonicalKey::node(
                "->",
                vec![canonicalize_arena_root(arena, parity, fn_entry.return_ty)],
            ));
            CanonicalKey::node("Fn", children)
        }
        TyEntry::Adt(adt_id, args) => {
            use cobrust_types::AdtId;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let raw = AdtId(adt_id as u32);
            let canon = parity.adt_id(raw);
            let children: Vec<CanonicalKey> = args
                .iter()
                .map(|c| canonicalize_arena_root(arena, parity, *c))
                .collect();
            CanonicalKey::node(&format!("Adt#{canon}"), children)
        }
        TyEntry::Alias(alias_id, args) => {
            use cobrust_types::AliasId;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let raw = AliasId(alias_id as u32);
            let canon = parity.alias_id(raw);
            let children: Vec<CanonicalKey> = args
                .iter()
                .map(|c| canonicalize_arena_root(arena, parity, *c))
                .collect();
            CanonicalKey::node(&format!("Alias#{canon}"), children)
        }
        TyEntry::Generic(g) => {
            use cobrust_types::GenericVar;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let raw = GenericVar(g as u32);
            let canon = parity.generic_var(raw);
            CanonicalKey::leaf(&format!("Generic#{canon}"))
        }
        TyEntry::Var(v) => {
            use cobrust_types::VarId;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let raw = VarId(v as u32);
            let canon = parity.var_id(raw);
            CanonicalKey::leaf(&format!("Var#{canon}"))
        }
        TyEntry::Ref(inner) => CanonicalKey::node(
            "Ref",
            vec![canonicalize_arena_root(arena, parity, inner)],
        ),
    }
}

// Silence unused-import warning — `RustRecord` is part of the doc surface
// (referenced in module docs) but not directly used in code.
#[allow(dead_code)]
fn _doc_anchor_record(_r: &RustRecord) {}

// =====================================================================
// 0055b: error_cb module + re-exports (ADR-0055b §4 re-export contract)
// =====================================================================

pub mod error_cb;

// Re-export mirrors: names preserved per ADR-0055b §4 re-export contract.
// Tier-2 (0055c infer.rs, 0055d check.rs) imports `use cobrust_types_cb::{TypeError, ...}`
// with the same shape as Rust `lib.rs`.
pub use error_cb::TypeErrorCb as TypeError;
