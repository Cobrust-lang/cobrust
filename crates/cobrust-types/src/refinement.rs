//! ADR-0080 Phase-1b-ii — per-field value refinements for validated-body
//! `class`es.
//!
//! A [`Refinement`] is the structured form of a class field's `where`-clause
//! predicate. Per ADR-0080 §2 Q2/Q6 it lives in a side-table keyed by
//! `(AdtId, field)` (see `check::Ctx::adt_refinements` /
//! `TypedModule::adt_refinements`), **not** in [`crate::ty::Ty`] — widening
//! `Ty::Int` to carry a predicate would ripple through every
//! `unify`/`subst`/`is_hashable` arm (high blast radius). The predicate is
//! metadata BESIDE the field, read by two projections of the ONE field
//! table (ADR-0080 §3 footgun #4 — cannot drift):
//!
//! - the boundary VALIDATOR (`cobrust-pit`'s `route_validated` trampoline),
//!   which range-checks the deserialized value and renders a typed 422 on a
//!   miss (ADR-0080 §5.4);
//! - the OpenAPI EMITTER (ADR-0080 §5.3 — a Phase-1b-iii deliverable; this
//!   crate carries the side-table it will walk).
//!
//! # Phase-1b-ii scope
//!
//! ONLY the INT-RANGE kind ([`Refinement::IntRange`]) is admitted in this
//! phase (ADR-0080 §6 Phase-1 + Q7). The fixed grammar (ADR-0080 Q6) is
//! `lo <= self <= hi` and its one-sided forms `lo <= self` / `self <= hi`
//! on an `i64` field. `len(self) <= n` (str length, Phase-2) and
//! `pattern(self, "<re>")` (Phase-3) are LATER phases; the type checker
//! rejects them — and any non-fixed predicate — with
//! [`crate::error::TypeError::UnsupportedRefinement`] carrying a §2.5-B FIX.

/// A structured value-level refinement on a class field. The variant set
/// grows by phase (ADR-0080 §6); Phase-1b-ii ships [`Self::IntRange`] only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refinement {
    /// An integer range bound on an `i64` field. At least one of `lo`/`hi`
    /// is `Some` (a `where`-clause with neither bound is meaningless and is
    /// rejected at parse-interpretation). Bounds are INCLUSIVE
    /// (`lo <= self`, `self <= hi`) — the only relational form the fixed
    /// grammar admits (ADR-0080 Q6). The OpenAPI projection maps `lo` →
    /// `minimum` and `hi` → `maximum` (ADR-0080 §5.3).
    IntRange { lo: Option<i64>, hi: Option<i64> },
}

impl Refinement {
    /// Render this refinement into the compact schema-descriptor suffix the
    /// `route_validated` trampoline parses (ADR-0080 §5.4). For an int
    /// range the suffix is `:<lo>:<hi>` where an absent bound is the empty
    /// string (e.g. `0 <= self` → `:0:`, `self <= 100` → `::100`,
    /// `0 <= self <= 100` → `:0:100`). Consumed by `cobrust-pit`'s
    /// `validate_against_schema` (the SAME source the OpenAPI emitter will
    /// walk — they cannot drift, ADR-0080 §3 footgun #4).
    #[must_use]
    pub fn schema_suffix(&self) -> String {
        match self {
            Self::IntRange { lo, hi } => {
                let lo_s = lo.map_or(String::new(), |n| n.to_string());
                let hi_s = hi.map_or(String::new(), |n| n.to_string());
                format!(":{lo_s}:{hi_s}")
            }
        }
    }
}
