//! Cython AST shim — M6.
//!
//! Per ADR-0010 §2: a **lexical** parser for Cython sources, not a full
//! Cython front-end. We recognise the `cdef` / `cpdef` / `def`
//! constructs the M6 corpus uses (msgpack `_packer.pyx`,
//! `_unpacker.pyx`) and emit a `CythonSource` summary the translator's
//! prompt builder uses to format the Cython prompt.
//!
//! This shim is **not** the full M6 emit pipeline — it only parses;
//! the Rust emission goes through the same synthetic-LLM provider as
//! the pure-Python translation, just keyed by `task = translate_cython`
//! instead of `task = translate`.
//!
//! M7+ may replace this shim with a real Cython parser if that becomes
//! the bottleneck. For now, the pattern set is: the .pyx file is read,
//! `parse(...)` extracts the type-annotation surface, the prompt
//! builder embeds the surface into the prompt body so the canned
//! provider has enough context to route.

use std::fmt;

/// Tokenised view of a Cython source. Carries enough metadata for the
/// translator's prompt builder to emit Rust signatures with the right
/// type mappings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CythonSource {
    pub functions: Vec<CythonFunction>,
    pub imports: Vec<String>,
}

/// One `def` / `cdef` / `cpdef` definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CythonFunction {
    pub name: String,
    pub kind: CythonFunctionKind,
    /// Inline marker (`cdef inline ...`); `#[inline]` in Rust.
    pub inline: bool,
    pub params: Vec<CythonParam>,
    pub return_type: Option<CythonType>,
}

/// Cython function kind — `cdef` (C-only), `cpdef` (Python-callable
/// + C-callable), `def` (Python-only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CythonFunctionKind {
    Cdef,
    Cpdef,
    Def,
}

/// One typed parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CythonParam {
    pub name: String,
    pub ty: Option<CythonType>,
}

/// Cython types we recognise. Anything else maps to `Custom(...)`,
/// which the translator emits as `serde_json::Value` (the M6 dynamic-
/// payload escape) — see [`CythonType::to_rust`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CythonType {
    Int,
    Long,
    UnsignedInt,
    UnsignedLong,
    PySsizeT,
    Bint,
    Float,
    Double,
    Str,
    Bytes,
    Unicode,
    Object,
    List,
    Dict,
    /// Tuple — Rust counterpart picked at emission time.
    Tuple,
    Custom(String),
}

impl CythonType {
    /// Map a Cython type to the Rust type the translator emits. Per
    /// ADR-0010 §2's mapping table.
    #[must_use]
    pub fn to_rust(&self) -> &str {
        match self {
            CythonType::Int | CythonType::Long | CythonType::PySsizeT => "i64",
            CythonType::UnsignedInt | CythonType::UnsignedLong => "u64",
            CythonType::Bint => "bool",
            CythonType::Float | CythonType::Double => "f64",
            CythonType::Str | CythonType::Unicode => "&str",
            CythonType::Bytes => "&[u8]",
            CythonType::List => "Vec<serde_json::Value>",
            CythonType::Dict => "serde_json::Map<String, serde_json::Value>",
            CythonType::Tuple => "(serde_json::Value, serde_json::Value)",
            CythonType::Object | CythonType::Custom(_) => "serde_json::Value",
        }
    }

    /// Resolve a Cython token to its [`CythonType`] equivalent.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "int" => CythonType::Int,
            "long" => CythonType::Long,
            "unsigned int" => CythonType::UnsignedInt,
            "unsigned long" => CythonType::UnsignedLong,
            "Py_ssize_t" => CythonType::PySsizeT,
            "bint" => CythonType::Bint,
            "float" => CythonType::Float,
            "double" => CythonType::Double,
            "str" => CythonType::Str,
            "bytes" => CythonType::Bytes,
            "unicode" => CythonType::Unicode,
            "object" => CythonType::Object,
            "list" => CythonType::List,
            "dict" => CythonType::Dict,
            "tuple" => CythonType::Tuple,
            other => CythonType::Custom(other.into()),
        }
    }
}

/// Errors the lexical shim raises.
#[derive(Debug)]
pub enum ShimError {
    /// The source contains a construct outside the M6 lexical
    /// recognition set (e.g. fused types, memoryviews). The string
    /// echoes the offending line for the curator.
    UnsupportedConstruct(String),
    /// The source could not be tokenised (mismatched parens,
    /// truncated def line, etc.).
    Malformed(String),
}

impl fmt::Display for ShimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShimError::UnsupportedConstruct(line) => {
                write!(f, "cython lexical shim: unsupported construct: {line}")
            }
            ShimError::Malformed(reason) => {
                write!(f, "cython lexical shim: malformed source: {reason}")
            }
        }
    }
}

impl std::error::Error for ShimError {}

/// Parse a Cython source via the M6 lexical shim. Whitespace-tolerant;
/// recognises `cdef`, `cpdef`, `def` function-definition lines and the
/// type-annotation subset documented in [`CythonType::from_token`].
///
/// Per ADR-0010 §2: this is **not** a full Cython parser. It only
/// extracts function signatures + their type annotations; bodies are
/// passed verbatim to the LLM via the `translate_cython` prompt task.
///
/// # Errors
/// `ShimError::Malformed` if a `def` line cannot be parsed;
/// `ShimError::UnsupportedConstruct` if we hit a Cython idiom outside
/// the M6 set (e.g. fused types).
pub fn parse(source: &str) -> Result<CythonSource, ShimError> {
    let mut imports: Vec<String> = Vec::new();
    let mut functions: Vec<CythonFunction> = Vec::new();
    for raw_line in source.lines() {
        let line = raw_line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("cimport ") || line.starts_with("from ") {
            imports.push(line.to_string());
            continue;
        }
        if line.starts_with("ctypedef ") {
            return Err(ShimError::UnsupportedConstruct(line.to_string()));
        }
        if let Some(fdef) = try_parse_function(line)? {
            functions.push(fdef);
        }
    }
    Ok(CythonSource { functions, imports })
}

fn try_parse_function(line: &str) -> Result<Option<CythonFunction>, ShimError> {
    // Recognise:
    //   `cdef <ret>? <name>(<params>)`
    //   `cpdef <ret>? <name>(<params>)`
    //   `cdef inline <ret>? <name>(<params>)`
    //   `def <name>(<params>)`
    //
    // Reject `cdef <type> <name> = <expr>` (variable decl) and
    // `cdef <name>` (variable decl with no value): those have no `(`
    // before any `=` or end-of-line.
    let mut rest = line;
    let kind: CythonFunctionKind;
    let mut inline = false;
    if let Some(stripped) = rest.strip_prefix("cdef inline ") {
        kind = CythonFunctionKind::Cdef;
        inline = true;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix("cdef ") {
        kind = CythonFunctionKind::Cdef;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix("cpdef ") {
        kind = CythonFunctionKind::Cpdef;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix("def ") {
        kind = CythonFunctionKind::Def;
        rest = stripped;
    } else {
        return Ok(None);
    }
    // If a `=` comes before any `(`, this is a variable decl, not a
    // function — skip silently.
    let pos_eq = rest.find('=');
    let pos_open = rest.find('(');
    match (pos_eq, pos_open) {
        (Some(e), Some(o)) if e < o => return Ok(None),
        (_, None) => return Ok(None), // bare `cdef <type> <name>` or `cdef <type> <name> = <expr>`
        _ => {}
    }
    // Remove trailing `:` and `:` annotations if present.
    let head = rest.trim_end_matches(':').trim_end();
    // The signature is everything up to (and including) the first
    // closing paren.
    let close = head
        .find(')')
        .ok_or_else(|| ShimError::Malformed(format!("missing ')' in: {head}")))?;
    let sig = &head[..=close];
    let open = sig
        .find('(')
        .ok_or_else(|| ShimError::Malformed(format!("missing '(' in: {sig}")))?;
    let pre = sig[..open].trim();
    let params_str = sig[open + 1..sig.len() - 1].trim();
    // pre is `[<ret>] <name>` — split on whitespace.
    let (ret_token, name): (Option<&str>, &str) = match pre.rsplit_once(' ') {
        Some((ret_str, name_str)) => (Some(ret_str.trim()), name_str.trim()),
        None => (None, pre.trim()),
    };
    let return_type = ret_token.map(CythonType::from_token);
    let params = parse_params(params_str)?;
    Ok(Some(CythonFunction {
        name: name.to_string(),
        kind,
        inline,
        params,
        return_type,
    }))
}

fn parse_params(params: &str) -> Result<Vec<CythonParam>, ShimError> {
    if params.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<CythonParam> = Vec::new();
    for raw in params.split(',') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        // A param is either `<name>` (untyped), `<type> <name>`, or
        // `<type> <name>=<default>`. Defaults are dropped at the shim
        // level — the LLM regenerates them.
        let no_default = part.split('=').next().unwrap_or(part).trim();
        let tokens: Vec<&str> = no_default.split_whitespace().collect();
        let (ty, name) = match tokens.len() {
            0 => return Err(ShimError::Malformed(format!("empty param: {part}"))),
            1 => (None, tokens[0]),
            2 => (Some(CythonType::from_token(tokens[0])), tokens[1]),
            _ => {
                // `unsigned int x` etc. — collapse any leading typequal.
                let last = tokens[tokens.len() - 1];
                let ty_token = tokens[..tokens.len() - 1].join(" ");
                (Some(CythonType::from_token(&ty_token)), last)
            }
        };
        out.push(CythonParam {
            name: name.to_string(),
            ty,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_cdef_with_typed_params() {
        let src = "cdef int pack_uint(unsigned long value, object out):";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.functions.len(), 1);
        let f = &parsed.functions[0];
        assert_eq!(f.name, "pack_uint");
        assert_eq!(f.kind, CythonFunctionKind::Cdef);
        assert!(!f.inline);
        assert_eq!(f.return_type, Some(CythonType::Int));
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "value");
        assert_eq!(f.params[0].ty, Some(CythonType::UnsignedLong));
        assert_eq!(f.params[1].name, "out");
        assert_eq!(f.params[1].ty, Some(CythonType::Object));
    }

    #[test]
    fn parses_cdef_inline() {
        let src = "cdef inline int pack_byte(object out, unsigned int value):";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.functions.len(), 1);
        let f = &parsed.functions[0];
        assert_eq!(f.name, "pack_byte");
        assert!(f.inline);
        assert_eq!(f.kind, CythonFunctionKind::Cdef);
    }

    #[test]
    fn parses_cpdef_returning_bytes() {
        let src = "cpdef bytes pack_obj(object value):";
        let parsed = parse(src).unwrap();
        let f = &parsed.functions[0];
        assert_eq!(f.kind, CythonFunctionKind::Cpdef);
        assert_eq!(f.return_type, Some(CythonType::Bytes));
    }

    #[test]
    fn parses_def_keyword() {
        let src = "def regular_python(name, value):";
        let parsed = parse(src).unwrap();
        let f = &parsed.functions[0];
        assert_eq!(f.kind, CythonFunctionKind::Def);
        assert_eq!(f.return_type, None);
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].ty, None);
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let src = r"
# top-level comment
   # indented comment

cdef int x():
        ";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.functions.len(), 1);
    }

    #[test]
    fn collects_imports() {
        let src = "cimport cython\nfrom .fallback import pack_int\ncdef int x():";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.imports.len(), 2);
        assert_eq!(parsed.functions.len(), 1);
    }

    #[test]
    fn unsupported_ctypedef_raises() {
        let src = "ctypedef fused floating: float, double";
        let err = parse(src).unwrap_err();
        match err {
            ShimError::UnsupportedConstruct(line) => assert!(line.contains("ctypedef")),
            ShimError::Malformed(_) => panic!("expected UnsupportedConstruct, got Malformed"),
        }
    }

    #[test]
    fn type_to_rust_handles_known_tokens() {
        assert_eq!(CythonType::Int.to_rust(), "i64");
        assert_eq!(CythonType::UnsignedLong.to_rust(), "u64");
        assert_eq!(CythonType::PySsizeT.to_rust(), "i64");
        assert_eq!(CythonType::Bint.to_rust(), "bool");
        assert_eq!(CythonType::Bytes.to_rust(), "&[u8]");
        assert_eq!(CythonType::Object.to_rust(), "serde_json::Value");
    }

    #[test]
    fn type_from_token_falls_back_to_custom() {
        match CythonType::from_token("FooBar") {
            CythonType::Custom(s) => assert_eq!(s, "FooBar"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[test]
    fn parses_msgpack_packer_subset() {
        // Drive the shim with a slice of the real corpus content.
        let src = r#"
cimport cython

cdef inline int pack_byte(bytes_out, unsigned int value):
    """Append one byte."""
    bytes_out.append(value & 0xff)


cdef int pack_uint_cython(bytes_out, unsigned long value):
    """Pack a non-negative integer."""
    return 0


cpdef bytes pack_obj_cython(object value):
    cdef object out = bytearray()
    return bytes(out)
"#;
        let parsed = parse(src).expect("should parse");
        assert_eq!(parsed.functions.len(), 3);
        let names: Vec<_> = parsed.functions.iter().map(|f| f.name.clone()).collect();
        assert_eq!(
            names,
            vec!["pack_byte", "pack_uint_cython", "pack_obj_cython"]
        );
        assert!(parsed.functions[0].inline);
    }
}
