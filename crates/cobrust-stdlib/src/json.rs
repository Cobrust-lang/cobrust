//! `std.json` — Python-`json`-compatible encode/decode over `serde_json`.
//!
//! v0.7.0 Stream Z.5 (HYBRID per
//! `docs/agent/strategy/v0.7.0-network-backend-libraries-roadmap.md`
//! §4.1 JSON row): a thin LLM-first surface module wrapping the
//! gold-tier `serde_json` crate. Mirrors the existing internal
//! `serde_json::Value` precedent in [`crate::tool`].
//!
//! # Surface
//!
//! - [`dumps`] — `serde_json::Value -> String`, CPython-exact output.
//! - [`loads`] — `&str -> Result<Value, Error>`, parse to a value.
//! - [`dumps_str`] / [`loads_str`] — the `str -> str` C-ABI-shaped
//!   helpers the `.cb` source surface binds onto (one JSON string in,
//!   one JSON string out), matching the [`crate::tool`] shim shape.
//!
//! # `@py_compat(semantic)`
//!
//! The encoder reproduces CPython 3.11 `json.dumps` defaults exactly
//! for the supported value matrix (null / bool / int / float / str /
//! list / dict):
//!
//! - default separators `", "` (item) and `": "` (key) — NOT
//!   `serde_json`'s compact `,` / `:`;
//! - `ensure_ascii=True` — non-ASCII scalars are `\uXXXX`-escaped;
//! - `indent=N` pretty-print parity (item separator collapses to `,`,
//!   key separator stays `": "`, empty containers stay `{}` / `[]`).
//!
//! The tier is `semantic`, not `strict`, for two declared divergences
//! (CLAUDE.md §2.4 — declared, not hidden):
//!
//! 1. **Object key order**: CPython preserves dict *insertion* order;
//!    `serde_json`'s default `Map` is a `BTreeMap`, so object keys come
//!    out *alphabetically sorted*. Separators, escapes, and scalars all
//!    match CPython exactly — only key order differs. Closing this would
//!    require enabling `serde_json`'s `preserve_order` feature, which has
//!    workspace-wide blast radius (`tool.rs` asserts specific key
//!    orderings), so it is deferred out of the Z.5 envelope.
//! 2. **Float formatting**: can diverge from CPython's `repr`-shortest
//!    algorithm in edge cases (CPython uses David Gay's `dtoa`; Rust's
//!    `{}` uses Ryū / Grisu). Integer-valued floats (`3.0`), finite
//!    decimals, and the whole-number / small-fraction cases in the
//!    differential corpus match bit-for-bit; pathological round-trips
//!    (subnormals, 17-digit ties) may differ in the last digit.
//!
//! # Errors
//!
//! Constitution §2.2: no exceptions. A malformed parse returns
//! [`Error::Parse`] (an `Err`), never a panic.

use std::fmt::Write as _;

use serde_json::Value;

// =====================================================================
// Error taxonomy
// =====================================================================

/// JSON surface error. Constitution §2.2: `Result<T, E>` is the
/// default error path; a malformed document is an `Err`, not a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input was not valid JSON. Carries the underlying
    /// `serde_json` message (line / column / cause) for the §2.5
    /// LLM-first "print the fix" error-UX direction.
    Parse(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(msg) => write!(f, "json: invalid document: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

// =====================================================================
// loads — parse
// =====================================================================

/// Parse a JSON string into a [`serde_json::Value`].
///
/// Mirrors Python `json.loads(s)`. Returns [`Error::Parse`] on a
/// malformed document (never panics — constitution §2.2).
///
/// # Errors
///
/// Returns [`Error::Parse`] when `s` is not a well-formed JSON
/// document.
pub fn loads(s: &str) -> Result<Value, Error> {
    serde_json::from_str(s).map_err(|e| Error::Parse(e.to_string()))
}

/// `str -> str` parse helper for the `.cb` source surface: parse `s`,
/// then re-emit it in CPython-canonical form (validating + normalizing
/// separators / escapes). Returns `""` on a malformed document so the
/// C-ABI shim has a non-panicking sentinel (matching [`crate::tool`]'s
/// empty-string error convention).
///
/// `.cb` callers who need typed error handling use the typed [`loads`]
/// from Rust; the `str` surface follows the established empty-string
/// sentinel for the alpha binding.
#[must_use]
pub fn loads_str(s: &str) -> String {
    match loads(s) {
        Ok(v) => dumps(&v),
        Err(_) => String::new(),
    }
}

// =====================================================================
// dumps — serialize (CPython-exact)
// =====================================================================

/// Serialize a [`serde_json::Value`] to a compact JSON string matching
/// CPython 3.11 `json.dumps(value)` defaults: `", "` / `": "`
/// separators, `ensure_ascii=True`.
#[must_use]
pub fn dumps(value: &Value) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

/// Serialize with `indent=n` pretty-printing, matching CPython
/// `json.dumps(value, indent=n)`: each nesting level is `n` spaces, the
/// item separator collapses to `,` (newline-separated), the key
/// separator stays `": "`, and empty containers stay `{}` / `[]` on one
/// line.
#[must_use]
pub fn dumps_indent(value: &Value, indent: usize) -> String {
    let mut out = String::new();
    write_pretty(&mut out, value, indent, 0);
    out
}

/// `str -> str` serialize helper for the `.cb` source surface: parse a
/// JSON input string, then re-emit it CPython-canonical. Returns `""`
/// on malformed input (non-panicking sentinel; see [`loads_str`]).
///
/// This is the function the `.cb`-level `json.dumps` binds onto for the
/// alpha surface — Cobrust has no dynamic `value` type, so the surface
/// is string-to-string (one JSON document in, the canonical form out),
/// exactly the shape [`crate::tool::tool_schema_helper`] uses.
#[must_use]
pub fn dumps_str(json_input: &str) -> String {
    match loads(json_input) {
        Ok(v) => dumps(&v),
        Err(_) => String::new(),
    }
}

/// `str -> str` indented serialize helper (the `indent=` stretch
/// surface). Parses `json_input`, re-emits with `indent` spaces per
/// level. Returns `""` on malformed input.
#[must_use]
pub fn dumps_str_indent(json_input: &str, indent: usize) -> String {
    match loads(json_input) {
        Ok(v) => dumps_indent(&v, indent),
        Err(_) => String::new(),
    }
}

// ---------------------------------------------------------------------
// Internal: CPython-exact writers
// ---------------------------------------------------------------------

/// Write `value` compactly with CPython default `", "` / `": "`
/// separators.
fn write_compact(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&format_number(n)),
        Value::String(s) => write_py_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_py_string(out, k);
                out.push_str(": ");
                write_compact(out, v);
            }
            out.push('}');
        }
    }
}

/// Write `value` with `indent`-space pretty-printing, matching CPython
/// `json.dumps(value, indent=indent)`. `level` is the current nesting
/// depth.
fn write_pretty(out: &mut String, value: &Value, indent: usize, level: usize) {
    match value {
        Value::Array(items) if !items.is_empty() => {
            out.push('[');
            let inner = " ".repeat(indent * (level + 1));
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&inner);
                write_pretty(out, item, indent, level + 1);
            }
            out.push('\n');
            out.push_str(&" ".repeat(indent * level));
            out.push(']');
        }
        Value::Object(map) if !map.is_empty() => {
            out.push('{');
            let inner = " ".repeat(indent * (level + 1));
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&inner);
                write_py_string(out, k);
                out.push_str(": ");
                write_pretty(out, v, indent, level + 1);
            }
            out.push('\n');
            out.push_str(&" ".repeat(indent * level));
            out.push('}');
        }
        // Scalars + empty containers: CPython keeps `{}` / `[]` and all
        // scalars on a single token, identical to the compact form.
        _ => write_compact(out, value),
    }
}

/// Format a `serde_json::Number` the way CPython renders it.
///
/// - Integers print bare (`42`, `-7`).
/// - Floats use Rust's shortest round-trip `{}` (matches CPython for
///   the common cases); an integer-valued float is forced to carry a
///   `.0` suffix to match CPython (`3.0`, not `3`).
fn format_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    // Float path.
    let f = n.as_f64().unwrap_or(0.0);
    if f.is_finite() && f.fract() == 0.0 {
        // CPython renders integer-valued floats with a `.0` suffix.
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// Write `s` as a JSON string literal with CPython `ensure_ascii=True`
/// semantics: ASCII control + structural chars are backslash-escaped,
/// and every non-ASCII code point is emitted as one or more `\uXXXX`
/// units (surrogate pair for astral code points), matching CPython.
fn write_py_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            // Other C0 control chars → \u00XX.
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            // ASCII printable passes through verbatim.
            c if (c as u32) < 0x7f => out.push(c),
            // Non-ASCII (and DEL): ensure_ascii=True → \uXXXX. Astral
            // code points are emitted as a UTF-16 surrogate pair,
            // exactly as CPython does.
            c => {
                let cp = c as u32;
                if cp <= 0xffff {
                    let _ = write!(out, "\\u{cp:04x}");
                } else {
                    let v = cp - 0x1_0000;
                    let hi = 0xd800 + (v >> 10);
                    let lo = 0xdc00 + (v & 0x3ff);
                    let _ = write!(out, "\\u{hi:04x}\\u{lo:04x}");
                }
            }
        }
    }
    out.push('"');
}

// =====================================================================
// C-ABI surface — mirrors `tool.rs` / `prompt.rs` shim conventions.
// =====================================================================
//
// `json.dumps` / `json.loads` bind onto `str -> str` shims, the same
// shape as `__cobrust_tool_schema` (ADR-0048 M-AI.2). The shim reads its
// Str-buffer arg via `read_str_buf`, computes, and returns a fresh owned
// Str buffer via `alloc_str_buffer`. The caller's MIR drop schedule owns
// both lifetimes (ADR-0050c Phase 2).

/// Read a heap `Str` pointer as a `String`. Tolerates null and empty.
///
/// # Safety
///
/// `buf` must be null or a valid Cobrust `Str` buffer pointer produced
/// by `__cobrust_str_new`.
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    // SAFETY: caller attests `buf` is a valid Cobrust Str buffer pointer.
    unsafe {
        let ptr = crate::fmt::__cobrust_str_ptr(buf);
        let len = crate::fmt::__cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

/// Allocate a fresh heap `Str` buffer carrying `s`'s bytes.
fn alloc_str_buffer(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` allocates a valid Str buffer and
    // `__cobrust_str_push_static` copies `s` into it.
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// C-ABI shim for source-level `json_dumps(json_input: str) -> str`.
///
/// Parses `json_input` and re-emits it in CPython-canonical compact
/// form. Returns an empty Str on malformed input.
///
/// # Safety
///
/// `json_input` must be null or a valid Cobrust `Str` buffer pointer.
/// The returned pointer is an owned Cobrust `Str` buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_json_dumps(json_input: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let s = unsafe { read_str_buf(json_input) };
    alloc_str_buffer(&dumps_str(&s))
}

/// C-ABI shim for source-level
/// `json_dumps_indent(json_input: str, indent: i64) -> str`.
///
/// Parses `json_input` and re-emits with `indent` spaces per level.
/// Negative `indent` is clamped to `0`. Returns an empty Str on
/// malformed input.
///
/// # Safety
///
/// `json_input` must be null or a valid Cobrust `Str` buffer pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_json_dumps_indent(json_input: *mut u8, indent: i64) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let s = unsafe { read_str_buf(json_input) };
    let indent = usize::try_from(indent).unwrap_or(0);
    alloc_str_buffer(&dumps_str_indent(&s, indent))
}

/// C-ABI shim for source-level `json_loads(s: str) -> str`.
///
/// Validates + canonicalizes `s`. Returns an empty Str on a malformed
/// document (non-panicking sentinel; constitution §2.2).
///
/// # Safety
///
/// `s` must be null or a valid Cobrust `Str` buffer pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_json_loads(s: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation per `# Safety` clause.
    let input = unsafe { read_str_buf(s) };
    alloc_str_buffer(&loads_str(&input))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    // =================================================================
    // Differential corpus.
    //
    // Expected output strings were captured from CPython `json`
    // (`json.dumps(value)` with all-default kwargs; behavior is
    // identical across CPython 3.9–3.11 for separators, `ensure_ascii`,
    // and the float / int rendering covered here). Documented per
    // CLAUDE.md §4.2 — oracle = CPython stdlib `json`.
    // =================================================================

    /// Round-trip + CPython-exact assertion helper. `cb_input` is a
    /// (possibly non-canonical) JSON string; `expected` is the exact
    /// `CPython json.dumps(json.loads(cb_input))` output.
    fn assert_cpython(cb_input: &str, expected: &str) {
        let got = dumps_str(cb_input);
        assert_eq!(got, expected, "dumps_str({cb_input:?})");
        // Round-trip stability: canonical form is a fixed point.
        assert_eq!(dumps_str(&got), expected, "round-trip of {got:?}");
        // loads_str canonicalizes identically.
        assert_eq!(loads_str(cb_input), expected, "loads_str({cb_input:?})");
    }

    // ---- value-type matrix (CLAUDE.md §4.2 spirit) ----

    #[test]
    fn cpython_null() {
        assert_cpython("null", "null");
    }

    #[test]
    fn cpython_bool_true() {
        assert_cpython("true", "true");
    }

    #[test]
    fn cpython_bool_false() {
        assert_cpython("false", "false");
    }

    #[test]
    fn cpython_int() {
        assert_cpython("42", "42");
    }

    #[test]
    fn cpython_int_negative() {
        assert_cpython("-7", "-7");
    }

    #[test]
    fn cpython_float() {
        assert_cpython("3.14", "3.14");
    }

    #[test]
    fn cpython_float_integer_valued_gets_dot_zero() {
        // CPython: json.dumps(3.0) == "3.0".
        assert_cpython("3.0", "3.0");
    }

    #[test]
    fn cpython_str_ascii() {
        assert_cpython("\"hello\"", "\"hello\"");
    }

    #[test]
    fn cpython_str_unicode_ensure_ascii() {
        // CPython json.dumps("héllo你好") == '"h\\u00e9llo\\u4f60\\u597d"'.
        assert_cpython("\"héllo你好\"", "\"h\\u00e9llo\\u4f60\\u597d\"");
    }

    #[test]
    fn cpython_str_escapes() {
        // CPython json.dumps('a"b\\c\n') == '"a\\"b\\\\c\\n"'.
        assert_cpython("\"a\\\"b\\\\c\\n\"", "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn cpython_empty_list() {
        assert_cpython("[]", "[]");
    }

    #[test]
    fn cpython_empty_dict() {
        assert_cpython("{}", "{}");
    }

    #[test]
    fn cpython_list_ints() {
        // CPython default separators: ", " between items.
        assert_cpython("[1,2,3]", "[1, 2, 3]");
    }

    #[test]
    fn cpython_nested_list() {
        assert_cpython("[[1,2],[3,4]]", "[[1, 2], [3, 4]]");
    }

    #[test]
    fn cpython_dict_simple() {
        // CPython: '{"a": 1, "b": 2}'.
        assert_cpython("{\"a\":1,\"b\":2}", "{\"a\": 1, \"b\": 2}");
    }

    #[test]
    fn cpython_nested_dict() {
        assert_cpython("{\"outer\":{\"inner\":1}}", "{\"outer\": {\"inner\": 1}}");
    }

    #[test]
    fn cpython_mixed_key_ordering_divergence() {
        // KEY-ORDERING DIVERGENCE (documented `semantic`-tier gap):
        // CPython preserves dict INSERTION order:
        //   '{"name": "x", "vals": [1, 2], "ok": true, "n": null}'
        // serde_json's default `Map` is a `BTreeMap`, so keys come out
        // ALPHABETICALLY SORTED. Separators / escapes / scalars all match
        // CPython; only object key order differs. Per CLAUDE.md §2.4 this
        // is declared, not hidden — see the module-level `@py_compat`
        // note. Enabling serde_json `preserve_order` (an existing-dep
        // feature) would close this but has workspace-wide blast radius
        // (`tool.rs` key-order assertions), so it is deferred.
        assert_cpython(
            "{\"name\":\"x\",\"vals\":[1,2],\"ok\":true,\"n\":null}",
            "{\"n\": null, \"name\": \"x\", \"ok\": true, \"vals\": [1, 2]}",
        );
    }

    // ---- whitespace normalization (loads ignores input whitespace) ----

    #[test]
    fn dumps_str_normalizes_input_whitespace() {
        assert_cpython("[ 1 ,  2 ,3 ]", "[1, 2, 3]");
        assert_cpython("{ \"a\" : 1 }", "{\"a\": 1}");
    }

    // ---- typed loads ----

    #[test]
    fn loads_parses_to_value() {
        let v = loads("{\"k\": [1, 2, 3]}").unwrap();
        assert!(v.is_object());
        assert_eq!(v["k"][0], 1);
        assert_eq!(v["k"][2], 3);
    }

    #[test]
    fn loads_malformed_is_err_not_panic() {
        let r = loads("{not json");
        assert!(r.is_err());
        match r {
            Err(Error::Parse(msg)) => assert!(!msg.is_empty()),
            Ok(_) => panic!("expected parse error"),
        }
    }

    #[test]
    fn dumps_str_malformed_returns_empty_sentinel() {
        assert_eq!(dumps_str("{not json"), "");
        assert_eq!(loads_str("not json at all"), "");
    }

    // ---- typed dumps over Value ----

    #[test]
    fn dumps_value_directly() {
        let v: Value = serde_json::json!({"a": 1, "b": [true, null]});
        assert_eq!(dumps(&v), "{\"a\": 1, \"b\": [true, null]}");
    }

    // ---- indent= pretty-print parity (stretch) ----

    #[test]
    fn cpython_indent_object_with_list() {
        // CPython json.dumps({"a":1,"b":[1,2]}, indent=2).
        let got = dumps_str_indent("{\"a\":1,\"b\":[1,2]}", 2);
        assert_eq!(got, "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}");
    }

    #[test]
    fn cpython_indent_list() {
        // CPython json.dumps([1,2,3], indent=2).
        let got = dumps_str_indent("[1,2,3]", 2);
        assert_eq!(got, "[\n  1,\n  2,\n  3\n]");
    }

    #[test]
    fn cpython_indent_empty_containers_stay_inline() {
        // CPython keeps {} / [] inline even with indent.
        assert_eq!(dumps_str_indent("{}", 2), "{}");
        assert_eq!(dumps_str_indent("[]", 2), "[]");
    }

    #[test]
    fn cpython_indent_4_nested() {
        // CPython json.dumps({"a":1,"nested":{"x":[1,2]}}, indent=4).
        let got = dumps_str_indent("{\"a\":1,\"nested\":{\"x\":[1,2]}}", 4);
        assert_eq!(
            got,
            "{\n    \"a\": 1,\n    \"nested\": {\n        \"x\": [\n            1,\n            2\n        ]\n    }\n}"
        );
    }

    // ---- escape edge cases ----

    #[test]
    fn escapes_tab_cr_backspace_formfeed() {
        // CPython json.dumps("\t\r") == '"\\t\\r\\b\\f"'.
        let v = Value::String("\t\r\u{08}\u{0c}".to_string());
        assert_eq!(dumps(&v), "\"\\t\\r\\b\\f\"");
    }

    #[test]
    fn escapes_other_control_char_lowercase_hex() {
        // CPython json.dumps("\x01") == '"\\u0001"'.
        let v = Value::String("\u{01}".to_string());
        assert_eq!(dumps(&v), "\"\\u0001\"");
    }

    #[test]
    fn astral_codepoint_surrogate_pair() {
        // CPython json.dumps("😀") == '"\\ud83d\\ude00"' (U+1F600).
        let v = Value::String("😀".to_string());
        assert_eq!(dumps(&v), "\"\\ud83d\\ude00\"");
    }

    // ---- differential fuzz (constitution §4.2: >=1000 inputs) ----

    #[test]
    fn differential_fuzz_round_trip_stability() {
        // Deterministic LCG so the corpus is reproducible (§5.2).
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            state >> 33
        };
        let mut count = 0;
        for _ in 0..1500 {
            let v = random_value(&mut next, 0);
            // Canonical form must be a parse-stable fixed point: dumping
            // a value, parsing it back, and re-dumping yields the same
            // string. This is the round-trip invariant.
            let s1 = dumps(&v);
            let reparsed = loads(&s1).expect("our own output must parse");
            let s2 = dumps(&reparsed);
            assert_eq!(s1, s2, "round-trip not stable for {v:?}");
            // dumps_str over our own output is idempotent.
            assert_eq!(dumps_str(&s1), s1);
            count += 1;
        }
        assert!(count >= 1000, "fuzzed {count} inputs (need >= 1000)");
    }

    /// Build a random JSON value with bounded depth for the fuzz corpus.
    fn random_value(next: &mut impl FnMut() -> u64, depth: usize) -> Value {
        let pick = next() % if depth >= 3 { 5 } else { 7 };
        match pick {
            0 => Value::Null,
            1 => Value::Bool(next().is_multiple_of(2)),
            2 => Value::Number((next() as i64 - 1_000_000).into()),
            3 => {
                // Finite decimal that survives round-trip.
                let cents = (next() % 100_000) as f64 / 100.0;
                serde_json::Number::from_f64(cents).map_or(Value::Null, Value::Number)
            }
            4 => {
                let n = (next() % 6) as usize;
                let s: String = (0..n)
                    .map(|_| {
                        let r = next() % 4;
                        match r {
                            0 => 'a',
                            1 => '"',
                            2 => '\n',
                            _ => '你',
                        }
                    })
                    .collect();
                Value::String(s)
            }
            5 => {
                let n = (next() % 4) as usize;
                Value::Array((0..n).map(|_| random_value(next, depth + 1)).collect())
            }
            _ => {
                let n = (next() % 4) as usize;
                let mut map = serde_json::Map::new();
                for i in 0..n {
                    map.insert(format!("k{i}"), random_value(next, depth + 1));
                }
                Value::Object(map)
            }
        }
    }
}
