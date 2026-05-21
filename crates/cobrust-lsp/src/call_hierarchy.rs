//! `textDocument/prepareCallHierarchy` + `callHierarchy/incomingCalls`
//! + `callHierarchy/outgoingCalls` handlers — ADR-0057f §3.3.
//!
//! Phase J wave-4 call hierarchy. Same-document fn-graph traversal:
//! given a cursor on a fn name, resolve it to a [`CallHierarchyItem`],
//! then find incoming callers + outgoing callees by AST walk over the
//! same source.
//!
//! Honest scope (per ADR-0057f §3.3):
//! - Same-document only. Cross-file caller / callee aggregation
//!   deferred to wave-5.
//! - The fn def-name span is recovered by scanning the source for the
//!   first word-boundary occurrence of the name within the fn def's
//!   `Stmt.span` — the same `first_word_occurrence` heuristic
//!   goto-def uses (ADR-0057e §3.1).

use cobrust_frontend::ast::{
    AccessKind, Block, CallArg, Expr, ExprKind, FnDef, Module, Stmt, StmtKind,
};
use cobrust_frontend::span::{FileId, Span};
use cobrust_types::TypeCheckCtx;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range,
    SymbolKind, Url,
};

use crate::completion::KEYWORDS;
use crate::hover::word_at_offset;
use crate::span_convert::LineMap;

/// Resolve the fn under the cursor to a [`CallHierarchyItem`].
///
/// Algorithm (ADR-0057f §3.3):
/// 1. Position → byte offset → `word_at_offset` to find the identifier.
/// 2. Guard: name must NOT be a keyword and must be a known binding.
/// 3. Locate the fn def in the AST whose `Stmt::Fn(FnDef { name, .. })`
///    matches the identifier, and recover the def-site name span via a
///    word-scan inside the fn def's `Stmt.span`.
/// 4. Build a `CallHierarchyItem` with `kind: SymbolKind::FUNCTION`,
///    `range` = fn def's whole span, `selection_range` = name span.
///
/// Returns `None` if the cursor is not on an identifier, the identifier
/// is a keyword, the binding is unknown, or no fn def with that name
/// exists in the same document.
#[must_use]
pub fn prepare_call_hierarchy(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
    uri: Url,
) -> Option<Vec<CallHierarchyItem>> {
    let byte_offset = line_map.position_to_byte(position)? as usize;
    let (word_start, word_end) = word_at_offset(source, byte_offset)?;
    let name = source.get(word_start..word_end)?;
    if KEYWORDS.contains(&name) {
        return None;
    }
    // Wave-4 honest-scope: ctx.lookup ensures the binding exists in
    // the cross-document type ctx; the fn might still live in another
    // file. Same-document scope filters via AST below.
    ctx.lookup(name)?;

    let module = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).ok()?;
    let fn_def = find_fn_def(&module, name)?;
    let item = fn_to_call_hierarchy_item(fn_def.stmt, name, line_map, uri)?;
    Some(vec![item])
}

/// Build the list of `CallHierarchyIncomingCall` for a target fn item
/// by walking the same document's AST.
///
/// For each `Expr::Call { callee: Name(name), .. }` where `name ==
/// item.name`, find the enclosing fn def and aggregate the call-site
/// span as a `from_range`. Multiple call-sites in the same caller
/// group into a single IncomingCall with `from_ranges` extended.
///
/// Returns an empty vector when no callers exist in the same document.
#[must_use]
pub fn build_incoming_calls(
    source: &str,
    line_map: &LineMap,
    item: &CallHierarchyItem,
) -> Vec<CallHierarchyIncomingCall> {
    let Ok(module) = cobrust_frontend::parse_str(source, FileId::SYNTHETIC) else {
        return Vec::new();
    };

    // (caller_name → (caller_item, [call_site_ranges]))
    let mut callers: std::collections::HashMap<String, (CallHierarchyItem, Vec<Range>)> =
        std::collections::HashMap::new();

    for stmt in &module.items {
        walk_for_incoming(stmt, &item.name, line_map, &item.uri, &mut callers);
    }

    callers
        .into_values()
        .map(|(caller, ranges)| CallHierarchyIncomingCall {
            from: caller,
            from_ranges: ranges,
        })
        .collect()
}

/// Build the list of `CallHierarchyOutgoingCall` for a target fn item
/// by walking the target fn def's body in the same document.
///
/// For each `Expr::Call { callee: Name(callee_name), .. }` inside the
/// fn body, build (or extend) an OutgoingCall for `callee_name`. The
/// `from_ranges` aggregate the call-site spans.
///
/// Returns an empty vector when the target fn cannot be located or
/// has no outgoing calls.
#[must_use]
pub fn build_outgoing_calls(
    source: &str,
    line_map: &LineMap,
    item: &CallHierarchyItem,
) -> Vec<CallHierarchyOutgoingCall> {
    let Ok(module) = cobrust_frontend::parse_str(source, FileId::SYNTHETIC) else {
        return Vec::new();
    };
    let Some(target) = find_fn_def(&module, &item.name) else {
        return Vec::new();
    };
    let mut callees: std::collections::HashMap<String, (CallHierarchyItem, Vec<Range>)> =
        std::collections::HashMap::new();

    let StmtKind::Fn(FnDef { body, .. }) = &target.stmt.kind else {
        // The decorated path strips a `Decorated` wrapper to the
        // inner fn def in find_fn_def; reaching here means the find
        // routine succeeded on something that wasn't a FnDef → bail.
        return Vec::new();
    };

    let body_ref: &Block = body;
    walk_block_for_outgoing(body_ref, line_map, &item.uri, &module, &mut callees);

    callees
        .into_values()
        .map(|(to, ranges)| CallHierarchyOutgoingCall {
            to,
            from_ranges: ranges,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// A resolved fn def lookup: the matched statement plus, if the call
/// came through a `Decorated` wrapper, the outer decorated span.
struct ResolvedFn<'a> {
    stmt: &'a Stmt,
}

/// Find the first fn def in `module.items` matching `name`. Walks past
/// `Decorated { inner, .. }` wrappers.
fn find_fn_def<'a>(module: &'a Module, name: &str) -> Option<ResolvedFn<'a>> {
    for stmt in &module.items {
        if let Some(found) = match_fn_def(stmt, name) {
            return Some(ResolvedFn { stmt: found });
        }
    }
    None
}

fn match_fn_def<'a>(stmt: &'a Stmt, name: &str) -> Option<&'a Stmt> {
    match &stmt.kind {
        StmtKind::Fn(FnDef { name: fn_name, .. }) if fn_name == name => Some(stmt),
        StmtKind::Decorated { inner, .. } => match_fn_def(inner, name),
        _ => None,
    }
}

/// Build a `CallHierarchyItem` from a fn def statement.
fn fn_to_call_hierarchy_item(
    stmt: &Stmt,
    name: &str,
    line_map: &LineMap,
    uri: Url,
) -> Option<CallHierarchyItem> {
    let selection_span = locate_name_in_span(line_map.source(), &stmt.span, name)?;
    let selection_range = Range {
        start: line_map.byte_to_position(selection_span.start),
        end: line_map.byte_to_position(selection_span.end),
    };
    let range = Range {
        start: line_map.byte_to_position(stmt.span.start),
        end: line_map.byte_to_position(stmt.span.end),
    };
    Some(CallHierarchyItem {
        name: name.to_string(),
        kind: SymbolKind::FUNCTION,
        tags: None,
        detail: None,
        uri,
        range,
        selection_range,
        data: None,
    })
}

/// Find the first word-boundary occurrence of `name` inside the byte
/// range `[span.start, span.end)` of `source`.
fn locate_name_in_span(source: &str, span: &Span, name: &str) -> Option<Span> {
    let start_idx = (span.start as usize).min(source.len());
    let end_idx = (span.end as usize).min(source.len());
    if start_idx >= end_idx || name.is_empty() {
        return None;
    }
    let segment = &source[start_idx..end_idx];
    let seg_bytes = segment.as_bytes();
    let name_bytes = name.as_bytes();
    let slen = seg_bytes.len();
    let nlen = name_bytes.len();
    if slen < nlen {
        return None;
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
                let abs_start = u32::try_from(start_idx + i).unwrap_or(u32::MAX);
                let abs_end = u32::try_from(start_idx + i + nlen).unwrap_or(u32::MAX);
                return Some(Span::new(FileId::SYNTHETIC, abs_start, abs_end));
            }
        }
        i += 1;
    }
    None
}

/// Walk a statement looking for `Expr::Call { callee: Name(target), .. }`
/// sites. When a hit is found, aggregate (caller-fn → call-site-range)
/// into `callers`. Caller fn = the closest enclosing fn def; if no
/// enclosing fn def, we treat the call-site as module-level.
fn walk_for_incoming(
    stmt: &Stmt,
    target: &str,
    line_map: &LineMap,
    uri: &Url,
    callers: &mut std::collections::HashMap<String, (CallHierarchyItem, Vec<Range>)>,
) {
    match &stmt.kind {
        StmtKind::Fn(FnDef { name, body, .. }) => {
            // Build the caller's CallHierarchyItem lazily if any call
            // hits inside the body.
            let mut hits: Vec<Range> = Vec::new();
            collect_call_sites_in_block(body, target, line_map, &mut hits);
            if !hits.is_empty()
                && let Some(item) = fn_to_call_hierarchy_item(stmt, name, line_map, uri.clone())
            {
                let entry = callers
                    .entry(name.clone())
                    .or_insert_with(|| (item.clone(), Vec::new()));
                entry.1.extend(hits);
            }
            // Recurse for nested fns (Cobrust allows fn-in-fn).
            for inner in &body.stmts {
                walk_for_incoming(inner, target, line_map, uri, callers);
            }
        }
        StmtKind::Decorated { inner, .. } => {
            walk_for_incoming(inner, target, line_map, uri, callers);
        }
        // Module-level expr/statement call sites — caller is the
        // implicit module. Wave-4 surfaces these under a synthetic
        // "<module>" caller name.
        _ => {
            let mut hits: Vec<Range> = Vec::new();
            collect_call_sites_in_stmt(stmt, target, line_map, &mut hits);
            if !hits.is_empty() {
                let mod_item = CallHierarchyItem {
                    name: "<module>".to_string(),
                    kind: SymbolKind::MODULE,
                    tags: None,
                    detail: None,
                    uri: uri.clone(),
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: u32::MAX,
                            character: 0,
                        },
                    },
                    selection_range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    data: None,
                };
                let entry = callers
                    .entry("<module>".to_string())
                    .or_insert_with(|| (mod_item, Vec::new()));
                entry.1.extend(hits);
            }
        }
    }
}

fn collect_call_sites_in_block(
    block: &Block,
    target: &str,
    line_map: &LineMap,
    hits: &mut Vec<Range>,
) {
    for stmt in &block.stmts {
        collect_call_sites_in_stmt(stmt, target, line_map, hits);
    }
}

fn collect_call_sites_in_stmt(
    stmt: &Stmt,
    target: &str,
    line_map: &LineMap,
    hits: &mut Vec<Range>,
) {
    match &stmt.kind {
        StmtKind::Expr(e) => collect_call_sites_in_expr(e, target, line_map, hits),
        StmtKind::Return(Some(e)) | StmtKind::Raise { exc: Some(e), .. } => {
            collect_call_sites_in_expr(e, target, line_map, hits);
        }
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } => {
            collect_call_sites_in_expr(value, target, line_map, hits);
        }
        StmtKind::If {
            cond,
            then_block,
            elifs,
            else_block,
        } => {
            collect_call_sites_in_expr(cond, target, line_map, hits);
            collect_call_sites_in_block(then_block, target, line_map, hits);
            for (c, b) in elifs {
                collect_call_sites_in_expr(c, target, line_map, hits);
                collect_call_sites_in_block(b, target, line_map, hits);
            }
            if let Some(b) = else_block {
                collect_call_sites_in_block(b, target, line_map, hits);
            }
        }
        StmtKind::While { cond, body, .. } => {
            collect_call_sites_in_expr(cond, target, line_map, hits);
            collect_call_sites_in_block(body, target, line_map, hits);
        }
        StmtKind::For { iter, body, .. } => {
            collect_call_sites_in_expr(iter, target, line_map, hits);
            collect_call_sites_in_block(body, target, line_map, hits);
        }
        StmtKind::Decorated { inner, .. } => {
            collect_call_sites_in_stmt(inner, target, line_map, hits);
        }
        StmtKind::Fn(FnDef { body, .. }) => {
            // Nested fn — descend so module-level walk reaches inner
            // calls too. The outer `walk_for_incoming` recurses, but
            // when called from a deeper stmt we still need coverage.
            collect_call_sites_in_block(body, target, line_map, hits);
        }
        _ => {}
    }
}

fn collect_call_sites_in_expr(expr: &Expr, target: &str, line_map: &LineMap, hits: &mut Vec<Range>) {
    if let ExprKind::Call { callee, args } = &expr.kind {
        if let ExprKind::Name(name) = &callee.kind
            && name == target
        {
            hits.push(Range {
                start: line_map.byte_to_position(expr.span.start),
                end: line_map.byte_to_position(expr.span.end),
            });
        }
        collect_call_sites_in_expr(callee, target, line_map, hits);
        for arg in args {
            match arg {
                CallArg::Positional(e)
                | CallArg::Keyword(_, e)
                | CallArg::StarArgs(e)
                | CallArg::StarStarKwargs(e) => collect_call_sites_in_expr(e, target, line_map, hits),
            }
        }
    } else if let ExprKind::Binary { lhs, rhs, .. } = &expr.kind {
        collect_call_sites_in_expr(lhs, target, line_map, hits);
        collect_call_sites_in_expr(rhs, target, line_map, hits);
    } else if let ExprKind::Unary { operand, .. }
    | ExprKind::Borrow(operand)
    | ExprKind::Await(operand)
    | ExprKind::YieldFrom(operand) = &expr.kind
    {
        collect_call_sites_in_expr(operand, target, line_map, hits);
    } else if let ExprKind::Cast { expr: inner, .. } = &expr.kind {
        collect_call_sites_in_expr(inner, target, line_map, hits);
    } else if let ExprKind::Access(AccessKind::Attribute { base, .. }) = &expr.kind {
        collect_call_sites_in_expr(base, target, line_map, hits);
    }
}

/// Walk a fn body collecting outgoing-call hits keyed by callee name.
fn walk_block_for_outgoing(
    block: &Block,
    line_map: &LineMap,
    uri: &Url,
    module: &Module,
    callees: &mut std::collections::HashMap<String, (CallHierarchyItem, Vec<Range>)>,
) {
    for stmt in &block.stmts {
        walk_stmt_for_outgoing(stmt, line_map, uri, module, callees);
    }
}

fn walk_stmt_for_outgoing(
    stmt: &Stmt,
    line_map: &LineMap,
    uri: &Url,
    module: &Module,
    callees: &mut std::collections::HashMap<String, (CallHierarchyItem, Vec<Range>)>,
) {
    match &stmt.kind {
        StmtKind::Expr(e) => walk_expr_for_outgoing(e, line_map, uri, module, callees),
        StmtKind::Return(Some(e)) | StmtKind::Raise { exc: Some(e), .. } => {
            walk_expr_for_outgoing(e, line_map, uri, module, callees);
        }
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } => {
            walk_expr_for_outgoing(value, line_map, uri, module, callees);
        }
        StmtKind::If {
            cond,
            then_block,
            elifs,
            else_block,
        } => {
            walk_expr_for_outgoing(cond, line_map, uri, module, callees);
            walk_block_for_outgoing(then_block, line_map, uri, module, callees);
            for (c, b) in elifs {
                walk_expr_for_outgoing(c, line_map, uri, module, callees);
                walk_block_for_outgoing(b, line_map, uri, module, callees);
            }
            if let Some(b) = else_block {
                walk_block_for_outgoing(b, line_map, uri, module, callees);
            }
        }
        StmtKind::While { cond, body, .. } => {
            walk_expr_for_outgoing(cond, line_map, uri, module, callees);
            walk_block_for_outgoing(body, line_map, uri, module, callees);
        }
        StmtKind::For { iter, body, .. } => {
            walk_expr_for_outgoing(iter, line_map, uri, module, callees);
            walk_block_for_outgoing(body, line_map, uri, module, callees);
        }
        StmtKind::Decorated { inner, .. } => {
            walk_stmt_for_outgoing(inner, line_map, uri, module, callees);
        }
        _ => {}
    }
}

fn walk_expr_for_outgoing(
    expr: &Expr,
    line_map: &LineMap,
    uri: &Url,
    module: &Module,
    callees: &mut std::collections::HashMap<String, (CallHierarchyItem, Vec<Range>)>,
) {
    if let ExprKind::Call { callee, args } = &expr.kind {
        if let ExprKind::Name(callee_name) = &callee.kind {
            // Add hit for this callee.
            let call_range = Range {
                start: line_map.byte_to_position(expr.span.start),
                end: line_map.byte_to_position(expr.span.end),
            };
            let entry = callees
                .entry(callee_name.clone())
                .or_insert_with(|| {
                    let item = if let Some(found) = find_fn_def(module, callee_name) {
                        fn_to_call_hierarchy_item(found.stmt, callee_name, line_map, uri.clone())
                    } else {
                        None
                    };
                    let item = item.unwrap_or_else(|| CallHierarchyItem {
                        name: callee_name.clone(),
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        detail: None,
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 0,
                            },
                        },
                        selection_range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 0,
                            },
                        },
                        data: None,
                    });
                    (item, Vec::new())
                });
            entry.1.push(call_range);
        }
        walk_expr_for_outgoing(callee, line_map, uri, module, callees);
        for arg in args {
            match arg {
                CallArg::Positional(e)
                | CallArg::Keyword(_, e)
                | CallArg::StarArgs(e)
                | CallArg::StarStarKwargs(e) => {
                    walk_expr_for_outgoing(e, line_map, uri, module, callees);
                }
            }
        }
    } else if let ExprKind::Binary { lhs, rhs, .. } = &expr.kind {
        walk_expr_for_outgoing(lhs, line_map, uri, module, callees);
        walk_expr_for_outgoing(rhs, line_map, uri, module, callees);
    } else if let ExprKind::Unary { operand, .. }
    | ExprKind::Borrow(operand)
    | ExprKind::Await(operand)
    | ExprKind::YieldFrom(operand) = &expr.kind
    {
        walk_expr_for_outgoing(operand, line_map, uri, module, callees);
    } else if let ExprKind::Cast { expr: inner, .. } = &expr.kind {
        walk_expr_for_outgoing(inner, line_map, uri, module, callees);
    } else if let ExprKind::Access(AccessKind::Attribute { base, .. }) = &expr.kind {
        walk_expr_for_outgoing(base, line_map, uri, module, callees);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobrust_frontend::span::FileId;
    use cobrust_types::check_incremental;

    fn checked_ctx(source: &str) -> TypeCheckCtx {
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).expect("parse");
        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).expect("lower");
        let mut ctx = TypeCheckCtx::new();
        let _ = check_incremental(&mut ctx, &hir, 1);
        ctx
    }

    fn uri(path: &str) -> Url {
        Url::parse(&format!("file:///{path}")).expect("uri")
    }

    #[test]
    fn locate_name_in_span_finds_first_match() {
        let source = "fn add(x: i64, y: i64) -> i64:\n    return x + y\n";
        let span = Span::new(
            FileId::SYNTHETIC,
            0,
            u32::try_from(source.len()).expect("u32"),
        );
        let res = locate_name_in_span(source, &span, "add").expect("found");
        // 'add' starts at byte 3 (after "fn ").
        assert_eq!(res.start, 3);
        assert_eq!(res.end, 6);
    }

    #[test]
    fn prepare_returns_none_for_unbound_symbol() {
        let source = "let x = 1\n";
        let line_map = LineMap::from_source(source);
        let ctx = checked_ctx(source);
        // Cursor on `x` resolves to the int binding (no fn def).
        let pos = Position {
            line: 0,
            character: 4,
        };
        let res = prepare_call_hierarchy(source, &line_map, pos, &ctx, uri("a.cb"));
        // No fn def with name "x" → None.
        assert!(res.is_none());
    }
}
