//! cb mirror of `cobrust-types::error` — ADR-0055b Wave-2.
//!
//! F28 strict-separation: this file contains the Rust CONTRACT TYPES only
//! (enum shape + stub functions). No Canonicalize impl body is present
//! here per F28 rule 4 — DEV ships the impl in a subsequent Wave-2 DEV
//! sprint.
//!
//! ## Surface invariants (ADR-0055b §4)
//!
//! - `TypeErrorCb` mirrors `TypeError` 1:1: 25 variants, identical names.
//! - `Ty` payload fields → `i64` arena handles (TyArena per 0055a §3).
//! - `VarId` payload (`OccursCheck::var`) → `i64` (per 0055a VarId-as-i64).
//! - `suggestion: Option<&'static str>` (Rust) → `pub suggestion: Option<String>` (cb owned).
//! - `Multiple(Vec<TypeError>)` → `Multiple(Vec<TypeErrorCb>)` flat-list (not arena).
//!
//! ## Anchor symbols (F34)
//!
//! - `error_cb.rs::TypeErrorCb` — the 25-variant mirror enum
//! - `error_cb.rs::type_error_cb_from_rust` — bridge stub
//! - `error_cb.rs::TypeErrorCb::Multiple` — the only recursive variant

use cobrust_frontend::span::Span;
use cobrust_types::TypeError;
use cobrust_types_parity::{Canonicalize, TyArena};

// =====================================================================
// TypeErrorCb — 25-variant mirror enum (ADR-0055b §4 compliance matrix)
// =====================================================================

/// cb mirror of `cobrust_types::TypeError`.
///
/// Every variant mirrors the Rust source 1:1 per ADR-0055b §4.
/// `Ty` payload fields are `i64` arena handles (consuming 0055a TyArena).
/// `suggestion` is `Option<String>` (owned; replaces Rust `Option<&'static str>`).
///
/// ## 4 shape classes per ADR-0055b §4.1
///
/// 1. **Name-only** (`BreakOutsideLoop`, `ContinueOutsideLoop`, `ReturnOutsideFn`,
///    `YieldOutsideFn`, `MutableDefault`, `AmbiguousType`, `DictSpreadNotSupported`) — span + suggestion only.
/// 2. **Name + String payload** (`UnknownName`, `KeywordArgMismatch`, `MissingArgument`,
///    `DuplicateField`, `UseOfDroppedFeature`) — str replaces Rust String/&'static str.
/// 3. **Name + Ty payload** (`TypeMismatch`, `RowConflict`, `ImplicitTruthiness`,
///    `NotCallable`, `NotIndexable`, `NotIterable`, `NotHashable`, `OccursCheck`) — `i64` arena handles.
/// 4. **Composite + special** (`ArityMismatch`, `NonExhaustiveMatch`, `BorrowOfNonPlace`,
///    `UnknownMethod`, `Multiple`) — see per-variant doc.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeErrorCb {
    // --- Class 1: Name-only (span + suggestion) -------------------------

    /// `break` outside any loop.
    /// Mirrors `TypeError::BreakOutsideLoop`.
    BreakOutsideLoop {
        span: Span,
        suggestion: Option<String>,
    },

    /// `continue` outside any loop.
    /// Mirrors `TypeError::ContinueOutsideLoop`.
    ContinueOutsideLoop {
        span: Span,
        suggestion: Option<String>,
    },

    /// `return` outside any function.
    /// Mirrors `TypeError::ReturnOutsideFn`.
    ReturnOutsideFn {
        span: Span,
        suggestion: Option<String>,
    },

    /// `yield` outside any function.
    /// Mirrors `TypeError::YieldOutsideFn`.
    YieldOutsideFn {
        span: Span,
        suggestion: Option<String>,
    },

    /// Mutable default argument — compile error.
    /// Mirrors `TypeError::MutableDefault`.
    MutableDefault {
        span: Span,
        suggestion: Option<String>,
    },

    /// Inference left a type variable un-resolved.
    /// Mirrors `TypeError::AmbiguousType`.
    AmbiguousType {
        span: Span,
        suggestion: Option<String>,
    },

    /// Dict spread (`**other`) not supported in Phase F.3.
    /// Mirrors `TypeError::DictSpreadNotSupported`.
    DictSpreadNotSupported {
        span: Span,
        suggestion: Option<String>,
    },

    // --- Class 2: Name + String payload ---------------------------------

    /// Unknown name (capture-only references during closure analysis).
    /// Mirrors `TypeError::UnknownName { name: String, span, suggestion }`.
    UnknownName {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// Unknown keyword argument at a call site.
    /// Mirrors `TypeError::KeywordArgMismatch { name: String, span, suggestion }`.
    KeywordArgMismatch {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// Missing required argument at a call site.
    /// Mirrors `TypeError::MissingArgument { name: String, span, suggestion }`.
    MissingArgument {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// Duplicate field in a record literal.
    /// Mirrors `TypeError::DuplicateField { name: String, span, suggestion }`.
    DuplicateField {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// Dropped feature snuck through; `name` is `String` (cb owned)
    /// replacing Rust `&'static str` per ADR-0055b §6 risk 1.
    /// Mirrors `TypeError::UseOfDroppedFeature { name: &'static str, span, suggestion }`.
    UseOfDroppedFeature {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    // --- Class 3: Name + Ty payload (i64 arena handles) -----------------

    /// Two types do not unify.
    /// `expected` + `actual` are `i64` TyArena handles.
    /// Mirrors `TypeError::TypeMismatch { expected: Ty, actual: Ty, span, suggestion }`.
    TypeMismatch {
        expected: i64,
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Row (record field) type conflict.
    /// `ty1` + `ty2` are `i64` TyArena handles.
    /// Mirrors `TypeError::RowConflict { field: String, ty1: Ty, ty2: Ty, span, suggestion }`.
    RowConflict {
        field: String,
        ty1: i64,
        ty2: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Non-bool used in truthiness position.
    /// `actual` is `i64` TyArena handle.
    /// Mirrors `TypeError::ImplicitTruthiness { actual: Ty, span, suggestion }`.
    ImplicitTruthiness {
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Unification would create an infinite type.
    /// `var` is `i64` VarId-as-i64; `ty` is `i64` TyArena handle.
    /// Mirrors `TypeError::OccursCheck { var: VarId, ty: Ty, span, suggestion }`.
    OccursCheck {
        var: i64,
        ty: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Not callable.
    /// `actual` is `i64` TyArena handle.
    /// Mirrors `TypeError::NotCallable { actual: Ty, span, suggestion }`.
    NotCallable {
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Not indexable.
    /// `actual` is `i64` TyArena handle.
    /// Mirrors `TypeError::NotIndexable { actual: Ty, span, suggestion }`.
    NotIndexable {
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Not iterable.
    /// `actual` is `i64` TyArena handle.
    /// Mirrors `TypeError::NotIterable { actual: Ty, span, suggestion }`.
    NotIterable {
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// Dict key type not Hashable.
    /// `actual` is `i64` TyArena handle.
    /// Mirrors `TypeError::NotHashable { actual: Ty, span, suggestion }`.
    NotHashable {
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    // --- Class 4: Composite + special -----------------------------------

    /// Wrong number of positional arguments.
    /// Mirrors `TypeError::ArityMismatch { expected: usize, actual: usize, span, suggestion }`.
    ArityMismatch {
        expected: usize,
        actual: usize,
        span: Span,
        suggestion: Option<String>,
    },

    /// Non-exhaustive match: missing constructor cases.
    /// `uncovered: Vec<String>` mirrors Rust `Vec<String>`.
    /// Mirrors `TypeError::NonExhaustiveMatch { uncovered: Vec<String>, span, suggestion }`.
    NonExhaustiveMatch {
        uncovered: Vec<String>,
        span: Span,
        suggestion: Option<String>,
    },

    /// Borrow of non-place expression.
    /// Mirrors `TypeError::BorrowOfNonPlace { span, suggestion }`.
    BorrowOfNonPlace {
        span: Span,
        suggestion: Option<String>,
    },

    /// Method name not found on a recognised receiver type.
    /// Mirrors `TypeError::UnknownMethod { type_name: String, method_name: String, span, suggestion }`.
    UnknownMethod {
        type_name: String,
        method_name: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// Composite error container — flat list of errors.
    ///
    /// ADR-0055b §3: `Multiple(list[TypeError])` — the only recursive
    /// variant. Tree-shaped, not arena-backed: `Vec<TypeErrorCb>` is
    /// heap-backed, analogous to Rust `Vec<TypeError>`. No depth-limit
    /// guard needed; callers flatten before construction per Rust surface.
    ///
    /// Mirrors `TypeError::Multiple(Vec<TypeError>)`.
    Multiple(Vec<TypeErrorCb>),
}

// =====================================================================
// Bridge stub: type_error_cb_from_rust
// =====================================================================

/// Convert a Rust-side `TypeError` to its cb mirror `TypeErrorCb`.
///
/// The `arena` argument is the `TyArena` that maps Rust `Ty` payloads to
/// `i64` handles per ADR-0055b §3 arena workaround.
///
/// **F28: STUB only — DEV fills the impl body in Wave-2 DEV sprint.**
///
/// # Anchor
/// `error_cb.rs::type_error_cb_from_rust`
pub fn type_error_cb_from_rust(_rust: &TypeError, _arena: &mut TyArena) -> TypeErrorCb {
    todo!("ADR-0055b Wave-2 DEV impl pending: type_error_cb_from_rust")
}

// =====================================================================
// Canonicalize impl stub
// =====================================================================

/// `Canonicalize` impl for `TypeErrorCb`.
///
/// Produces the same `CanonicalKey` as the Rust `TypeError` counterpart
/// so the parity harness (ADR-0055e §6 BLOCK rule 5) can diff-test the
/// two impls with `parity_check`.
///
/// **F28: STUB only — DEV fills the impl body in Wave-2 DEV sprint.**
impl Canonicalize for TypeErrorCb {
    fn canonicalize(
        &self,
        _arena: &mut TyArena,
    ) -> cobrust_types_parity::CanonicalKey {
        todo!("ADR-0055b Wave-2 DEV impl pending: TypeErrorCb::canonicalize")
    }
}

// =====================================================================
// Display stub
// =====================================================================

impl std::fmt::Display for TypeErrorCb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // DEV replaces with hand-rolled `display_error` reproducing each
        // Rust `#[error("...")]` format string byte-for-byte per
        // ADR-0055b §4 + §6 risk 2.
        //
        // F28: STUB only.
        write!(f, "TypeErrorCb::{}", type_error_cb_variant_name(self))
    }
}

/// Variant-name discriminant for `TypeErrorCb` — mirrors
/// `cobrust_types_parity::type_error_variant_name` for the cb side.
///
/// # Anchor
/// `error_cb.rs::type_error_cb_variant_name`
#[must_use]
pub fn type_error_cb_variant_name(err: &TypeErrorCb) -> &'static str {
    match err {
        TypeErrorCb::UnknownName { .. } => "UnknownName",
        TypeErrorCb::ArityMismatch { .. } => "ArityMismatch",
        TypeErrorCb::KeywordArgMismatch { .. } => "KeywordArgMismatch",
        TypeErrorCb::MissingArgument { .. } => "MissingArgument",
        TypeErrorCb::TypeMismatch { .. } => "TypeMismatch",
        TypeErrorCb::NonExhaustiveMatch { .. } => "NonExhaustiveMatch",
        TypeErrorCb::RowConflict { .. } => "RowConflict",
        TypeErrorCb::ImplicitTruthiness { .. } => "ImplicitTruthiness",
        TypeErrorCb::UseOfDroppedFeature { .. } => "UseOfDroppedFeature",
        TypeErrorCb::MutableDefault { .. } => "MutableDefault",
        TypeErrorCb::AmbiguousType { .. } => "AmbiguousType",
        TypeErrorCb::DuplicateField { .. } => "DuplicateField",
        TypeErrorCb::OccursCheck { .. } => "OccursCheck",
        TypeErrorCb::NotCallable { .. } => "NotCallable",
        TypeErrorCb::NotIndexable { .. } => "NotIndexable",
        TypeErrorCb::NotIterable { .. } => "NotIterable",
        TypeErrorCb::BreakOutsideLoop { .. } => "BreakOutsideLoop",
        TypeErrorCb::ContinueOutsideLoop { .. } => "ContinueOutsideLoop",
        TypeErrorCb::ReturnOutsideFn { .. } => "ReturnOutsideFn",
        TypeErrorCb::YieldOutsideFn { .. } => "YieldOutsideFn",
        TypeErrorCb::NotHashable { .. } => "NotHashable",
        TypeErrorCb::DictSpreadNotSupported { .. } => "DictSpreadNotSupported",
        TypeErrorCb::Multiple(_) => "Multiple",
        TypeErrorCb::BorrowOfNonPlace { .. } => "BorrowOfNonPlace",
        TypeErrorCb::UnknownMethod { .. } => "UnknownMethod",
    }
}
