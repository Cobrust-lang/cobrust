// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: tomli 2.0.1
// oracle: cpython 3.11 (module: tomllib)
// functions translated: 12
// see PROVENANCE.toml for the full manifest.

//! Translated parser body.
//!
//! Each emitted block carries its own per-function provenance comment.

// fn:loads provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:96c64a5cdc37efba43bbb3cf502729d8474bbeb0801c94678ac02464657cb919
// Module preamble emitted with the first function (loads). Subsequent
// entries emit only their own function definition.

use std::collections::BTreeMap;
use std::fmt;

/// Heterogeneous TOML value. Subset per M4 scope window
/// (see corpus/tomli/README.md).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Boolean.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// UTF-8 string.
    Str(String),
    /// Heterogeneous array.
    Array(Vec<Value>),
    /// Nested table.
    Table(BTreeMap<String, Value>),
}

/// Single error type for tomli parse failures.
#[derive(Clone, Debug)]
pub struct TomliError {
    /// Human-readable message.
    pub message: String,
    /// Byte offset of the error in the source.
    pub pos: usize,
}

impl fmt::Display for TomliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tomli error at byte {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for TomliError {}

impl TomliError {
    fn new(message: impl Into<String>, pos: usize) -> Self {
        Self {
            message: message.into(),
            pos,
        }
    }
}

/// Cursor over the input source; helpers advance `pos` and may
/// raise `TomliError`.
struct State<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> State<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    fn expect(&mut self, ch: u8) -> Result<(), TomliError> {
        if self.peek() == Some(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(TomliError::new(
                format!("expected {:?}", char::from(ch)),
                self.pos,
            ))
        }
    }
}

/// Walk into the table at `path`, creating intermediate tables.
fn ensure_path<'a>(
    root: &'a mut BTreeMap<String, Value>,
    path: &[String],
) -> Result<&'a mut BTreeMap<String, Value>, TomliError> {
    let mut cursor: &'a mut BTreeMap<String, Value> = root;
    for part in path {
        let entry = cursor
            .entry(part.clone())
            .or_insert_with(|| Value::Table(BTreeMap::new()));
        cursor = match entry {
            Value::Table(t) => t,
            _ => {
                return Err(TomliError::new(
                    format!("path conflicts with non-table at {part:?}"),
                    0,
                ));
            }
        };
    }
    Ok(cursor)
}

/// Parse a TOML string into a dict.
///
/// Subset semantics: see `corpus/tomli/README.md` for the M4 scope
/// window. CPython `tomllib` is the oracle for inputs in scope.
///
/// # Errors
/// Returns [`TomliError`] for any malformed input. The differential
/// gate ensures CPython `tomllib` rejects the same inputs.
pub fn loads(src: &str) -> Result<BTreeMap<String, Value>, TomliError> {
    let mut state = State::new(src);
    let mut root: BTreeMap<String, Value> = BTreeMap::new();
    let mut current_path: Vec<String> = Vec::new();
    loop {
        skip_whitespace(&mut state);
        if state.eof() {
            return Ok(root);
        }
        if state.peek() == Some(b'[') {
            current_path = parse_table_header(&mut state)?;
            ensure_path(&mut root, &current_path)?;
            continue;
        }
        let cursor = ensure_path(&mut root, &current_path)?;
        parse_kv(&mut state, cursor)?;
    }
}

/// Convert a parsed value to its serde_json representation. Used by
/// the L3 differential gate to compare against CPython's
/// `tomllib.loads()` output.
#[must_use]
pub fn to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(to_json).collect()),
        Value::Table(t) => {
            let mut m = serde_json::Map::new();
            for (k, v) in t {
                m.insert(k.clone(), to_json(v));
            }
            serde_json::Value::Object(m)
        }
    }
}

/// Convert a top-level table to a JSON object.
#[must_use]
pub fn table_to_json(t: &BTreeMap<String, Value>) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (k, v) in t {
        m.insert(k.clone(), to_json(v));
    }
    serde_json::Value::Object(m)
}

// fn:parse_array provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:f0c509acbdd8ffe05acde0e0aa642af1fb7fc0bd78b3507ddc055b61d783ae62
fn parse_array(state: &mut State<'_>) -> Result<Vec<Value>, TomliError> {
    state.expect(b'[')?;
    let mut out = Vec::new();
    skip_whitespace(state);
    if state.peek() == Some(b']') {
        state.pos += 1;
        return Ok(out);
    }
    loop {
        out.push(parse_value(state)?);
        skip_whitespace(state);
        match state.peek() {
            Some(b',') => {
                state.pos += 1;
                skip_whitespace(state);
                if state.peek() == Some(b']') {
                    state.pos += 1;
                    return Ok(out);
                }
            }
            Some(b']') => {
                state.pos += 1;
                return Ok(out);
            }
            _ => return Err(TomliError::new("expected , or ]", state.pos)),
        }
    }
}

// fn:parse_basic_string provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:bd12f0128199934e3b2d1cc22e60cbf020d194fd447c12b3d80718287bc0d5e0
fn parse_basic_string(state: &mut State<'_>) -> Result<String, TomliError> {
    state.expect(b'"')?;
    let mut out = String::new();
    while let Some(b) = state.advance() {
        if b == b'"' {
            return Ok(out);
        }
        if b == b'\\' {
            let esc = state
                .advance()
                .ok_or_else(|| TomliError::new("unterminated escape", state.pos))?;
            match esc {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'\\' => out.push('\\'),
                b'"' => out.push('"'),
                _ => {
                    return Err(TomliError::new(
                        format!("bad escape \\{}", char::from(esc)),
                        state.pos,
                    ));
                }
            }
        } else {
            out.push(char::from(b));
        }
    }
    Err(TomliError::new("unterminated string", state.pos))
}

// fn:parse_bool provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:4d0809d2f147742edeceb487626528e94da4224179fdf268bbf2ff3dd44d0281
fn parse_bool(state: &mut State<'_>) -> Result<bool, TomliError> {
    let rem = &state.src[state.pos..];
    if rem.starts_with("true") {
        state.pos += 4;
        return Ok(true);
    }
    if rem.starts_with("false") {
        state.pos += 5;
        return Ok(false);
    }
    Err(TomliError::new("expected bool", state.pos))
}

// fn:parse_inline_table provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:b27909f9cdcaf601d5f457ded6f54dd4484b2043cb056736d727ac2fc5d5c977
fn parse_inline_table(state: &mut State<'_>) -> Result<BTreeMap<String, Value>, TomliError> {
    state.expect(b'{')?;
    let mut out = BTreeMap::new();
    skip_whitespace(state);
    if state.peek() == Some(b'}') {
        state.pos += 1;
        return Ok(out);
    }
    loop {
        let key = parse_key(state)?;
        skip_whitespace(state);
        state.expect(b'=')?;
        let value = parse_value(state)?;
        out.insert(key, value);
        skip_whitespace(state);
        match state.peek() {
            Some(b',') => {
                state.pos += 1;
                skip_whitespace(state);
                if state.peek() == Some(b'}') {
                    return Err(TomliError::new("trailing comma in inline table", state.pos));
                }
            }
            Some(b'}') => {
                state.pos += 1;
                return Ok(out);
            }
            _ => return Err(TomliError::new("expected , or }", state.pos)),
        }
    }
}

// fn:parse_int provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:dda8407939759210635067a329e1e3d37602d9fd4b39df75f7d5ea09c8c33f40
fn parse_int(state: &mut State<'_>) -> Result<i64, TomliError> {
    let start = state.pos;
    if matches!(state.peek(), Some(b'-' | b'+')) {
        state.pos += 1;
    }
    let digits_start = state.pos;
    while let Some(b) = state.peek() {
        if b.is_ascii_digit() {
            state.pos += 1;
        } else {
            break;
        }
    }
    if state.pos == digits_start {
        return Err(TomliError::new("expected digit", start));
    }
    let slice = &state.src[start..state.pos];
    slice
        .parse::<i64>()
        .map_err(|e| TomliError::new(format!("int parse: {e}"), start))
}

// fn:parse_key provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:a8fa7bb8d3f4f14f25227c564850d5d3a821e4c54500c8bca13e199546ffdcba
fn parse_key(state: &mut State<'_>) -> Result<String, TomliError> {
    skip_whitespace(state);
    let start = state.pos;
    while let Some(b) = state.peek() {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
            state.pos += 1;
        } else {
            break;
        }
    }
    if state.pos == start {
        return Err(TomliError::new("expected key", start));
    }
    Ok(state.src[start..state.pos].to_string())
}

// fn:parse_kv provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:824c928dbd9f0c98324214724e2c8cb0787973e40525b6317418b4ee5ab6fc6d
fn parse_kv(state: &mut State<'_>, dest: &mut BTreeMap<String, Value>) -> Result<(), TomliError> {
    let key = parse_key(state)?;
    skip_whitespace(state);
    state.expect(b'=')?;
    let value = parse_value(state)?;
    dest.insert(key, value);
    Ok(())
}

// fn:parse_literal_string provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:675b114b8bc925494aeffb3d8de59707bbe51a0dc8a404399d7b2559805cb161
fn parse_literal_string(state: &mut State<'_>) -> Result<String, TomliError> {
    state.expect(b'\'')?;
    let mut out = String::new();
    while let Some(b) = state.advance() {
        if b == b'\'' {
            return Ok(out);
        }
        out.push(char::from(b));
    }
    Err(TomliError::new("unterminated literal string", state.pos))
}

// fn:parse_table_header provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:57dd8ea52ef675c1ad610da0402ea19936df31673a9f3d15d68bcf176b6a7c63
fn parse_table_header(state: &mut State<'_>) -> Result<Vec<String>, TomliError> {
    state.expect(b'[')?;
    let mut parts = Vec::new();
    parts.push(parse_key(state)?);
    while state.peek() == Some(b'.') {
        state.pos += 1;
        parts.push(parse_key(state)?);
    }
    skip_whitespace(state);
    state.expect(b']')?;
    Ok(parts)
}

// fn:parse_value provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:82945a219503558962a7c60db666724dca632d6907795dc4cd5bf47c61b51dcc
fn parse_value(state: &mut State<'_>) -> Result<Value, TomliError> {
    skip_whitespace(state);
    let b = state
        .peek()
        .ok_or_else(|| TomliError::new("expected value", state.pos))?;
    match b {
        b'"' => Ok(Value::Str(parse_basic_string(state)?)),
        b'\'' => Ok(Value::Str(parse_literal_string(state)?)),
        b'[' => Ok(Value::Array(parse_array(state)?)),
        b'{' => Ok(Value::Table(parse_inline_table(state)?)),
        b't' | b'f' => Ok(Value::Bool(parse_bool(state)?)),
        b'-' | b'+' | b'0'..=b'9' => Ok(Value::Int(parse_int(state)?)),
        _ => Err(TomliError::new(
            format!("unexpected character {:?}", char::from(b)),
            state.pos,
        )),
    }
}

// fn:skip_whitespace provider=synthetic model=tomli-canned-v1 cache_hit=true decision_id=blake3:32b5a760aa948d8373599109b5247be268438fb2b8f8b84fae352bf85ba3199c
fn skip_whitespace(state: &mut State<'_>) {
    while let Some(b) = state.peek() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            state.pos += 1;
        } else if b == b'#' {
            while let Some(b2) = state.peek() {
                if b2 == b'\n' {
                    break;
                }
                state.pos += 1;
            }
        } else {
            return;
        }
    }
}
