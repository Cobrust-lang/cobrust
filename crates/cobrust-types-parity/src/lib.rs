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
// `*_canon.len() as u32` casts are bounded by the canonical-id namespace
// allocation — the canonicalization arena cannot exceed u32::MAX entries
// (no realistic Ty tree approaches this). F37-honest: pre-existing pattern
// across the dense-pack arena; surface here for the new arena-id sites.
#![allow(clippy::cast_possible_truncation)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use cobrust_types::{AdtId, AliasId, FnTy, GenericVar, Record, Ty, TypeError, VarId};

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
    /// ADR-0055b: TyPayload encounter counter for `TypeError` Ty-bearing
    /// variants. Each Ty payload inside a `TypeError` arm gets a
    /// position-based canonical id (handle-equivalent), preserving
    /// per-variant Ty-payload count without recursing into Ty kind.
    /// This is required for parity_check between Rust `TypeError` (with
    /// inline `Ty`) and cb `TypeErrorCb` (with `i64` arena handle); both
    /// sides emit `TyPayload#{n}` in encounter order.
    pub ty_payload_counter: u32,
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

    /// Allocate a new canonical FnTyId (dense-pack counter).
    pub fn fresh_fn_ty_id(&mut self) -> u32 {
        let id = self.fn_ty_counter;
        self.fn_ty_counter = self.fn_ty_counter.checked_add(1).expect("FnTyId overflow");
        id
    }

    /// Allocate a new canonical RecordId (dense-pack counter).
    pub fn fresh_record_id(&mut self) -> u32 {
        let id = self.record_counter;
        self.record_counter = self
            .record_counter
            .checked_add(1)
            .expect("RecordId overflow");
        id
    }

    /// Allocate a new TyPayload encounter id for `TypeError` Ty fields.
    /// ADR-0055b: parity_check between Rust `TypeError` and cb
    /// `TypeErrorCb` uses position-based Ty-payload canonical ids so the
    /// cb-side `i64` arena handle matches the Rust-side inline `Ty`
    /// without requiring structural Ty introspection.
    pub fn fresh_ty_payload_id(&mut self) -> u32 {
        let id = self.ty_payload_counter;
        self.ty_payload_counter = self
            .ty_payload_counter
            .checked_add(1)
            .expect("ty_payload_counter overflow");
        id
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

/// Post-order canonicalization for `Ty` per ADR-0055e §3.
///
/// Algorithm:
///  1. For each `Ty` node, recurse on children first (post-order).
///  2. For nodes carrying raw arena ids (`Var`, `Generic`, `Adt`, `Alias`),
///     consult `TyArena` for the dense-pack canonical id; first
///     encounter assigns `0`, the second distinct raw id assigns `1`,
///     and so on. Repeat encounters of the same raw id reuse their
///     canonical id (this is the arena-id renaming tolerance that
///     makes `Var(VarId(7))` ≡ `Var(VarId(3))` when both are
///     first-encountered in the same traversal slot).
///  3. Emit a `CanonicalKey { kind, children }` tuple per ADR-0055e §3.
///
/// 5 independent namespaces per §3 amendment 2026-05-18:
///  - `AdtId`, `AliasId`, `VarId`, `GenericVar` — first-encounter rename.
///  - `FnTyId`, `RecordId` — dense-pack on construction (no raw id today;
///    `Ty::Fn` and `Ty::Record` are inline payload, no arena-id
///    indirection at the recursive `Ty` level).
impl Canonicalize for Ty {
    fn canonicalize(&self, arena: &mut TyArena) -> CanonicalKey {
        match self {
            Ty::Bool => CanonicalKey::leaf("Bool"),
            Ty::Int => CanonicalKey::leaf("Int"),
            Ty::Float => CanonicalKey::leaf("Float"),
            Ty::Imag => CanonicalKey::leaf("Imag"),
            Ty::Str => CanonicalKey::leaf("Str"),
            Ty::Bytes => CanonicalKey::leaf("Bytes"),
            Ty::None => CanonicalKey::leaf("None"),
            Ty::Never => CanonicalKey::leaf("Never"),
            Ty::Tuple(items) => {
                let children = items.iter().map(|t| t.canonicalize(arena)).collect();
                CanonicalKey::node("Tuple", children)
            }
            Ty::List(inner) => {
                let c = inner.canonicalize(arena);
                CanonicalKey::node("List", vec![c])
            }
            Ty::Set(inner) => {
                let c = inner.canonicalize(arena);
                CanonicalKey::node("Set", vec![c])
            }
            Ty::Dict(k, v) => {
                let kc = k.canonicalize(arena);
                let vc = v.canonicalize(arena);
                CanonicalKey::node("Dict", vec![kc, vc])
            }
            Ty::Record(r) => canonicalize_record(r, arena),
            Ty::Fn(fn_ty) => canonicalize_fn(fn_ty, arena),
            Ty::Adt(id, args) => {
                let canon = arena.adt_id(*id);
                let children = args.iter().map(|t| t.canonicalize(arena)).collect();
                CanonicalKey::node(&format!("Adt#{canon}"), children)
            }
            Ty::Alias(id, args) => {
                let canon = arena.alias_id(*id);
                let children = args.iter().map(|t| t.canonicalize(arena)).collect();
                CanonicalKey::node(&format!("Alias#{canon}"), children)
            }
            Ty::Generic(g) => {
                let canon = arena.generic_var(*g);
                CanonicalKey::leaf(&format!("Generic#{canon}"))
            }
            Ty::Var(v) => {
                let canon = arena.var_id(*v);
                CanonicalKey::leaf(&format!("Var#{canon}"))
            }
            Ty::Ref(inner) => {
                let c = inner.canonicalize(arena);
                CanonicalKey::node("Ref", vec![c])
            }
            // ADR-0060b + ADR-0060c additions: IntN width tier + fixed-size
            // Array. No arena-id renaming required — both carry inline
            // primitive payloads (width + length); canonical form mirrors
            // `Display`.
            Ty::IntN(w) => CanonicalKey::leaf(&format!("IntN#{w}")),
            Ty::Array(elem, n) => {
                let c = elem.canonicalize(arena);
                CanonicalKey::node(&format!("Array#{n}"), vec![c])
            }
        }
    }
}

/// Helper: canonicalize a `Record` per ADR-0055e §3 amendment 2026-05-18
/// RecordId namespace. The fields are BTreeMap-ordered (sorted by name)
/// → deterministic. A fresh `RecordId` is allocated per `Record`
/// occurrence; children are the field-tagged sub-keys.
fn canonicalize_record(r: &Record, arena: &mut TyArena) -> CanonicalKey {
    let _rec_id = arena.fresh_record_id();
    let children: Vec<CanonicalKey> = r
        .fields
        .iter()
        .map(|(name, t)| CanonicalKey::node(name.as_str(), vec![t.canonicalize(arena)]))
        .collect();
    CanonicalKey::node("Record", children)
}

/// Helper: canonicalize a `FnTy` per ADR-0055e §3 amendment 2026-05-18
/// FnTyId namespace. A fresh `FnTyId` is allocated per `FnTy`
/// occurrence; children are positional params + named-param tagged
/// pairs + a `"->"` return-type child.
fn canonicalize_fn(fn_ty: &FnTy, arena: &mut TyArena) -> CanonicalKey {
    let _fn_id = arena.fresh_fn_ty_id();
    let mut children: Vec<CanonicalKey> = fn_ty
        .positional
        .iter()
        .map(|t| t.canonicalize(arena))
        .collect();
    for (name, t) in &fn_ty.named {
        children.push(CanonicalKey::node(
            name.as_str(),
            vec![t.canonicalize(arena)],
        ));
    }
    if let Some(vp) = &fn_ty.var_positional {
        children.push(CanonicalKey::node("*args", vec![vp.canonicalize(arena)]));
    }
    if let Some(vk) = &fn_ty.var_keyword {
        children.push(CanonicalKey::node("**kwargs", vec![vk.canonicalize(arena)]));
    }
    children.push(CanonicalKey::node(
        "->",
        vec![fn_ty.return_ty.canonicalize(arena)],
    ));
    CanonicalKey::node("Fn", children)
}

/// `TypeError` canonicalization per ADR-0055e §6.
///
/// The canonical key encodes the variant name + canonicalized `Ty`
/// payloads. `Span` and `suggestion` are intentionally **NOT** folded
/// into the canonical key — they are diffed raw by dedicated rules in
/// `parity_check` (rule 3: Span raw equality; rule 4: suggestion
/// equality). Folding them here would collapse rule 3/4 with rule 5
/// and lose the BLOCK-rule discrimination ADR-0055e §6 requires.
///
/// `Multiple(errs)` is canonicalized by canonicalizing each child error
/// in order — order is significant (per ADR-0055e §3 traversal-order
/// determinism).
impl Canonicalize for TypeError {
    fn canonicalize(&self, arena: &mut TyArena) -> CanonicalKey {
        let variant = type_error_variant_name(self);
        let children: Vec<CanonicalKey> = match self {
            // ADR-0055b: Ty-payload encoded as positional handle so cb
            // mirror (with `i64` arena handle) produces an equal canonical
            // key without requiring structural Ty introspection.
            TypeError::TypeMismatch { .. } => {
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
            TypeError::RowConflict { field, .. } => {
                let t1 = arena.fresh_ty_payload_id();
                let t2 = arena.fresh_ty_payload_id();
                vec![
                    CanonicalKey::leaf(field.as_str()),
                    CanonicalKey::node("ty1", vec![CanonicalKey::leaf(&format!("TyPayload#{t1}"))]),
                    CanonicalKey::node("ty2", vec![CanonicalKey::leaf(&format!("TyPayload#{t2}"))]),
                ]
            }
            TypeError::ImplicitTruthiness { .. }
            | TypeError::NotCallable { .. }
            | TypeError::NotIndexable { .. }
            | TypeError::NotIterable { .. }
            // ADR-0088 §3 — `LenArgNotSized` carries a single `actual: Ty`
            // payload, the same shape as the truthiness/not-callable group.
            | TypeError::LenArgNotSized { .. }
            | TypeError::NotHashable { .. } => {
                let a = arena.fresh_ty_payload_id();
                vec![CanonicalKey::node(
                    "actual",
                    vec![CanonicalKey::leaf(&format!("TyPayload#{a}"))],
                )]
            }
            TypeError::OccursCheck { var, .. } => {
                let canon_var = arena.var_id(*var);
                let t = arena.fresh_ty_payload_id();
                vec![
                    CanonicalKey::leaf(&format!("Var#{canon_var}")),
                    CanonicalKey::node("ty", vec![CanonicalKey::leaf(&format!("TyPayload#{t}"))]),
                ]
            }
            TypeError::NonExhaustiveMatch { uncovered, .. } => uncovered
                .iter()
                .map(|s| CanonicalKey::leaf(s.as_str()))
                .collect(),
            TypeError::Multiple(errs) => errs.iter().map(|e| e.canonicalize(arena)).collect(),
            TypeError::UnknownName { name, .. }
            | TypeError::KeywordArgMismatch { name, .. }
            | TypeError::MissingArgument { name, .. }
            | TypeError::DuplicateField { name, .. } => vec![CanonicalKey::leaf(name.as_str())],
            TypeError::ArityMismatch {
                expected, actual, ..
            } => vec![
                CanonicalKey::leaf(&format!("expected={expected}")),
                CanonicalKey::leaf(&format!("actual={actual}")),
            ],
            TypeError::UseOfDroppedFeature { name, .. } => vec![CanonicalKey::leaf(name)],
            TypeError::UnknownMethod {
                type_name,
                method_name,
                ..
            } => vec![
                CanonicalKey::leaf(type_name.as_str()),
                CanonicalKey::leaf(method_name.as_str()),
            ],
            // ADR-0080 Phase-1a — key on the offending field + the
            // declared-field list (both String, mirror-able). The
            // `adt` Ty payload is elided per the parity convention that
            // avoids over-keying on Ty renderings that differ per run.
            TypeError::UnknownField {
                field,
                known_fields,
                ..
            } => {
                let mut keys = vec![CanonicalKey::leaf(field.as_str())];
                keys.extend(known_fields.iter().map(|f| CanonicalKey::leaf(f.as_str())));
                keys
            }
            // ADR-0080 Phase-1b-ii — key on the offending field name (a
            // mirror-able String). The accepted-grammar text lives in the
            // Display message, not the canonical key.
            TypeError::UnsupportedRefinement { field, .. } => {
                vec![CanonicalKey::leaf(field.as_str())]
            }
            // ADR-0092 — key on the offending id + the declared-output
            // list (both String, mirror-able). The `nearest` suggestion is
            // a Display-only derivation, elided from the key (same shape as
            // `UnknownField`'s `field` + `known_fields`).
            TypeError::DoraUnknownOutputId { id, declared, .. } => {
                let mut keys = vec![CanonicalKey::leaf(id.as_str())];
                keys.extend(declared.iter().map(|d| CanonicalKey::leaf(d.as_str())));
                keys
            }
            // Variants with no extra payload (Span + suggestion only).
            TypeError::MutableDefault { .. }
            | TypeError::AmbiguousType { .. }
            | TypeError::BreakOutsideLoop { .. }
            | TypeError::ContinueOutsideLoop { .. }
            | TypeError::ReturnOutsideFn { .. }
            | TypeError::YieldOutsideFn { .. }
            | TypeError::DictSpreadNotSupported { .. }
            | TypeError::BorrowOfNonPlace { .. }
            // ADR-0073 callback-slot variants — also payload-free for
            // parity canonicalization (the FnTy payload of
            // CallbackSignatureMismatch is intentionally elided here;
            // child keys would over-key on FnTy renderings that
            // legitimately differ across runs).
            | TypeError::CallbackArgMustBeFnName { .. }
            | TypeError::CallbackSignatureMismatch { .. }
            // ADR-0093 Phase-2 — unsupported bytes-slice shape; payload-free
            // (Span + suggestion only). The supported `b[lo:hi]` form lives
            // in the Display message, not the canonical key.
            | TypeError::UnsupportedSliceShape { .. }
            // F90 / ADR-0102 — `int ** int` negative-literal exponent;
            // payload-free (Span + suggestion only). The FIX (use a float
            // base) lives in the Display message, not the canonical key.
            | TypeError::NegativePowExponent { .. } => vec![],
        };
        CanonicalKey::node(variant, children)
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
    #[error(
        "Span raw mismatch on variant `{variant}`: rust_span={rust_span:?}, cb_span={cb_span:?}"
    )]
    SpanRawMismatch {
        variant: String,
        rust_span: String,
        cb_span: String,
    },

    /// BLOCK rule 4: `suggestion` field divergence.
    #[error(
        "suggestion field mismatch on variant `{variant}`: rust={rust_suggestion:?}, cb={cb_suggestion:?}"
    )]
    SuggestionMismatch {
        variant: String,
        rust_suggestion: Option<String>,
        cb_suggestion: Option<String>,
    },

    /// BLOCK rule 5: canonical `Ty` payload divergence.  The
    /// `CanonicalKey` strings are the serialized JSON form for
    /// readability in diagnostics.
    #[error("canonical Ty payload divergence: rust_key={rust_key}, cb_key={cb_key}")]
    CanonicalPayloadMismatch { rust_key: String, cb_key: String },
}

// =====================================================================
// parity_check — the harness entrypoint
// =====================================================================

/// Run the parity check between a Rust-side value and a cb-side value.
///
/// Both `T: Canonicalize` — canonicalization is run in **independent
/// fresh sub-arenas** so that arena-id renaming on each side is
/// computed in isolation. This preserves the §3 tolerance: two `Var`
/// types with different raw ids both rename to canonical id `0` when
/// each is the first var encountered on its own side.
///
/// Returns `Ok(())` iff the canonical keys match; `Err(ParityError)`
/// naming the first BLOCK-rule violation.
///
/// **Scope**: this generic entrypoint implements BLOCK rule 5
/// (canonical-key equality) of ADR-0055e §6. Rules 1-4
/// (accept/reject + variant + Span raw + suggestion) apply at the
/// `Result<_, TypeError>` level and are exercised by the Phase 3 cb
/// runner once it lands. The `ParityError` variants for rules 1-4 are
/// preserved in the public surface so the cb runner can construct them
/// without surface-breaking changes.
pub fn parity_check<R: Canonicalize, C: Canonicalize>(
    rust: &R,
    cb: &C,
    _arena: &mut TyArena,
) -> Result<(), ParityError> {
    // Each side gets its own fresh sub-arena: arena-id renaming is
    // a per-impl operation that must NOT cross over (a `Var(7)` on the
    // Rust side and a `Var(3)` on the cb side both rename to `0`
    // independently → equal canonical keys → Ok). Sharing the arena
    // would over-merge namespaces and either over-tolerate (collapse
    // distinct ids that happen to coincide in canonical order) or
    // under-tolerate (renumber so the second side starts at `N+1`
    // instead of `0`).
    let mut rust_arena = TyArena::new();
    let mut cb_arena = TyArena::new();
    let rust_key = rust.canonicalize(&mut rust_arena);
    let cb_key = cb.canonicalize(&mut cb_arena);
    if rust_key == cb_key {
        Ok(())
    } else {
        let rust_str = serde_json::to_string(&rust_key).unwrap_or_else(|_| format!("{rust_key:?}"));
        let cb_str = serde_json::to_string(&cb_key).unwrap_or_else(|_| format!("{cb_key:?}"));
        Err(ParityError::CanonicalPayloadMismatch {
            rust_key: rust_str,
            cb_key: cb_str,
        })
    }
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
        // ADR-0073 callback-slot variants.
        TypeError::CallbackArgMustBeFnName { .. } => "CallbackArgMustBeFnName",
        TypeError::CallbackSignatureMismatch { .. } => "CallbackSignatureMismatch",
        // ADR-0080 Phase-1a — typed-field-access variant.
        TypeError::UnknownField { .. } => "UnknownField",
        // ADR-0080 Phase-1b-ii — non-fixed refinement-predicate variant.
        TypeError::UnsupportedRefinement { .. } => "UnsupportedRefinement",
        // ADR-0088 §3 — `len(x)` on a non-sized argument.
        TypeError::LenArgNotSized { .. } => "LenArgNotSized",
        // ADR-0092 — undeclared dora output id.
        TypeError::DoraUnknownOutputId { .. } => "DoraUnknownOutputId",
        // ADR-0093 Phase-2 — unsupported bytes-slice shape.
        TypeError::UnsupportedSliceShape { .. } => "UnsupportedSliceShape",
        // F90 / ADR-0102 — `int ** int` negative-literal exponent.
        TypeError::NegativePowExponent { .. } => "NegativePowExponent",
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
        Ty::Tuple(items) => {
            CanonicalKey::node("Tuple", items.iter().map(manual_canonical_key).collect())
        }
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
        // ADR-0060b + ADR-0060c additions: IntN width tier + fixed-size Array.
        // Manual canonical form mirrors `Display`: `i{N}` for IntN, `[T; N]` for Array.
        Ty::IntN(w) => CanonicalKey::leaf(&format!("IntN#{w}")),
        Ty::Array(elem, n) => {
            CanonicalKey::node(&format!("Array#{n}"), vec![manual_canonical_key(elem)])
        }
    }
}
