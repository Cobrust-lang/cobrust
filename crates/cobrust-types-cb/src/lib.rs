//! Cobrust-cb arena-form mirror of `cobrust_types` — ADR-0055a Phase H Wave-2.
//!
//! # Scope (F28 strict-separation — TEST only)
//!
//! This crate ships **contract types + test corpus only**. No `Canonicalize`
//! impl bodies; no `ty_cb_arena_from_rust` implementation. All impl
//! sites are `todo!()` stubs; DEV fills them in a subsequent Wave-2 sprint
//! per ADR-0055a §9.3 dispatch shape.
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
//! - `TyArena` = `Vec<TyEntry>` — dense-pack array; handle is the index.
//! - `TyEntry` = the `Ty` enum mirrored 1:1 with recursive children as
//!   `i64` handles instead of `Box<Ty>` / `Vec<Ty>`.
//! - `FnTyArena` / `RecordArena` — parallel arenas for `FnTy` / `Record`
//!   payloads whose field structure does not fit the uniform `TyEntry` shape.
//!
//! # F34 symbol anchors
//!
//! - `TyEntry::Tuple` — list-of-TyId payload per ADR-0055a §3 table row 1.
//! - `TyEntry::Ref` — single TyId payload per ADR-0055a §3 table row 9.
//! - `TyArena::insert` — dense-pack push returning the new TyId handle.
//! - `ty_cb_arena_from_rust` — conversion bridge stub (DEV fills).
//! - `parity_check` re-export — test corpus calls via this crate's surface.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
// F28: stubs suppress the workspace `todo = "warn"` pedantic lint in test scope.
#![allow(clippy::todo)]

use cobrust_types::Ty;
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

/// Arena handle for a [`FnTyEntry`] in [`FnTyArena`].
pub type FnTyId = i64;

/// Arena handle for a [`RecordEntry`] in [`RecordArena`].
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
/// DEV `display_ty` MUST emit the trailing comma for `items.len() == 1`.
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

    /// Closed structural record: `RecordId` handle into [`RecordArena`].
    ///
    /// Parallel arena per ADR-0055a §3 paragraph on parallel arenas.
    Record(RecordId),

    /// Function type: `FnTyId` handle into [`FnTyArena`].
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
    /// `is_hashable` stub below). Display glyph: `&{inner}`.
    Ref(TyId),
}

// =====================================================================
// FnTyEntry — parallel arena entry for function types
// =====================================================================

/// Arena-form mirror of `cobrust_types::FnTy`.
///
/// Lives in [`FnTyArena`] rather than `TyArena` because its field structure
/// (positional + named + var-positional + var-keyword + return) does not
/// fit the uniform `TyEntry` shape per ADR-0055a §3 parallel-arenas rationale.
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
/// Lives in [`RecordArena`] rather than `TyArena` (parallel arenas per
/// ADR-0055a §3). Fields are `(name, TyId)` pairs sorted by name for
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
// TyArena — dense-pack vec-arena for TyEntry
// =====================================================================

/// Dense-pack arena for [`TyEntry`] values.
///
/// `TyId` handles are `i64` indices into `entries`. `entries[0]` = handle 0,
/// `entries[1]` = handle 1, etc. Negative handles are invalid sentinels.
///
/// Mirrors `list[TyEntry]` from ADR-0055a §2 Cobrust-surface description.
///
/// # F34 anchor: `TyArena::insert`
///
/// `insert` is the only mutation site. All arena construction flows through it.
/// `subst_var` and `clone_into_arena` both call `insert` for fresh handles.
#[derive(Clone, Debug, Default)]
pub struct TyArena {
    /// Dense-pack entries; index = TyId handle.
    pub entries: Vec<TyEntry>,
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

    /// Look up a `TyEntry` by handle.
    ///
    /// Panics if `id < 0` or `id >= entries.len()` (dangling handle).
    #[must_use]
    pub fn lookup(&self, id: TyId) -> &TyEntry {
        let idx = usize::try_from(id).expect("TyArena::lookup: negative TyId");
        &self.entries[idx]
    }
}

// =====================================================================
// FnTyArena + RecordArena — parallel arenas
// =====================================================================

/// Dense-pack arena for [`FnTyEntry`] values.
///
/// Parallel arena per ADR-0055a §3. `FnTyId` handles index here.
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
        let id = i64::try_from(self.entries.len())
            .expect("FnTyArena overflow");
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

/// Dense-pack arena for [`RecordEntry`] values.
///
/// Parallel arena per ADR-0055a §3. `RecordId` handles index here.
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
        let id = i64::try_from(self.entries.len())
            .expect("RecordArena overflow");
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
// Arena-utility free functions (stubs)
// =====================================================================

/// Build a cb-form arena from a Rust `Ty` tree.
///
/// # F28 stub
///
/// Returns `(root_TyId, arena)` where `root_TyId` is the handle for the
/// root entry corresponding to `rust_ty`. DEV implements the recursive
/// post-order traversal in the Wave-2 PAIR sprint.
///
/// # F34 anchor: `ty_cb_arena_from_rust`
pub fn ty_cb_arena_from_rust(
    _rust: &Ty,
) -> (TyId, TyArena) {
    todo!("ADR-0055a Wave-2 DEV: implement arena-form conversion from Rust Ty")
}

/// Insert sorted-by-name record pairs into `RecordArena`.
///
/// Mirrors `cobrust_types::Record::from_pairs` per ADR-0055a §4
/// `Record::from_pairs` surface invariant. Returns the `RecordId` handle.
///
/// # F28 stub
pub fn record_from_pairs(
    _arena: &mut RecordArena,
    _pairs: Vec<(String, TyId)>,
) -> RecordId {
    todo!("ADR-0055a Wave-2 DEV: insert sorted RecordEntry into RecordArena")
}

/// Arity of a `FnTyEntry` in `FnTyArena`.
///
/// Mirrors `ty.rs::FnTy::arity` per ADR-0055a §4 `FnTy::arity` surface invariant.
///
/// # F28 stub
pub fn fn_ty_arity(_fn_arena: &FnTyArena, _fn_id: FnTyId) -> i64 {
    todo!("ADR-0055a Wave-2 DEV: lookup FnTyEntry + return positional.len() + named.len()")
}

/// `is_mutable_container` predicate over `TyArena`.
///
/// Mirrors `ty.rs::Ty::is_mutable_container` per ADR-0055a §4:
/// returns `true` iff the `TyEntry` at `id` is `List` | `Set` | `Dict`.
///
/// # F28 stub
pub fn is_mutable_container(_arena: &TyArena, _id: TyId) -> bool {
    todo!("ADR-0055a Wave-2 DEV: single-level arena lookup for mutable container predicate")
}

/// `is_hashable` predicate over `TyArena`.
///
/// Mirrors `ty.rs::Ty::is_hashable` per ADR-0055a §4 + ADR-0050d Decision 7A.
/// Tuple arm recurses through arena handles. `Ref` arm returns `false` per
/// ADR-0052a Wave-1.
///
/// # F28 stub
pub fn is_hashable(_arena: &TyArena, _id: TyId) -> bool {
    todo!("ADR-0055a Wave-2 DEV: recursive arena-walk for is_hashable")
}

/// `display_ty` — byte-identical to `impl Display for Ty` per ADR-0055a §4.
///
/// 5 special-case glyphs per §4 closing list:
/// - `(T,)` — 1-tuple trailing comma
/// - `Adt#{id}[...]` — AdtId prefix
/// - `Alias#{id}[...]` — AliasId prefix
/// - `T{n}` — Generic
/// - `?{n}` — Var
/// - `&{inner}` — Ref
///
/// # F28 stub
pub fn display_ty(_arena: &TyArena, _fn_arena: &FnTyArena, _rec_arena: &RecordArena, _id: TyId) -> String {
    todo!("ADR-0055a Wave-2 DEV: recursive display_ty matching ty.rs Display for Ty")
}

/// Cross-arena clone utility.
///
/// Deep-copies the sub-tree rooted at `src_id` from `src_arena` into
/// `dst_arena`. Returns the new handle in `dst_arena`. See ADR-0055a §6
/// risk 3 (clone semantics under arena) and §2 `clone_into_arena` listing.
///
/// # F28 stub
pub fn clone_into_arena(
    _src_arena: &TyArena,
    _src_id: TyId,
    _dst_arena: &mut TyArena,
) -> TyId {
    todo!("ADR-0055a Wave-2 DEV: recursive deep-clone across arenas")
}

/// `subst_var` — substitute `VarId`-as-i64 throughout the arena sub-tree.
///
/// Returns a **fresh handle** (new entry inserted) for composite results;
/// returns a copy of an existing handle for leaf results that are not the
/// target var. Matches `ty.rs::Ty::subst_var` value semantics. See ADR-0055a
/// §4 surface invariant "`subst_var` returns a fresh arena handle".
///
/// # F28 stub
pub fn subst_var(
    _arena: &mut TyArena,
    _src_id: TyId,
    _var_id: TyId,
    _replacement_id: TyId,
) -> TyId {
    todo!("ADR-0055a Wave-2 DEV: arena-aware subst_var with fresh-insert invariant")
}

/// `free_vars` — deduplicated `VarId`-as-i64 list for the arena sub-tree.
///
/// Mirrors `ty.rs::Ty::free_vars` + `Ty::collect_vars` per ADR-0055a §4.
/// Output set is deduplicated (first-encounter order, same as Rust collect_vars).
///
/// # F28 stub
pub fn free_vars(_arena: &TyArena, _id: TyId) -> Vec<TyId> {
    todo!("ADR-0055a Wave-2 DEV: recursive arena-walk for free_vars")
}

// =====================================================================
// Canonicalize for TyEntry — F28 stub
// =====================================================================

/// `Canonicalize` impl for `TyEntry` — DEV fills this in the Wave-2 PAIR
/// sprint per ADR-0055a §9.3.
///
/// The impl performs a post-order traversal over the `TyArena` / `FnTyArena`
/// / `RecordArena` and emits `CanonicalKey` per ADR-0055e §3. Arena handles
/// (`TyId`, `FnTyId`, `RecordId`) are renamed via the 5-namespace dense-pack
/// allocators in `TyArena` per ADR-0055e §3 amendment 2026-05-18.
///
/// # F28 stub — test corpus can reference this trait bound; impl is `todo!()`
impl Canonicalize for TyEntry {
    fn canonicalize(&self, _arena: &mut ParityArena) -> CanonicalKey {
        todo!("ADR-0055a Wave-2 DEV: Canonicalize for TyEntry — post-order arena traversal")
    }
}
