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
//! - Phase-3a ([`Refinement::FloatRange`]) — `lo <= self <= hi` and its
//!   one-sided forms on an `f64` field (ADR-0080 §Phase-3a). The precise
//!   MIRROR of [`Refinement::IntRange`] with `f64` bounds — INCLUSIVE
//!   value-range, `minimum`/`maximum` in OpenAPI's `{type:number}`.
//!
//! Any non-fixed predicate — and any fixed form on the wrong base type — is
//! rejected by the type checker with
//! [`crate::error::TypeError::UnsupportedRefinement`] carrying a §2.5-B FIX.

/// A structured value-level refinement on a class field. The variant set
/// grows by phase (ADR-0080 §6).
///
/// `IntRange`/`StrLen` carry `Copy` integer bounds; `FloatRange` carries
/// `Copy` `f64` bounds (ADR-0080 Phase-3a); `Pattern` carries an owned regex
/// `String`. The enum is therefore `Clone` (not `Copy`).
///
/// # Why `PartialEq` only (not `Eq`) — ADR-0080 Phase-3a D1
///
/// [`Refinement::FloatRange`] carries `f64` bounds, and `f64` is `PartialEq`
/// but NOT `Eq` (IEEE-754 `NaN != NaN`), so the derive DROPS `Eq` here as of
/// Phase-3a. This is SAFE: `Refinement` is used EXCLUSIVELY as a `HashMap`
/// VALUE (`adt_refinements: HashMap<(AdtId, String), Refinement>` —
/// `check.rs` / `TypedModule`); it is never a `HashMap`/`HashSet` KEY and no
/// site bounds it `: Eq`/`: Hash`. The fixed grammar (`parse_subject_bound_f64`)
/// never produces a `NaN`/`inf` bound (those are not int/float *literals* the
/// bound-parser admits), so the partial-equality is total in practice — but
/// the derive must still be `PartialEq` for the type to compile with an `f64`
/// field. Were any future site to need `Refinement: Eq`, it would fail to
/// compile loudly at that site (no silent semantic change).
#[derive(Clone, Debug, PartialEq)]
pub enum Refinement {
    /// An integer range bound on an `i64` field. At least one of `lo`/`hi`
    /// is `Some` (a `where`-clause with neither bound is meaningless and is
    /// rejected at parse-interpretation). Bounds are INCLUSIVE
    /// (`lo <= self`, `self <= hi`) — the only relational form the fixed
    /// grammar admits (ADR-0080 Q6). The OpenAPI projection maps `lo` →
    /// `minimum` and `hi` → `maximum` (ADR-0080 §5.3).
    IntRange { lo: Option<i64>, hi: Option<i64> },
    /// A floating-point value range bound on an `f64` field (ADR-0080
    /// Phase-3a) — the precise MIRROR of [`Self::IntRange`] with `f64`
    /// bounds. At least one of `lo`/`hi` is `Some` (a `where`-clause with
    /// neither bound is meaningless and is rejected at parse-interpretation,
    /// exactly as `IntRange`). Bounds are INCLUSIVE (`lo <= self`,
    /// `self <= hi`). `NaN`/`inf` are NOT producible by the fixed grammar
    /// (the bound-parser admits only finite float literals), so the
    /// partial-order comparison the validator runs is total in practice. The
    /// OpenAPI projection maps `lo` → `minimum` and `hi` → `maximum` on a
    /// `{"type":"number"}` schema (ADR-0080 §5.3 / Phase-3a D4).
    FloatRange { lo: Option<f64>, hi: Option<f64> },
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
    /// - [`Self::FloatRange`] → `f64:<lo>:<hi>` (keeps `base_kind = f64`,
    ///   appends the SAME `:lo:hi` suffix SHAPE as `IntRange` but with `f64`
    ///   `Display` bounds — `0 <= self and self <= 100` → `f64:0:100`,
    ///   one-sided `0 <= self` → `f64:0:`, `self <= 100` → `f64::100`;
    ///   ADR-0080 Phase-3a D3). The `f64` base kind tells the
    ///   validator/emitter the bounds are FLOAT value ranges parsed as `f64`.
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
            // FloatRange keeps the `f64` base kind and appends the SAME
            // `:lo:hi` suffix shape as IntRange, but with `f64`-Display
            // bounds (ADR-0080 Phase-3a D3). The `f64` kind discriminates a
            // FLOAT value-range from the int range at decode.
            Self::FloatRange { lo, hi } => format!("{base_kind}{}", float_suffix(*lo, *hi)),
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

/// The `:lo:hi` numeric suffix for [`Refinement::FloatRange`] (ADR-0080
/// Phase-3a D3) — the `f64` dual of [`int_suffix`]. An absent bound is the
/// empty string (`Some(0.0), None` → `:0:`; `None, Some(100.0)` → `::100`;
/// `Some(0.0), Some(99.9)` → `:0:99.9`).
///
/// Bounds are rendered with `f64`'s `Display` (`{}`), which emits the
/// SHORTEST round-trippable decimal (`0.0` → `"0"`, `100.0` → `"100"`,
/// `99.9` → `"99.9"`, `0.5` → `"0.5"`). This is the ENCODE half of the
/// cannot-drift pair (ADR-0080 §3 footgun #4): `cobrust-pit`'s `parse_schema`
/// DECODEs it with `str::parse::<f64>()`, which accepts every string `f64`
/// `Display` emits — the two halves round-trip exactly. The fixed grammar
/// (`check.rs::parse_subject_bound_f64`) never produces a `NaN`/`inf` bound,
/// so the suffix is always a finite decimal both halves agree on.
fn float_suffix(lo: Option<f64>, hi: Option<f64>) -> String {
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
    fn float_range_payload_keeps_f64_kind_and_appends_bounds() {
        // ADR-0080 Phase-3a D3 — the precise MIRROR of the IntRange payload,
        // with `f64`-Display bounds. Whole-valued bounds render WITHOUT a
        // trailing `.0` (Rust `f64` Display: `100.0` → "100").
        assert_eq!(
            Refinement::FloatRange {
                lo: Some(0.0),
                hi: Some(100.0)
            }
            .descriptor_payload("f64"),
            "f64:0:100"
        );
        assert_eq!(
            Refinement::FloatRange {
                lo: Some(0.0),
                hi: None
            }
            .descriptor_payload("f64"),
            "f64:0:"
        );
        assert_eq!(
            Refinement::FloatRange {
                lo: None,
                hi: Some(100.0)
            }
            .descriptor_payload("f64"),
            "f64::100"
        );
    }

    #[test]
    fn float_range_payload_preserves_fractional_bounds() {
        // A genuinely-fractional bound round-trips through `f64` Display →
        // `parse::<f64>` (the cannot-drift contract). `0.5` and `99.9` are
        // representable exactly enough that Display emits the short form.
        assert_eq!(
            Refinement::FloatRange {
                lo: Some(0.5),
                hi: Some(99.9)
            }
            .descriptor_payload("f64"),
            "f64:0.5:99.9"
        );
        // The encoded suffix bounds parse back to the SAME f64 (the DECODE
        // half lives in cobrust-pit `parse_schema`; this asserts the encode
        // is round-trippable so the pair cannot drift).
        let payload = Refinement::FloatRange {
            lo: Some(0.5),
            hi: Some(99.9),
        }
        .descriptor_payload("f64");
        // payload == "f64:0.5:99.9" → after the kind token the suffix is
        // "0.5":"99.9".
        let suffix = payload.strip_prefix("f64:").expect("f64 kind prefix");
        let mut parts = suffix.split(':');
        let lo: f64 = parts
            .next()
            .expect("lo segment present")
            .parse()
            .expect("lo parses as f64");
        let hi: f64 = parts
            .next()
            .expect("hi segment present")
            .parse()
            .expect("hi parses as f64");
        assert!((lo - 0.5).abs() < f64::EPSILON);
        assert!((hi - 99.9).abs() < f64::EPSILON);
    }

    #[test]
    fn float_range_negative_bounds_render() {
        // A negative float bound (`-1.5 <= self`) renders with the sign.
        assert_eq!(
            Refinement::FloatRange {
                lo: Some(-1.5),
                hi: Some(1.5)
            }
            .descriptor_payload("f64"),
            "f64:-1.5:1.5"
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
