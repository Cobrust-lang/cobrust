//! ADR-0080 Phase-1b-ii — the request-body validation engine.
//!
//! The `route_validated` trampoline ([`crate::cabi`]) hands this module the
//! compact SCHEMA descriptor the Cobrust compiler synthesised from the body
//! class's field table + refinement side-table (the SAME source the type
//! checker resolved field access against — ADR-0080 §3 footgun #4, cannot
//! drift), plus the request's parsed JSON. [`validate_against_schema`]
//! performs the TOTAL boundary deserialization (ADR-0080 §3 Q3 / footgun
//! #1):
//!
//! - the JSON must be an object;
//! - EVERY declared field must be present with the declared base type
//!   (a missing key, an extra key, or a wrong JSON type → `Err`);
//! - each int-range refinement (`minimum`/`maximum`) must hold;
//! - each f64 value-range refinement (`minimum`/`maximum` on a `{type:number}`
//!   field, ADR-0080 Phase-3a) must hold;
//! - each str-LENGTH refinement (`minLength`/`maxLength`, ADR-0080 Phase-2)
//!   must hold;
//! - each str-PATTERN refinement (`pattern`, ADR-0080 Phase-2/3) must match.
//!
//! On success the value's structure provably matches the declared types, so
//! it can never be re-checked in the handler. On failure the trampoline
//! renders a typed **422** [`crate::response::Response`] from the
//! [`ValidationError`] WITHOUT entering the handler (ADR-0080 §5.4 step 4 /
//! footgun #2 — the Result-error path stays in Rust as a Response, never a
//! throw/panic).
//!
//! # Schema descriptor grammar (the compiler↔runtime contract)
//!
//! One line per field, `field<TAB>payload`, optionally preceded by a single
//! `# <BodyName>` header line naming the body class:
//!
//! ```text
//! # SignupBody
//! name\tstr
//! rank\ti64:0:100
//! low\ti64:0:
//! username\tstr:1:20
//! email\tpat:.+@.+
//! ```
//!
//! The `payload` is `<kind-token>[<suffix>]` (rendered by the ONE encoder,
//! [`cobrust_types::Refinement::descriptor_payload`]):
//!
//! - `kind-token ∈ {str, i64, f64, bool, pat, any}` — the field's base type
//!   (`pat` = a `str` field with a regex pattern; `any` = a non-Phase-1b-ii
//!   scalar, presence-only check);
//! - the numeric `:<lo>:<hi>` suffix (absent bound = empty string) carries
//!   the int RANGE for an `i64` field (`minimum`/`maximum`), the FLOAT value
//!   RANGE for an `f64` field (`minimum`/`maximum`, ADR-0080 Phase-3a — the
//!   bounds parse as `f64`, so `f64:0.5:99.9` is admitted), and the LENGTH
//!   bound for a `str` field (`minLength`/`maxLength`, ADR-0080 Phase-2);
//! - a `pat` field's payload is `pat:<regex>` — the raw regex is EVERYTHING
//!   after the first `:` (so a `:` inside the regex is preserved).
//! - the optional `# <BodyName>` header line carries the body class's
//!   source name (ADR-0080 Phase-1b-iii — used by the OpenAPI emitter to
//!   key `components/schemas/<BodyName>`). The VALIDATOR ignores it for
//!   free: it carries no TAB, so [`parse_schema`]'s `split_once('\t')`
//!   yields `None` and the line is skipped. This is the single-source
//!   discipline — the body name lives in the SAME descriptor string the
//!   validator reads, so the schema name and the validated fields cannot
//!   come from two declarations (footgun #4).
//!
//! An EMPTY schema (no lines) means "validate JSON-object-ness only" (a
//! defensive fallback when the compiler could not resolve the body class;
//! the type checker has already accepted the program).
//!
//! # The ONE source the OpenAPI emitter walks (ADR-0080 §5.3, footgun #4)
//!
//! [`parse_schema`] + [`FieldSpec`] + [`FieldKind`] are `pub(crate)` so the
//! sibling OpenAPI emitter ([`crate::openapi`]) derives the
//! `components/schemas/<Body>` JSON by walking the EXACT SAME parsed
//! representation this validator checks against — there is no second schema
//! declaration to drift from. The bound the validator enforces
//! (`FieldSpec::lo`/`hi` for an int range OR str length;
//! `FieldSpec::pattern` for a regex) IS the bound the schema advertises
//! (`minimum`/`maximum`, `minLength`/`maxLength`, or `pattern`).

use serde_json::Value;

/// A field-level validation failure. Rendered into the 422 response body
/// (a small JSON document) by [`ValidationError::to_json_body`]. Closed
/// enum — extends the `PitError`-style `Result`-default discipline
/// (ADR-0080 §3 footgun #2; never an exception).
///
/// `PartialEq` only (not `Eq`) as of ADR-0080 Phase-3a: [`Self::FloatOutOfRange`]
/// carries `f64` value + bounds, and `f64` is `PartialEq`-but-not-`Eq`. SAFE —
/// `ValidationError` is only `==`/`matches!`-compared in tests + flows through
/// `Result`/`Err`; it is never a `HashMap`/`HashSet` key and no site bounds it
/// `: Eq` (mirrors the `Refinement` Eq-drop, same rationale).
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationError {
    /// The request body was not a JSON object (e.g. an array, a scalar,
    /// or malformed JSON the trampoline already failed to parse).
    NotAnObject,
    /// A declared field was absent from the body.
    MissingField { field: String },
    /// A field carried a JSON value of the wrong base type.
    WrongType {
        field: String,
        expected: &'static str,
    },
    /// An `i64` field violated its int-range refinement bound.
    OutOfRange {
        field: String,
        value: i64,
        lo: Option<i64>,
        hi: Option<i64>,
    },
    /// An `f64` field violated its float value-range refinement bound
    /// (ADR-0080 Phase-3a) — the precise MIRROR of [`Self::OutOfRange`] with
    /// `f64` value + bounds. Bounds are INCLUSIVE; `lo`/`hi` are the same
    /// finite bounds the OpenAPI schema advertised as `minimum`/`maximum`.
    FloatOutOfRange {
        field: String,
        value: f64,
        lo: Option<f64>,
        hi: Option<f64>,
    },
    /// A `str` field violated its length refinement bound (ADR-0080
    /// Phase-2). `len` is the character count of the deserialized string.
    LengthOutOfRange {
        field: String,
        len: i64,
        lo: Option<i64>,
        hi: Option<i64>,
    },
    /// A `str` field failed its regex pattern refinement (ADR-0080
    /// Phase-2/Phase-3). `pattern` is the raw regex the schema declared.
    PatternMismatch { field: String, pattern: String },
    /// The body contained a key the body class does not declare (total
    /// boundary deserialization rejects unknown keys — ADR-0080 footgun
    /// #1: a value that does not match the declared shape cannot reach the
    /// handler).
    UnknownField { field: String },
}

impl ValidationError {
    /// Render this error into the 422 response body — a compact JSON
    /// document `{"error":"validation_failed","detail":"…"}`. The detail
    /// is a human + LLM readable description (§2.5-B — the fix is legible).
    #[must_use]
    pub fn to_json_body(&self) -> String {
        let detail = self.detail();
        // Hand-render so we don't depend on a serde derive for this tiny,
        // fixed shape; `serde_json::to_string` on the detail escapes it
        // safely.
        let detail_json =
            serde_json::to_string(&detail).unwrap_or_else(|_| "\"validation failed\"".to_string());
        format!("{{\"error\":\"validation_failed\",\"detail\":{detail_json}}}")
    }

    /// The human-readable detail string for this failure.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::NotAnObject => "request body must be a JSON object".to_string(),
            Self::MissingField { field } => format!("missing required field `{field}`"),
            Self::WrongType { field, expected } => {
                format!("field `{field}` must be of type {expected}")
            }
            Self::OutOfRange {
                field,
                value,
                lo,
                hi,
            } => {
                let bound = match (lo, hi) {
                    (Some(l), Some(h)) => format!("in [{l}, {h}]"),
                    (Some(l), None) => format!(">= {l}"),
                    (None, Some(h)) => format!("<= {h}"),
                    (None, None) => "within its declared range".to_string(),
                };
                format!("field `{field}` value {value} must be {bound}")
            }
            Self::FloatOutOfRange {
                field,
                value,
                lo,
                hi,
            } => {
                // Mirror `OutOfRange`'s §2.5-B FIX shape with `f64` bounds.
                let bound = match (lo, hi) {
                    (Some(l), Some(h)) => format!("in [{l}, {h}]"),
                    (Some(l), None) => format!(">= {l}"),
                    (None, Some(h)) => format!("<= {h}"),
                    (None, None) => "within its declared range".to_string(),
                };
                format!("field `{field}` value {value} must be {bound}")
            }
            Self::LengthOutOfRange { field, len, lo, hi } => {
                let bound = match (lo, hi) {
                    (Some(l), Some(h)) => format!("between {l} and {h} characters"),
                    (Some(l), None) => format!("at least {l} characters"),
                    (None, Some(h)) => format!("at most {h} characters"),
                    (None, None) => "within its declared length".to_string(),
                };
                format!("field `{field}` length {len} must be {bound}")
            }
            Self::PatternMismatch { field, pattern } => {
                format!("field `{field}` must match pattern `{pattern}`")
            }
            Self::UnknownField { field } => {
                format!("unknown field `{field}` (not declared on the request body)")
            }
        }
    }
}

/// One parsed schema field descriptor. `pub(crate)` so the sibling
/// OpenAPI emitter ([`crate::openapi`]) derives the field's JSON schema
/// from the SAME parsed representation the validator checks (the
/// cannot-drift single source — ADR-0080 §5.3 / footgun #4).
///
/// `lo`/`hi` carry the int-range bound for an [`FieldKind::I64`] field
/// (`minimum`/`maximum`) AND the length bound for an [`FieldKind::Str`]
/// field (`minLength`/`maxLength`, ADR-0080 Phase-2) — the `kind`
/// discriminates value-range from length. `lo_f`/`hi_f` carry the FLOAT
/// value-range bound for an [`FieldKind::F64`] field (`minimum`/`maximum` on
/// a `{type:number}` schema, ADR-0080 Phase-3a) — a SEPARATE pair from the
/// integer `lo`/`hi` because an `f64` bound (`0.5`) is not an `i64`.
/// `pattern` carries the raw regex for a [`FieldKind::Pat`] field
/// (`pattern`, ADR-0080 Phase-2/3).
pub(crate) struct FieldSpec {
    pub(crate) name: String,
    pub(crate) kind: FieldKind,
    pub(crate) lo: Option<i64>,
    pub(crate) hi: Option<i64>,
    /// The float value-range bounds for an [`FieldKind::F64`] field (ADR-0080
    /// Phase-3a); `None` for every other kind. A `f64` bound (e.g. `0.5`) is
    /// not representable as the integer `lo`/`hi`, so it lives in its own
    /// pair — the validator float-range-checks against these, and the
    /// OpenAPI emitter advertises them as `minimum`/`maximum`.
    pub(crate) lo_f: Option<f64>,
    pub(crate) hi_f: Option<f64>,
    /// The raw regex for a [`FieldKind::Pat`] field; `None` otherwise.
    pub(crate) pattern: Option<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum FieldKind {
    /// A `str` field. Carries an OPTIONAL length bound in `lo`/`hi`
    /// (ADR-0080 Phase-2: `minLength`/`maxLength`).
    Str,
    I64,
    F64,
    Bool,
    /// A `str` field with a regex PATTERN refinement (ADR-0080
    /// Phase-2/Phase-3, descriptor token `pat`). The base JSON type is still
    /// `string`; the raw regex lives in [`FieldSpec::pattern`].
    Pat,
    /// A non-Phase-1b-ii scalar — presence-only (no type/range check).
    Any,
}

impl FieldKind {
    fn parse(s: &str) -> Self {
        match s {
            "str" => Self::Str,
            "i64" => Self::I64,
            "f64" => Self::F64,
            "bool" => Self::Bool,
            "pat" => Self::Pat,
            _ => Self::Any,
        }
    }

    /// The validation-error label for a wrong-type 422 detail. A `pat` field
    /// is a string (the pattern constrains a string value).
    fn type_name(self) -> &'static str {
        match self {
            Self::Str | Self::Pat => "string",
            Self::I64 => "integer",
            Self::F64 => "number",
            Self::Bool => "boolean",
            Self::Any => "any",
        }
    }

    /// The OpenAPI 3.1 `type` keyword for this field kind (ADR-0080 §5.3):
    /// `str → string`, `i64 → integer`, `f64 → number`, `bool → boolean`,
    /// `pat → string` (a pattern constrains a string). An `Any` field (a
    /// non-Phase-1b-ii scalar) has no statically-known OpenAPI type, so the
    /// emitter omits the `type` keyword for it (`None` here). Consumed by
    /// [`crate::openapi`].
    pub(crate) fn openapi_type(self) -> Option<&'static str> {
        match self {
            Self::Str | Self::Pat => Some("string"),
            Self::I64 => Some("integer"),
            Self::F64 => Some("number"),
            Self::Bool => Some("boolean"),
            Self::Any => None,
        }
    }
}

/// The body-class name carried by the optional `# <BodyName>` header line
/// of a schema descriptor (see the module header). Returns `None` when the
/// descriptor carries no header line (the defensive empty-schema fallback,
/// or a pre-Phase-1b-iii descriptor). `pub(crate)` so the OpenAPI emitter
/// keys `components/schemas/<BodyName>` from the SAME descriptor string the
/// validator reads (footgun #4 — one source).
pub(crate) fn body_name(schema: &str) -> Option<String> {
    schema.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    })
}

/// Parse the compact schema descriptor (see the module header) into the
/// field-spec list. Malformed lines — including the optional `# <BodyName>`
/// header line (no TAB → `split_once` yields `None`) — are skipped
/// defensively (the compiler emits well-formed descriptors; this never
/// panics on bad input). `pub(crate)` so the OpenAPI emitter walks the
/// SAME parse the validator does (the cannot-drift single source).
///
/// The payload after `field<TAB>` is `<kind-token>[<suffix>]` (mirroring
/// [`cobrust_types::Refinement::descriptor_payload`], the ONE encoder):
///
/// - `i64[:lo:hi]` / `str[:lo:hi]` — a `:`-delimited numeric suffix. For
///   `i64` the bounds are an int RANGE (`minimum`/`maximum`); for `str` they
///   are a LENGTH bound (`minLength`/`maxLength`, ADR-0080 Phase-2). An
///   absent bound is the empty string.
/// - `f64[:lo:hi]` — a `:`-delimited FLOAT suffix (ADR-0080 Phase-3a). The
///   bounds parse as `f64` (so `f64:0.5:99.9` is admitted) into the SEPARATE
///   `lo_f`/`hi_f` pair; they are a value RANGE (`minimum`/`maximum` on a
///   `{type:number}` schema). An absent bound is the empty string.
/// - `pat:<regex>` — a regex PATTERN; the regex is EVERYTHING after the
///   first `:` (so a `:` inside the regex is preserved). The base JSON type
///   is `string`.
pub(crate) fn parse_schema(schema: &str) -> Vec<FieldSpec> {
    let mut specs = Vec::new();
    for line in schema.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        // The kind token is everything up to the FIRST `:`; the remainder
        // is the kind-specific suffix (a `:lo:hi` numeric pair, or — for a
        // `pat` field — the raw regex, which may itself contain `:`).
        let (kind_token, suffix) = match rest.split_once(':') {
            Some((k, s)) => (k, Some(s)),
            None => (rest, None),
        };
        let kind = FieldKind::parse(kind_token);
        let (lo, hi, lo_f, hi_f, pattern) = match kind {
            FieldKind::Pat => {
                // The pattern payload is the raw regex (the whole remainder
                // after the first `:`). An empty/absent remainder → no pattern.
                (
                    None,
                    None,
                    None,
                    None,
                    suffix.filter(|s| !s.is_empty()).map(str::to_string),
                )
            }
            FieldKind::F64 => {
                // ADR-0080 Phase-3a — an `f64` field's bounds parse as `f64`
                // into the SEPARATE float pair (an `i64` parse would reject a
                // fractional bound like `0.5`). The DECODE half of the
                // cannot-drift pair (the ENCODE is `float_suffix` in
                // cobrust-types).
                let (lo_f, hi_f) = parse_float_suffix(suffix);
                (None, None, lo_f, hi_f, None)
            }
            // Every other kind carries the `:lo:hi` INTEGER suffix (an
            // absent bound is the empty string).
            FieldKind::Str | FieldKind::I64 | FieldKind::Bool | FieldKind::Any => {
                let (lo, hi) = parse_numeric_suffix(suffix);
                (lo, hi, None, None, None)
            }
        };
        specs.push(FieldSpec {
            name: name.to_string(),
            kind,
            lo,
            hi,
            lo_f,
            hi_f,
            pattern,
        });
    }
    specs
}

/// Parse the `:lo:hi` numeric suffix (after the kind token's first `:` has
/// already been split off, so `suffix` is `lo:hi`). Either bound may be the
/// empty string (absent). A malformed bound parses to `None` (defensive).
fn parse_numeric_suffix(suffix: Option<&str>) -> (Option<i64>, Option<i64>) {
    let Some(suffix) = suffix else {
        return (None, None);
    };
    let mut parts = suffix.split(':');
    let lo = parts.next().and_then(|s| s.parse::<i64>().ok());
    let hi = parts.next().and_then(|s| s.parse::<i64>().ok());
    (lo, hi)
}

/// Parse the `:lo:hi` FLOAT suffix of an `f64` field (ADR-0080 Phase-3a) —
/// the `f64` dual of [`parse_numeric_suffix`]. Each bound is parsed with
/// `str::parse::<f64>()`, which accepts every string the ENCODE half
/// (`cobrust-types::float_suffix`, via `f64` `Display`) emits — so the pair
/// round-trips exactly (the cannot-drift contract, ADR-0080 §3 footgun #4).
/// Either bound may be the empty string (absent → `None`); a malformed bound
/// parses to `None` (defensive — the compiler emits well-formed suffixes).
fn parse_float_suffix(suffix: Option<&str>) -> (Option<f64>, Option<f64>) {
    let Some(suffix) = suffix else {
        return (None, None);
    };
    let mut parts = suffix.split(':');
    let lo = parts.next().and_then(|s| s.parse::<f64>().ok());
    let hi = parts.next().and_then(|s| s.parse::<f64>().ok());
    (lo, hi)
}

/// Validate `body` against `schema` (ADR-0080 §5.4). Returns `Ok(())` iff
/// the body is an object whose keys EXACTLY match the declared fields, each
/// of the declared base type, with every int-range / f64 value-range /
/// str-length / str-pattern refinement satisfied.
///
/// This is the TOTAL boundary deserialization (footgun #1): a missing key,
/// an extra key, a wrong JSON type, or an out-of-range value yields `Err`,
/// so a structurally-invalid body is unable to reach the handler.
///
/// # Errors
///
/// Returns the FIRST [`ValidationError`] encountered (checked in a stable
/// order: object-ness → unknown keys → per-field presence/type/range).
pub fn validate_against_schema(schema: &str, body: &Value) -> Result<(), ValidationError> {
    let Value::Object(map) = body else {
        return Err(ValidationError::NotAnObject);
    };
    let specs = parse_schema(schema);

    // Reject unknown keys (total deserialization — no extra fields).
    for key in map.keys() {
        if !specs.iter().any(|s| &s.name == key) {
            return Err(ValidationError::UnknownField { field: key.clone() });
        }
    }

    // Every declared field must be present, of the right type, in range.
    for spec in &specs {
        let Some(value) = map.get(&spec.name) else {
            return Err(ValidationError::MissingField {
                field: spec.name.clone(),
            });
        };
        check_field(spec, value)?;
    }
    Ok(())
}

/// Type-check (and range/length/pattern-check) one field's JSON value
/// against its spec.
fn check_field(spec: &FieldSpec, value: &Value) -> Result<(), ValidationError> {
    match spec.kind {
        FieldKind::Str => {
            let Some(s) = value.as_str() else {
                return Err(wrong_type(spec));
            };
            // Str-LENGTH refinement (ADR-0080 Phase-2). The length is the
            // Unicode scalar count (Python `len()` / JSON Schema
            // minLength/maxLength semantics — codepoints, not bytes). `lo`/
            // `hi` are `None` for a plain `str` field (no length bound).
            check_str_len(spec, s)?;
        }
        FieldKind::Pat => {
            // A `str` field with a regex PATTERN refinement (ADR-0080
            // Phase-2/3). Must be a string AND match the regex.
            let Some(s) = value.as_str() else {
                return Err(wrong_type(spec));
            };
            check_pattern(spec, s)?;
        }
        FieldKind::Bool => {
            if !value.is_boolean() {
                return Err(wrong_type(spec));
            }
        }
        FieldKind::F64 => {
            // Accept any JSON number (an integer literal is a valid f64).
            // `as_f64` returns `Some` for every JSON number (integer OR
            // float) and `None` for a non-number, so it doubles as the
            // type check and the value extraction.
            let Some(n) = value.as_f64() else {
                return Err(wrong_type(spec));
            };
            // Float value-range refinement (ADR-0080 Phase-3a) — the precise
            // mirror of the i64 range-check, against the SEPARATE `lo_f`/`hi_f`
            // bounds. Bounds are finite (the fixed grammar never emits
            // NaN/inf), so the partial comparison is total here.
            if spec.lo_f.is_some_and(|lo| n < lo) || spec.hi_f.is_some_and(|hi| n > hi) {
                return Err(float_out_of_range(spec, n));
            }
        }
        FieldKind::I64 => {
            // Must be a JSON integer (NOT a float like 1.5, NOT a string).
            // `serde_json::Value::as_i64` returns `Some` only for an
            // integer-valued number.
            let Some(n) = value.as_i64() else {
                return Err(wrong_type(spec));
            };
            // Int-range refinement (ADR-0080 §5.4 range-check).
            if spec.lo.is_some_and(|lo| n < lo) || spec.hi.is_some_and(|hi| n > hi) {
                return Err(out_of_range(spec, n));
            }
        }
        FieldKind::Any => {
            // Presence-only — any JSON value is accepted (no type/range
            // check for a non-Phase-1b-ii scalar field).
        }
    }
    Ok(())
}

/// Enforce a `str` field's length refinement (ADR-0080 Phase-2). `lo`/`hi`
/// are inclusive character-count bounds (`None` = unbounded on that side).
fn check_str_len(spec: &FieldSpec, s: &str) -> Result<(), ValidationError> {
    // Codepoint count (Python `len()` semantics). `i64` is safe: a string
    // long enough to overflow `i64` is not representable in memory.
    #[allow(clippy::cast_possible_wrap)]
    let len = s.chars().count() as i64;
    if spec.lo.is_some_and(|lo| len < lo) || spec.hi.is_some_and(|hi| len > hi) {
        return Err(ValidationError::LengthOutOfRange {
            field: spec.name.clone(),
            len,
            lo: spec.lo,
            hi: spec.hi,
        });
    }
    Ok(())
}

/// Enforce a `str` field's regex pattern refinement (ADR-0080 Phase-2/3).
///
/// The regex was already compile-checked at type-check time (cobrust-types
/// `interpret_refinement` rejects a bad regex with a build-time
/// `TypeError`), so a compile failure here is unexpected. We compile it once
/// per call rather than caching: the regexes are tiny and the schema is
/// already re-parsed per request, so this matches the existing per-request
/// cost profile (a process-wide compiled-regex cache is a future
/// optimisation, not a correctness concern). A defensive compile failure is
/// treated as a mismatch (it never enters the handler — fail closed).
fn check_pattern(spec: &FieldSpec, s: &str) -> Result<(), ValidationError> {
    let Some(pattern) = &spec.pattern else {
        // A `pat` field with no regex carries no constraint (defensive — the
        // compiler always emits the regex for a Pattern refinement).
        return Ok(());
    };
    let matches = regex::Regex::new(pattern).is_ok_and(|re| re.is_match(s));
    if matches {
        Ok(())
    } else {
        Err(ValidationError::PatternMismatch {
            field: spec.name.clone(),
            pattern: pattern.clone(),
        })
    }
}

fn wrong_type(spec: &FieldSpec) -> ValidationError {
    ValidationError::WrongType {
        field: spec.name.clone(),
        expected: spec.kind.type_name(),
    }
}

fn out_of_range(spec: &FieldSpec, value: i64) -> ValidationError {
    ValidationError::OutOfRange {
        field: spec.name.clone(),
        value,
        lo: spec.lo,
        hi: spec.hi,
    }
}

/// ADR-0080 Phase-3a — the `f64` mirror of [`out_of_range`], reading the
/// SEPARATE float bound pair.
fn float_out_of_range(spec: &FieldSpec, value: f64) -> ValidationError {
    ValidationError::FloatOutOfRange {
        field: spec.name.clone(),
        value,
        lo: spec.lo_f,
        hi: spec.hi_f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SCHEMA: &str = "name\tstr\nrank\ti64:0:100";

    #[test]
    fn valid_body_passes() {
        let v = json!({"name": "a", "rank": 50});
        assert_eq!(validate_against_schema(SCHEMA, &v), Ok(()));
    }

    #[test]
    fn rank_above_max_out_of_range() {
        let v = json!({"name": "a", "rank": 200});
        assert!(matches!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::OutOfRange { value: 200, .. })
        ));
    }

    #[test]
    fn rank_below_min_out_of_range() {
        let v = json!({"name": "a", "rank": -1});
        assert!(matches!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::OutOfRange { value: -1, .. })
        ));
    }

    #[test]
    fn missing_field_rejected() {
        let v = json!({"rank": 50});
        assert!(matches!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::MissingField { field }) if field == "name"
        ));
    }

    #[test]
    fn wrong_type_rejected() {
        let v = json!({"name": "a", "rank": "x"});
        assert!(matches!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::WrongType { field, .. }) if field == "rank"
        ));
    }

    #[test]
    fn float_for_int_field_rejected() {
        // 1.5 is a JSON number but not an integer → wrong type for i64.
        let v = json!({"name": "a", "rank": 1.5});
        assert!(matches!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::WrongType { .. })
        ));
    }

    #[test]
    fn extra_field_rejected() {
        let v = json!({"name": "a", "rank": 50, "extra": 1});
        assert!(matches!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::UnknownField { field }) if field == "extra"
        ));
    }

    #[test]
    fn non_object_rejected() {
        let v = json!([1, 2, 3]);
        assert_eq!(
            validate_against_schema(SCHEMA, &v),
            Err(ValidationError::NotAnObject)
        );
    }

    #[test]
    fn one_sided_lower_bound() {
        let schema = "n\ti64:0:";
        assert_eq!(validate_against_schema(schema, &json!({"n": 0})), Ok(()));
        assert!(validate_against_schema(schema, &json!({"n": -1})).is_err());
        // No upper bound — a large value passes.
        assert_eq!(
            validate_against_schema(schema, &json!({"n": 1_000_000})),
            Ok(())
        );
    }

    #[test]
    fn empty_schema_checks_object_ness_only() {
        // An empty schema with an empty object passes; a non-object fails.
        assert_eq!(validate_against_schema("", &json!({})), Ok(()));
        assert_eq!(
            validate_against_schema("", &json!(5)),
            Err(ValidationError::NotAnObject)
        );
    }

    #[test]
    fn body_name_header_line_parsed() {
        // ADR-0080 Phase-1b-iii — the optional `# <BodyName>` header line
        // names the schema; the body name lives in the SAME descriptor the
        // validator reads (footgun #4 — one source).
        let schema = "# CreateScore\nname\tstr\nrank\ti64:0:100";
        assert_eq!(body_name(schema).as_deref(), Some("CreateScore"));
        // No header line → None.
        assert_eq!(body_name("name\tstr"), None);
        assert_eq!(body_name(""), None);
    }

    #[test]
    fn header_line_ignored_by_validator() {
        // The `# CreateScore` header line carries no TAB, so the validator
        // skips it — a body with exactly the declared fields still passes,
        // and the bounds still enforce (the header changes nothing for the
        // validator; it is metadata for the OpenAPI emitter).
        let schema = "# CreateScore\nname\tstr\nrank\ti64:0:100";
        assert_eq!(
            validate_against_schema(schema, &json!({"name":"a","rank":50})),
            Ok(())
        );
        assert!(matches!(
            validate_against_schema(schema, &json!({"name":"a","rank":200})),
            Err(ValidationError::OutOfRange { value: 200, .. })
        ));
    }

    #[test]
    fn openapi_type_keywords() {
        // ADR-0080 §5.3 — the field-kind → OpenAPI `type` mapping the
        // emitter walks. `Any` has no statically-known type → None.
        assert_eq!(FieldKind::Str.openapi_type(), Some("string"));
        assert_eq!(FieldKind::I64.openapi_type(), Some("integer"));
        assert_eq!(FieldKind::F64.openapi_type(), Some("number"));
        assert_eq!(FieldKind::Bool.openapi_type(), Some("boolean"));
        assert_eq!(FieldKind::Any.openapi_type(), None);
    }

    // ----- #156 Phase-3b lock: bool validated-body field -----------------
    // bool is plumbed end-to-end (front-end lower.rs `Ty::Bool→"bool"`
    // descriptor → `FieldKind::Bool` here → `is_boolean()` check), and
    // registration is `route_validated`-driven, NOT refinement-driven (a
    // body needs no `where` to be validated — every declared field's base
    // kind is enforced). But the bool VALIDATION path (is_boolean → 422 on a
    // non-bool) had NO direct test — only the OpenAPI-type mapping above.
    // These pin it so it cannot silently regress.
    const BOOL_SCHEMA: &str = "# Flags\nflag\tbool";

    #[test]
    fn bool_field_accepts_json_booleans() {
        assert_eq!(
            validate_against_schema(BOOL_SCHEMA, &json!({"flag": true})),
            Ok(())
        );
        assert_eq!(
            validate_against_schema(BOOL_SCHEMA, &json!({"flag": false})),
            Ok(())
        );
    }

    #[test]
    fn bool_field_rejects_non_bool_with_wrong_type_422() {
        // A string `"true"` and a number `1` are NOT JSON booleans → 422
        // WrongType (NumPy-of-the-web semantics: the type is enforced even
        // with no refinement). The string case is the classic footgun
        // (`"true"` is truthy in many langs but is not a bool here).
        for bad in [json!({"flag": "true"}), json!({"flag": 1})] {
            assert!(
                matches!(
                    validate_against_schema(BOOL_SCHEMA, &bad),
                    Err(ValidationError::WrongType { .. })
                ),
                "non-bool flag must be a WrongType 422; got {:?}",
                validate_against_schema(BOOL_SCHEMA, &bad)
            );
        }
    }

    #[test]
    fn error_body_is_valid_json() {
        let e = ValidationError::OutOfRange {
            field: "rank".to_string(),
            value: 200,
            lo: Some(0),
            hi: Some(100),
        };
        let body = e.to_json_body();
        // Round-trips as JSON, carries the marker + a legible detail.
        let parsed: Value = serde_json::from_str(&body).expect("422 body is valid JSON");
        assert_eq!(parsed["error"], "validation_failed");
        let detail = parsed["detail"].as_str().expect("detail is a string");
        assert!(detail.contains("rank"));
    }

    // ----- ADR-0080 Phase-2: str LENGTH refinement (StrLen) ------------

    /// `username: str where 1 <= len(self) and len(self) <= 20`
    /// (descriptor `str:1:20`) + `email: str` (plain).
    const STR_SCHEMA: &str = "# SignupBody\nemail\tstr\nusername\tstr:1:20";

    #[test]
    fn str_length_in_bounds_passes() {
        let v = json!({"username": "bob", "email": "x"});
        assert_eq!(validate_against_schema(STR_SCHEMA, &v), Ok(()));
    }

    #[test]
    fn str_length_above_max_rejected() {
        // 21 chars > max 20.
        let v = json!({"username": "a".repeat(21), "email": "x"});
        assert!(matches!(
            validate_against_schema(STR_SCHEMA, &v),
            Err(ValidationError::LengthOutOfRange {
                len: 21,
                hi: Some(20),
                ..
            })
        ));
    }

    #[test]
    fn str_length_below_min_rejected() {
        // Empty string, len 0 < min 1.
        let v = json!({"username": "", "email": "x"});
        assert!(matches!(
            validate_against_schema(STR_SCHEMA, &v),
            Err(ValidationError::LengthOutOfRange {
                len: 0,
                lo: Some(1),
                ..
            })
        ));
    }

    #[test]
    fn str_length_counts_unicode_scalars_not_bytes() {
        // "é" is 1 codepoint but 2 UTF-8 bytes; a one-sided `len <= 3`
        // bound counts codepoints (Python `len()` semantics).
        let schema = "s\tstr::3";
        // "ééé" = 3 codepoints (6 bytes) → in bounds.
        assert_eq!(
            validate_against_schema(schema, &json!({"s": "ééé"})),
            Ok(())
        );
        // "éééé" = 4 codepoints → over.
        assert!(matches!(
            validate_against_schema(schema, &json!({"s": "éééé"})),
            Err(ValidationError::LengthOutOfRange { len: 4, .. })
        ));
    }

    #[test]
    fn plain_str_field_has_no_length_bound() {
        // `email: str` (no suffix) accepts any length.
        let v = json!({"username": "bob", "email": "a".repeat(10000)});
        assert_eq!(validate_against_schema(STR_SCHEMA, &v), Ok(()));
    }

    #[test]
    fn str_length_non_string_value_is_wrong_type() {
        let v = json!({"username": 42, "email": "x"});
        assert!(matches!(
            validate_against_schema(STR_SCHEMA, &v),
            Err(ValidationError::WrongType { field, .. }) if field == "username"
        ));
    }

    // ----- ADR-0080 Phase-2/3: str PATTERN refinement (Pattern) --------

    /// `email: str where pattern(self, ".+@.+")` (descriptor `pat:.+@.+`).
    const PAT_SCHEMA: &str = "# SignupBody\nemail\tpat:.+@.+";

    #[test]
    fn pattern_match_passes() {
        assert_eq!(
            validate_against_schema(PAT_SCHEMA, &json!({"email": "b@x.com"})),
            Ok(())
        );
    }

    #[test]
    fn pattern_mismatch_rejected() {
        assert!(matches!(
            validate_against_schema(PAT_SCHEMA, &json!({"email": "notanemail"})),
            Err(ValidationError::PatternMismatch { field, pattern })
                if field == "email" && pattern == ".+@.+"
        ));
    }

    #[test]
    fn pattern_non_string_value_is_wrong_type() {
        assert!(matches!(
            validate_against_schema(PAT_SCHEMA, &json!({"email": 7})),
            Err(ValidationError::WrongType { field, .. }) if field == "email"
        ));
    }

    #[test]
    fn pattern_with_colon_in_regex_preserved() {
        // A `:` inside the regex must survive the descriptor's
        // `kind:remainder` split (the regex is everything after the first
        // `:`). The field separator is a real TAB; the regex `^\d+:\d+$`
        // matches "12:34".
        let schema = "port\tpat:^\\d+:\\d+$";
        assert_eq!(
            validate_against_schema(schema, &json!({"port": "12:34"})),
            Ok(())
        );
        assert!(matches!(
            validate_against_schema(schema, &json!({"port": "nope"})),
            Err(ValidationError::PatternMismatch { .. })
        ));
    }

    #[test]
    fn str_len_descriptor_parses_to_str_kind_with_bounds() {
        // The decode side of the cannot-drift contract: `str:1:20` parses
        // to a Str field carrying lo=1, hi=20 (the SAME bounds the OpenAPI
        // emitter reads for minLength/maxLength).
        let specs = parse_schema("u\tstr:1:20");
        assert_eq!(specs.len(), 1);
        assert!(matches!(specs[0].kind, FieldKind::Str));
        assert_eq!(specs[0].lo, Some(1));
        assert_eq!(specs[0].hi, Some(20));
        assert_eq!(specs[0].pattern, None);
    }

    #[test]
    fn pat_descriptor_parses_to_pat_kind_with_regex() {
        let specs = parse_schema("e\tpat:.+@.+");
        assert_eq!(specs.len(), 1);
        assert!(matches!(specs[0].kind, FieldKind::Pat));
        assert_eq!(specs[0].pattern.as_deref(), Some(".+@.+"));
        assert_eq!(specs[0].lo, None);
        assert_eq!(specs[0].hi, None);
    }

    // ----- ADR-0080 Phase-3a: f64 value-range refinement (FloatRange) ---

    /// `name: str` + `ratio: f64 where 0.0 <= self and self <= 1.0`
    /// (descriptor `f64:0:1`). The MIRROR of the Phase-1 int SCHEMA.
    const F64_SCHEMA: &str = "# Reading\nname\tstr\nratio\tf64:0:1";

    #[test]
    fn float_in_range_passes() {
        // A float strictly inside [0, 1].
        let v = json!({"name": "a", "ratio": 0.5});
        assert_eq!(validate_against_schema(F64_SCHEMA, &v), Ok(()));
        // The inclusive endpoints pass.
        assert_eq!(
            validate_against_schema(F64_SCHEMA, &json!({"name": "a", "ratio": 0.0})),
            Ok(())
        );
        assert_eq!(
            validate_against_schema(F64_SCHEMA, &json!({"name": "a", "ratio": 1.0})),
            Ok(())
        );
    }

    #[test]
    fn float_above_max_out_of_range() {
        let v = json!({"name": "a", "ratio": 1.5});
        assert!(matches!(
            validate_against_schema(F64_SCHEMA, &v),
            Err(ValidationError::FloatOutOfRange { value, hi: Some(h), .. })
                if (value - 1.5).abs() < f64::EPSILON && (h - 1.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn float_below_min_out_of_range() {
        let v = json!({"name": "a", "ratio": -0.5});
        assert!(matches!(
            validate_against_schema(F64_SCHEMA, &v),
            Err(ValidationError::FloatOutOfRange { lo: Some(l), .. })
                if (l - 0.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn integer_json_accepted_for_f64_field() {
        // A JSON integer literal is a valid f64 (0 is in [0, 1]); `as_f64`
        // accepts it. This is the mirror of `float_for_int_field_rejected`
        // in the OTHER direction — f64 is the permissive numeric kind.
        let v = json!({"name": "a", "ratio": 1});
        assert_eq!(validate_against_schema(F64_SCHEMA, &v), Ok(()));
        // An integer OUT of range still fails (2 > 1).
        assert!(matches!(
            validate_against_schema(F64_SCHEMA, &json!({"name": "a", "ratio": 2})),
            Err(ValidationError::FloatOutOfRange { .. })
        ));
    }

    #[test]
    fn non_number_for_f64_field_is_wrong_type() {
        let v = json!({"name": "a", "ratio": "x"});
        assert!(matches!(
            validate_against_schema(F64_SCHEMA, &v),
            Err(ValidationError::WrongType { field, expected: "number" }) if field == "ratio"
        ));
    }

    #[test]
    fn float_one_sided_lower_bound() {
        // `0.5 <= self` (no upper) → descriptor `f64:0.5:`.
        let schema = "x\tf64:0.5:";
        assert_eq!(validate_against_schema(schema, &json!({"x": 0.5})), Ok(()));
        assert!(validate_against_schema(schema, &json!({"x": 0.4})).is_err());
        // No upper bound — a large float passes.
        assert_eq!(validate_against_schema(schema, &json!({"x": 1e9})), Ok(()));
    }

    #[test]
    fn float_one_sided_upper_bound() {
        // `self <= 100.0` (no lower) → descriptor `f64::100`.
        let schema = "x\tf64::100";
        assert_eq!(
            validate_against_schema(schema, &json!({"x": 100.0})),
            Ok(())
        );
        assert!(validate_against_schema(schema, &json!({"x": 100.1})).is_err());
        // No lower bound — a very negative float passes.
        assert_eq!(validate_against_schema(schema, &json!({"x": -1e9})), Ok(()));
    }

    #[test]
    fn plain_f64_field_has_no_range_bound() {
        // `ratio: f64` (no `where`) → descriptor `f64` (no suffix) → any
        // number passes. Confirms a bare f64 field is not constrained.
        let schema = "x\tf64";
        assert_eq!(validate_against_schema(schema, &json!({"x": 1e30})), Ok(()));
        assert_eq!(
            validate_against_schema(schema, &json!({"x": -1e30})),
            Ok(())
        );
    }

    #[test]
    fn f64_range_descriptor_parses_to_f64_kind_with_float_bounds() {
        // The DECODE side of the cannot-drift contract: `f64:0.5:99.9`
        // parses to an F64 field carrying lo_f=0.5, hi_f=99.9 (the SAME
        // bounds the OpenAPI emitter reads for minimum/maximum) — and the
        // INTEGER `lo`/`hi` pair stays empty (the bounds live in the float
        // pair, not the int pair).
        let specs = parse_schema("r\tf64:0.5:99.9");
        assert_eq!(specs.len(), 1);
        assert!(matches!(specs[0].kind, FieldKind::F64));
        assert_eq!(specs[0].lo_f, Some(0.5));
        assert_eq!(specs[0].hi_f, Some(99.9));
        assert_eq!(specs[0].lo, None);
        assert_eq!(specs[0].hi, None);
    }

    #[test]
    fn float_out_of_range_error_body_is_valid_json_with_fix() {
        // §2.5-D6: the FloatOutOfRange detail PRINTS THE FIX (the bound),
        // mirroring OutOfRange. The 422 body round-trips as JSON.
        let e = ValidationError::FloatOutOfRange {
            field: "ratio".to_string(),
            value: 1.5,
            lo: Some(0.0),
            hi: Some(1.0),
        };
        let body = e.to_json_body();
        let parsed: Value = serde_json::from_str(&body).expect("422 body is valid JSON");
        assert_eq!(parsed["error"], "validation_failed");
        let detail = parsed["detail"].as_str().expect("detail is a string");
        // The detail names the field, the offending value, and the bound.
        assert!(detail.contains("ratio"), "detail names the field: {detail}");
        assert!(detail.contains("1.5"), "detail names the value: {detail}");
        assert!(
            detail.contains('[') && detail.contains(']'),
            "detail prints the inclusive bound (the FIX): {detail}"
        );
    }
}
