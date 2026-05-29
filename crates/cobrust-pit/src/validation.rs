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
//! - each int-range refinement (`minimum`/`maximum`) must hold.
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
//! One line per field, `field<TAB>kind[suffix]`:
//!
//! ```text
//! name\tstr
//! rank\ti64:0:100
//! low\ti64:0:
//! ```
//!
//! - `kind ∈ {str, i64, f64, bool, any}` — the field's declared base type
//!   (`any` = a non-Phase-1b-ii scalar; presence-only check);
//! - the optional int-range `suffix` is `:<lo>:<hi>` (an absent bound is
//!   the empty string). Emitted ONLY for an `i64` field carrying a `where`
//!   int-range refinement ([`cobrust_types::Refinement::schema_suffix`]).
//!
//! An EMPTY schema (no lines) means "validate JSON-object-ness only" (a
//! defensive fallback when the compiler could not resolve the body class;
//! the type checker has already accepted the program).

use serde_json::Value;

/// A field-level validation failure. Rendered into the 422 response body
/// (a small JSON document) by [`ValidationError::to_json_body`]. Closed
/// enum — extends the `PitError`-style `Result`-default discipline
/// (ADR-0080 §3 footgun #2; never an exception).
#[derive(Clone, Debug, Eq, PartialEq)]
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
            Self::UnknownField { field } => {
                format!("unknown field `{field}` (not declared on the request body)")
            }
        }
    }
}

/// One parsed schema field descriptor.
struct FieldSpec {
    name: String,
    kind: FieldKind,
    lo: Option<i64>,
    hi: Option<i64>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FieldKind {
    Str,
    I64,
    F64,
    Bool,
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
            _ => Self::Any,
        }
    }

    fn type_name(self) -> &'static str {
        match self {
            Self::Str => "string",
            Self::I64 => "integer",
            Self::F64 => "number",
            Self::Bool => "boolean",
            Self::Any => "any",
        }
    }
}

/// Parse the compact schema descriptor (see the module header) into the
/// field-spec list. Malformed lines are skipped defensively (the compiler
/// emits well-formed descriptors; this never panics on bad input).
fn parse_schema(schema: &str) -> Vec<FieldSpec> {
    let mut specs = Vec::new();
    for line in schema.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        // `rest` is `kind[:lo:hi]`.
        let mut parts = rest.split(':');
        let kind = FieldKind::parse(parts.next().unwrap_or("any"));
        let lo = parts.next().and_then(|s| s.parse::<i64>().ok());
        let hi = parts.next().and_then(|s| s.parse::<i64>().ok());
        specs.push(FieldSpec {
            name: name.to_string(),
            kind,
            lo,
            hi,
        });
    }
    specs
}

/// Validate `body` against `schema` (ADR-0080 §5.4). Returns `Ok(())` iff
/// the body is an object whose keys EXACTLY match the declared fields, each
/// of the declared base type, with every int-range refinement satisfied.
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

/// Type-check (and range-check) one field's JSON value against its spec.
fn check_field(spec: &FieldSpec, value: &Value) -> Result<(), ValidationError> {
    match spec.kind {
        FieldKind::Str => {
            if !value.is_string() {
                return Err(wrong_type(spec));
            }
        }
        FieldKind::Bool => {
            if !value.is_boolean() {
                return Err(wrong_type(spec));
            }
        }
        FieldKind::F64 => {
            // Accept any JSON number (an integer literal is a valid f64).
            if !value.is_number() {
                return Err(wrong_type(spec));
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
}
