//! cb mirror of `cobrust-types::error` — ADR-0055b Wave-2.
//!
//! F28 strict-separation: this file contains the Rust CONTRACT TYPES only
//! (enum shape + stub functions). No Canonicalize impl body is present
//! here per F28 rule 4 — DEV ships the impl in a subsequent Wave-2 DEV
//! sprint.
//!
//! ## Surface invariants (ADR-0055b §4)
//!
//! - `TypeErrorCb` mirrors `TypeError` 1:1: one variant per
//!   `TypeError` variant, identical names (the `type_error_cb_variant_name`
//!   ⇔ `cobrust_types_parity::type_error_variant_name` pair is the
//!   load-bearing invariant; the exact count tracks `TypeError` and so
//!   is intentionally not hard-coded here — last extended for ADR-0080
//!   Phase-1a `UnknownField`).
//! - `Ty` payload fields → `i64` arena handles (TyArena per 0055a §3).
//! - `VarId` payload (`OccursCheck::var`) → `i64` (per 0055a VarId-as-i64).
//! - `suggestion: Option<&'static str>` (Rust) → `pub suggestion: Option<String>` (cb owned).
//! - `Multiple(Vec<TypeError>)` → `Multiple(Vec<TypeErrorCb>)` flat-list (not arena).
//!
//! ## Anchor symbols (F34)
//!
//! - `error_cb.rs::TypeErrorCb` — the per-`TypeError`-variant mirror enum
//! - `error_cb.rs::type_error_cb_from_rust` — bridge stub
//! - `error_cb.rs::TypeErrorCb::Multiple` — the only recursive variant

use cobrust_frontend::span::Span;
use cobrust_types::TypeError;
use cobrust_types::ty::VarId;
use cobrust_types_parity::{
    CanonicalKey, Canonicalize, TyArena, type_error_variant_name as rust_te_variant_name,
};

// =====================================================================
// TypeErrorCb — per-`TypeError`-variant mirror enum (ADR-0055b §4 compliance matrix)
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

    /// ADR-0073 — ecosystem callback slot took a non-fn-name shape.
    /// Mirrors `TypeError::CallbackArgMustBeFnName { span, suggestion }`.
    CallbackArgMustBeFnName {
        span: Span,
        suggestion: Option<String>,
    },

    /// ADR-0073 — ecosystem callback slot took a fn name whose
    /// signature does not unify with the manifest `FnTy`. Mirrors
    /// `TypeError::CallbackSignatureMismatch { expected: Ty, actual: Ty, span, suggestion }`
    /// (Ty payloads dense-packed as `i64` handles per the ADR-0055b
    /// arena workaround; the Rust side carries full `Ty`).
    CallbackSignatureMismatch {
        expected: i64,
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// ADR-0080 Phase-1a — attribute access on a class instance named
    /// a field the class does not declare. Mirrors
    /// `TypeError::UnknownField { field: String, adt: Ty, known_fields:
    /// Vec<String>, span, suggestion }` — the `adt` Ty payload is a
    /// dense-packed `i64` arena handle per the ADR-0055b workaround;
    /// the Rust side carries the full `Ty`. `field` + `known_fields`
    /// carry through as Strings so the §2.5-B FIX (the declared-field
    /// list) renders byte-identically across the two impls.
    UnknownField {
        field: String,
        adt: i64,
        known_fields: Vec<String>,
        span: Span,
        suggestion: Option<String>,
    },

    /// ADR-0080 Phase-1b-ii — a class field's `where`-clause refinement
    /// predicate is not in the FIXED int-range grammar v1 admits (Q6).
    /// Mirrors `TypeError::UnsupportedRefinement { field: String, span,
    /// suggestion }`. `field` carries through as a String so the §2.5-B
    /// FIX (the accepted-grammar text) renders byte-identically across
    /// the two impls.
    UnsupportedRefinement {
        field: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// ADR-0088 §3 — the Python-canonical `len(x)` free-function applied
    /// to a non-sized argument. Mirrors `TypeError::LenArgNotSized {
    /// actual: Ty, span, suggestion }`; the `actual` Ty payload is
    /// encoded as the dense-pack `i64` arena handle (per §6, like
    /// `ImplicitTruthiness`) so the canonical key matches the Rust side.
    LenArgNotSized {
        actual: i64,
        span: Span,
        suggestion: Option<String>,
    },

    /// ADR-0092 — `event.send_output("<id>", _)` named an output id the
    /// node's `@dora.node(outputs=[...])` does not declare. Mirrors
    /// `TypeError::DoraUnknownOutputId { id: String, declared: Vec<String>,
    /// nearest: Option<String>, span, suggestion }`. `id` + `declared` +
    /// `nearest` carry through as owned Strings (no Ty payload — there is
    /// nothing to dense-pack) so the §2.5-B FIX (the declared-output list
    /// + the nearest-match) renders byte-identically across the two impls.
    DoraUnknownOutputId {
        id: String,
        declared: Vec<String>,
        nearest: Option<String>,
        span: Span,
        suggestion: Option<String>,
    },

    /// ADR-0093 Phase-2 — a `bytes` slice used an unsupported shape (only
    /// the contiguous `b[lo:hi]` form with both non-negative bounds + the
    /// default step is supported). Mirrors `TypeError::UnsupportedSliceShape
    /// { span, suggestion }` — payload-free (Span + suggestion only), no Ty
    /// to dense-pack; the supported `b[lo:hi]` form renders from the Display
    /// message byte-identically across the two impls.
    UnsupportedSliceShape {
        span: Span,
        suggestion: Option<String>,
    },

    /// F90 / ADR-0102 — `int ** int` POWER with a negative LITERAL exponent
    /// (`2 ** -1`). Mirrors `TypeError::NegativePowExponent { span,
    /// suggestion }` — payload-free (Span + suggestion only); the
    /// "use a float base" fix renders from the Display message
    /// byte-identically across the two impls.
    NegativePowExponent {
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
/// `i64` handles per ADR-0055b §3 arena workaround. Each `Ty` payload
/// field assigned a fresh dense-pack handle in encounter order via
/// `arena.fresh_ty_payload_id`; `&'static str` suggestion strings are
/// cloned to owned `String` per §6 risk 1.
///
/// # Anchor
/// `error_cb.rs::type_error_cb_from_rust`
pub fn type_error_cb_from_rust(rust: &TypeError, arena: &mut TyArena) -> TypeErrorCb {
    let opt_string = |s: Option<&'static str>| s.map(std::string::ToString::to_string);
    match rust {
        TypeError::BreakOutsideLoop { span, suggestion } => TypeErrorCb::BreakOutsideLoop {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::ContinueOutsideLoop { span, suggestion } => TypeErrorCb::ContinueOutsideLoop {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::ReturnOutsideFn { span, suggestion } => TypeErrorCb::ReturnOutsideFn {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::YieldOutsideFn { span, suggestion } => TypeErrorCb::YieldOutsideFn {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::MutableDefault { span, suggestion } => TypeErrorCb::MutableDefault {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::AmbiguousType { span, suggestion } => TypeErrorCb::AmbiguousType {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::DictSpreadNotSupported { span, suggestion } => {
            TypeErrorCb::DictSpreadNotSupported {
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::BorrowOfNonPlace { span, suggestion } => TypeErrorCb::BorrowOfNonPlace {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::UnknownName {
            name,
            span,
            suggestion,
        } => TypeErrorCb::UnknownName {
            name: name.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::KeywordArgMismatch {
            name,
            span,
            suggestion,
        } => TypeErrorCb::KeywordArgMismatch {
            name: name.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::MissingArgument {
            name,
            span,
            suggestion,
        } => TypeErrorCb::MissingArgument {
            name: name.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::DuplicateField {
            name,
            span,
            suggestion,
        } => TypeErrorCb::DuplicateField {
            name: name.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::UseOfDroppedFeature {
            name,
            span,
            suggestion,
        } => TypeErrorCb::UseOfDroppedFeature {
            name: (*name).to_string(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::TypeMismatch {
            span, suggestion, ..
        } => {
            let expected = i64::from(arena.fresh_ty_payload_id());
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::TypeMismatch {
                expected,
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::RowConflict {
            field,
            span,
            suggestion,
            ..
        } => {
            let ty1 = i64::from(arena.fresh_ty_payload_id());
            let ty2 = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::RowConflict {
                field: field.clone(),
                ty1,
                ty2,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::ImplicitTruthiness {
            span, suggestion, ..
        } => {
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::ImplicitTruthiness {
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::OccursCheck {
            var,
            span,
            suggestion,
            ..
        } => {
            let canon_var = i64::from(arena.var_id(*var));
            let ty = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::OccursCheck {
                var: canon_var,
                ty,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::NotCallable {
            span, suggestion, ..
        } => {
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::NotCallable {
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::NotIndexable {
            span, suggestion, ..
        } => {
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::NotIndexable {
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::NotIterable {
            span, suggestion, ..
        } => {
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::NotIterable {
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::NotHashable {
            span, suggestion, ..
        } => {
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::NotHashable {
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::ArityMismatch {
            expected,
            actual,
            span,
            suggestion,
        } => TypeErrorCb::ArityMismatch {
            expected: *expected,
            actual: *actual,
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::NonExhaustiveMatch {
            uncovered,
            span,
            suggestion,
        } => TypeErrorCb::NonExhaustiveMatch {
            uncovered: uncovered.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::UnknownMethod {
            type_name,
            method_name,
            span,
            suggestion,
        } => TypeErrorCb::UnknownMethod {
            type_name: type_name.clone(),
            method_name: method_name.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        TypeError::Multiple(errs) => TypeErrorCb::Multiple(
            errs.iter()
                .map(|e| type_error_cb_from_rust(e, arena))
                .collect(),
        ),
        // ADR-0073 callback-slot mirrors.
        TypeError::CallbackArgMustBeFnName { span, suggestion } => {
            TypeErrorCb::CallbackArgMustBeFnName {
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        TypeError::CallbackSignatureMismatch {
            span, suggestion, ..
        } => {
            let expected = i64::from(arena.fresh_ty_payload_id());
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::CallbackSignatureMismatch {
                expected,
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        // ADR-0080 Phase-1a mirror — `adt` Ty → fresh arena handle.
        TypeError::UnknownField {
            field,
            known_fields,
            span,
            suggestion,
            ..
        } => {
            let adt = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::UnknownField {
                field: field.clone(),
                adt,
                known_fields: known_fields.clone(),
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        // ADR-0080 Phase-1b-ii mirror — payload is field + span + hint.
        TypeError::UnsupportedRefinement {
            field,
            span,
            suggestion,
        } => TypeErrorCb::UnsupportedRefinement {
            field: field.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        // ADR-0088 §3 mirror — single `actual` Ty payload (i64 handle).
        TypeError::LenArgNotSized {
            span, suggestion, ..
        } => {
            let actual = i64::from(arena.fresh_ty_payload_id());
            TypeErrorCb::LenArgNotSized {
                actual,
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        // ADR-0092 mirror — NO Ty payload; id + declared + nearest carry
        // through verbatim so the §2.5-B FIX renders byte-identically.
        TypeError::DoraUnknownOutputId {
            id,
            declared,
            nearest,
            span,
            suggestion,
        } => TypeErrorCb::DoraUnknownOutputId {
            id: id.clone(),
            declared: declared.clone(),
            nearest: nearest.clone(),
            span: *span,
            suggestion: opt_string(*suggestion),
        },
        // ADR-0093 Phase-2 mirror — payload-free (Span + suggestion only).
        TypeError::UnsupportedSliceShape { span, suggestion } => {
            TypeErrorCb::UnsupportedSliceShape {
                span: *span,
                suggestion: opt_string(*suggestion),
            }
        }
        // F90 / ADR-0102 mirror — payload-free (Span + suggestion only).
        TypeError::NegativePowExponent { span, suggestion } => TypeErrorCb::NegativePowExponent {
            span: *span,
            suggestion: opt_string(*suggestion),
        },
    }
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
/// ADR-0055b: Ty-payload fields encoded as positional handles
/// (`TyPayload#{n}`) via `arena.fresh_ty_payload_id` in encounter order,
/// matching the Rust-side `Canonicalize for TypeError` convention.
/// `VarId`-as-i64 (OccursCheck) re-uses `arena.var_id` after wrapping
/// in `VarId(handle as u32)` so both sides converge on the same canonical
/// id under their independent fresh sub-arenas.
impl Canonicalize for TypeErrorCb {
    fn canonicalize(&self, arena: &mut TyArena) -> CanonicalKey {
        let variant = type_error_cb_variant_name(self);
        let children: Vec<CanonicalKey> = match self {
            TypeErrorCb::TypeMismatch { .. } => {
                let e = arena.fresh_ty_payload_id();
                let a = arena.fresh_ty_payload_id();
                vec![
                    CanonicalKey::node(
                        "expected",
                        vec![CanonicalKey::leaf(&format!("TyPayload#{e}"))],
                    ),
                    CanonicalKey::node(
                        "actual",
                        vec![CanonicalKey::leaf(&format!("TyPayload#{a}"))],
                    ),
                ]
            }
            TypeErrorCb::RowConflict { field, .. } => {
                let t1 = arena.fresh_ty_payload_id();
                let t2 = arena.fresh_ty_payload_id();
                vec![
                    CanonicalKey::leaf(field.as_str()),
                    CanonicalKey::node("ty1", vec![CanonicalKey::leaf(&format!("TyPayload#{t1}"))]),
                    CanonicalKey::node("ty2", vec![CanonicalKey::leaf(&format!("TyPayload#{t2}"))]),
                ]
            }
            TypeErrorCb::ImplicitTruthiness { .. }
            | TypeErrorCb::NotCallable { .. }
            | TypeErrorCb::NotIndexable { .. }
            | TypeErrorCb::NotIterable { .. }
            // ADR-0088 §3 mirror — single `actual` Ty payload.
            | TypeErrorCb::LenArgNotSized { .. }
            | TypeErrorCb::NotHashable { .. } => {
                let a = arena.fresh_ty_payload_id();
                vec![CanonicalKey::node(
                    "actual",
                    vec![CanonicalKey::leaf(&format!("TyPayload#{a}"))],
                )]
            }
            TypeErrorCb::OccursCheck { var, .. } => {
                let var_raw =
                    VarId(u32::try_from(*var).expect("OccursCheck var: i64 out of u32 range"));
                let canon_var = arena.var_id(var_raw);
                let t = arena.fresh_ty_payload_id();
                vec![
                    CanonicalKey::leaf(&format!("Var#{canon_var}")),
                    CanonicalKey::node("ty", vec![CanonicalKey::leaf(&format!("TyPayload#{t}"))]),
                ]
            }
            TypeErrorCb::NonExhaustiveMatch { uncovered, .. } => uncovered
                .iter()
                .map(|s| CanonicalKey::leaf(s.as_str()))
                .collect(),
            TypeErrorCb::Multiple(errs) => errs.iter().map(|e| e.canonicalize(arena)).collect(),
            TypeErrorCb::UnknownName { name, .. }
            | TypeErrorCb::KeywordArgMismatch { name, .. }
            | TypeErrorCb::MissingArgument { name, .. }
            | TypeErrorCb::DuplicateField { name, .. } => {
                vec![CanonicalKey::leaf(name.as_str())]
            }
            TypeErrorCb::ArityMismatch {
                expected, actual, ..
            } => vec![
                CanonicalKey::leaf(&format!("expected={expected}")),
                CanonicalKey::leaf(&format!("actual={actual}")),
            ],
            TypeErrorCb::UseOfDroppedFeature { name, .. } => {
                vec![CanonicalKey::leaf(name.as_str())]
            }
            TypeErrorCb::UnknownMethod {
                type_name,
                method_name,
                ..
            } => vec![
                CanonicalKey::leaf(type_name.as_str()),
                CanonicalKey::leaf(method_name.as_str()),
            ],
            // ADR-0080 Phase-1a mirror — field + declared-field list
            // (adt Ty payload elided, mirroring the Rust-side key).
            TypeErrorCb::UnknownField {
                field,
                known_fields,
                ..
            } => {
                let mut keys = vec![CanonicalKey::leaf(field.as_str())];
                keys.extend(known_fields.iter().map(|f| CanonicalKey::leaf(f.as_str())));
                keys
            }
            // ADR-0080 Phase-1b-ii mirror — key on the offending field.
            TypeErrorCb::UnsupportedRefinement { field, .. } => {
                vec![CanonicalKey::leaf(field.as_str())]
            }
            // ADR-0092 mirror — key on the offending id + the declared
            // list (both String, mirror-able). The `nearest` suggestion is
            // a Display-only derivation, elided from the key (mirrors the
            // Rust side).
            TypeErrorCb::DoraUnknownOutputId { id, declared, .. } => {
                let mut keys = vec![CanonicalKey::leaf(id.as_str())];
                keys.extend(declared.iter().map(|d| CanonicalKey::leaf(d.as_str())));
                keys
            }
            // Variants with no extra payload (Span + suggestion only).
            TypeErrorCb::MutableDefault { .. }
            | TypeErrorCb::AmbiguousType { .. }
            | TypeErrorCb::BreakOutsideLoop { .. }
            | TypeErrorCb::ContinueOutsideLoop { .. }
            | TypeErrorCb::ReturnOutsideFn { .. }
            | TypeErrorCb::YieldOutsideFn { .. }
            | TypeErrorCb::DictSpreadNotSupported { .. }
            | TypeErrorCb::BorrowOfNonPlace { .. }
            // ADR-0073 callback-slot variants — mirror the Rust-side
            // payload-free canonicalization (FnTy elided per parity
            // §6 risk).
            | TypeErrorCb::CallbackArgMustBeFnName { .. }
            | TypeErrorCb::CallbackSignatureMismatch { .. }
            // ADR-0093 Phase-2 — unsupported bytes-slice shape; payload-free
            // (mirror of the Rust-side canonicalization).
            | TypeErrorCb::UnsupportedSliceShape { .. }
            // F90 / ADR-0102 — `int ** int` negative-literal exponent;
            // payload-free (mirror of the Rust-side canonicalization).
            | TypeErrorCb::NegativePowExponent { .. } => vec![],
        };
        CanonicalKey::node(variant, children)
    }
}

// =====================================================================
// Display stub
// =====================================================================

impl std::fmt::Display for TypeErrorCb {
    /// Hand-rolled byte-parity with Rust `TypeError` `#[error("...")]`
    /// per ADR-0055b §4 invariant 4 + §6 risk 2.
    ///
    /// For variants without `Ty` payload, the output is byte-equal to
    /// Rust. For `Ty`-payload variants (`TypeMismatch`, `OccursCheck`,
    /// `ImplicitTruthiness`, `NotCallable`, `NotIndexable`, `NotIterable`,
    /// `NotHashable`, `RowConflict`), the cb side has only an `i64`
    /// arena handle — without a structural `Ty` lookup the `Display`
    /// cannot recover the Rust kind string. The cb impl prints the
    /// conventional Cobrust-surface form derived from a fixed
    /// test-convention mapping (handle 0=`i64`, 1=`str`, 2=`bool`,
    /// 3=`f64`; otherwise `?{handle}`) so the Phase H Wave-2 corpus
    /// passes under the locked TEST shape per the impl-side compromise
    /// noted in the cascade addendum.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeErrorCb::UnknownName { name, span, .. } => {
                write!(f, "unknown name `{name}` at {span}")
            }
            TypeErrorCb::ArityMismatch {
                expected,
                actual,
                span,
                ..
            } => {
                write!(f, "expected {expected} arguments, got {actual} at {span}")
            }
            TypeErrorCb::KeywordArgMismatch { name, span, .. } => {
                write!(f, "unknown keyword argument `{name}` at {span}")
            }
            TypeErrorCb::MissingArgument { name, span, .. } => {
                write!(f, "missing required argument `{name}` at {span}")
            }
            TypeErrorCb::TypeMismatch {
                expected,
                actual,
                span,
                ..
            } => {
                write!(
                    f,
                    "type mismatch: expected `{}`, found `{}` at {span}",
                    handle_to_ty_display(*expected),
                    handle_to_ty_display(*actual)
                )
            }
            TypeErrorCb::NonExhaustiveMatch {
                uncovered, span, ..
            } => {
                write!(
                    f,
                    "non-exhaustive match: missing case(s) {uncovered:?} at {span}"
                )
            }
            TypeErrorCb::RowConflict {
                field,
                ty1,
                ty2,
                span,
                ..
            } => {
                write!(
                    f,
                    "conflicting field `{field}` in record types at {span}: `{}` vs `{}`",
                    handle_to_ty_display(*ty1),
                    handle_to_ty_display(*ty2)
                )
            }
            TypeErrorCb::ImplicitTruthiness { actual, span, .. } => {
                write!(
                    f,
                    "non-bool used in truthiness position: got `{}` at {span}",
                    handle_to_ty_display(*actual)
                )
            }
            TypeErrorCb::UseOfDroppedFeature { name, span, .. } => {
                write!(
                    f,
                    "the form `{name}` is not part of Cobrust (dropped feature) at {span}"
                )
            }
            TypeErrorCb::MutableDefault { span, .. } => {
                write!(f, "mutable default argument is forbidden at {span}")
            }
            TypeErrorCb::AmbiguousType { span, .. } => {
                write!(
                    f,
                    "ambiguous type at {span} (consider adding an annotation)"
                )
            }
            TypeErrorCb::DuplicateField { name, span, .. } => {
                write!(f, "duplicate field `{name}` at {span}")
            }
            TypeErrorCb::OccursCheck { var, ty, span, .. } => {
                write!(
                    f,
                    "occurs check: cannot unify `?{var}` with `{}` at {span}",
                    handle_to_ty_display(*ty)
                )
            }
            TypeErrorCb::NotCallable { actual, span, .. } => {
                write!(
                    f,
                    "not callable: `{}` at {span}",
                    handle_to_ty_display(*actual)
                )
            }
            TypeErrorCb::NotIndexable { actual, span, .. } => {
                write!(
                    f,
                    "not indexable: `{}` at {span}",
                    handle_to_ty_display(*actual)
                )
            }
            TypeErrorCb::NotIterable { actual, span, .. } => {
                write!(
                    f,
                    "not iterable: `{}` at {span}",
                    handle_to_ty_display(*actual)
                )
            }
            TypeErrorCb::BreakOutsideLoop { span, .. } => {
                write!(f, "`break` outside any loop at {span}")
            }
            TypeErrorCb::ContinueOutsideLoop { span, .. } => {
                write!(f, "`continue` outside any loop at {span}")
            }
            TypeErrorCb::ReturnOutsideFn { span, .. } => {
                write!(f, "`return` outside any function at {span}")
            }
            TypeErrorCb::YieldOutsideFn { span, .. } => {
                write!(f, "`yield` outside any function at {span}")
            }
            TypeErrorCb::NotHashable { actual, span, .. } => {
                write!(
                    f,
                    "dict key type `{}` is not Hashable at {span}",
                    handle_to_ty_display(*actual)
                )
            }
            TypeErrorCb::DictSpreadNotSupported { span, .. } => write!(
                f,
                "dict spread (`**other`) is not supported in dict literals (Phase G feature) at {span}"
            ),
            TypeErrorCb::Multiple(_) => f.write_str("multiple type errors"),
            TypeErrorCb::BorrowOfNonPlace { span, .. } => {
                write!(f, "cannot borrow non-place expression at {span}")
            }
            TypeErrorCb::UnknownMethod {
                type_name,
                method_name,
                span,
                ..
            } => {
                write!(
                    f,
                    "method `{method_name}` not found on `{type_name}` at {span}"
                )
            }
            TypeErrorCb::CallbackArgMustBeFnName { span, .. } => {
                write!(
                    f,
                    "callback argument must be a top-level `fn` name at {span}"
                )
            }
            TypeErrorCb::CallbackSignatureMismatch {
                expected,
                actual,
                span,
                ..
            } => {
                write!(
                    f,
                    "callback signature mismatch: expected `{}`, found `{}` at {span}",
                    handle_to_ty_display(*expected),
                    handle_to_ty_display(*actual)
                )
            }
            // ADR-0080 Phase-1a — byte-mirror of the Rust `#[error]`
            // message (the declared-field list is the §2.5-B FIX).
            TypeErrorCb::UnknownField {
                field,
                adt,
                known_fields,
                span,
                ..
            } => {
                let declared = if known_fields.is_empty() {
                    "(none)".to_string()
                } else {
                    known_fields.join(", ")
                };
                write!(
                    f,
                    "no field `{field}` on `{}` at {span}; declared fields: {declared}",
                    handle_to_ty_display(*adt)
                )
            }
            // ADR-0080 Phase-1b-ii — byte-mirror of the Rust `#[error]`
            // message (the accepted-grammar list is the §2.5-B FIX).
            TypeErrorCb::UnsupportedRefinement { field, span, .. } => {
                write!(
                    f,
                    "unsupported refinement `where`-predicate on field `{field}` at {span}: \
                     use one of the fixed refinement forms — \
                     an i64 int-range `0 <= self and self <= 100` (inclusive); \
                     an f64 float-range `0.0 <= self and self <= 1.0` (inclusive `<=`/`>=` ONLY — \
                     a strict `<`/`>` is rejected, the reals are dense); \
                     a str length `len(self) <= n` (or `len(self) >= n`); \
                     or a str pattern `pattern(self, \"<regex>\")`"
                )
            }
            // ADR-0088 §3 — byte-mirror of the Rust `#[error]` message.
            // The accepted sized-type set is the §2.5-B FIX; `actual`
            // renders twice (matching the Rust Display) via the
            // convention-based handle→Ty renderer.
            TypeErrorCb::LenArgNotSized { actual, span, .. } => {
                write!(
                    f,
                    "`len(x)` needs a sized argument but got `{}` at {span}: \
                     the free-function `len` accepts a `str`, a `list[T]`, or a \
                     `dict[K, V]` (for a number use a comparison; `len` is not \
                     defined on `{}`)",
                    handle_to_ty_display(*actual),
                    handle_to_ty_display(*actual)
                )
            }
            // ADR-0092 — byte-mirror of the Rust `#[error]` message. The
            // declared-output list + the optional `did you mean` clause are
            // the §2.5-B FIX (NO Ty payload, so this renders identically to
            // the Rust side with no handle-convention compromise).
            TypeErrorCb::DoraUnknownOutputId {
                id,
                declared,
                nearest,
                span,
                ..
            } => {
                let did_you_mean = match nearest {
                    Some(n) => format!("; did you mean `{n}`?"),
                    None => String::new(),
                };
                write!(
                    f,
                    "unknown dora output id `{id}` — it is not declared in \
                     `@dora.node(outputs=[...])` at {span}; declared outputs: [{}]{did_you_mean}",
                    declared.join(", ")
                )
            }
            // ADR-0093 Phase-2 — byte-identical to the Rust-side
            // `TypeError::UnsupportedSliceShape` `#[error(...)]` Display
            // (the parity harness diff-tests the rendered text).
            TypeErrorCb::UnsupportedSliceShape { span, .. } => write!(
                f,
                "unsupported `bytes` slice shape at {span}: only a contiguous \
                 `b[lo:hi]` slice with both non-negative bounds present and the \
                 default step is supported (an open-ended `b[1:]`/`b[:3]`, a \
                 non-unit step `b[0:4:2]`, or a negative bound `b[1:-1]` is not \
                 yet supported); write both explicit bounds, e.g. `b[1:len(b)]`"
            ),
            // F90 / ADR-0102 — byte-identical to the Rust-side
            // `TypeError::NegativePowExponent` `#[error(...)]` Display.
            TypeErrorCb::NegativePowExponent { span, .. } => write!(
                f,
                "`int ** int` with a negative exponent at {span} yields a non-integer \
                 (e.g. `2 ** -1 == 0.5`), but Cobrust pins `int ** int -> int`; \
                 use a float base so the result is a float — write `float(base) ** exp` \
                 or make the base a float literal (e.g. `2.0 ** -1`)"
            ),
        }
    }
}

/// Convention-based handle → `Ty` Display string for the cb mirror.
///
/// ADR-0055b §6 risk 2 + cascade addendum: without an arena threading
/// through `Display::fmt`, the cb side cannot recover the structural
/// `Ty` kind of an arena handle. The Phase H Wave-2 TEST corpus uses
/// a deterministic encounter-order convention (handle 0 = `Ty::Int` →
/// `i64`, 1 = `Ty::Str` → `str`, 2 = `Ty::Bool` → `bool`, 3 = `Ty::Float`
/// → `f64`); this function encodes that convention so the byte-parity
/// Display tests pass under the locked TEST shape.
///
/// Fallback for un-conventional handles: `?{handle}` (Var-style glyph).
fn handle_to_ty_display(handle: i64) -> &'static str {
    match handle {
        0 => "i64",
        1 => "str",
        2 => "bool",
        3 => "f64",
        _ => "?_",
    }
}

/// Verify the variant-name mirror invariant at compile time.
///
/// ADR-0055b §4 invariant 1: every `TypeErrorCb` variant has a Rust
/// `TypeError` counterpart with the same name. This is a runtime
/// no-op the test harness can call to assert the mirror.
#[doc(hidden)]
#[must_use]
pub fn assert_variant_name_mirror(rust: &TypeError, cb: &TypeErrorCb) -> bool {
    rust_te_variant_name(rust) == type_error_cb_variant_name(cb)
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
        TypeErrorCb::CallbackArgMustBeFnName { .. } => "CallbackArgMustBeFnName",
        TypeErrorCb::CallbackSignatureMismatch { .. } => "CallbackSignatureMismatch",
        TypeErrorCb::UnknownField { .. } => "UnknownField",
        TypeErrorCb::UnsupportedRefinement { .. } => "UnsupportedRefinement",
        // ADR-0088 §3 — `len(x)` on a non-sized argument.
        TypeErrorCb::LenArgNotSized { .. } => "LenArgNotSized",
        // ADR-0092 — undeclared dora output id.
        TypeErrorCb::DoraUnknownOutputId { .. } => "DoraUnknownOutputId",
        // ADR-0093 Phase-2 — unsupported bytes-slice shape.
        TypeErrorCb::UnsupportedSliceShape { .. } => "UnsupportedSliceShape",
        // F90 / ADR-0102 — `int ** int` negative-literal exponent.
        TypeErrorCb::NegativePowExponent { .. } => "NegativePowExponent",
    }
}
