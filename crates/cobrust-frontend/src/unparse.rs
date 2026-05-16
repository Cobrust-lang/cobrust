//! Unparser: AST → canonical Cobrust source.
//!
//! Used by the round-trip integration test (`tests/round_trip.rs`) to
//! verify `parse(unparse(ast)) == ast` modulo span normalization. The
//! output is **canonical** but not byte-identical to the original
//! source — comments, blank lines and arbitrary whitespace are not
//! preserved.

#![allow(clippy::too_many_lines)]

use std::fmt::Write as _;

use crate::ast::{
    AccessKind, AssignOp, BinOp, Block, BreakKind, CallArg, ClassDef, CollectionLit, Comprehension,
    ComprehensionElem, ComprehensionKind, DictEntry, Expr, ExprKind, FStrPart, FnDef, ImportStmt,
    IndexKind, Literal, MatchArm, Module, Param, Params, Pattern, PatternKind, Stmt, StmtKind,
    Type, TypeKind, UnaryOp, WithItem,
};

/// Render a module to canonical Cobrust source.
#[must_use]
pub fn unparse(module: &Module) -> String {
    let mut w = Writer::default();
    w.write_module(module);
    w.out
}

#[derive(Default)]
struct Writer {
    out: String,
    indent: usize,
}

impl Writer {
    fn pad(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn newline(&mut self) {
        self.out.push('\n');
    }

    fn push(&mut self, s: &str) {
        self.out.push_str(s);
    }

    // -------- module / stmt -------------------------------------------

    fn write_module(&mut self, m: &Module) {
        if let Some(d) = &m.docstring {
            self.pad();
            self.push(&format!("{:?}", d));
            self.newline();
        }
        for s in &m.items {
            self.write_stmt(s);
        }
    }

    fn write_block(&mut self, b: &Block) {
        if b.stmts.is_empty() {
            self.indent += 1;
            self.pad();
            self.push("pass");
            self.newline();
            self.indent -= 1;
            return;
        }
        self.indent += 1;
        for s in &b.stmts {
            self.write_stmt(s);
        }
        self.indent -= 1;
    }

    fn write_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Pass => {
                self.pad();
                self.push("pass");
                self.newline();
            }
            StmtKind::BreakContinue(BreakKind::Break) => {
                self.pad();
                self.push("break");
                self.newline();
            }
            StmtKind::BreakContinue(BreakKind::Continue) => {
                self.pad();
                self.push("continue");
                self.newline();
            }
            StmtKind::Return(e) => {
                self.pad();
                self.push("return");
                if let Some(v) = e {
                    self.push(" ");
                    self.write_expr(v);
                }
                self.newline();
            }
            StmtKind::Raise { exc, cause } => {
                self.pad();
                self.push("raise");
                if let Some(v) = exc {
                    self.push(" ");
                    self.write_expr(v);
                }
                if let Some(c) = cause {
                    self.push(" from ");
                    self.write_expr(c);
                }
                self.newline();
            }
            StmtKind::Expr(e) => {
                self.pad();
                self.write_expr(e);
                self.newline();
            }
            StmtKind::Assign { target, op, value } => {
                self.pad();
                self.write_expr(target);
                self.push(match op {
                    AssignOp::Eq => " = ",
                    AssignOp::PlusEq => " += ",
                    AssignOp::MinusEq => " -= ",
                    AssignOp::StarEq => " *= ",
                    AssignOp::StarStarEq => " **= ",
                    AssignOp::SlashEq => " /= ",
                    AssignOp::SlashSlashEq => " //= ",
                    AssignOp::PercentEq => " %= ",
                    AssignOp::AmpEq => " &= ",
                    AssignOp::PipeEq => " |= ",
                    AssignOp::CaretEq => " ^= ",
                    AssignOp::ShlEq => " <<= ",
                    AssignOp::ShrEq => " >>= ",
                });
                self.write_expr(value);
                self.newline();
            }
            StmtKind::Let {
                target,
                annot,
                value,
            } => {
                self.pad();
                self.push("let ");
                self.write_pattern(target);
                if let Some(t) = annot {
                    self.push(": ");
                    self.write_type(t);
                }
                self.push(" = ");
                self.write_expr(value);
                self.newline();
            }
            StmtKind::If {
                cond,
                then_block,
                elifs,
                else_block,
            } => {
                self.pad();
                self.push("if ");
                self.write_expr(cond);
                self.push(":");
                self.newline();
                self.write_block(then_block);
                for (c, b) in elifs {
                    self.pad();
                    self.push("elif ");
                    self.write_expr(c);
                    self.push(":");
                    self.newline();
                    self.write_block(b);
                }
                if let Some(e) = else_block {
                    self.pad();
                    self.push("else:");
                    self.newline();
                    self.write_block(e);
                }
            }
            StmtKind::While {
                cond,
                body,
                else_block,
            } => {
                self.pad();
                self.push("while ");
                self.write_expr(cond);
                self.push(":");
                self.newline();
                self.write_block(body);
                if let Some(e) = else_block {
                    self.pad();
                    self.push("else:");
                    self.newline();
                    self.write_block(e);
                }
            }
            StmtKind::For {
                target,
                iter,
                body,
                else_block,
            } => {
                self.pad();
                self.push("for ");
                self.write_pattern(target);
                self.push(" in ");
                self.write_expr(iter);
                self.push(":");
                self.newline();
                self.write_block(body);
                if let Some(e) = else_block {
                    self.pad();
                    self.push("else:");
                    self.newline();
                    self.write_block(e);
                }
            }
            StmtKind::Match { scrutinee, arms } => {
                self.pad();
                self.push("match ");
                self.write_expr(scrutinee);
                self.push(":");
                self.newline();
                self.indent += 1;
                for a in arms {
                    self.write_match_arm(a);
                }
                self.indent -= 1;
            }
            StmtKind::With { items, body } => {
                self.pad();
                self.push("with ");
                let mut first = true;
                for it in items {
                    if !first {
                        self.push(", ");
                    }
                    first = false;
                    self.write_with_item(it);
                }
                self.push(":");
                self.newline();
                self.write_block(body);
            }
            StmtKind::Try {
                body,
                handlers,
                else_block,
                finally_block,
            } => {
                self.pad();
                self.push("try:");
                self.newline();
                self.write_block(body);
                for h in handlers {
                    self.pad();
                    self.push("except ");
                    self.write_type(&h.exc_type);
                    if let Some(name) = &h.binding {
                        self.push(" as ");
                        self.push(name);
                    }
                    self.push(":");
                    self.newline();
                    self.write_block(&h.body);
                }
                if let Some(e) = else_block {
                    self.pad();
                    self.push("else:");
                    self.newline();
                    self.write_block(e);
                }
                if let Some(f) = finally_block {
                    self.pad();
                    self.push("finally:");
                    self.newline();
                    self.write_block(f);
                }
            }
            StmtKind::Import(imp) => {
                self.pad();
                match imp {
                    ImportStmt::Import { path, alias } => {
                        self.push("import ");
                        self.push(&path.join("."));
                        if let Some(a) = alias {
                            self.push(" as ");
                            self.push(a);
                        }
                    }
                    ImportStmt::From { path, targets } => {
                        self.push("from ");
                        self.push(&path.join("."));
                        self.push(" import ");
                        let mut first = true;
                        for t in targets {
                            if !first {
                                self.push(", ");
                            }
                            first = false;
                            self.push(&t.name);
                            if let Some(a) = &t.alias {
                                self.push(" as ");
                                self.push(a);
                            }
                        }
                    }
                }
                self.newline();
            }
            StmtKind::Fn(fd) => {
                self.write_fn(fd);
            }
            StmtKind::Class(cd) => {
                self.write_class(cd);
            }
            StmtKind::TypeAlias(ta) => {
                self.pad();
                self.push("type ");
                self.push(&ta.name);
                if !ta.type_params.is_empty() {
                    self.push("[");
                    self.push(&ta.type_params.join(", "));
                    self.push("]");
                }
                self.push(" = ");
                self.write_type(&ta.value);
                self.newline();
            }
            StmtKind::Decorated { decorators, inner } => {
                for d in decorators {
                    self.pad();
                    self.push("@");
                    self.write_expr(d);
                    self.newline();
                }
                self.write_stmt(inner);
            }
        }
    }

    fn write_match_arm(&mut self, a: &MatchArm) {
        self.pad();
        self.push("case ");
        self.write_pattern(&a.pattern);
        if let Some(g) = &a.guard {
            self.push(" if ");
            self.write_expr(g);
        }
        self.push(":");
        self.newline();
        self.write_block(&a.body);
    }

    fn write_with_item(&mut self, it: &WithItem) {
        self.write_expr(&it.context);
        if let Some(t) = &it.target {
            self.push(" as ");
            self.write_pattern(t);
        }
    }

    fn write_fn(&mut self, fd: &FnDef) {
        self.pad();
        self.push("fn ");
        self.push(&fd.name);
        self.push("(");
        self.write_params(&fd.params);
        self.push(")");
        if let Some(rt) = &fd.return_type {
            self.push(" -> ");
            self.write_type(rt);
        }
        self.push(":");
        self.newline();
        self.write_block(&fd.body);
    }

    fn write_class(&mut self, cd: &ClassDef) {
        self.pad();
        self.push("class ");
        self.push(&cd.name);
        if let Some(b) = &cd.base {
            self.push("(");
            self.write_expr(b);
            self.push(")");
        }
        if !cd.traits.is_empty() {
            self.push(": ");
            let mut first = true;
            for t in &cd.traits {
                if !first {
                    self.push(", ");
                }
                first = false;
                self.write_type(t);
            }
        }
        self.push(":");
        self.newline();
        self.write_block(&cd.body);
    }

    fn write_params(&mut self, params: &Params) {
        let mut first = true;
        for p in &params.positional {
            if !first {
                self.push(", ");
            }
            first = false;
            self.write_param(p);
        }
        if let Some(vp) = &params.var_positional {
            if !first {
                self.push(", ");
            }
            first = false;
            self.push("*");
            self.write_param(vp);
        } else if !params.keyword_only.is_empty() {
            if !first {
                self.push(", ");
            }
            first = false;
            self.push("*");
        }
        for p in &params.keyword_only {
            if !first {
                self.push(", ");
            }
            first = false;
            self.write_param(p);
        }
        if let Some(vk) = &params.var_keyword {
            if !first {
                self.push(", ");
            }
            self.push("**");
            self.write_param(vk);
        }
    }

    fn write_param(&mut self, p: &Param) {
        self.push(&p.name);
        if let Some(t) = &p.annot {
            self.push(": ");
            self.write_type(t);
        }
        if let Some(d) = &p.default {
            self.push(" = ");
            self.write_literal(d);
        }
    }

    // -------- expr ----------------------------------------------------

    fn write_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Literal(l) => self.write_literal(l),
            ExprKind::FString(parts) => self.write_fstring(parts),
            ExprKind::Name(n) => self.push(n),
            ExprKind::Collection(c) => self.write_collection(c),
            ExprKind::Comprehension(c) => self.write_comprehension(c),
            ExprKind::Lambda { params, body } => {
                self.push("lambda");
                if !is_empty_params(params) {
                    self.push(" ");
                    self.write_params(params);
                }
                self.push(": ");
                self.write_expr(body);
            }
            ExprKind::Call { callee, args } => {
                self.write_expr(callee);
                self.push("(");
                let mut first = true;
                for a in args {
                    if !first {
                        self.push(", ");
                    }
                    first = false;
                    match a {
                        CallArg::Positional(e) => self.write_expr(e),
                        CallArg::Keyword(n, e) => {
                            self.push(n);
                            self.push("=");
                            self.write_expr(e);
                        }
                        CallArg::StarArgs(e) => {
                            self.push("*");
                            self.write_expr(e);
                        }
                        CallArg::StarStarKwargs(e) => {
                            self.push("**");
                            self.write_expr(e);
                        }
                    }
                }
                self.push(")");
            }
            ExprKind::Access(AccessKind::Attribute { base, name }) => {
                self.write_expr(base);
                self.push(".");
                self.push(name);
            }
            ExprKind::Access(AccessKind::Index { base, index }) => {
                self.write_expr(base);
                self.push("[");
                self.write_index(index);
                self.push("]");
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.push("(");
                self.write_expr(lhs);
                self.push(" ");
                self.push(binop_str(*op));
                self.push(" ");
                self.write_expr(rhs);
                self.push(")");
            }
            ExprKind::Unary { op, operand } => {
                self.push("(");
                self.push(unaryop_str(*op));
                if matches!(op, UnaryOp::Not) {
                    self.push(" ");
                }
                self.write_expr(operand);
                self.push(")");
            }
            ExprKind::Await(e) => {
                self.push("(await ");
                self.write_expr(e);
                self.push(")");
            }
            ExprKind::Yield(None) => {
                self.push("(yield)");
            }
            ExprKind::Yield(Some(e)) => {
                self.push("(yield ");
                self.write_expr(e);
                self.push(")");
            }
            ExprKind::YieldFrom(e) => {
                self.push("(yield from ");
                self.write_expr(e);
                self.push(")");
            }
            ExprKind::Cast { expr, target } => {
                self.push("(");
                self.write_expr(expr);
                self.push(" as ");
                self.write_type(target);
                self.push(")");
            }
        }
    }

    fn write_literal(&mut self, l: &Literal) {
        match l {
            Literal::Int(s) | Literal::Float(s) | Literal::Imag(s) => self.push(s),
            Literal::Str(s) => {
                let _ = write!(self.out, "{:?}", s);
            }
            Literal::Bytes(bs) => {
                self.push("b\"");
                for b in bs {
                    match *b {
                        b'\\' => self.push("\\\\"),
                        b'"' => self.push("\\\""),
                        b'\n' => self.push("\\n"),
                        b'\t' => self.push("\\t"),
                        b'\r' => self.push("\\r"),
                        c if c.is_ascii_graphic() || c == b' ' => {
                            self.out.push(c as char);
                        }
                        c => {
                            let _ = write!(self.out, "\\x{:02x}", c);
                        }
                    }
                }
                self.push("\"");
            }
            Literal::Bool(true) => self.push("True"),
            Literal::Bool(false) => self.push("False"),
            Literal::None => self.push("None"),
        }
    }

    fn write_fstring(&mut self, parts: &[FStrPart]) {
        self.push("f\"");
        for p in parts {
            match p {
                FStrPart::Lit(s) => {
                    for ch in s.chars() {
                        match ch {
                            '"' => self.push("\\\""),
                            '\\' => self.push("\\\\"),
                            '{' => self.push("{{"),
                            '}' => self.push("}}"),
                            '\n' => self.push("\\n"),
                            _ => self.out.push(ch),
                        }
                    }
                }
                FStrPart::Expr {
                    expr,
                    debug_equals,
                    format_spec,
                } => {
                    self.push("{");
                    self.write_expr(expr);
                    if *debug_equals {
                        self.push("=");
                    }
                    if let Some(spec) = format_spec {
                        self.push(":");
                        self.push(spec);
                    }
                    self.push("}");
                }
            }
        }
        self.push("\"");
    }

    fn write_collection(&mut self, c: &CollectionLit) {
        match c {
            CollectionLit::Tuple(items) => {
                self.push("(");
                if items.len() == 1 {
                    self.write_expr(&items[0]);
                    self.push(",");
                } else {
                    for (i, e) in items.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.write_expr(e);
                    }
                }
                self.push(")");
            }
            CollectionLit::List(items) => {
                self.push("[");
                for (i, e) in items.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_expr(e);
                }
                self.push("]");
            }
            CollectionLit::Set(items) => {
                if items.is_empty() {
                    // Empty set is `set()` not `{}`. Round-trip rule:
                    // we never produce an empty set literal.
                    self.push("set()");
                } else {
                    self.push("{");
                    for (i, e) in items.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.write_expr(e);
                    }
                    self.push("}");
                }
            }
            CollectionLit::Dict(entries) => {
                self.push("{");
                for (i, e) in entries.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    match e {
                        DictEntry::Pair(k, v) => {
                            self.write_expr(k);
                            self.push(": ");
                            self.write_expr(v);
                        }
                        DictEntry::Spread(e) => {
                            self.push("**");
                            self.write_expr(e);
                        }
                    }
                }
                self.push("}");
            }
        }
    }

    fn write_comprehension(&mut self, c: &Comprehension) {
        let (open, close) = match c.kind {
            ComprehensionKind::List => ("[", "]"),
            ComprehensionKind::Set => ("{", "}"),
            ComprehensionKind::Dict => ("{", "}"),
            ComprehensionKind::Generator => ("(", ")"),
        };
        self.push(open);
        match &c.element {
            ComprehensionElem::Single(e) => self.write_expr(e),
            ComprehensionElem::KeyValue(k, v) => {
                self.write_expr(k);
                self.push(": ");
                self.write_expr(v);
            }
        }
        for cl in &c.clauses {
            self.push(" for ");
            self.write_pattern(&cl.target);
            self.push(" in ");
            self.write_expr(&cl.iter);
            for g in &cl.guards {
                self.push(" if ");
                self.write_expr(g);
            }
        }
        self.push(close);
    }

    fn write_index(&mut self, idx: &IndexKind) {
        match idx {
            IndexKind::Expr(e) => self.write_expr(e),
            IndexKind::Slice { start, stop, step } => {
                if let Some(e) = start {
                    self.write_expr(e);
                }
                self.push(":");
                if let Some(e) = stop {
                    self.write_expr(e);
                }
                if let Some(e) = step {
                    self.push(":");
                    self.write_expr(e);
                }
            }
            IndexKind::Tuple(items) => {
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_index(it);
                }
            }
        }
    }

    // -------- type / pattern ------------------------------------------

    fn write_type(&mut self, t: &Type) {
        match &t.kind {
            TypeKind::Name(parts) => self.push(&parts.join(".")),
            TypeKind::Generic { base, args } => {
                self.push(&base.join("."));
                self.push("[");
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_type(a);
                }
                self.push("]");
            }
            TypeKind::Union(parts) => {
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        self.push(" | ");
                    }
                    self.write_type(p);
                }
            }
            TypeKind::Fn {
                params,
                return_type,
            } => {
                self.push("(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_type(p);
                }
                self.push(") -> ");
                self.write_type(return_type);
            }
            TypeKind::Tuple(parts) => {
                self.push("(");
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_type(p);
                }
                if parts.len() == 1 {
                    self.push(",");
                }
                self.push(")");
            }
        }
    }

    fn write_pattern(&mut self, p: &Pattern) {
        match &p.kind {
            PatternKind::Wildcard => self.push("_"),
            PatternKind::Binding(n) => self.push(n),
            PatternKind::Literal(l) => self.write_literal(l),
            PatternKind::Sequence { items, rest } => {
                self.push("[");
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_pattern(it);
                }
                if let Some(r) = rest {
                    if !items.is_empty() {
                        self.push(", ");
                    }
                    self.push("*");
                    self.write_pattern(r);
                }
                self.push("]");
            }
            PatternKind::Mapping { entries, rest } => {
                self.push("{");
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_expr(k);
                    self.push(": ");
                    self.write_pattern(v);
                }
                if let Some(r) = rest {
                    if !entries.is_empty() {
                        self.push(", ");
                    }
                    self.push("**");
                    self.push(r);
                }
                self.push("}");
            }
            PatternKind::Class {
                base,
                positional,
                keyword,
            } => {
                self.push(&base.join("."));
                self.push("(");
                let mut first = true;
                for p in positional {
                    if !first {
                        self.push(", ");
                    }
                    first = false;
                    self.write_pattern(p);
                }
                for (n, p) in keyword {
                    if !first {
                        self.push(", ");
                    }
                    first = false;
                    self.push(n);
                    self.push("=");
                    self.write_pattern(p);
                }
                self.push(")");
            }
            PatternKind::Or(alts) => {
                for (i, p) in alts.iter().enumerate() {
                    if i > 0 {
                        self.push(" | ");
                    }
                    self.write_pattern(p);
                }
            }
        }
    }
}

fn is_empty_params(p: &Params) -> bool {
    p.positional.is_empty()
        && p.var_positional.is_none()
        && p.keyword_only.is_empty()
        && p.var_keyword.is_none()
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::MatMul => "@",
        BinOp::Div => "/",
        BinOp::FloorDiv => "//",
        BinOp::Mod => "%",
        BinOp::Pow => "**",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::LtEq => "<=",
        BinOp::Gt => ">",
        BinOp::GtEq => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::In => "in",
        BinOp::NotIn => "not in",
    }
}

fn unaryop_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Plus => "+",
        UnaryOp::Neg => "-",
        UnaryOp::BitNot => "~",
        UnaryOp::Not => "not",
    }
}
