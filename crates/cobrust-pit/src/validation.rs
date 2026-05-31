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
//! The descriptor is MULTI-BLOCK (ADR-0080 §6 Phase-4 (b) / #156). Each block
//! is a `# <ClassName>` header line followed by one `field<TAB>payload` line
//! per field. The FIRST block is the ROOT (the request body's class); every
//! later block is a TRANSITIVELY-referenced nested validated class. A
//! FLAT-only body (no nested-class field) is a SINGLE block — BYTE-IDENTICAL
//! to the pre-Phase-4 descriptor (one `# <BodyName>` header + its field
//! lines):
//!
//! ```text
//! # CreateUser
//! address\tobj:Address
//! name\tstr
//! # Address
//! city\tstr
//! zip\ti64:0:99999
//! ```
//!
//! The `payload` is `<kind-token>[<suffix>]` (rendered by the ONE encoder,
//! [`cobrust_types::Refinement::descriptor_payload`], EXCEPT the nested
//! `obj:` token which MIR's `emit_class_block` renders directly — the
//! cannot-drift mirror is `parse_schema`'s decode of it):
//!
//! - `kind-token ∈ {str, i64, f64, bool, pat, obj, any}` — the field's base
//!   type (`pat` = a `str` field with a regex pattern; `obj:<ClassName>` = a
//!   field whose type is ANOTHER validated class, recursively validated
//!   against that class's block; `any` = a non-Phase-1b-ii scalar,
//!   presence-only check);
//! - the numeric `:<lo>:<hi>` suffix (absent bound = empty string) carries
//!   the int RANGE for an `i64` field (`minimum`/`maximum`), the FLOAT value
//!   RANGE for an `f64` field (`minimum`/`maximum`, ADR-0080 Phase-3a — the
//!   bounds parse as `f64`, so `f64:0.5:99.9` is admitted), and the LENGTH
//!   bound for a `str` field (`minLength`/`maxLength`, ADR-0080 Phase-2);
//! - a `pat` field's payload is `pat:<regex>` — the raw regex is EVERYTHING
//!   after the first `:` (so a `:` inside the regex is preserved).
//! - an `obj` field's payload is `obj:<ClassName>` — the name of the nested
//!   validated class. The validator looks the name up in the parsed
//!   per-block map ([`parse_schema_blocks`]) and RECURSIVELY validates the
//!   field's JSON value (which MUST be a JSON object) against that block's
//!   field specs (ADR-0080 §6 Phase-4 (b) / #156). The ENCODE (MIR
//!   `emit_class_block`) and this DECODE are mirror inverses — a value that
//!   breaks if they drift is pinned by [`tests::obj_token_round_trips`].
//! - each `# <ClassName>` header line names a block (the ROOT body class for
//!   the FIRST block; a nested validated class for the rest). It is used by
//!   the OpenAPI emitter to key `components/schemas/<ClassName>` AND by the
//!   multi-block decoder ([`parse_schema_blocks`]) to key each block. The
//!   per-FIELD VALIDATOR ignores it for free: it carries no TAB, so
//!   [`parse_schema`]'s `split_once('\t')` yields `None` and the line is
//!   skipped. This is the single-source discipline — the class names live in
//!   the SAME descriptor string the validator reads, so the schema names and
//!   the validated fields cannot come from two declarations (footgun #4).
//!
//! An EMPTY schema (no lines) means "validate JSON-object-ness only" (a
//! defensive fallback when the compiler could not resolve the body class;
//! the type checker has already accepted the program).
//!
//! # Recursion depth cap (ADR-0080 §6 Phase-4 (b) / #156, D3)
//!
//! A nested `obj` field recurses into its named block. Recursion terminates
//! on any FINITE JSON body (each recursion consumes one JSON-object nesting
//! level). A defensive depth cap ([`MAX_NESTING_DEPTH`]) guards a
//! PATHOLOGICAL cyclic SCHEMA (a class graph the compiler would normally
//! reject, but which — if ever emitted — would otherwise recurse against a
//! deeply-nested adversarial body without bound): exceeding it returns a
//! clear [`ValidationError::NestingTooDeep`] 422 rather than overflowing the
//! stack.
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
    /// Nested-object validation (ADR-0080 §6 Phase-4 (b) / #156) exceeded the
    /// defensive recursion depth cap ([`MAX_NESTING_DEPTH`]) — a guard
    /// against a PATHOLOGICAL cyclic schema. A well-formed schema (an acyclic
    /// class graph, which the compiler enforces) never reaches this against a
    /// finite body. `field` is the nested field at which the cap tripped.
    NestingTooDeep { field: String },
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
            Self::NestingTooDeep { field } => {
                format!(
                    "nested field `{field}` exceeds the maximum object-nesting depth \
                     ({MAX_NESTING_DEPTH}); flatten the request body or reduce nesting"
                )
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

/// `Clone` (not `Copy`) as of ADR-0080 §6 Phase-4 (b) / #156:
/// [`Self::Obj`] carries an owned `String` (the nested class name), so the
/// enum is no longer trivially copyable. SAFE — `FieldKind` is matched
/// by-reference (`match &spec.kind`) at every use site, and its methods take
/// `&self`; it is never a `HashMap`/`HashSet` key.
#[derive(Clone, Eq, PartialEq)]
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
    /// A field whose type is ANOTHER validated class (ADR-0080 §6 Phase-4
    /// (b) / #156, descriptor token `obj:<ClassName>`). The base JSON type is
    /// `object`; the JSON value is RECURSIVELY validated against the named
    /// class's block (looked up in the [`parse_schema_blocks`] map). The
    /// `String` is the nested class name (the `# <ClassName>` block header).
    Obj(String),
    /// A non-Phase-1b-ii scalar — presence-only (no type/range check).
    Any,
}

impl FieldKind {
    /// Parse a kind TOKEN (the part before the first `:` of a payload) into a
    /// [`FieldKind`]. `obj` carries the nested class NAME, supplied
    /// separately by the caller (the `obj:<ClassName>` suffix is the class
    /// name); every other token is suffix-free here.
    fn parse(s: &str) -> Self {
        match s {
            "str" => Self::Str,
            "i64" => Self::I64,
            "f64" => Self::F64,
            "bool" => Self::Bool,
            "pat" => Self::Pat,
            // `obj` is handled in `parse_schema` (it needs the suffix = the
            // nested class name); a bare `obj` with no name falls through to
            // `Any` (defensive — the compiler always emits `obj:<Name>`).
            _ => Self::Any,
        }
    }

    /// The validation-error label for a wrong-type 422 detail. A `pat` field
    /// is a string (the pattern constrains a string value); an `obj` field is
    /// an object.
    fn type_name(&self) -> &'static str {
        match self {
            Self::Str | Self::Pat => "string",
            Self::I64 => "integer",
            Self::F64 => "number",
            Self::Bool => "boolean",
            Self::Obj(_) => "object",
            Self::Any => "any",
        }
    }

    /// The OpenAPI 3.1 `type` keyword for this field kind (ADR-0080 §5.3):
    /// `str → string`, `i64 → integer`, `f64 → number`, `bool → boolean`,
    /// `pat → string` (a pattern constrains a string). An `Obj` field is NOT
    /// rendered via a `type` keyword at all — the emitter renders it as a
    /// `$ref` to the nested component (ADR-0080 §6 Phase-4 (b), D4), so this
    /// returns `None` for it (the `$ref` path in [`crate::openapi`] handles
    /// `Obj` before consulting the `type` keyword). An `Any` field (a
    /// non-Phase-1b-ii scalar) has no statically-known OpenAPI type, so the
    /// emitter likewise omits the `type` keyword (`None`).
    pub(crate) fn openapi_type(&self) -> Option<&'static str> {
        match self {
            Self::Str | Self::Pat => Some("string"),
            Self::I64 => Some("integer"),
            Self::F64 => Some("number"),
            Self::Bool => Some("boolean"),
            Self::Obj(_) | Self::Any => None,
        }
    }
}

/// The ROOT body-class name carried by the FIRST `# <ClassName>` block
/// header of a schema descriptor (see the module header). Returns `None`
/// when the descriptor carries no header line (the defensive empty-schema
/// fallback, or a pre-Phase-1b-iii descriptor). `pub(crate)` so the OpenAPI
/// emitter keys `components/schemas/<BodyName>` from the SAME descriptor
/// string the validator reads (footgun #4 — one source). Under the
/// multi-block descriptor (ADR-0080 §6 Phase-4 (b) / #156) the FIRST `# `
/// line is the ROOT block's name — exactly what this returned before, so the
/// flat-body behavior is byte-identical.
pub(crate) fn body_name(schema: &str) -> Option<String> {
    schema.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    })
}

/// ADR-0080 §6 Phase-4 (b) / #156 — the parsed MULTI-BLOCK descriptor: an
/// ORDERED list of `(class_name, field_specs)` blocks. The FIRST block is
/// the ROOT (the request body's class); the rest are nested validated
/// classes. Preserves descriptor order (so the ROOT is `blocks[0]`); a
/// by-name lookup ([`Self::block`]) resolves an `obj:<ClassName>` field's
/// nested specs. A FLAT-only body yields exactly ONE block.
pub(crate) struct SchemaBlocks {
    pub(crate) blocks: Vec<(String, Vec<FieldSpec>)>,
}

impl SchemaBlocks {
    /// The ROOT block's field specs (the FIRST block), or an empty slice for
    /// an empty/headerless descriptor (the defensive object-ness-only case).
    /// `pub(crate)` so the OpenAPI emitter reads the same root the validator
    /// does.
    pub(crate) fn root(&self) -> &[FieldSpec] {
        self.blocks.first().map_or(&[], |(_, specs)| specs)
    }

    /// The field specs of the block named `name` (an `obj:<name>` field's
    /// nested class), or `None` if no such block exists (a dangling `obj:`
    /// token the compiler would never emit — treated as a validation
    /// failure by the caller, fail-closed). `pub(crate)` so the OpenAPI
    /// emitter resolves the same nested block the validator recurses into.
    pub(crate) fn block(&self, name: &str) -> Option<&[FieldSpec]> {
        self.blocks
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, specs)| specs.as_slice())
    }
}

/// Parse the compact MULTI-BLOCK schema descriptor (see the module header)
/// into its ordered `(class_name, field_specs)` blocks. A `# <ClassName>`
/// line opens a new block; subsequent `field<TAB>payload` lines belong to
/// it. Lines before the first `# ` header (and any malformed line — a
/// `field<TAB>…` line with no TAB is skipped by [`parse_field_line`]) attach
/// to an unnamed leading block ONLY when a TAB-bearing field line precedes
/// any header (defensive — the compiler always emits a `# ` header first).
/// `pub(crate)` so the OpenAPI emitter walks the SAME parse the validator
/// does (the cannot-drift single source).
pub(crate) fn parse_schema_blocks(schema: &str) -> SchemaBlocks {
    let mut blocks: Vec<(String, Vec<FieldSpec>)> = Vec::new();
    for line in schema.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("# ") {
            let name = name.trim().to_string();
            if !name.is_empty() {
                blocks.push((name, Vec::new()));
                continue;
            }
        }
        let Some(spec) = parse_field_line(line) else {
            continue;
        };
        // Attach to the current (last) block, or open an unnamed one if the
        // descriptor began with a field line before any header (defensive).
        if let Some((_, specs)) = blocks.last_mut() {
            specs.push(spec);
        } else {
            blocks.push((String::new(), vec![spec]));
        }
    }
    SchemaBlocks { blocks }
}

/// Parse a schema descriptor into the ROOT block's field-spec list. The
/// back-compat shim over [`parse_schema_blocks`]: returns the FIRST block's
/// specs (the request body's fields). The OpenAPI emitter's per-field
/// helpers + the existing tests consume this; the multi-block recursion uses
/// [`parse_schema_blocks`] directly. For a FLAT body (one block) this is
/// byte-identical to the pre-Phase-4 single-block parse.
pub(crate) fn parse_schema(schema: &str) -> Vec<FieldSpec> {
    parse_schema_blocks(schema)
        .blocks
        .into_iter()
        .next()
        .map_or_else(Vec::new, |(_, specs)| specs)
}

/// Parse ONE `field<TAB>payload` field line into a [`FieldSpec`]. Returns
/// `None` for a malformed line (no TAB — including a `# <ClassName>` header,
/// which the block parser handles separately). The payload is
/// `<kind-token>[<suffix>]` (mirroring [`cobrust_types::Refinement::descriptor_payload`]
/// — the ONE encoder — for scalars, and MIR `emit_class_block` for the `obj:`
/// token):
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
/// - `obj:<ClassName>` — a NESTED validated class (ADR-0080 §6 Phase-4 (b) /
///   #156). The class name is EVERYTHING after the first `:`; the field is
///   recursively validated against that class's block. The base JSON type is
///   `object`.
fn parse_field_line(line: &str) -> Option<FieldSpec> {
    let (name, rest) = line.split_once('\t')?;
    // The kind token is everything up to the FIRST `:`; the remainder is the
    // kind-specific suffix (a `:lo:hi` numeric pair, the raw regex of a `pat`
    // field, or the nested class name of an `obj` field — any of which may
    // itself contain `:`).
    let (kind_token, suffix) = match rest.split_once(':') {
        Some((k, s)) => (k, Some(s)),
        None => (rest, None),
    };
    // `obj:<ClassName>` is decoded here (it needs the suffix = the nested
    // class name, which `FieldKind::parse` cannot carry). The DECODE mirror
    // of MIR `emit_class_block`'s ENCODE (footgun #4, cannot drift).
    if kind_token == "obj" {
        let class_name = suffix.unwrap_or("").to_string();
        return Some(FieldSpec {
            name: name.to_string(),
            kind: FieldKind::Obj(class_name),
            lo: None,
            hi: None,
            lo_f: None,
            hi_f: None,
            pattern: None,
        });
    }
    let kind = FieldKind::parse(kind_token);
    let (lo, hi, lo_f, hi_f, pattern) = match kind {
        FieldKind::Pat => {
            // The pattern payload is the raw regex (the whole remainder after
            // the first `:`). An empty/absent remainder → no pattern.
            (
                None,
                None,
                None,
                None,
                suffix.filter(|s| !s.is_empty()).map(str::to_string),
            )
        }
        FieldKind::F64 => {
            // ADR-0080 Phase-3a — an `f64` field's bounds parse as `f64` into
            // the SEPARATE float pair (an `i64` parse would reject a
            // fractional bound like `0.5`). The DECODE half of the
            // cannot-drift pair (the ENCODE is `float_suffix` in
            // cobrust-types).
            let (lo_f, hi_f) = parse_float_suffix(suffix);
            (None, None, lo_f, hi_f, None)
        }
        // Every other kind carries the `:lo:hi` INTEGER suffix (an absent
        // bound is the empty string). `Obj` is handled above (early return).
        FieldKind::Str | FieldKind::I64 | FieldKind::Bool | FieldKind::Obj(_) | FieldKind::Any => {
            let (lo, hi) = parse_numeric_suffix(suffix);
            (lo, hi, None, None, None)
        }
    };
    Some(FieldSpec {
        name: name.to_string(),
        kind,
        lo,
        hi,
        lo_f,
        hi_f,
        pattern,
    })
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

/// The defensive recursion depth cap for nested-object validation (ADR-0080
/// §6 Phase-4 (b) / #156, D3). The ROOT body is depth 0; each nested `obj`
/// field recurses one level deeper. A WELL-FORMED schema (an acyclic class
/// graph, which the compiler enforces) bounds its own nesting at the class
/// graph's depth, far below this; the cap exists ONLY to guard a
/// PATHOLOGICAL cyclic schema against a deeply-nested adversarial body —
/// exceeding it returns [`ValidationError::NestingTooDeep`] rather than
/// overflowing the stack. 64 levels is generous for any real nested model.
pub(crate) const MAX_NESTING_DEPTH: usize = 64;

/// Validate `body` against `schema` (ADR-0080 §5.4). Returns `Ok(())` iff
/// the body is an object whose keys EXACTLY match the ROOT block's declared
/// fields, each of the declared base type, with every int-range / f64
/// value-range / str-length / str-pattern refinement satisfied — AND every
/// nested `obj` field's value is itself an object recursively valid against
/// its named block (ADR-0080 §6 Phase-4 (b) / #156).
///
/// This is the TOTAL boundary deserialization (footgun #1): a missing key,
/// an extra key, a wrong JSON type, an out-of-range value, or an invalid
/// nested object yields `Err`, so a structurally-invalid body is unable to
/// reach the handler.
///
/// # Errors
///
/// Returns the FIRST [`ValidationError`] encountered (checked in a stable
/// order: object-ness → unknown keys → per-field presence/type/range, with
/// a nested `obj` field recursed in-line).
pub fn validate_against_schema(schema: &str, body: &Value) -> Result<(), ValidationError> {
    let blocks = parse_schema_blocks(schema);
    validate_block(blocks.root(), body, &blocks, 0)
}

/// Validate `body` against one block's `specs` within the multi-block
/// `blocks` (ADR-0080 §6 Phase-4 (b) / #156). The SAME total-deserialization
/// policy at every nesting level (the ROOT and each nested block): object-
/// ness, no unknown keys, every declared field present + type/range-checked,
/// a nested `obj` field recursed against its named block. `depth` is the
/// current nesting level (the ROOT is 0); the recursion is bounded by
/// [`MAX_NESTING_DEPTH`].
fn validate_block(
    specs: &[FieldSpec],
    body: &Value,
    blocks: &SchemaBlocks,
    depth: usize,
) -> Result<(), ValidationError> {
    let Value::Object(map) = body else {
        return Err(ValidationError::NotAnObject);
    };

    // Reject unknown keys (total deserialization — no extra fields). SAME
    // policy at every level, so a nested extra key is rejected exactly like a
    // root extra key.
    for key in map.keys() {
        if !specs.iter().any(|s| &s.name == key) {
            return Err(ValidationError::UnknownField { field: key.clone() });
        }
    }

    // Every declared field must be present, of the right type, in range —
    // and a nested `obj` field recursed against its named block.
    for spec in specs {
        let Some(value) = map.get(&spec.name) else {
            return Err(ValidationError::MissingField {
                field: spec.name.clone(),
            });
        };
        check_field(spec, value, blocks, depth)?;
    }
    Ok(())
}

/// Type-check (and range/length/pattern-check, OR recursively validate a
/// nested `obj`) one field's JSON value against its spec. `blocks` + `depth`
/// thread the multi-block map and the recursion bound for an `obj` field.
fn check_field(
    spec: &FieldSpec,
    value: &Value,
    blocks: &SchemaBlocks,
    depth: usize,
) -> Result<(), ValidationError> {
    match &spec.kind {
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
        FieldKind::Obj(class_name) => {
            // ADR-0080 §6 Phase-4 (b) / #156 (D3) — a nested validated class.
            // The value MUST be a JSON object (a non-object is a WrongType
            // 422, the SAME as any other type mismatch); it is then
            // RECURSIVELY validated against the named class's block. The
            // depth cap guards a pathological cyclic schema.
            if depth + 1 > MAX_NESTING_DEPTH {
                return Err(ValidationError::NestingTooDeep {
                    field: spec.name.clone(),
                });
            }
            if !value.is_object() {
                return Err(wrong_type(spec));
            }
            // Resolve the nested block by name. A dangling `obj:` token (no
            // such block — the compiler never emits one) fails closed as a
            // WrongType (it never reaches the handler).
            let Some(nested_specs) = blocks.block(class_name) else {
                return Err(wrong_type(spec));
            };
            validate_block(nested_specs, value, blocks, depth + 1)?;
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

    // ----- ADR-0080 §6 Phase-4 (b) / #156: nested-object validation -----

    /// The MULTI-BLOCK nested descriptor: a `CreateUser` ROOT with a flat
    /// `name: str` field and a nested `address: Address` field, plus an
    /// `Address` block with a flat `city: str` and a range-refined
    /// `zip: i64 where 0 <= self <= 99999`. Field lines are in `BTreeMap`
    /// name order (the MIR encoder's order: `address` before `name`).
    const NESTED_SCHEMA: &str =
        "# CreateUser\naddress\tobj:Address\nname\tstr\n# Address\ncity\tstr\nzip\ti64:0:99999";

    #[test]
    fn nested_valid_body_passes() {
        let v = json!({"name": "a", "address": {"city": "NYC", "zip": 10001}});
        assert_eq!(validate_against_schema(NESTED_SCHEMA, &v), Ok(()));
    }

    #[test]
    fn nested_out_of_range_zip_rejected() {
        // 100000 > 99999 — the RECURSIVE range-check fires on the nested field.
        let v = json!({"name": "a", "address": {"city": "NYC", "zip": 100_000}});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &v),
            Err(ValidationError::OutOfRange { value: 100_000, .. })
        ));
    }

    #[test]
    fn nested_missing_field_rejected() {
        // The nested object omits `city` — the SAME missing-field policy
        // applied recursively.
        let v = json!({"name": "a", "address": {"zip": 10001}});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &v),
            Err(ValidationError::MissingField { field }) if field == "city"
        ));
    }

    #[test]
    fn nested_wrong_type_field_rejected() {
        // address.zip as a string → WrongType, recursed.
        let v = json!({"name": "a", "address": {"city": "NYC", "zip": "x"}});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &v),
            Err(ValidationError::WrongType { field, expected: "integer" }) if field == "zip"
        ));
    }

    #[test]
    fn nested_not_an_object_is_wrong_type() {
        // D3 — an `obj` field's value MUST be a JSON object; a string is a
        // WrongType 422 (the field, `address`, is named; `expected` = object).
        let v = json!({"name": "a", "address": "oops"});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &v),
            Err(ValidationError::WrongType { field, expected: "object" }) if field == "address"
        ));
    }

    #[test]
    fn nested_extra_key_rejected() {
        // D3 — a nested undeclared key follows the SAME unknown-key policy as
        // the flat validator (UnknownField), recursed.
        let v = json!({"name": "a", "address": {"city": "NYC", "zip": 10001, "country": "US"}});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &v),
            Err(ValidationError::UnknownField { field }) if field == "country"
        ));
    }

    #[test]
    fn nested_root_flat_field_still_validated() {
        // The ROOT's flat `name` field is still required + type-checked under
        // the nested schema (D1 flat-byte-identical — a nested field does not
        // regress the root block).
        let missing_name = json!({"address": {"city": "NYC", "zip": 10001}});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &missing_name),
            Err(ValidationError::MissingField { field }) if field == "name"
        ));
        let wrong_name = json!({"name": 42, "address": {"city": "NYC", "zip": 10001}});
        assert!(matches!(
            validate_against_schema(NESTED_SCHEMA, &wrong_name),
            Err(ValidationError::WrongType { field, .. }) if field == "name"
        ));
    }

    #[test]
    fn obj_token_round_trips() {
        // The cannot-drift pin (footgun #4): the `obj:Address` token the MIR
        // encoder emits decodes back to `FieldKind::Obj("Address")`, and the
        // multi-block descriptor parses into the ROOT + nested blocks keyed by
        // their `# <Name>` headers. If the ENCODE (MIR `emit_class_block`) and
        // this DECODE (`parse_schema_blocks`) ever drift, this breaks.
        let blocks = parse_schema_blocks(NESTED_SCHEMA);
        assert_eq!(blocks.blocks.len(), 2, "ROOT + one nested block");
        assert_eq!(blocks.blocks[0].0, "CreateUser", "ROOT is the FIRST block");
        assert_eq!(blocks.blocks[1].0, "Address");
        // The root's `address` field decodes to `Obj("Address")`.
        let root = blocks.root();
        let addr = root
            .iter()
            .find(|s| s.name == "address")
            .expect("address field");
        assert!(matches!(&addr.kind, FieldKind::Obj(n) if n == "Address"));
        // The nested block resolves by name and carries the refined `zip`.
        let nested = blocks
            .block("Address")
            .expect("Address block resolvable by name");
        let zip = nested.iter().find(|s| s.name == "zip").expect("zip field");
        assert!(matches!(zip.kind, FieldKind::I64));
        assert_eq!(
            zip.hi,
            Some(99999),
            "the nested range bound survives the round-trip"
        );
    }

    #[test]
    fn deeply_nested_object_validates_recursively() {
        // Two levels of nesting (a -> b -> c). Exercises the recursion past
        // one level and confirms the deepest scalar is range-checked.
        let schema = "# A\nb\tobj:B\n# B\nc\tobj:C\n# C\nn\ti64:0:10";
        let ok = json!({"b": {"c": {"n": 5}}});
        assert_eq!(validate_against_schema(schema, &ok), Ok(()));
        // The deepest field is range-checked (11 > 10).
        let bad = json!({"b": {"c": {"n": 11}}});
        assert!(matches!(
            validate_against_schema(schema, &bad),
            Err(ValidationError::OutOfRange { value: 11, .. })
        ));
        // A middle level that is not an object → WrongType at `c`.
        let not_obj = json!({"b": {"c": "x"}});
        assert!(matches!(
            validate_against_schema(schema, &not_obj),
            Err(ValidationError::WrongType { field, expected: "object" }) if field == "c"
        ));
    }

    #[test]
    fn cyclic_schema_depth_cap_returns_clear_error() {
        // A PATHOLOGICAL cyclic schema (`A.next: A`) — the compiler would
        // normally reject building this, but if it ever reached the validator,
        // a deeply-nested adversarial body must hit the depth cap, NOT
        // overflow the stack. Build a body nested past MAX_NESTING_DEPTH.
        let schema = "# A\nnext\tobj:A";
        // Construct a JSON object nested `MAX_NESTING_DEPTH + 5` levels deep
        // under the key `next`. The innermost value omits `next` (it would
        // need to be present for a valid leaf, but the cap trips before that).
        let mut v = json!({});
        for _ in 0..(MAX_NESTING_DEPTH + 5) {
            v = json!({ "next": v });
        }
        assert!(matches!(
            validate_against_schema(schema, &v),
            Err(ValidationError::NestingTooDeep { field }) if field == "next"
        ));
    }

    #[test]
    fn nesting_too_deep_error_body_is_valid_json_with_fix() {
        // §2.5-B — the NestingTooDeep detail prints a FIX (flatten / reduce
        // nesting). The 422 body round-trips as JSON.
        let e = ValidationError::NestingTooDeep {
            field: "address".to_string(),
        };
        let body = e.to_json_body();
        let parsed: Value = serde_json::from_str(&body).expect("422 body is valid JSON");
        assert_eq!(parsed["error"], "validation_failed");
        let detail = parsed["detail"].as_str().expect("detail is a string");
        assert!(
            detail.contains("address"),
            "detail names the field: {detail}"
        );
        assert!(
            detail.contains("flatten") || detail.contains("nesting"),
            "detail prints a FIX (flatten/reduce nesting): {detail}"
        );
    }

    #[test]
    fn flat_descriptor_is_byte_identical_single_block() {
        // The locked constraint: a FLAT-only descriptor (no nested field) is
        // a SINGLE block, parsed identically to the pre-Phase-4 path. The root
        // block IS the whole field list; there is exactly one block.
        let flat = "# CreateScore\nname\tstr\nrank\ti64:0:100";
        let blocks = parse_schema_blocks(flat);
        assert_eq!(blocks.blocks.len(), 1, "a flat body is ONE block");
        assert_eq!(blocks.blocks[0].0, "CreateScore");
        // And the back-compat `parse_schema` returns exactly the root specs.
        let specs = parse_schema(flat);
        assert_eq!(specs.len(), 2);
        assert!(specs.iter().any(|s| s.name == "name"));
        assert!(specs.iter().any(|s| s.name == "rank" && s.hi == Some(100)));
    }
}
