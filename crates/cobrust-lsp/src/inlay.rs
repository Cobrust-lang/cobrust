//! `textDocument/inlayHint` handler — ADR-0057f §3.1.
//!
//! Phase J wave-4 inlay hints. Emits inline type annotations for
//! `let x = expr` bindings without explicit `: Type`, plus
//! parameter-name hints at non-literal call-argument sites.
//!
//! § 2.5 compile-time-catch surfaced as UX: the inferred type from
//! `TypeCheckCtx::lookup(name)` is already known after a successful
//! type-check; surfacing it inline lets the agent-LLM read the type
//! at the cursor without provoking an error first.
//!
//! Honest scope:
//! - `let` hints: single-binding patterns only (no tuple / sequence
//!   destructuring at wave-4).
//! - Param-name hints: require same-document fn def visibility for
//!   parameter-name extraction. Cross-file param-name resolution
//!   deferred to wave-5.
//! - The visible-range filter from `InlayHintParams.range` is
//!   honoured by intersecting each emit candidate's source span with
//!   the requested range.

use cobrust_frontend::ast::{
    self, Expr, ExprKind, FnDef, Module, Pattern, PatternKind, Stmt, StmtKind,
};
use cobrust_frontend::span::{FileId, Span};
use cobrust_types::TypeCheckCtx;
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};

use crate::span_convert::LineMap;

/// Build inlay hints for `source` constrained to the LSP `range`.
///
/// Returns one [`InlayHint`] per `let`-without-annot binding inside
/// the range (type hint) plus one per non-literal positional call-arg
/// where the callee's parameter name is resolvable in the same
/// document (parameter-name hint).
///
/// Per ADR-0057f §3.1, all hints carry an empty modifier set;
/// modifier refinement deferred to wave-5.
#[must_use]
pub fn build_inlay_hints(
    source: &str,
    line_map: &LineMap,
    range: Range,
    ctx: &TypeCheckCtx,
) -> Vec<InlayHint> {
    let Ok(module) = cobrust_frontend::parse_str(source, FileId::SYNTHETIC) else {
        // Wave-4 honest-scope: if the source fails to parse, the LSP
        // pipeline emits no hints. Diagnostics already surface the
        // parse error via the publishDiagnostics path.
        return Vec::new();
    };

    let param_index = collect_param_names(&module);
    let mut hints: Vec<InlayHint> = Vec::new();

    for stmt in &module.items {
        collect_hints_in_stmt(stmt, ctx, &param_index, line_map, &range, &mut hints);
    }

    hints
}

/// Same-document fn name → ordered positional parameter names.
type ParamIndex = std::collections::HashMap<String, Vec<String>>;

/// Walk module items and collect `fn name(params) -> ...` into a
/// `name → [param_names]` table. Used by the param-name hint pass to
/// look up the parameter at each call argument's positional slot.
fn collect_param_names(module: &Module) -> ParamIndex {
    let mut out: ParamIndex = ParamIndex::new();
    for stmt in &module.items {
        collect_param_names_in_stmt(stmt, &mut out);
    }
    out
}

fn collect_param_names_in_stmt(stmt: &Stmt, out: &mut ParamIndex) {
    match &stmt.kind {
        StmtKind::Fn(FnDef { name, params, .. }) => {
            let names: Vec<String> = params.positional.iter().map(|p| p.name.clone()).collect();
            out.insert(name.clone(), names);
        }
        StmtKind::Decorated { inner, .. } => {
            collect_param_names_in_stmt(inner, out);
        }
        _ => {}
    }
}

/// Recursively walk a statement collecting hint candidates.
fn collect_hints_in_stmt(
    stmt: &Stmt,
    ctx: &TypeCheckCtx,
    param_index: &ParamIndex,
    line_map: &LineMap,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    match &stmt.kind {
        StmtKind::Let {
            target,
            annot,
            value,
        } => {
            if annot.is_none()
                && let Some(hint) = let_type_hint(target, ctx, line_map, range)
            {
                hints.push(hint);
            }
            collect_hints_in_expr(value, ctx, param_index, line_map, range, hints);
        }
        StmtKind::Fn(FnDef { body, .. }) => {
            for inner in &body.stmts {
                collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
            }
        }
        StmtKind::If {
            cond,
            then_block,
            elifs,
            else_block,
        } => {
            collect_hints_in_expr(cond, ctx, param_index, line_map, range, hints);
            for inner in &then_block.stmts {
                collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
            }
            for (cond, block) in elifs {
                collect_hints_in_expr(cond, ctx, param_index, line_map, range, hints);
                for inner in &block.stmts {
                    collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
                }
            }
            if let Some(block) = else_block {
                for inner in &block.stmts {
                    collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
                }
            }
        }
        StmtKind::While { cond, body, .. } => {
            collect_hints_in_expr(cond, ctx, param_index, line_map, range, hints);
            for inner in &body.stmts {
                collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
            }
        }
        StmtKind::For { iter, body, .. } => {
            collect_hints_in_expr(iter, ctx, param_index, line_map, range, hints);
            for inner in &body.stmts {
                collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
            }
        }
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => {
            collect_hints_in_expr(expr, ctx, param_index, line_map, range, hints);
        }
        StmtKind::Assign { value, .. } => {
            collect_hints_in_expr(value, ctx, param_index, line_map, range, hints);
        }
        StmtKind::Decorated { inner, .. } => {
            collect_hints_in_stmt(inner, ctx, param_index, line_map, range, hints);
        }
        _ => {}
    }
}

/// Recursively walk an expression collecting call-arg param-name hints.
///
/// `ctx` is threaded through unused in wave-4 — placeholder for wave-5
/// where param-name hints will also surface callee types in tooltips.
#[allow(clippy::only_used_in_recursion)]
fn collect_hints_in_expr(
    expr: &Expr,
    ctx: &TypeCheckCtx,
    param_index: &ParamIndex,
    line_map: &LineMap,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            if let ExprKind::Name(name) = &callee.kind
                && let Some(param_names) = param_index.get(name)
            {
                for (idx, arg) in args.iter().enumerate() {
                    if let ast::CallArg::Positional(arg_expr) = arg {
                        if !arg_emits_param_hint(arg_expr, param_names.get(idx).map(String::as_str))
                        {
                            continue;
                        }
                        let Some(param_name) = param_names.get(idx) else {
                            continue;
                        };
                        if let Some(hint) = param_name_hint(arg_expr, param_name, line_map, range) {
                            hints.push(hint);
                        }
                    }
                }
            }
            collect_hints_in_expr(callee, ctx, param_index, line_map, range, hints);
            for arg in args {
                match arg {
                    ast::CallArg::Positional(e)
                    | ast::CallArg::Keyword(_, e)
                    | ast::CallArg::StarArgs(e)
                    | ast::CallArg::StarStarKwargs(e) => {
                        collect_hints_in_expr(e, ctx, param_index, line_map, range, hints);
                    }
                }
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_hints_in_expr(lhs, ctx, param_index, line_map, range, hints);
            collect_hints_in_expr(rhs, ctx, param_index, line_map, range, hints);
        }
        ExprKind::Unary { operand, .. } => {
            collect_hints_in_expr(operand, ctx, param_index, line_map, range, hints);
        }
        ExprKind::Cast { expr, .. } | ExprKind::Borrow(expr) | ExprKind::Await(expr) => {
            collect_hints_in_expr(expr, ctx, param_index, line_map, range, hints);
        }
        _ => {}
    }
}

/// Decide whether to emit a param-name hint for `arg`. Skip:
/// - literals (their value already shows their slot purpose),
/// - identifier args that happen to match the param name verbatim,
/// - borrows of a single name that matches the param name.
fn arg_emits_param_hint(arg: &Expr, param_name: Option<&str>) -> bool {
    match (&arg.kind, param_name) {
        (ExprKind::Literal(_), _) => false,
        (ExprKind::Name(arg_name), Some(p)) if arg_name == p => false,
        (ExprKind::Borrow(inner), Some(p)) => {
            if let ExprKind::Name(arg_name) = &inner.kind {
                arg_name != p
            } else {
                true
            }
        }
        _ => true,
    }
}

/// Build a `let`-binder type hint if the pattern is a single binding
/// (the wave-4 honest-scope subset).
fn let_type_hint(
    target: &Pattern,
    ctx: &TypeCheckCtx,
    line_map: &LineMap,
    range: &Range,
) -> Option<InlayHint> {
    let PatternKind::Binding(name) = &target.kind else {
        return None;
    };
    let ty = ctx.lookup(name)?;

    if !span_intersects_range(&target.span, range, line_map) {
        return None;
    }

    let position = line_map.byte_to_position(target.span.end);
    Some(InlayHint {
        position,
        label: InlayHintLabel::String(format!(": {ty}")),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(false),
        data: None,
    })
}

/// Build a parameter-name hint for a single positional call-arg.
fn param_name_hint(
    arg: &Expr,
    param_name: &str,
    line_map: &LineMap,
    range: &Range,
) -> Option<InlayHint> {
    if !span_intersects_range(&arg.span, range, line_map) {
        return None;
    }
    let position = line_map.byte_to_position(arg.span.start);
    Some(InlayHint {
        position,
        label: InlayHintLabel::String(format!("{param_name}:")),
        kind: Some(InlayHintKind::PARAMETER),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: Some(true),
        data: None,
    })
}

/// Test whether the byte-span `span` intersects the LSP `range`.
/// Uses the inclusive convention that a span ending exactly at the
/// range's start byte still intersects (so a binder span ending at
/// the visible range's leading edge still emits).
fn span_intersects_range(span: &Span, range: &Range, line_map: &LineMap) -> bool {
    let start_pos = line_map.byte_to_position(span.start);
    let end_pos = line_map.byte_to_position(span.end);
    !lsp_position_before(end_pos, range.start) && !lsp_position_before(range.end, start_pos)
}

fn lsp_position_before(a: Position, b: Position) -> bool {
    (a.line, a.character) < (b.line, b.character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobrust_frontend::span::FileId;
    use cobrust_types::check_incremental;

    fn checked_ctx(source: &str) -> TypeCheckCtx {
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse failed");
        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).expect("lower failed");
        let mut ctx = TypeCheckCtx::new();
        let _ = check_incremental(&mut ctx, &hir, 1);
        ctx
    }

    fn full_range(source: &str) -> Range {
        let line_map = LineMap::from_source(source);
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: line_map.byte_to_position(u32::try_from(source.len()).unwrap_or(u32::MAX)),
        }
    }

    #[test]
    fn collects_param_names_simple_fn() {
        let source = "fn add(x: i64, y: i64) -> i64:\n    return x + y\n";
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse");
        let index = collect_param_names(&ast);
        let params = index.get("add").expect("add fn indexed");
        assert_eq!(params, &vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn arg_emits_param_hint_skips_literal() {
        let source = "let x = 1\n";
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse");
        let StmtKind::Let { value, .. } = &ast.items[0].kind else {
            panic!("expected let");
        };
        assert!(!arg_emits_param_hint(value, Some("x")));
    }

    #[test]
    fn arg_emits_param_hint_skips_name_matching_param() {
        // `add(x, y)` where param names are also `x`, `y` — skip both.
        let source = "let x: i64 = 1\nlet y: i64 = 2\nadd(x, y)\n";
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse");
        // statements: 0,1 are lets, 2 is the expr stmt with the call.
        let StmtKind::Expr(call_expr) = &ast.items[2].kind else {
            panic!("expected expr stmt");
        };
        let ExprKind::Call { args, .. } = &call_expr.kind else {
            panic!("expected call");
        };
        let ast::CallArg::Positional(first) = &args[0] else {
            panic!("expected positional");
        };
        assert!(!arg_emits_param_hint(first, Some("x")));
    }

    #[test]
    fn build_inlay_hints_let_emits_type_hint() {
        let source = "let x = 42\n";
        let line_map = LineMap::from_source(source);
        let ctx = checked_ctx(source);
        let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
        assert_eq!(hints.len(), 1, "expected 1 hint for single let");
        if let InlayHintLabel::String(s) = &hints[0].label {
            assert!(s.starts_with(": "), "label should start with `: `; got {s}");
        } else {
            panic!("expected string label");
        }
        assert_eq!(hints[0].kind, Some(InlayHintKind::TYPE));
    }

    #[test]
    fn build_inlay_hints_let_with_annot_emits_nothing() {
        let source = "let x: i64 = 42\n";
        let line_map = LineMap::from_source(source);
        let ctx = checked_ctx(source);
        let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
        assert!(hints.is_empty(), "explicit annot → no hint");
    }

    #[test]
    fn build_inlay_hints_parse_failure_returns_empty() {
        // Unterminated string fails the lexer. The function MUST NOT panic.
        let source = "let x = \"unterminated";
        let line_map = LineMap::from_source(source);
        let ctx = TypeCheckCtx::new();
        let hints = build_inlay_hints(source, &line_map, full_range(source), &ctx);
        assert!(hints.is_empty());
    }
}
