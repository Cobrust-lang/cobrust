//! ADR-0080 Phase-1b-ii / Phase-2 — per-field value refinements for
//! validated-body `class`es.
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
//!   which range/length/pattern-checks the deserialized value and renders a
//!   typed 422 on a miss (ADR-0080 §5.4);
//! - the OpenAPI EMITTER (ADR-0080 §5.3), which projects each refinement to
//!   `minimum`/`maximum` (int range), `minLength`/`maxLength` (str length),
//!   or `pattern` (str regex).
//!
//! # Phase scope
//!
//! - Phase-1b-ii ([`Refinement::IntRange`]) — `lo <= self <= hi` and its
//!   one-sided forms on an `i64` field (ADR-0080 §6 Phase-1 + Q7).
//! - Phase-2 ([`Refinement::StrLen`]) — `lo <= len(self) <= hi` and its
//!   one-sided forms (`len(self) <= n` / `len(self) >= n`) on a `str` field
//!   (ADR-0080 §6 Phase-2).
//! - Phase-2 ([`Refinement::Pattern`]) — `pattern(self, "<re>")` (a LITERAL
//!   regex) on a `str` field (ADR-0080 §6 Phase-3, landed together with the
//!   length form as "Phase-2 string refinements").
//!
//! Any non-fixed predicate — and any fixed form on the wrong base type — is
//! rejected by the type checker with
//! [`crate::error::TypeError::UnsupportedRefinement`] carrying a §2.5-B FIX.

/// A structured value-level refinement on a class field. The variant set
/// grows by phase (ADR-0080 §6).
///
/// `IntRange`/`StrLen` carry only `Copy` integer bounds; `Pattern` carries an
/// owned regex `String`, so the enum is `Clone` (not `Copy`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Refinement {
    /// An integer range bound on an `i64` field. At least one of `lo`/`hi`
    /// is `Some` (a `where`-clause with neither bound is meaningless and is
    /// rejected at parse-interpretation). Bounds are INCLUSIVE
    /// (`lo <= self`, `self <= hi`) — the only relational form the fixed
    /// grammar admits (ADR-0080 Q6). The OpenAPI projection maps `lo` →
    /// `minimum` and `hi` → `maximum` (ADR-0080 §5.3).
    IntRange { lo: Option<i64>, hi: Option<i64> },
    /// A string-LENGTH bound on a `str` field (ADR-0080 Phase-2). The fixed
    /// grammar is `lo <= len(self) <= hi` and its one-sided forms
    /// (`len(self) <= n` → `hi`; `len(self) >= n` → `lo`). Bounds are
    /// INCLUSIVE character-count bounds. The OpenAPI projection maps `lo` →
    /// `minLength` and `hi` → `maxLength` (ADR-0080 §5.3 line 331).
    StrLen { lo: Option<i64>, hi: Option<i64> },
    /// A regex PATTERN on a `str` field (ADR-0080 Phase-2/Phase-3). The
    /// fixed grammar is `pattern(self, "<re>")` with a LITERAL regex string.
    /// The validator regex-matches the deserialized string; the OpenAPI
    /// projection maps it to `pattern` (the raw regex, ADR-0080 §5.3 line
    /// 339).
    Pattern { regex: String },
}

impl Refinement {
    /// Render this refinement into the complete descriptor PAYLOAD segment
    /// (the part after the `field<TAB>` prefix) the `route_validated`
    /// trampoline parses (ADR-0080 §5.4). `base_kind` is the field's declared
    /// base-type token (`str`/`i64`/`f64`/`bool`) the descriptor would carry
    /// WITHOUT a refinement; a refinement may keep that token + append a
    /// suffix, or REPLACE it with a discriminating token:
    ///
    /// - [`Self::IntRange`] → `i64:<lo>:<hi>` (keeps `base_kind = i64`,
    ///   appends a `:lo:hi` numeric suffix; an absent bound is the empty
    ///   string — `0 <= self` → `i64:0:`, `self <= 100` → `i64::100`).
    /// - [`Self::StrLen`] → `str:<lo>:<hi>` (keeps `base_kind = str`, appends
    ///   the SAME `:lo:hi` numeric suffix shape, reusing the int-range
    ///   decode; the `str` kind tells the validator/emitter the bounds are
    ///   LENGTHS, not value ranges).
    /// - [`Self::Pattern`] → `pat:<regex>` (REPLACES the kind token with
    ///   `pat` and carries the raw regex as everything after the FIRST `:`,
    ///   so a `:` inside the regex is safe).
    ///
    /// This is the SINGLE encoding source; [`crate::Refinement`]'s consumer,
    /// `cobrust-pit`'s `parse_schema`, is the SINGLE decode source — the two
    /// cannot drift (ADR-0080 §3 footgun #4). The regex MUST NOT contain a
    /// literal TAB (`\t`), which would break the line's `field<TAB>payload`
    /// split; the fixed grammar carries the regex as a `.cb` string literal,
    /// where an embedded TAB is not produced by the parser.
    #[must_use]
    pub fn descriptor_payload(&self, base_kind: &str) -> String {
        match self {
            Self::IntRange { lo, hi } => format!("{base_kind}{}", int_suffix(*lo, *hi)),
            // StrLen reuses the int-range `:lo:hi` numeric suffix; the `str`
            // base kind discriminates LENGTH from value-range at decode.
            Self::StrLen { lo, hi } => format!("{base_kind}{}", int_suffix(*lo, *hi)),
            // Pattern REPLACES the kind token with `pat`; the regex follows
            // the first `:` verbatim (a `:` inside the regex is preserved).
            Self::Pattern { regex } => format!("pat:{regex}"),
        }
    }
}

/// The shared `:lo:hi` numeric suffix for the range/length kinds. An absent
/// bound is the empty string (`Some(0), None` → `:0:`; `None, Some(100)` →
/// `::100`; `Some(0), Some(100)` → `:0:100`).
fn int_suffix(lo: Option<i64>, hi: Option<i64>) -> String {
    let lo_s = lo.map_or(String::new(), |n| n.to_string());
    let hi_s = hi.map_or(String::new(), |n| n.to_string());
    format!(":{lo_s}:{hi_s}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_range_payload_keeps_i64_kind_and_appends_bounds() {
        assert_eq!(
            Refinement::IntRange {
                lo: Some(0),
                hi: Some(100)
            }
            .descriptor_payload("i64"),
            "i64:0:100"
        );
        assert_eq!(
            Refinement::IntRange {
                lo: Some(0),
                hi: None
            }
            .descriptor_payload("i64"),
            "i64:0:"
        );
        assert_eq!(
            Refinement::IntRange {
                lo: None,
                hi: Some(100)
            }
            .descriptor_payload("i64"),
            "i64::100"
        );
    }

    #[test]
    fn str_len_payload_keeps_str_kind_and_appends_length_bounds() {
        // StrLen reuses the `:lo:hi` suffix shape; the `str` kind tells the
        // decoder these are LENGTHS, not value ranges.
        assert_eq!(
            Refinement::StrLen {
                lo: Some(1),
                hi: Some(20)
            }
            .descriptor_payload("str"),
            "str:1:20"
        );
        assert_eq!(
            Refinement::StrLen {
                lo: None,
                hi: Some(255)
            }
            .descriptor_payload("str"),
            "str::255"
        );
    }

    #[test]
    fn pattern_payload_replaces_kind_with_pat_and_carries_raw_regex() {
        assert_eq!(
            Refinement::Pattern {
                regex: ".+@.+".to_string()
            }
            .descriptor_payload("str"),
            "pat:.+@.+"
        );
        // A `:` inside the regex is preserved verbatim (decode takes the
        // remainder after the FIRST `:`).
        assert_eq!(
            Refinement::Pattern {
                regex: "a:b".to_string()
            }
            .descriptor_payload("str"),
            "pat:a:b"
        );
    }
}
