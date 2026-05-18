//! Parity harness contract types — ADR-0055e Phase H Wave 1.
//!
//! This crate is TEST scope only (F28 strict-separation). All
//! `canonicalize` / `parity_check` *implementations* are DEV scope:
//! stubs here return `todo!()` so that the harness contract and the
//! property-test corpus compile and are inspectable without any impl.
//!
//! ## Public surface
//!
//! - [`CanonicalKey`] — dense-pack canonical representation of a `Ty` tree.
//! - [`TyArena`] — stub arena handle passed to `Canonicalize` implementors.
//! - [`Canonicalize`] — the trait DEV implements for `Ty` + `TypeError`.
//! - [`parity_check`] — diff entrypoint: `Ok(())` iff canonical outputs match.
//! - [`ParityError`] — the 5 BLOCK-rule failure kinds per ADR-0055e §6.
//!
//! ## Canonicalization namespaces (§3 amendment 2026-05-18)
//!
//! 5 primary arenas: `TyId`, `AdtId`, `AliasId`, `FnTyId`, `RecordId`.
//! Each gets an independent dense-pack counter. `VarId` + `GenericVar`
//! are auxiliary namespaces with the same dense-pack rule.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use cobrust_types::{AdtId, AliasId, GenericVar, Ty, TypeError, VarId};

// =====================================================================
// CanonicalKey — post-order dense-pack canonical form of a Ty tree
// =====================================================================

/// Dense-pack canonical representation produced by post-order traversal
/// of a `Ty` tree.  Two structurally-equivalent types canonicalize to
/// the same `CanonicalKey` regardless of raw arena ids.
///
/// DEV implements `From<&Ty>` + the full traversal; this stub captures
/// the shape so the test corpus can reference the type today.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CanonicalKey {
    /// Variant tag (e.g. `"Int"`, `"List"`, `"Dict"`, `"Adt"`, …).
    pub kind: String,
    /// Children in post-order traversal order (canonical ids, not raw).
    pub children: Vec<CanonicalKey>,
}

impl CanonicalKey {
    /// Leaf constructor — kinds with no children.
    #[must_use]
    pub fn leaf(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            children: vec![],
        }
    }

    /// Node constructor — kind with ordered children.
    #[must_use]
    pub fn node(kind: &str, children: Vec<CanonicalKey>) -> Self {
        Self {
            kind: kind.to_string(),
            children,
        }
    }
}

// =====================================================================
// TyArena — stub arena context for Canonicalize implementations
// =====================================================================

/// Stub arena context.  Phase 1 does not require actual arena
/// indirection (Ty is recursive, not indexed). DEV extends this to
/// hold the 5-namespace dense-pack allocators per §3 amendment.
///
/// The 5 primary canonical namespaces (§3 amendment):
///  - TyId counter (for `Ty::Var` / `Ty::Generic` references)
///  - AdtId counter
///  - AliasId counter
///  - FnTyId counter  (FnTyArena per ADR-0055a §3)
///  - RecordId counter (RecordArena per ADR-0055a §3)
///
/// Auxiliary:
///  - VarId counter
///  - GenericVar counter
#[derive(Debug, Default)]
pub struct TyArena {
    /// AdtId renaming map: raw `AdtId` → canonical dense-pack id.
    pub adt_canon: HashMap<AdtId, u32>,
    /// AliasId renaming map: raw `AliasId` → canonical dense-pack id.
    pub alias_canon: HashMap<AliasId, u32>,
    /// VarId renaming map: raw `VarId` → canonical dense-pack id.
    pub var_canon: HashMap<VarId, u32>,
    /// GenericVar renaming map: raw `GenericVar` → canonical dense-pack id.
    pub generic_canon: HashMap<GenericVar, u32>,
    /// FnTyId counter (dense-pack; no raw ids in Phase 1 Ty tree).
    pub fn_ty_counter: u32,
    /// RecordId counter (dense-pack; no raw ids in Phase 1 Ty tree).
    pub record_counter: u32,
}

impl TyArena {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Canonical dense-pack id for an `AdtId` (first-encounter order).
    pub fn adt_id(&mut self, raw: AdtId) -> u32 {
        let next = self.adt_canon.len() as u32;
        *self.adt_canon.entry(raw).or_insert(next)
    }

    /// Canonical dense-pack id for an `AliasId`.
    pub fn alias_id(&mut self, raw: AliasId) -> u32 {
        let next = self.alias_canon.len() as u32;
        *self.alias_canon.entry(raw).or_insert(next)
    }

    /// Canonical dense-pack id for a `VarId`.
    pub fn var_id(&mut self, raw: VarId) -> u32 {
        let next = self.var_canon.len() as u32;
        *self.var_canon.entry(raw).or_insert(next)
    }

    /// Canonical dense-pack id for a `GenericVar`.
    pub fn generic_var(&mut self, raw: GenericVar) -> u32 {
        let next = self.generic_canon.len() as u32;
        *self.generic_canon.entry(raw).or_insert(next)
    }
}

// =====================================================================
// Canonicalize trait — DEV implements for Ty + TypeError
// =====================================================================

/// Contract trait: produce a [`CanonicalKey`] from `&self` given a
/// mutable arena context.
///
/// DEV implements this for `Ty` (post-order traversal per §3) and for
/// `TypeError` (canonicalize payload Ty + raw Span equality per §6).
///
/// **Phase 1 stubs**: default impl panics via `todo!()` so the harness
/// can reference the trait in property tests without requiring DEV impl.
pub trait Canonicalize {
    fn canonicalize(&self, arena: &mut TyArena) -> CanonicalKey;
}

/// Stub implementation for `Ty` — DEV replaces with real post-order
/// traversal per ADR-0055e §3.
impl Canonicalize for Ty {
    fn canonicalize(&self, _arena: &mut TyArena) -> CanonicalKey {
        todo!("ADR-0055e DEV: implement post-order canonicalization for Ty")
    }
}

/// Stub implementation for `TypeError` — DEV implements per §6 BLOCK
/// rules: canonicalize Ty payloads, assert raw Span equality, assert
/// suggestion equality.
impl Canonicalize for TypeError {
    fn canonicalize(&self, _arena: &mut TyArena) -> CanonicalKey {
        todo!("ADR-0055e DEV: implement TypeError canonicalization")
    }
}

// =====================================================================
// ParityError — 5 BLOCK rules per ADR-0055e §6
// =====================================================================

/// Harness failure type. Each variant encodes one of the 5 BLOCK rules
/// from ADR-0055e §6. Any `ParityError` → test binary fails → CI fails
/// → Phase H ratification halts.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ParityError {
    /// BLOCK rule 1: one impl accepted, the other rejected.
    #[error("accept/reject divergence: rust={rust_accepted}, cb={cb_accepted}")]
    AcceptReject {
        rust_accepted: bool,
        cb_accepted: bool,
    },

    /// BLOCK rule 2: both rejected but with different `TypeError` variant names.
    #[error("TypeError variant mismatch: rust={rust_variant}, cb={cb_variant}")]
    VariantMismatch {
        rust_variant: String,
        cb_variant: String,
    },

    /// BLOCK rule 3: `Span` raw byte-offset divergence on any
    /// `TypeError` variant. `Span` is **not** canonicalized per §3.
    #[error("Span raw mismatch on variant `{variant}`: rust_span={rust_span:?}, cb_span={cb_span:?}")]
    SpanRawMismatch {
        variant: String,
        rust_span: String,
        cb_span: String,
    },

    /// BLOCK rule 4: `suggestion` field divergence.
    #[error("suggestion field mismatch on variant `{variant}`: rust={rust_suggestion:?}, cb={cb_suggestion:?}")]
    SuggestionMismatch {
        variant: String,
        rust_suggestion: Option<String>,
        cb_suggestion: Option<String>,
    },

    /// BLOCK rule 5: canonical `Ty` payload divergence.  The
    /// `CanonicalKey` strings are the serialized JSON form for
    /// readability in diagnostics.
    #[error("canonical Ty payload divergence: rust_key={rust_key}, cb_key={cb_key}")]
    CanonicalPayloadMismatch {
        rust_key: String,
        cb_key: String,
    },
}

// =====================================================================
// parity_check — the harness entrypoint
// =====================================================================

/// Run the parity check between a Rust-side value and a cb-side value.
///
/// Both `T: Canonicalize` — canonicalization is run in the same fresh
/// `TyArena` per §3 (caller manages arena lifetime for cross-input
/// state isolation).
///
/// Returns `Ok(())` iff the canonical keys match; `Err(ParityError)`
/// naming the first BLOCK-rule violation.
///
/// DEV implements the full diff logic (accept/reject + variant check +
/// Span check + suggestion check + payload check). This stub satisfies
/// the type signature so property tests can reference it.
pub fn parity_check<T: Canonicalize>(
    rust: &T,
    cb: &T,
    arena: &mut TyArena,
) -> Result<(), ParityError> {
    let rust_key = rust.canonicalize(arena);
    let cb_key = cb.canonicalize(arena);
    if rust_key != cb_key {
        return Err(ParityError::CanonicalPayloadMismatch {
            rust_key: serde_json::to_string(&rust_key)
                .unwrap_or_else(|_| format!("{rust_key:?}")),
            cb_key: serde_json::to_string(&cb_key)
                .unwrap_or_else(|_| format!("{cb_key:?}")),
        });
    }
    Ok(())
}

/// Variant-name discriminant for a `TypeError` (string form).
///
/// Used by `ParityError::VariantMismatch` to report which variant
/// each impl produced without requiring full canonicalization.
#[must_use]
pub fn type_error_variant_name(err: &TypeError) -> &'static str {
    match err {
        TypeError::UnknownName { .. } => "UnknownName",
        TypeError::ArityMismatch { .. } => "ArityMismatch",
        TypeError::KeywordArgMismatch { .. } => "KeywordArgMismatch",
        TypeError::MissingArgument { .. } => "MissingArgument",
        TypeError::TypeMismatch { .. } => "TypeMismatch",
        TypeError::NonExhaustiveMatch { .. } => "NonExhaustiveMatch",
        TypeError::RowConflict { .. } => "RowConflict",
        TypeError::ImplicitTruthiness { .. } => "ImplicitTruthiness",
        TypeError::UseOfDroppedFeature { .. } => "UseOfDroppedFeature",
        TypeError::MutableDefault { .. } => "MutableDefault",
        TypeError::AmbiguousType { .. } => "AmbiguousType",
        TypeError::DuplicateField { .. } => "DuplicateField",
        TypeError::OccursCheck { .. } => "OccursCheck",
        TypeError::NotCallable { .. } => "NotCallable",
        TypeError::NotIndexable { .. } => "NotIndexable",
        TypeError::NotIterable { .. } => "NotIterable",
        TypeError::BreakOutsideLoop { .. } => "BreakOutsideLoop",
        TypeError::ContinueOutsideLoop { .. } => "ContinueOutsideLoop",
        TypeError::ReturnOutsideFn { .. } => "ReturnOutsideFn",
        TypeError::YieldOutsideFn { .. } => "YieldOutsideFn",
        TypeError::NotHashable { .. } => "NotHashable",
        TypeError::DictSpreadNotSupported { .. } => "DictSpreadNotSupported",
        TypeError::Multiple(_) => "Multiple",
        TypeError::BorrowOfNonPlace { .. } => "BorrowOfNonPlace",
        TypeError::UnknownMethod { .. } => "UnknownMethod",
    }
}

// =====================================================================
// Manual CanonicalKey canonicalization helpers (TEST-scope utilities)
// =====================================================================

/// Build a `CanonicalKey` for a `Ty` using only the public Ty API —
/// used by property tests to construct expected keys before DEV ships
/// the real `Canonicalize` impl.
///
/// This is an **approximation** for test harness authoring only; the
/// DEV impl in `canon.rs` is authoritative. The approximation is
/// correct for leaf types and single-level containers; deeply nested
/// Adt/Alias ids are NOT remapped here (raw id used as string).
#[must_use]
pub fn manual_canonical_key(ty: &Ty) -> CanonicalKey {
    match ty {
        Ty::Bool => CanonicalKey::leaf("Bool"),
        Ty::Int => CanonicalKey::leaf("Int"),
        Ty::Float => CanonicalKey::leaf("Float"),
        Ty::Imag => CanonicalKey::leaf("Imag"),
        Ty::Str => CanonicalKey::leaf("Str"),
        Ty::Bytes => CanonicalKey::leaf("Bytes"),
        Ty::None => CanonicalKey::leaf("None"),
        Ty::Never => CanonicalKey::leaf("Never"),
        Ty::Tuple(items) => CanonicalKey::node(
            "Tuple",
            items.iter().map(manual_canonical_key).collect(),
        ),
        Ty::List(inner) => CanonicalKey::node("List", vec![manual_canonical_key(inner)]),
        Ty::Set(inner) => CanonicalKey::node("Set", vec![manual_canonical_key(inner)]),
        Ty::Dict(k, v) => CanonicalKey::node(
            "Dict",
            vec![manual_canonical_key(k), manual_canonical_key(v)],
        ),
        Ty::Record(r) => {
            // fields are BTreeMap — already sorted by name, deterministic.
            let children: Vec<CanonicalKey> = r
                .fields
                .iter()
                .map(|(name, t)| CanonicalKey::node(name.as_str(), vec![manual_canonical_key(t)]))
                .collect();
            CanonicalKey::node("Record", children)
        }
        Ty::Fn(fn_ty) => {
            let mut children: Vec<CanonicalKey> =
                fn_ty.positional.iter().map(manual_canonical_key).collect();
            for (name, t) in &fn_ty.named {
                children.push(CanonicalKey::node(
                    name.as_str(),
                    vec![manual_canonical_key(t)],
                ));
            }
            children.push(CanonicalKey::node(
                "->",
                vec![manual_canonical_key(&fn_ty.return_ty)],
            ));
            CanonicalKey::node("Fn", children)
        }
        // Raw id used — DEV replaces with dense-pack canonical id.
        Ty::Adt(id, args) => CanonicalKey::node(
            &format!("Adt#{}", id.0),
            args.iter().map(manual_canonical_key).collect(),
        ),
        Ty::Alias(id, args) => CanonicalKey::node(
            &format!("Alias#{}", id.0),
            args.iter().map(manual_canonical_key).collect(),
        ),
        // Raw id used — DEV replaces with dense-pack canonical id.
        Ty::Generic(g) => CanonicalKey::leaf(&format!("Generic#{}", g.0)),
        Ty::Var(v) => CanonicalKey::leaf(&format!("Var#{}", v.0)),
        Ty::Ref(inner) => CanonicalKey::node("Ref", vec![manual_canonical_key(inner)]),
    }
}
