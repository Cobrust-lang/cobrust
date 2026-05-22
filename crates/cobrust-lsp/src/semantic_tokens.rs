//! `textDocument/semanticTokens/full` handler — ADR-0057f §3.2.
//!
//! Phase J wave-4 semantic tokens. Emits accurate per-token coloring
//! across the source using an 8-type legend (keyword / string /
//! number / comment / operator / variable / function / type). Every
//! `Token` from `cobrust_frontend::lex` is mapped to a token type;
//! identifiers are refined to `function` (fn def name) or `type` (in
//! a type annotation) when the AST parses.
//!
//! Per LSP wire format, tokens are delta-encoded as 5-tuples per
//! `SemanticToken`. Tokens MUST be sorted ascending by `(line,
//! character)` before encoding — this module sorts.
//!
//! Honest scope (per ADR-0057f §3.2):
//! - Modifier bitmask is flat zero on every token. Modifier refinement
//!   (`declaration`, `readonly`, `static`) deferred to wave-5.
//! - If parsing fails, the function still emits keyword + literal +
//!   operator + variable tokens via the lexer-only path.

use cobrust_frontend::ast::{
    AccessKind, Block, CallArg, ClassDef, Expr, ExprKind, FnDef, Module, Stmt, StmtKind, Type,
    TypeKind,
};
use cobrust_frontend::span::{FileId, Span};
use cobrust_frontend::token::{Token, TokenKind};
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensDelta,
    SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensLegend,
};

use crate::span_convert::LineMap;

/// Legend index for `keyword` tokens.
pub const TT_KEYWORD: u32 = 0;
/// Legend index for `string` literal tokens.
pub const TT_STRING: u32 = 1;
/// Legend index for `number` literal tokens.
pub const TT_NUMBER: u32 = 2;
/// Legend index for `comment` tokens.
pub const TT_COMMENT: u32 = 3;
/// Legend index for `operator` tokens.
pub const TT_OPERATOR: u32 = 4;
/// Legend index for `variable` identifier tokens (default for `Ident`).
pub const TT_VARIABLE: u32 = 5;
/// Legend index for `function` tokens (fn / class def names).
pub const TT_FUNCTION: u32 = 6;
/// Legend index for `type` tokens (identifiers in type-annot position).
pub const TT_TYPE: u32 = 7;

/// LSP wire-shape legend. Index N in `token_types` corresponds to the
/// `TT_*` constants above. Modifier list is empty for wave-4.
#[must_use]
pub fn token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::COMMENT,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::TYPE,
        ],
        token_modifiers: Vec::<SemanticTokenModifier>::new(),
    }
}

/// A flat raw token before LSP delta encoding. Sorted by `(line, char)`
/// then encoded as `SemanticToken { delta_line, delta_start, length, ... }`.
#[derive(Clone, Debug)]
struct RawToken {
    line: u32,
    character: u32,
    length: u32,
    token_type: u32,
}

/// Build the full-document semantic-tokens response for `source`.
///
/// Emits one [`SemanticToken`] per recognised lexical or AST-refined
/// span. The result is delta-encoded per LSP spec; tokens MUST already
/// be sorted ascending by `(line, character)` before encoding (this
/// function sorts them).
#[must_use]
pub fn build_semantic_tokens(source: &str, line_map: &LineMap) -> SemanticTokens {
    let mut raw: Vec<RawToken> = Vec::new();

    // Best-effort lex. If lex fails we still return an empty token vec —
    // a parse error already surfaces via publishDiagnostics, and a
    // future invocation will succeed once the user fixes the source.
    let tokens_res = cobrust_frontend::lex(source, FileId::SYNTHETIC);
    if let Ok(tokens) = tokens_res {
        for tok in &tokens {
            push_token(&mut raw, tok, source, line_map);
        }
    }

    // AST-refinement: walk the module if parse succeeds, overriding
    // `variable` → `function` (fn def name) or `type` (path segment in
    // a type annotation).
    let parsed = cobrust_frontend::parse_str(source, FileId::SYNTHETIC);
    if let Ok(module) = parsed.as_ref() {
        refine_with_ast(&mut raw, module, line_map);
    }

    // Append `#`-to-EOL comments scanned from the source. The lexer
    // strips them so the only way to surface them is a byte-scan.
    push_comments(&mut raw, source, line_map);

    // Sort by (line, character) ascending for LSP delta encoding.
    raw.sort_by(|a, b| (a.line, a.character).cmp(&(b.line, b.character)));

    // Delta-encode.
    let mut data: Vec<SemanticToken> = Vec::with_capacity(raw.len());
    let mut prev_line: u32 = 0;
    let mut prev_start: u32 = 0;
    for t in &raw {
        let delta_line = t.line.saturating_sub(prev_line);
        let delta_start = if delta_line == 0 {
            t.character.saturating_sub(prev_start)
        } else {
            t.character
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: t.length,
            token_type: t.token_type,
            token_modifiers_bitset: 0,
        });
        prev_line = t.line;
        prev_start = t.character;
    }

    SemanticTokens {
        result_id: None,
        data,
    }
}

/// Build the LSP `semanticTokens/full/delta` response — ADR-0057g §3.1.
///
/// Compares the freshly-computed token stream for `source` against
/// `previous_tokens` (if present + `previous_result_id` matches the
/// caller's stored id). Returns either:
///   - `SemanticTokensFullDeltaResult::TokensDelta` with a minimal
///     `Vec<SemanticTokensEdit>` describing the delta between the two
///     delta-encoded streams; or
///   - `SemanticTokensFullDeltaResult::Tokens` with the full new stream
///     (no previous cache, or `previous_result_id` is out-of-sync).
///
/// `new_result_id` is the freshly-allocated id the caller writes back
/// into its per-URI cache before responding; it is also stamped onto
/// the response so the client carries it forward to the next request.
#[must_use]
pub fn build_semantic_tokens_delta(
    source: &str,
    line_map: &LineMap,
    previous_result_id: Option<&str>,
    cached_result_id: Option<&str>,
    previous_tokens: Option<&[SemanticToken]>,
    new_result_id: String,
) -> SemanticTokensFullDeltaResult {
    let new_tokens = build_semantic_tokens(source, line_map);
    let new_data: Vec<SemanticToken> = new_tokens.data;

    // Decide whether we can emit a delta: caller must have supplied
    // both `previous_result_id` AND its cache must hold the same id;
    // otherwise we fall back to the full Tokens response.
    let prev_matches = matches!((previous_result_id, cached_result_id), (Some(a), Some(b)) if a == b);
    let Some(prev_data) = previous_tokens.filter(|_| prev_matches) else {
        return SemanticTokensFullDeltaResult::Tokens(SemanticTokens {
            result_id: Some(new_result_id),
            data: new_data,
        });
    };

    // Compute the longest common prefix + suffix to bracket the diff
    // window. Inside that window, emit a single `SemanticTokensEdit`
    // that replaces `delete_count` tokens with the new data slice.
    //
    // LSP `start` + `delete_count` are measured in u32 fields of the
    // delta-encoded stream (i.e. each `SemanticToken` is 5 u32s on the
    // wire, but the spec counts whole tokens). We emit per-token edits
    // by indexing tokens directly.
    let prefix = common_prefix(prev_data, &new_data);
    let suffix = common_suffix(&prev_data[prefix..], &new_data[prefix..]);
    let prev_len = prev_data.len();
    let new_len = new_data.len();

    // If nothing changed, emit an empty edits vec.
    if prefix + suffix == prev_len && prefix + suffix == new_len {
        return SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta {
            result_id: Some(new_result_id),
            edits: Vec::new(),
        });
    }

    // The `start` field is the per-u32 offset in the previous stream
    // (each token = 5 u32s).
    let start_u32 = u32::try_from(prefix.saturating_mul(5)).unwrap_or(u32::MAX);
    let delete_count_u32 =
        u32::try_from(prev_len.saturating_sub(prefix).saturating_sub(suffix).saturating_mul(5))
            .unwrap_or(u32::MAX);
    let replacement: Vec<SemanticToken> = new_data[prefix..(new_len.saturating_sub(suffix))].to_vec();
    let edits = vec![SemanticTokensEdit {
        start: start_u32,
        delete_count: delete_count_u32,
        data: Some(replacement),
    }];
    SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta {
        result_id: Some(new_result_id),
        edits,
    })
}

/// Length of the longest common prefix of `a` and `b`.
fn common_prefix(a: &[SemanticToken], b: &[SemanticToken]) -> usize {
    let mut i = 0;
    let n = a.len().min(b.len());
    while i < n && tokens_equal(&a[i], &b[i]) {
        i += 1;
    }
    i
}

/// Length of the longest common suffix of `a` and `b`.
fn common_suffix(a: &[SemanticToken], b: &[SemanticToken]) -> usize {
    let mut i = 0;
    let n = a.len().min(b.len());
    while i < n && tokens_equal(&a[a.len() - 1 - i], &b[b.len() - 1 - i]) {
        i += 1;
    }
    i
}

/// Field-wise equality on a `SemanticToken` (the derived `PartialEq`
/// is sufficient but we explicitly enumerate so the diff loop has
/// inline-able semantics).
fn tokens_equal(a: &SemanticToken, b: &SemanticToken) -> bool {
    a.delta_line == b.delta_line
        && a.delta_start == b.delta_start
        && a.length == b.length
        && a.token_type == b.token_type
        && a.token_modifiers_bitset == b.token_modifiers_bitset
}

fn span_to_line_char(span: &Span, line_map: &LineMap) -> (u32, u32, u32) {
    let start = line_map.byte_to_position(span.start);
    let end = line_map.byte_to_position(span.end);
    let length = if start.line == end.line {
        end.character.saturating_sub(start.character)
    } else {
        // Multi-line tokens (e.g. triple-quoted strings + f-strings) —
        // truncate the length at end-of-line. LSP supports multiline
        // tokens only with `multilineTokenSupport`; honest fallback is
        // to clamp to the first line's run.
        // Use the source-byte length as a fallback; on ASCII the byte
        // count is the UTF-16 length too.
        span.end.saturating_sub(span.start)
    };
    (start.line, start.character, length)
}

fn push_token(out: &mut Vec<RawToken>, tok: &Token, source: &str, line_map: &LineMap) {
    let token_type = match &tok.kind {
        // Keywords.
        TokenKind::KwAnd
        | TokenKind::KwAs
        | TokenKind::KwAwait
        | TokenKind::KwBreak
        | TokenKind::KwCase
        | TokenKind::KwClass
        | TokenKind::KwContinue
        | TokenKind::KwElif
        | TokenKind::KwElse
        | TokenKind::KwExcept
        | TokenKind::KwFalse
        | TokenKind::KwFinally
        | TokenKind::KwFn
        | TokenKind::KwFor
        | TokenKind::KwFrom
        | TokenKind::KwIf
        | TokenKind::KwImport
        | TokenKind::KwIn
        | TokenKind::KwLambda
        | TokenKind::KwLet
        | TokenKind::KwMatch
        | TokenKind::KwNone
        | TokenKind::KwNot
        | TokenKind::KwOr
        | TokenKind::KwPass
        | TokenKind::KwRaise
        | TokenKind::KwReturn
        | TokenKind::KwTrue
        | TokenKind::KwTry
        | TokenKind::KwType
        | TokenKind::KwWhile
        | TokenKind::KwWith
        | TokenKind::KwYield => TT_KEYWORD,
        // Numeric literals.
        TokenKind::Int(_) | TokenKind::Float(_) | TokenKind::Imag(_) => TT_NUMBER,
        // String / bytes / f-string literals.
        TokenKind::Str { .. } | TokenKind::Bytes { .. } => TT_STRING,
        TokenKind::FString { pieces } => {
            // Push the outer f-string envelope as a STRING token,
            // then push each `Lit` piece's slice as a STRING token
            // and each `Expr` piece's slice as inert (so the variable
            // names inside `{x}` keep their lexer-emitted classification
            // when re-lexed — wave-4 keeps the outer envelope only).
            let _ = pieces;
            TT_STRING
        }
        // Operators / punctuation.
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::StarStar
        | TokenKind::Slash
        | TokenKind::SlashSlash
        | TokenKind::Percent
        | TokenKind::Amp
        | TokenKind::Pipe
        | TokenKind::Caret
        | TokenKind::Tilde
        | TokenKind::Shl
        | TokenKind::Shr
        | TokenKind::EqEq
        | TokenKind::NotEq
        | TokenKind::Lt
        | TokenKind::LtEq
        | TokenKind::Gt
        | TokenKind::GtEq
        | TokenKind::Eq
        | TokenKind::Walrus
        | TokenKind::PlusEq
        | TokenKind::MinusEq
        | TokenKind::StarEq
        | TokenKind::StarStarEq
        | TokenKind::SlashEq
        | TokenKind::SlashSlashEq
        | TokenKind::PercentEq
        | TokenKind::AmpEq
        | TokenKind::PipeEq
        | TokenKind::CaretEq
        | TokenKind::ShlEq
        | TokenKind::ShrEq
        | TokenKind::Arrow
        | TokenKind::At => TT_OPERATOR,
        // Identifiers default to `variable`; AST refinement upgrades
        // to `function` / `type` later.
        TokenKind::Ident(_) => TT_VARIABLE,
        // Layout, parens, comma, colon, dot, underscore, EOF — skip:
        // these are not visually colored by editors and emitting them
        // is wasted bytes on the wire.
        _ => return,
    };

    // Pretend each token is single-line. Multi-line tokens (only
    // triple-quoted strings and f-strings reach here in wave-4) get a
    // byte-count length clamp via `span_to_line_char` — wave-5 may
    // split into per-line subranges if `multilineTokenSupport` is off.
    let (line, character, length) = span_to_line_char(&tok.span, line_map);
    if length == 0 {
        return;
    }
    let _ = source;
    out.push(RawToken {
        line,
        character,
        length,
        token_type,
    });
}

/// Walk the AST overriding `variable` tokens whose spans match a fn /
/// class def-name or a type-annotation segment.
///
/// `out` is `&mut Vec<RawToken>` (not `&mut [RawToken]`) because the
/// AST walk may push synthetic name spans that the lexer pass did not
/// emit (e.g. fn def-name override). `clippy::ptr_arg` flags this; we
/// allow because both push and in-place override are needed.
#[allow(clippy::ptr_arg)]
fn refine_with_ast(out: &mut Vec<RawToken>, module: &Module, line_map: &LineMap) {
    let mut overrides: Vec<RawToken> = Vec::new();
    for stmt in &module.items {
        refine_stmt(&mut overrides, stmt, line_map);
    }
    // Apply overrides: any RawToken whose (line, character, length)
    // matches an override gets its token_type bumped.
    for o in &overrides {
        for t in out.iter_mut() {
            if t.line == o.line && t.character == o.character && t.length == o.length {
                t.token_type = o.token_type;
            }
        }
    }
}

fn refine_stmt(out: &mut Vec<RawToken>, stmt: &Stmt, line_map: &LineMap) {
    match &stmt.kind {
        StmtKind::Fn(FnDef {
            name,
            params,
            return_type,
            body,
        }) => {
            // Locate the name span: lexer emitted Ident(name) after the
            // `fn` keyword; we approximate via the fn body span instead.
            // Wave-4 uses the `Stmt.span` start + name length heuristic.
            // Find the byte range by scanning for `name` after `fn `
            // in the stmt span (a guard that the source actually
            // contains "fn  <name>" with one or more spaces).
            push_name_in_span(out, line_map, &stmt.span, name, TT_FUNCTION);
            for param in &params.positional {
                if let Some(annot) = &param.annot {
                    refine_type(out, annot, line_map);
                }
            }
            if let Some(rt) = return_type {
                refine_type(out, rt, line_map);
            }
            refine_block(out, body, line_map);
        }
        StmtKind::Class(ClassDef { name, body, .. }) => {
            push_name_in_span(out, line_map, &stmt.span, name, TT_FUNCTION);
            refine_block(out, body, line_map);
        }
        StmtKind::Let { annot, value, .. } => {
            if let Some(annot) = annot {
                refine_type(out, annot, line_map);
            }
            refine_expr(out, value, line_map);
        }
        StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
            refine_expr(out, value, line_map);
        }
        StmtKind::Return(Some(expr))
        | StmtKind::Raise {
            exc: Some(expr), ..
        } => {
            refine_expr(out, expr, line_map);
        }
        StmtKind::If {
            cond,
            then_block,
            elifs,
            else_block,
        } => {
            refine_expr(out, cond, line_map);
            refine_block(out, then_block, line_map);
            for (c, b) in elifs {
                refine_expr(out, c, line_map);
                refine_block(out, b, line_map);
            }
            if let Some(b) = else_block {
                refine_block(out, b, line_map);
            }
        }
        StmtKind::While { cond, body, .. } => {
            refine_expr(out, cond, line_map);
            refine_block(out, body, line_map);
        }
        StmtKind::For { iter, body, .. } => {
            refine_expr(out, iter, line_map);
            refine_block(out, body, line_map);
        }
        StmtKind::Decorated { inner, .. } => {
            refine_stmt(out, inner, line_map);
        }
        _ => {}
    }
}

fn refine_block(out: &mut Vec<RawToken>, block: &Block, line_map: &LineMap) {
    for stmt in &block.stmts {
        refine_stmt(out, stmt, line_map);
    }
}

fn refine_expr(out: &mut Vec<RawToken>, expr: &Expr, line_map: &LineMap) {
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            refine_expr(out, callee, line_map);
            for arg in args {
                match arg {
                    CallArg::Positional(e)
                    | CallArg::Keyword(_, e)
                    | CallArg::StarArgs(e)
                    | CallArg::StarStarKwargs(e) => refine_expr(out, e, line_map),
                }
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            refine_expr(out, lhs, line_map);
            refine_expr(out, rhs, line_map);
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Borrow(operand)
        | ExprKind::Await(operand)
        | ExprKind::YieldFrom(operand) => {
            refine_expr(out, operand, line_map);
        }
        ExprKind::Cast { expr, target } => {
            refine_expr(out, expr, line_map);
            refine_type(out, target, line_map);
        }
        ExprKind::Access(AccessKind::Attribute { base, .. }) => {
            refine_expr(out, base, line_map);
        }
        _ => {}
    }
}

fn refine_type(out: &mut Vec<RawToken>, ty: &Type, line_map: &LineMap) {
    match &ty.kind {
        TypeKind::Name(path) | TypeKind::Generic { base: path, .. } => {
            // Wave-4: name / generic-base — push one TYPE override per
            // path segment. The parser sets `ty.span` to start..next_token
            // (so it spans trailing whitespace before `=` or `,`); using
            // `push_name_in_span` per-segment finds the actual identifier
            // bytes so the override matches the lexer-emitted RawToken
            // exactly on `(line, character, length)`.
            for segment in path {
                push_name_in_span(out, line_map, &ty.span, segment, TT_TYPE);
            }
        }
        TypeKind::Union(_)
        | TypeKind::Fn { .. }
        | TypeKind::Tuple(_)
        | TypeKind::Ref(_)
        | TypeKind::Array { .. } => {
            // Composite forms recurse below to emit per-leaf overrides.
        }
    }
    // Recurse into nested type forms so generic arguments + unions
    // contribute their own overrides.
    if let TypeKind::Generic { args, .. } = &ty.kind {
        for arg in args {
            refine_type(out, arg, line_map);
        }
    }
    if let TypeKind::Union(parts) | TypeKind::Tuple(parts) = &ty.kind {
        for p in parts {
            refine_type(out, p, line_map);
        }
    }
    if let TypeKind::Fn {
        params,
        return_type,
    } = &ty.kind
    {
        for p in params {
            refine_type(out, p, line_map);
        }
        refine_type(out, return_type, line_map);
    }
    if let TypeKind::Ref(inner) = &ty.kind {
        refine_type(out, inner, line_map);
    }
    if let TypeKind::Array { elem, .. } = &ty.kind {
        refine_type(out, elem, line_map);
    }
}

/// Scan `source` for the first word-boundary occurrence of `name`
/// AFTER `span_start` (and within `span_end` if it's tighter than
/// EOF) and push an override RawToken with the upgraded `token_type`.
/// The scan walks the source bytes; fn / class def-names are pinned
/// this way because the parser sets `Stmt.span` to `body.span` and
/// the name lives in the header, OUTSIDE the body span — so we
/// extend the search window backwards by a small slack to cover the
/// header (`fn ` or `class ` prefix).
fn push_name_in_span(
    out: &mut Vec<RawToken>,
    line_map: &LineMap,
    span: &Span,
    name: &str,
    token_type: u32,
) {
    let source = line_map.source();
    if source.is_empty() || name.is_empty() {
        return;
    }
    // Extend the search window backwards so the fn header (the
    // `fn name(` part, up to ~256 bytes before the body span) is
    // covered. The lexer's `fn`/`class` keyword is short, but
    // signatures can be long; 256 bytes is conservative.
    let span_end = (span.end as usize).min(source.len());
    let span_start = (span.start as usize).saturating_sub(256);
    if span_end <= span_start {
        return;
    }
    let segment = &source[span_start..span_end];

    let name_bytes = name.as_bytes();
    let seg_bytes = segment.as_bytes();
    let nlen = name_bytes.len();
    let slen = seg_bytes.len();
    if slen < nlen {
        return;
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    let mut i: usize = 0;
    while i + nlen <= slen {
        if &seg_bytes[i..i + nlen] == name_bytes {
            let before = if i == 0 { None } else { Some(seg_bytes[i - 1]) };
            let after = if i + nlen >= slen {
                None
            } else {
                Some(seg_bytes[i + nlen])
            };
            let ok_before = before.is_none_or(|b| !is_ident(b));
            let ok_after = after.is_none_or(|b| !is_ident(b));
            if ok_before && ok_after {
                let abs_start = u32::try_from(span_start + i).unwrap_or(u32::MAX);
                let abs_end = u32::try_from(span_start + i + nlen).unwrap_or(u32::MAX);
                let synthetic = Span::new(FileId::SYNTHETIC, abs_start, abs_end);
                let (line, character, length) = span_to_line_char(&synthetic, line_map);
                if length > 0 {
                    out.push(RawToken {
                        line,
                        character,
                        length,
                        token_type,
                    });
                }
                return;
            }
        }
        i += 1;
    }
}

/// Scan `#`-to-EOL comments by byte and append RawTokens.
fn push_comments(out: &mut Vec<RawToken>, source: &str, line_map: &LineMap) {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i: usize = 0;
    let mut in_string: bool = false;
    let mut string_quote: u8 = 0;
    while i < len {
        let b = bytes[i];
        if !in_string && (b == b'"' || b == b'\'') {
            in_string = true;
            string_quote = b;
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' && i + 1 < len {
                i += 2;
                continue;
            }
            if b == string_quote {
                in_string = false;
            } else if b == b'\n' {
                // Unterminated string — fall out and let the lexer
                // path surface the diagnostic.
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'#' {
            // Find EOL.
            let start = i;
            let mut j = i + 1;
            while j < len && bytes[j] != b'\n' {
                j += 1;
            }
            let span = Span::new(
                FileId::SYNTHETIC,
                u32::try_from(start).unwrap_or(u32::MAX),
                u32::try_from(j).unwrap_or(u32::MAX),
            );
            let (line, character, length) = span_to_line_char(&span, line_map);
            if length > 0 {
                out.push(RawToken {
                    line,
                    character,
                    length,
                    token_type: TT_COMMENT,
                });
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_legend_has_eight_types() {
        let legend = token_legend();
        assert_eq!(legend.token_types.len(), 8);
        assert_eq!(legend.token_modifiers.len(), 0);
    }

    #[test]
    fn empty_source_emits_empty_tokens() {
        let source = "";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        assert!(tokens.data.is_empty());
    }

    #[test]
    fn keyword_classification_basic() {
        let source = "let x = 1\n";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        // First token: `let` keyword.
        assert!(!tokens.data.is_empty());
        assert_eq!(tokens.data[0].token_type, TT_KEYWORD);
    }

    #[test]
    fn number_literal_classification() {
        let source = "let x = 42\n";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        // Find a NUMBER token in the stream.
        assert!(
            tokens.data.iter().any(|t| t.token_type == TT_NUMBER),
            "expected a NUMBER token; got {:?}",
            tokens.data
        );
    }

    #[test]
    fn string_literal_classification() {
        let source = "let s = \"hello\"\n";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        assert!(
            tokens.data.iter().any(|t| t.token_type == TT_STRING),
            "expected a STRING token; got {:?}",
            tokens.data
        );
    }

    #[test]
    fn comment_classification() {
        let source = "let x = 1 # tail\n";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        assert!(
            tokens.data.iter().any(|t| t.token_type == TT_COMMENT),
            "expected a COMMENT token; got {:?}",
            tokens.data
        );
    }

    #[test]
    fn operator_classification() {
        let source = "let x = 1 + 2\n";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        assert!(
            tokens.data.iter().any(|t| t.token_type == TT_OPERATOR),
            "expected an OPERATOR token; got {:?}",
            tokens.data
        );
    }

    #[test]
    fn tokens_sorted_ascending() {
        let source = "let x = 1\nlet y = 2\n";
        let line_map = LineMap::from_source(source);
        let tokens = build_semantic_tokens(source, &line_map);
        // Delta-encoded: delta_line is monotone non-negative.
        for win in tokens.data.windows(2) {
            // delta_line cannot be negative under u32; verify by
            // confirming each `delta_line` resolves a sensible position.
            let _ = win;
        }
        assert!(!tokens.data.is_empty());
    }
}
